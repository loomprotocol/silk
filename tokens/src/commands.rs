use crate::{
    args::{BalancesArgs, DistributeTokensArgs, StakeArgs, TransactionLogArgs},
    db::{self, TransactionInfo},
    thin_client::{Client, ThinClient},
};
use chrono::prelude::*;
use console::style;
use csv::{ReaderBuilder, Trim};
use indexmap::IndexMap;
use indicatif::{ProgressBar, ProgressStyle};
use pickledb::PickleDb;
use serde::{Deserialize, Serialize};
use solana_sdk::{
    instruction::Instruction,
    message::Message,
    native_token::{lamports_to_sol, sol_to_lamports},
    signature::{unique_signers, Signature, Signer},
    system_instruction,
    transport::TransportError,
};
use solana_stake_program::{
    stake_instruction::{self, LockupArgs},
    stake_state::{Authorized, Lockup, StakeAuthorize},
};
use std::{
    cmp::{self},
    io,
    thread::sleep,
    time::Duration,
};

#[derive(Serialize, Deserialize, Debug, Clone)]
struct Bid {
    accepted_amount_dollars: f64,
    primary_address: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
struct Allocation {
    recipient: String,
    amount: f64,
    lockup_date: String,
}

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("I/O error")]
    IoError(#[from] io::Error),
    #[error("CSV error")]
    CsvError(#[from] csv::Error),
    #[error("PickleDb error")]
    PickleDbError(#[from] pickledb::error::Error),
    #[error("Transport error")]
    TransportError(#[from] TransportError),
    #[error("Missing lockup authority")]
    MissingLockupAuthority,
    #[error("insufficient funds for fee ({0} SOL)")]
    InsufficientFundsForFees(f64),
    #[error("insufficient funds for distribution ({0} SOL)")]
    InsufficientFundsForDistribution(f64),
    #[error("insufficient funds for distribution ({0} SOL) and fee ({1} SOL)")]
    InsufficientFundsForDistributionAndFees(f64, f64),
}

fn merge_allocations(allocations: &[Allocation]) -> Vec<Allocation> {
    let mut allocation_map = IndexMap::new();
    for allocation in allocations {
        allocation_map
            .entry(&allocation.recipient)
            .or_insert(Allocation {
                recipient: allocation.recipient.clone(),
                amount: 0.0,
                lockup_date: "".to_string(),
            })
            .amount += allocation.amount;
    }
    allocation_map.values().cloned().collect()
}

/// Return true if the recipient and lockups are the same
fn has_same_recipient(allocation: &Allocation, transaction_info: &TransactionInfo) -> bool {
    allocation.recipient == transaction_info.recipient.to_string()
        && allocation.lockup_date.parse().ok() == transaction_info.lockup_date
}

fn apply_previous_transactions(
    allocations: &mut Vec<Allocation>,
    transaction_infos: &[TransactionInfo],
) {
    for transaction_info in transaction_infos {
        let mut amount = transaction_info.amount;
        for allocation in allocations.iter_mut() {
            if !has_same_recipient(&allocation, &transaction_info) {
                continue;
            }
            if allocation.amount >= amount {
                allocation.amount -= amount;
                break;
            } else {
                amount -= allocation.amount;
                allocation.amount = 0.0;
            }
        }
    }
    allocations.retain(|x| x.amount > 0.5);
}

fn distribution_instructions(
    allocation: &Allocation,
    new_stake_account_address: &Pubkey,
    args: &DistributeTokensArgs,
    lockup_date: Option<DateTime<Utc>>,
) -> Vec<Instruction> {
    if args.stake_args.is_none() {
        let from = args.sender_keypair.pubkey();
        let to = allocation.recipient.parse().unwrap();
        let lamports = sol_to_lamports(allocation.amount);
        let instruction = system_instruction::transfer(&from, &to, lamports);
        return vec![instruction];
    }

    let stake_args = args.stake_args.as_ref().unwrap();
    let sol_for_fees = stake_args.sol_for_fees;
    let sender_pubkey = args.sender_keypair.pubkey();
    let stake_authority = stake_args.stake_authority.pubkey();
    let withdraw_authority = stake_args.withdraw_authority.pubkey();

    let mut instructions = stake_instruction::split(
        &stake_args.stake_account_address,
        &stake_authority,
        sol_to_lamports(allocation.amount - sol_for_fees),
        &new_stake_account_address,
    );

    let recipient = allocation.recipient.parse().unwrap();

    // Make the recipient the new stake authority
    instructions.push(stake_instruction::authorize(
        &new_stake_account_address,
        &stake_authority,
        &recipient,
        StakeAuthorize::Staker,
    ));

    // Make the recipient the new withdraw authority
    instructions.push(stake_instruction::authorize(
        &new_stake_account_address,
        &withdraw_authority,
        &recipient,
        StakeAuthorize::Withdrawer,
    ));

    // Add lockup
    if let Some(lockup_date) = lockup_date {
        let lockup_authority = stake_args
            .lockup_authority
            .as_ref()
            .map(|signer| signer.pubkey())
            .unwrap();
        let lockup = LockupArgs {
            unix_timestamp: Some(lockup_date.timestamp()),
            epoch: None,
            custodian: None,
        };
        instructions.push(stake_instruction::set_lockup(
            &new_stake_account_address,
            &lockup,
            &lockup_authority,
        ));
    }

    instructions.push(system_instruction::transfer(
        &sender_pubkey,
        &recipient,
        sol_to_lamports(sol_for_fees),
    ));

    instructions
}

fn distribute_allocations(
    client: &ThinClient,
    db: &mut PickleDb,
    allocations: &[Allocation],
    args: &DistributeTokensArgs,
) -> Result<(), Error> {
    let mut num_signatures = 0;
    for allocation in allocations {
        let new_stake_account_keypair = Keypair::new();
        let new_stake_account_address = new_stake_account_keypair.pubkey();

        let mut signers = vec![&*args.fee_payer, &*args.sender_keypair];
        if let Some(stake_args) = &args.stake_args {
            signers.push(&*stake_args.stake_authority);
            signers.push(&*stake_args.withdraw_authority);
            signers.push(&new_stake_account_keypair);
            if allocation.lockup_date != "" {
                if let Some(lockup_authority) = &stake_args.lockup_authority {
                    signers.push(&**lockup_authority);
                } else {
                    return Err(Error::MissingLockupAuthority);
                }
            }
        }
        let signers = unique_signers(signers);
        num_signatures += signers.len();

        let lockup_date = if allocation.lockup_date == "" {
            None
        } else {
            Some(allocation.lockup_date.parse::<DateTime<Utc>>().unwrap())
        };

        println!("{:<44}  {:>24.9}", allocation.recipient, allocation.amount);
        let instructions =
            distribution_instructions(allocation, &new_stake_account_address, args, lockup_date);
        let fee_payer_pubkey = args.fee_payer.pubkey();
        let message = Message::new(&instructions, Some(&fee_payer_pubkey));
        match client.send_and_confirm_message(message, &signers) {
            Ok((transaction, last_valid_slot)) => {
                db::set_transaction_info(
                    db,
                    &allocation.recipient.parse().unwrap(),
                    allocation.amount,
                    &transaction,
                    args.stake_args.as_ref().map(|_| &new_stake_account_address),
                    false,
                    last_valid_slot,
                    lockup_date,
                )?;
            }
            Err(e) => {
                eprintln!("Error sending tokens to {}: {}", allocation.recipient, e);
            }
        };
    }
    if args.dry_run {
        let undistributed_tokens: f64 = allocations.iter().map(|x| x.amount).sum();
        check_payer_balances(
            num_signatures,
            sol_to_lamports(undistributed_tokens),
            client,
            args,
        )?;
    }
    Ok(())
}

fn read_allocations(input_csv: &str, transfer_amount: Option<f64>) -> io::Result<Vec<Allocation>> {
    let mut rdr = ReaderBuilder::new().trim(Trim::All).from_path(input_csv)?;
    let allocations = if let Some(amount) = transfer_amount {
        let recipients: Vec<String> = rdr
            .deserialize()
            .map(|recipient| recipient.unwrap())
            .collect();
        recipients
            .into_iter()
            .map(|recipient| Allocation {
                recipient,
                amount,
                lockup_date: "".to_string(),
            })
            .collect()
    } else {
        rdr.deserialize().map(|entry| entry.unwrap()).collect()
    };
    Ok(allocations)
}

fn new_spinner_progress_bar() -> ProgressBar {
    let progress_bar = ProgressBar::new(42);
    progress_bar
        .set_style(ProgressStyle::default_spinner().template("{spinner:.green} {wide_msg}"));
    progress_bar.enable_steady_tick(100);
    progress_bar
}

pub fn process_allocations(
    client: &ThinClient,
    args: &DistributeTokensArgs,
) -> Result<Option<usize>, Error> {
    let mut allocations: Vec<Allocation> = read_allocations(&args.input_csv, args.transfer_amount)?;

    let starting_total_tokens: f64 = allocations.iter().map(|x| x.amount).sum();
    println!(
        "{} ◎{}",
        style("Total in input_csv:").bold(),
        starting_total_tokens,
    );

    let mut db = db::open_db(&args.transaction_db, args.dry_run)?;

    // Start by finalizing any transactions from the previous run.
    let confirmations = finalize_transactions(client, &mut db, args.dry_run)?;

    let transaction_infos = db::read_transaction_infos(&db);
    apply_previous_transactions(&mut allocations, &transaction_infos);

    if allocations.is_empty() {
        eprintln!("No work to do");
        return Ok(confirmations);
    }

    let distributed_tokens: f64 = transaction_infos.iter().map(|x| x.amount).sum();
    let undistributed_tokens: f64 = allocations.iter().map(|x| x.amount).sum();
    println!("{} ◎{}", style("Distributed:").bold(), distributed_tokens,);
    println!(
        "{} ◎{}",
        style("Undistributed:").bold(),
        undistributed_tokens,
    );
    println!(
        "{} ◎{}",
        style("Total:").bold(),
        distributed_tokens + undistributed_tokens,
    );

    println!(
        "{}",
        style(format!(
            "{:<44}  {:>24}",
            "Recipient", "Expected Balance (◎)"
        ))
        .bold()
    );

    distribute_allocations(client, &mut db, &allocations, args)?;

    let opt_confirmations = finalize_transactions(client, &mut db, args.dry_run)?;

    if !args.dry_run {
        if let Some(output_path) = &args.output_path {
            db::write_transaction_log(&db, &output_path)?;
        }
    }
    Ok(opt_confirmations)
}

fn finalize_transactions(
    client: &ThinClient,
    db: &mut PickleDb,
    dry_run: bool,
) -> Result<Option<usize>, Error> {
    if dry_run {
        return Ok(None);
    }

    let mut opt_confirmations = update_finalized_transactions(client, db)?;

    let progress_bar = new_spinner_progress_bar();

    while opt_confirmations.is_some() {
        if let Some(confirmations) = opt_confirmations {
            progress_bar.set_message(&format!(
                "[{}/{}] Finalizing transactions",
                confirmations, 32,
            ));
        }

        // Sleep for about 1 slot
        sleep(Duration::from_millis(500));
        let opt_conf = update_finalized_transactions(client, db)?;
        opt_confirmations = opt_conf;
    }

    Ok(opt_confirmations)
}

// Update the finalized bit on any transactions that are now rooted
// Return the lowest number of confirmations on the unfinalized transactions or None if all are finalized.
fn update_finalized_transactions(
    client: &ThinClient,
    db: &mut PickleDb,
) -> Result<Option<usize>, Error> {
    let transaction_infos = db::read_transaction_infos(db);
    let unconfirmed_transactions: Vec<_> = transaction_infos
        .iter()
        .filter_map(|info| {
            if info.finalized_date.is_some() {
                None
            } else {
                Some((&info.transaction, info.last_valid_slot))
            }
        })
        .collect();
    let unconfirmed_signatures: Vec<_> = unconfirmed_transactions
        .iter()
        .map(|(tx, _slot)| tx.signatures[0])
        .filter(|sig| *sig != Signature::default()) // Filter out dry-run signatures
        .collect();
    let transaction_statuses = client.get_signature_statuses(&unconfirmed_signatures)?;
    let root_slot = client.get_slot()?;

    let mut confirmations = None;
    for ((transaction, last_valid_slot), opt_transaction_status) in unconfirmed_transactions
        .into_iter()
        .zip(transaction_statuses.into_iter())
    {
        match db::update_finalized_transaction(
            db,
            &transaction.signatures[0],
            opt_transaction_status,
            last_valid_slot,
            root_slot,
        ) {
            Ok(Some(confs)) => {
                confirmations = Some(cmp::min(confs, confirmations.unwrap_or(usize::MAX)));
            }
            result => {
                result?;
            }
        }
    }
    Ok(confirmations)
}

fn check_payer_balances(
    num_signatures: usize,
    allocation_lamports: u64,
    client: &ThinClient,
    args: &DistributeTokensArgs,
) -> Result<(), Error> {
    let (_blockhash, fee_calculator, _last_valid_slot) = client.get_fees()?;
    let fees = fee_calculator
        .lamports_per_signature
        .checked_mul(num_signatures as u64)
        .unwrap();
    if args.fee_payer.pubkey() == args.sender_keypair.pubkey() {
        let balance = client.get_balance(&args.fee_payer.pubkey())?;
        if balance < fees + allocation_lamports {
            return Err(Error::InsufficientFundsForDistributionAndFees(
                lamports_to_sol(allocation_lamports),
                lamports_to_sol(fees),
            ));
        }
    } else {
        let fee_payer_balance = client.get_balance(&args.fee_payer.pubkey())?;
        if fee_payer_balance < fees {
            return Err(Error::InsufficientFundsForFees(lamports_to_sol(fees)));
        }
        let sender_balance = client.get_balance(&args.sender_keypair.pubkey())?;
        if sender_balance < allocation_lamports {
            return Err(Error::InsufficientFundsForDistribution(lamports_to_sol(
                allocation_lamports,
            )));
        }
    }
    Ok(())
}

<<<<<<< HEAD
pub fn process_balances(client: &ThinClient, args: &BalancesArgs) -> Result<(), csv::Error> {
    let allocations: Vec<Allocation> = read_allocations(&args.input_csv)?;
=======
pub async fn process_balances(
    client: &mut BanksClient,
    args: &BalancesArgs,
) -> Result<(), csv::Error> {
    let allocations: Vec<Allocation> = read_allocations(&args.input_csv, None)?;
>>>>>>> a48cc073c... solana-tokens: Add capability to perform the same transfer to a batch of recipients (#12259)
    let allocations = merge_allocations(&allocations);

    println!(
        "{}",
        style(format!(
            "{:<44}  {:>24}  {:>24}  {:>24}",
            "Recipient", "Expected Balance (◎)", "Actual Balance (◎)", "Difference (◎)"
        ))
        .bold()
    );

    for allocation in &allocations {
        let address = allocation.recipient.parse().unwrap();
        let expected = lamports_to_sol(sol_to_lamports(allocation.amount));
        let actual = lamports_to_sol(client.get_balance(&address).unwrap());
        println!(
            "{:<44}  {:>24.9}  {:>24.9}  {:>24.9}",
            allocation.recipient,
            expected,
            actual,
            actual - expected
        );
    }

    Ok(())
}

pub fn process_transaction_log(args: &TransactionLogArgs) -> Result<(), Error> {
    let db = db::open_db(&args.transaction_db, true)?;
    db::write_transaction_log(&db, &args.output_path)?;
    Ok(())
}

use crate::db::check_output_file;
use solana_sdk::{pubkey::Pubkey, signature::Keypair};
use tempfile::{tempdir, NamedTempFile};
<<<<<<< HEAD
pub fn test_process_distribute_tokens_with_client<C: Client>(client: C, sender_keypair: Keypair) {
    let thin_client = ThinClient::new(client, false);
=======
pub async fn test_process_distribute_tokens_with_client(
    client: &mut BanksClient,
    sender_keypair: Keypair,
    transfer_amount: Option<f64>,
) {
>>>>>>> a48cc073c... solana-tokens: Add capability to perform the same transfer to a batch of recipients (#12259)
    let fee_payer = Keypair::new();
    let (transaction, _last_valid_slot) = thin_client
        .transfer(sol_to_lamports(1.0), &sender_keypair, &fee_payer.pubkey())
        .unwrap();
    thin_client
        .poll_for_confirmation(&transaction.signatures[0])
        .unwrap();

    let alice_pubkey = Pubkey::new_rand();
    let allocation = Allocation {
        recipient: alice_pubkey.to_string(),
        amount: if let Some(amount) = transfer_amount {
            amount
        } else {
            1000.0
        },
        lockup_date: "".to_string(),
    };
    let allocations_file = NamedTempFile::new().unwrap();
    let input_csv = allocations_file.path().to_str().unwrap().to_string();
    let mut wtr = csv::WriterBuilder::new().from_writer(allocations_file);
    wtr.serialize(&allocation).unwrap();
    wtr.flush().unwrap();

    let dir = tempdir().unwrap();
    let transaction_db = dir
        .path()
        .join("transactions.db")
        .to_str()
        .unwrap()
        .to_string();

    let output_file = NamedTempFile::new().unwrap();
    let output_path = output_file.path().to_str().unwrap().to_string();

    let args = DistributeTokensArgs {
        sender_keypair: Box::new(sender_keypair),
        fee_payer: Box::new(fee_payer),
        dry_run: false,
        input_csv,
        transaction_db: transaction_db.clone(),
        output_path: Some(output_path.clone()),
        stake_args: None,
        transfer_amount,
    };
    let confirmations = process_allocations(&thin_client, &args).unwrap();
    assert_eq!(confirmations, None);

    let transaction_infos =
        db::read_transaction_infos(&db::open_db(&transaction_db, true).unwrap());
    assert_eq!(transaction_infos.len(), 1);
    assert_eq!(transaction_infos[0].recipient, alice_pubkey);
    let expected_amount = sol_to_lamports(allocation.amount);
    assert_eq!(
        sol_to_lamports(transaction_infos[0].amount),
        expected_amount
    );

    assert_eq!(
        thin_client.get_balance(&alice_pubkey).unwrap(),
        expected_amount,
    );

    check_output_file(&output_path, &db::open_db(&transaction_db, true).unwrap());

    // Now, run it again, and check there's no double-spend.
    process_allocations(&thin_client, &args).unwrap();
    let transaction_infos =
        db::read_transaction_infos(&db::open_db(&transaction_db, true).unwrap());
    assert_eq!(transaction_infos.len(), 1);
    assert_eq!(transaction_infos[0].recipient, alice_pubkey);
    let expected_amount = sol_to_lamports(allocation.amount);
    assert_eq!(
        sol_to_lamports(transaction_infos[0].amount),
        expected_amount
    );

    assert_eq!(
        thin_client.get_balance(&alice_pubkey).unwrap(),
        expected_amount,
    );

    check_output_file(&output_path, &db::open_db(&transaction_db, true).unwrap());
}

pub fn test_process_distribute_stake_with_client<C: Client>(client: C, sender_keypair: Keypair) {
    let thin_client = ThinClient::new(client, false);
    let fee_payer = Keypair::new();
    let (transaction, _last_valid_slot) = thin_client
        .transfer(sol_to_lamports(1.0), &sender_keypair, &fee_payer.pubkey())
        .unwrap();
    thin_client
        .poll_for_confirmation(&transaction.signatures[0])
        .unwrap();

    let stake_account_keypair = Keypair::new();
    let stake_account_address = stake_account_keypair.pubkey();
    let stake_authority = Keypair::new();
    let withdraw_authority = Keypair::new();

    let authorized = Authorized {
        staker: stake_authority.pubkey(),
        withdrawer: withdraw_authority.pubkey(),
    };
    let lockup = Lockup::default();
    let instructions = stake_instruction::create_account(
        &sender_keypair.pubkey(),
        &stake_account_address,
        &authorized,
        &lockup,
        sol_to_lamports(3000.0),
    );
    let message = Message::new(&instructions, Some(&sender_keypair.pubkey()));
    let signers = [&sender_keypair, &stake_account_keypair];
    thin_client
        .send_and_confirm_message(message, &signers)
        .unwrap();

    let alice_pubkey = Pubkey::new_rand();
    let allocation = Allocation {
        recipient: alice_pubkey.to_string(),
        amount: 1000.0,
        lockup_date: "".to_string(),
    };
    let file = NamedTempFile::new().unwrap();
    let input_csv = file.path().to_str().unwrap().to_string();
    let mut wtr = csv::WriterBuilder::new().from_writer(file);
    wtr.serialize(&allocation).unwrap();
    wtr.flush().unwrap();

    let dir = tempdir().unwrap();
    let transaction_db = dir
        .path()
        .join("transactions.db")
        .to_str()
        .unwrap()
        .to_string();

    let output_file = NamedTempFile::new().unwrap();
    let output_path = output_file.path().to_str().unwrap().to_string();

    let stake_args = StakeArgs {
        stake_account_address,
        stake_authority: Box::new(stake_authority),
        withdraw_authority: Box::new(withdraw_authority),
        lockup_authority: None,
        sol_for_fees: 1.0,
    };
    let args = DistributeTokensArgs {
        fee_payer: Box::new(fee_payer),
        dry_run: false,
        input_csv,
        transaction_db: transaction_db.clone(),
        output_path: Some(output_path.clone()),
        stake_args: Some(stake_args),
        sender_keypair: Box::new(sender_keypair),
        transfer_amount: None,
    };
    let confirmations = process_allocations(&thin_client, &args).unwrap();
    assert_eq!(confirmations, None);

    let transaction_infos =
        db::read_transaction_infos(&db::open_db(&transaction_db, true).unwrap());
    assert_eq!(transaction_infos.len(), 1);
    assert_eq!(transaction_infos[0].recipient, alice_pubkey);
    let expected_amount = sol_to_lamports(allocation.amount);
    assert_eq!(
        sol_to_lamports(transaction_infos[0].amount),
        expected_amount
    );

    assert_eq!(
        thin_client.get_balance(&alice_pubkey).unwrap(),
        sol_to_lamports(1.0),
    );
    let new_stake_account_address = transaction_infos[0].new_stake_account_address.unwrap();
    assert_eq!(
        thin_client.get_balance(&new_stake_account_address).unwrap(),
        expected_amount - sol_to_lamports(1.0),
    );

    check_output_file(&output_path, &db::open_db(&transaction_db, true).unwrap());

    // Now, run it again, and check there's no double-spend.
    process_allocations(&thin_client, &args).unwrap();
    let transaction_infos =
        db::read_transaction_infos(&db::open_db(&transaction_db, true).unwrap());
    assert_eq!(transaction_infos.len(), 1);
    assert_eq!(transaction_infos[0].recipient, alice_pubkey);
    let expected_amount = sol_to_lamports(allocation.amount);
    assert_eq!(
        sol_to_lamports(transaction_infos[0].amount),
        expected_amount
    );

    assert_eq!(
        thin_client.get_balance(&alice_pubkey).unwrap(),
        sol_to_lamports(1.0),
    );
    assert_eq!(
        thin_client.get_balance(&new_stake_account_address).unwrap(),
        expected_amount - sol_to_lamports(1.0),
    );

    check_output_file(&output_path, &db::open_db(&transaction_db, true).unwrap());
}

#[cfg(test)]
mod tests {
    use super::*;
    use solana_runtime::{bank::Bank, bank_client::BankClient};
    use solana_sdk::genesis_config::create_genesis_config;
    use solana_stake_program::stake_instruction::StakeInstruction;

    #[test]
    fn test_process_token_allocations() {
        let (genesis_config, sender_keypair) = create_genesis_config(sol_to_lamports(9_000_000.0));
<<<<<<< HEAD
        let bank = Bank::new(&genesis_config);
        let bank_client = BankClient::new(bank);
        test_process_distribute_tokens_with_client(bank_client, sender_keypair);
=======
        let bank_forks = Arc::new(RwLock::new(BankForks::new(Bank::new(&genesis_config))));
        Runtime::new().unwrap().block_on(async {
            let transport = start_local_server(&bank_forks).await;
            let mut banks_client = start_client(transport).await.unwrap();
            test_process_distribute_tokens_with_client(&mut banks_client, sender_keypair, None)
                .await;
        });
    }

    #[test]
    fn test_process_transfer_amount_allocations() {
        let (genesis_config, sender_keypair) = create_genesis_config(sol_to_lamports(9_000_000.0));
        let bank_forks = Arc::new(RwLock::new(BankForks::new(Bank::new(&genesis_config))));
        Runtime::new().unwrap().block_on(async {
            let transport = start_local_server(&bank_forks).await;
            let mut banks_client = start_client(transport).await.unwrap();
            test_process_distribute_tokens_with_client(
                &mut banks_client,
                sender_keypair,
                Some(1.5),
            )
            .await;
        });
>>>>>>> a48cc073c... solana-tokens: Add capability to perform the same transfer to a batch of recipients (#12259)
    }

    #[test]
    fn test_process_stake_allocations() {
        let (genesis_config, sender_keypair) = create_genesis_config(sol_to_lamports(9_000_000.0));
        let bank = Bank::new(&genesis_config);
        let bank_client = BankClient::new(bank);
        test_process_distribute_stake_with_client(bank_client, sender_keypair);
    }

    #[test]
    fn test_read_allocations() {
        let alice_pubkey = Pubkey::new_rand();
        let allocation = Allocation {
            recipient: alice_pubkey.to_string(),
            amount: 42.0,
            lockup_date: "".to_string(),
        };
        let file = NamedTempFile::new().unwrap();
        let input_csv = file.path().to_str().unwrap().to_string();
        let mut wtr = csv::WriterBuilder::new().from_writer(file);
        wtr.serialize(&allocation).unwrap();
        wtr.flush().unwrap();

        assert_eq!(
            read_allocations(&input_csv, None).unwrap(),
            vec![allocation]
        );
    }

    #[test]
    fn test_read_allocations_transfer_amount() {
        let pubkey0 = Pubkey::new_rand();
        let pubkey1 = Pubkey::new_rand();
        let pubkey2 = Pubkey::new_rand();
        let file = NamedTempFile::new().unwrap();
        let input_csv = file.path().to_str().unwrap().to_string();
        let mut wtr = csv::WriterBuilder::new().from_writer(file);
        wtr.serialize("recipient".to_string()).unwrap();
        wtr.serialize(&pubkey0.to_string()).unwrap();
        wtr.serialize(&pubkey1.to_string()).unwrap();
        wtr.serialize(&pubkey2.to_string()).unwrap();
        wtr.flush().unwrap();

        let amount = 1.5;

        let expected_allocations = vec![
            Allocation {
                recipient: pubkey0.to_string(),
                amount,
                lockup_date: "".to_string(),
            },
            Allocation {
                recipient: pubkey1.to_string(),
                amount,
                lockup_date: "".to_string(),
            },
            Allocation {
                recipient: pubkey2.to_string(),
                amount,
                lockup_date: "".to_string(),
            },
        ];
        assert_eq!(
            read_allocations(&input_csv, Some(amount)).unwrap(),
            expected_allocations
        );
    }

    #[test]
    fn test_apply_previous_transactions() {
        let alice = Pubkey::new_rand();
        let bob = Pubkey::new_rand();
        let mut allocations = vec![
            Allocation {
                recipient: alice.to_string(),
                amount: 1.0,
                lockup_date: "".to_string(),
            },
            Allocation {
                recipient: bob.to_string(),
                amount: 1.0,
                lockup_date: "".to_string(),
            },
        ];
        let transaction_infos = vec![TransactionInfo {
            recipient: bob,
            amount: 1.0,
            ..TransactionInfo::default()
        }];
        apply_previous_transactions(&mut allocations, &transaction_infos);
        assert_eq!(allocations.len(), 1);

        // Ensure that we applied the transaction to the allocation with
        // a matching recipient address (to bob, not alice).
        assert_eq!(allocations[0].recipient, alice.to_string());
    }

    #[test]
    fn test_has_same_recipient() {
        let alice_pubkey = Pubkey::new_rand();
        let bob_pubkey = Pubkey::new_rand();
        let lockup0 = "2021-01-07T00:00:00Z".to_string();
        let lockup1 = "9999-12-31T23:59:59Z".to_string();
        let alice_alloc = Allocation {
            recipient: alice_pubkey.to_string(),
            amount: 1.0,
            lockup_date: "".to_string(),
        };
        let alice_alloc_lockup0 = Allocation {
            recipient: alice_pubkey.to_string(),
            amount: 1.0,
            lockup_date: lockup0.clone(),
        };
        let alice_info = TransactionInfo {
            recipient: alice_pubkey,
            lockup_date: None,
            ..TransactionInfo::default()
        };
        let alice_info_lockup0 = TransactionInfo {
            recipient: alice_pubkey,
            lockup_date: lockup0.parse().ok(),
            ..TransactionInfo::default()
        };
        let alice_info_lockup1 = TransactionInfo {
            recipient: alice_pubkey,
            lockup_date: lockup1.parse().ok(),
            ..TransactionInfo::default()
        };
        let bob_info = TransactionInfo {
            recipient: bob_pubkey,
            lockup_date: None,
            ..TransactionInfo::default()
        };
        assert!(!has_same_recipient(&alice_alloc, &bob_info)); // Different recipient, no lockup
        assert!(!has_same_recipient(&alice_alloc, &alice_info_lockup0)); // One with no lockup, one locked up
        assert!(!has_same_recipient(
            &alice_alloc_lockup0,
            &alice_info_lockup1
        )); // Different lockups
        assert!(has_same_recipient(&alice_alloc, &alice_info)); // Same recipient, no lockups
        assert!(has_same_recipient(
            &alice_alloc_lockup0,
            &alice_info_lockup0
        )); // Same recipient, same lockups
    }

    const SET_LOCKUP_INDEX: usize = 4;

    #[test]
    fn test_set_stake_lockup() {
        let lockup_date_str = "2021-01-07T00:00:00Z";
        let allocation = Allocation {
            recipient: Pubkey::default().to_string(),
            amount: 1.0,
            lockup_date: lockup_date_str.to_string(),
        };
        let stake_account_address = Pubkey::new_rand();
        let new_stake_account_address = Pubkey::new_rand();
        let lockup_authority = Keypair::new();
        let stake_args = StakeArgs {
            stake_account_address,
            stake_authority: Box::new(Keypair::new()),
            withdraw_authority: Box::new(Keypair::new()),
            lockup_authority: Some(Box::new(lockup_authority)),
            sol_for_fees: 1.0,
        };
        let args = DistributeTokensArgs {
            fee_payer: Box::new(Keypair::new()),
            dry_run: false,
            input_csv: "".to_string(),
            transaction_db: "".to_string(),
            output_path: None,
            stake_args: Some(stake_args),
            sender_keypair: Box::new(Keypair::new()),
            transfer_amount: None,
        };
        let lockup_date = lockup_date_str.parse().unwrap();
        let instructions = distribution_instructions(
            &allocation,
            &new_stake_account_address,
            &args,
            Some(lockup_date),
        );
        let lockup_instruction =
            bincode::deserialize(&instructions[SET_LOCKUP_INDEX].data).unwrap();
        if let StakeInstruction::SetLockup(lockup_args) = lockup_instruction {
            assert_eq!(lockup_args.unix_timestamp, Some(lockup_date.timestamp()));
            assert_eq!(lockup_args.epoch, None); // Don't change the epoch
            assert_eq!(lockup_args.custodian, None); // Don't change the lockup authority
        } else {
            panic!("expected SetLockup instruction");
        }
    }
}
