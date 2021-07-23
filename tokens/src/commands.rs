use crate::{
    args::{BalancesArgs, DistributeTokensArgs, SenderStakeArgs, StakeArgs, TransactionLogArgs},
    db::{self, TransactionInfo},
    spl_token::*,
    token_display::Token,
};
use chrono::prelude::*;
use console::style;
use csv::{ReaderBuilder, Trim};
use indexmap::IndexMap;
use indicatif::{ProgressBar, ProgressStyle};
use pickledb::PickleDb;
use serde::{Deserialize, Serialize};
use solana_account_decoder::parse_token::{
    pubkey_from_spl_token_v2_0, real_number_string, spl_token_v2_0_pubkey,
};
use solana_client::{
    client_error::{ClientError, Result as ClientResult},
    rpc_client::RpcClient,
    rpc_config::RpcSendTransactionConfig,
    rpc_request::MAX_GET_SIGNATURE_STATUSES_QUERY_ITEMS,
};
use solana_sdk::{
    clock::Slot,
    commitment_config::CommitmentConfig,
    instruction::Instruction,
    message::Message,
    native_token::{lamports_to_sol, sol_to_lamports},
    signature::{unique_signers, Signature, Signer},
    stake::{
        instruction::{self as stake_instruction, LockupArgs},
        state::{Authorized, Lockup, StakeAuthorize},
    },
    system_instruction,
    transaction::Transaction,
};
use solana_transaction_status::TransactionStatus;
use spl_associated_token_account_v1_0::get_associated_token_address;
use spl_token_v2_0::solana_program::program_error::ProgramError;
use std::{
    cmp::{self},
    io,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread::sleep,
    time::Duration,
};

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct Allocation {
    pub recipient: String,
    pub amount: u64,
    pub lockup_date: String,
}

#[derive(Debug, PartialEq)]
pub enum FundingSource {
    FeePayer,
    SplTokenAccount,
    StakeAccount,
    SystemAccount,
}

pub struct FundingSources(Vec<FundingSource>);

impl std::fmt::Debug for FundingSources {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for (i, source) in self.0.iter().enumerate() {
            if i > 0 {
                write!(f, "/")?;
            }
            write!(f, "{:?}", source)?;
        }
        Ok(())
    }
}

impl PartialEq for FundingSources {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

impl From<Vec<FundingSource>> for FundingSources {
    fn from(sources_vec: Vec<FundingSource>) -> Self {
        Self(sources_vec)
    }
}

type StakeExtras = Vec<(Keypair, Option<DateTime<Utc>>)>;

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("I/O error")]
    IoError(#[from] io::Error),
    #[error("CSV error")]
    CsvError(#[from] csv::Error),
    #[error("PickleDb error")]
    PickleDbError(#[from] pickledb::error::Error),
    #[error("Transport error")]
    ClientError(#[from] ClientError),
    #[error("Missing lockup authority")]
    MissingLockupAuthority,
    #[error("insufficient funds in {0:?}, requires {1}")]
    InsufficientFunds(FundingSources, String),
    #[error("Program error")]
    ProgramError(#[from] ProgramError),
    #[error("Exit signal received")]
    ExitSignal,
}

fn merge_allocations(allocations: &[Allocation]) -> Vec<Allocation> {
    let mut allocation_map = IndexMap::new();
    for allocation in allocations {
        allocation_map
            .entry(&allocation.recipient)
            .or_insert(Allocation {
                recipient: allocation.recipient.clone(),
                amount: 0,
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
            if !has_same_recipient(allocation, transaction_info) {
                continue;
            }
            if allocation.amount >= amount {
                allocation.amount -= amount;
                break;
            } else {
                amount -= allocation.amount;
                allocation.amount = 0;
            }
        }
    }
    allocations.retain(|x| x.amount > 0);
}

fn transfer<S: Signer>(
    client: &RpcClient,
    lamports: u64,
    sender_keypair: &S,
    to_pubkey: &Pubkey,
) -> ClientResult<Transaction> {
    let create_instruction =
        system_instruction::transfer(&sender_keypair.pubkey(), to_pubkey, lamports);
    let message = Message::new(&[create_instruction], Some(&sender_keypair.pubkey()));
    let (recent_blockhash, _fees) = client.get_recent_blockhash()?;
    Ok(Transaction::new(
        &[sender_keypair],
        message,
        recent_blockhash,
    ))
}

fn distribution_instructions(
    allocation: &Allocation,
    new_stake_account_address: &Pubkey,
    args: &DistributeTokensArgs,
    lockup_date: Option<DateTime<Utc>>,
    do_create_associated_token_account: bool,
) -> Vec<Instruction> {
    if args.spl_token_args.is_some() {
        return build_spl_token_instructions(allocation, args, do_create_associated_token_account);
    }

    match &args.stake_args {
        // No stake args; a simple token transfer.
        None => {
            let from = args.sender_keypair.pubkey();
            let to = allocation.recipient.parse().unwrap();
            let lamports = allocation.amount;
            let instruction = system_instruction::transfer(&from, &to, lamports);
            vec![instruction]
        }

        // Stake args provided, so create a recipient stake account.
        Some(stake_args) => {
            let unlocked_sol = stake_args.unlocked_sol;
            let sender_pubkey = args.sender_keypair.pubkey();
            let recipient = allocation.recipient.parse().unwrap();

            let mut instructions = match &stake_args.sender_stake_args {
                // No source stake account, so create a recipient stake account directly.
                None => {
                    // Make the recipient both the new stake and withdraw authority
                    let authorized = Authorized {
                        staker: recipient,
                        withdrawer: recipient,
                    };
                    let mut lockup = Lockup::default();
                    if let Some(lockup_date) = lockup_date {
                        lockup.unix_timestamp = lockup_date.timestamp();
                    }
                    if let Some(lockup_authority) = stake_args.lockup_authority {
                        lockup.custodian = lockup_authority;
                    }
                    stake_instruction::create_account(
                        &sender_pubkey,
                        new_stake_account_address,
                        &authorized,
                        &lockup,
                        allocation.amount - unlocked_sol,
                    )
                }

                // A sender stake account was provided, so create a recipient stake account by
                // splitting the sender account.
                Some(sender_stake_args) => {
                    let stake_authority = sender_stake_args.stake_authority.pubkey();
                    let withdraw_authority = sender_stake_args.withdraw_authority.pubkey();
                    let mut instructions = stake_instruction::split(
                        &sender_stake_args.stake_account_address,
                        &stake_authority,
                        allocation.amount - unlocked_sol,
                        new_stake_account_address,
                    );

                    // Make the recipient the new stake authority
                    instructions.push(stake_instruction::authorize(
                        new_stake_account_address,
                        &stake_authority,
                        &recipient,
                        StakeAuthorize::Staker,
                        None,
                    ));

                    // Make the recipient the new withdraw authority
                    instructions.push(stake_instruction::authorize(
                        new_stake_account_address,
                        &withdraw_authority,
                        &recipient,
                        StakeAuthorize::Withdrawer,
                        None,
                    ));

                    // Add lockup
                    if let Some(lockup_date) = lockup_date {
                        let lockup = LockupArgs {
                            unix_timestamp: Some(lockup_date.timestamp()),
                            epoch: None,
                            custodian: None,
                        };
                        instructions.push(stake_instruction::set_lockup(
                            new_stake_account_address,
                            &lockup,
                            &stake_args.lockup_authority.unwrap(),
                        ));
                    }

                    instructions
                }
            };

            // Transfer some unlocked tokens to recipient, which they can use for transaction fees.
            instructions.push(system_instruction::transfer(
                &sender_pubkey,
                &recipient,
                unlocked_sol,
            ));

            instructions
        }
    }
}

fn build_messages(
    client: &RpcClient,
    db: &mut PickleDb,
    allocations: &[Allocation],
    args: &DistributeTokensArgs,
    exit: Arc<AtomicBool>,
    messages: &mut Vec<Message>,
    stake_extras: &mut StakeExtras,
    created_accounts: &mut u64,
) -> Result<(), Error> {
    for allocation in allocations.iter() {
        if exit.load(Ordering::SeqCst) {
            db.dump()?;
            return Err(Error::ExitSignal);
        }
        let new_stake_account_keypair = Keypair::new();
        let lockup_date = if allocation.lockup_date.is_empty() {
            None
        } else {
            Some(allocation.lockup_date.parse::<DateTime<Utc>>().unwrap())
        };

        let do_create_associated_token_account = if let Some(spl_token_args) = &args.spl_token_args
        {
            let wallet_address = allocation.recipient.parse().unwrap();
            let associated_token_address = get_associated_token_address(
                &wallet_address,
                &spl_token_v2_0_pubkey(&spl_token_args.mint),
            );
            let do_create_associated_token_account = client
                .get_multiple_accounts(&[pubkey_from_spl_token_v2_0(&associated_token_address)])?
                [0]
            .is_none();
            if do_create_associated_token_account {
                *created_accounts += 1;
            }
            println!(
                "{:<44}  {:>24}",
                allocation.recipient,
                real_number_string(allocation.amount, spl_token_args.decimals)
            );
            do_create_associated_token_account
        } else {
            println!(
                "{:<44}  {:>24.9}",
                allocation.recipient,
                lamports_to_sol(allocation.amount)
            );
            false
        };
        let instructions = distribution_instructions(
            allocation,
            &new_stake_account_keypair.pubkey(),
            args,
            lockup_date,
            do_create_associated_token_account,
        );
        let fee_payer_pubkey = args.fee_payer.pubkey();
        let message = Message::new(&instructions, Some(&fee_payer_pubkey));
        messages.push(message);
        stake_extras.push((new_stake_account_keypair, lockup_date));
    }
    Ok(())
}

fn send_messages(
    client: &RpcClient,
    db: &mut PickleDb,
    allocations: &[Allocation],
    args: &DistributeTokensArgs,
    exit: Arc<AtomicBool>,
    messages: Vec<Message>,
    stake_extras: StakeExtras,
) -> Result<(), Error> {
    for ((allocation, message), (new_stake_account_keypair, lockup_date)) in
        allocations.iter().zip(messages).zip(stake_extras)
    {
        if exit.load(Ordering::SeqCst) {
            db.dump()?;
            return Err(Error::ExitSignal);
        }
        let new_stake_account_address = new_stake_account_keypair.pubkey();

        let mut signers = vec![&*args.fee_payer, &*args.sender_keypair];
        if let Some(stake_args) = &args.stake_args {
            signers.push(&new_stake_account_keypair);
            if let Some(sender_stake_args) = &stake_args.sender_stake_args {
                signers.push(&*sender_stake_args.stake_authority);
                signers.push(&*sender_stake_args.withdraw_authority);
                signers.push(&new_stake_account_keypair);
                if !allocation.lockup_date.is_empty() {
                    if let Some(lockup_authority) = &sender_stake_args.lockup_authority {
                        signers.push(&**lockup_authority);
                    } else {
                        return Err(Error::MissingLockupAuthority);
                    }
                }
            }
        }
        let signers = unique_signers(signers);
        let result: ClientResult<(Transaction, u64)> = {
            if args.dry_run {
                Ok((Transaction::new_unsigned(message), std::u64::MAX))
            } else {
                let (blockhash, _fee_calculator, last_valid_slot) = client
                    .get_recent_blockhash_with_commitment(CommitmentConfig::default())?
                    .value;
                let transaction = Transaction::new(&signers, message, blockhash);
                let config = RpcSendTransactionConfig {
                    skip_preflight: true,
                    ..RpcSendTransactionConfig::default()
                };
                client.send_transaction_with_config(&transaction, config)?;
                Ok((transaction, last_valid_slot))
            }
        };
        match result {
            Ok((transaction, last_valid_slot)) => {
                let new_stake_account_address_option =
                    args.stake_args.as_ref().map(|_| &new_stake_account_address);
                db::set_transaction_info(
                    db,
                    &allocation.recipient.parse().unwrap(),
                    allocation.amount,
                    &transaction,
                    new_stake_account_address_option,
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
    Ok(())
}

fn distribute_allocations(
    client: &RpcClient,
    db: &mut PickleDb,
    allocations: &[Allocation],
    args: &DistributeTokensArgs,
    exit: Arc<AtomicBool>,
) -> Result<(), Error> {
    let mut messages: Vec<Message> = vec![];
    let mut stake_extras: StakeExtras = vec![];
    let mut created_accounts = 0;

    build_messages(
        client,
        db,
        allocations,
        args,
        exit.clone(),
        &mut messages,
        &mut stake_extras,
        &mut created_accounts,
    )?;

    let num_signatures = messages
        .iter()
        .map(|message| message.header.num_required_signatures as usize)
        .sum();
    if args.spl_token_args.is_some() {
        check_spl_token_balances(num_signatures, allocations, client, args, created_accounts)?;
    } else {
        check_payer_balances(num_signatures, allocations, client, args)?;
    }

    send_messages(client, db, allocations, args, exit, messages, stake_extras)?;

    db.dump()?;
    Ok(())
}

#[allow(clippy::needless_collect)]
fn read_allocations(
    input_csv: &str,
    transfer_amount: Option<u64>,
    require_lockup_heading: bool,
    raw_amount: bool,
) -> io::Result<Vec<Allocation>> {
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
    } else if require_lockup_heading {
        let recipients: Vec<(String, f64, String)> = rdr
            .deserialize()
            .map(|recipient| recipient.unwrap())
            .collect();
        recipients
            .into_iter()
            .map(|(recipient, amount, lockup_date)| Allocation {
                recipient,
                amount: sol_to_lamports(amount),
                lockup_date,
            })
            .collect()
    } else if raw_amount {
        let recipients: Vec<(String, u64)> = rdr
            .deserialize()
            .map(|recipient| recipient.unwrap())
            .collect();
        recipients
            .into_iter()
            .map(|(recipient, amount)| Allocation {
                recipient,
                amount,
                lockup_date: "".to_string(),
            })
            .collect()
    } else {
        let recipients: Vec<(String, f64)> = rdr
            .deserialize()
            .map(|recipient| recipient.unwrap())
            .collect();
        recipients
            .into_iter()
            .map(|(recipient, amount)| Allocation {
                recipient,
                amount: sol_to_lamports(amount),
                lockup_date: "".to_string(),
            })
            .collect()
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
    client: &RpcClient,
    args: &DistributeTokensArgs,
    exit: Arc<AtomicBool>,
) -> Result<Option<usize>, Error> {
    let require_lockup_heading = args.stake_args.is_some();
    let mut allocations: Vec<Allocation> = read_allocations(
        &args.input_csv,
        args.transfer_amount,
        require_lockup_heading,
        args.spl_token_args.is_some(),
    )?;

    let starting_total_tokens = allocations.iter().map(|x| x.amount).sum();
    let starting_total_tokens = if let Some(spl_token_args) = &args.spl_token_args {
        Token::spl_token(starting_total_tokens, spl_token_args.decimals)
    } else {
        Token::sol(starting_total_tokens)
    };
    println!(
        "{} {}",
        style("Total in input_csv:").bold(),
        starting_total_tokens,
    );

    let mut db = db::open_db(&args.transaction_db, args.dry_run)?;

    // Start by finalizing any transactions from the previous run.
    let confirmations = finalize_transactions(client, &mut db, args.dry_run, exit.clone())?;

    let transaction_infos = db::read_transaction_infos(&db);
    apply_previous_transactions(&mut allocations, &transaction_infos);

    if allocations.is_empty() {
        eprintln!("No work to do");
        return Ok(confirmations);
    }

    let distributed_tokens = transaction_infos.iter().map(|x| x.amount).sum();
    let undistributed_tokens = allocations.iter().map(|x| x.amount).sum();
    let (distributed_tokens, undistributed_tokens) =
        if let Some(spl_token_args) = &args.spl_token_args {
            (
                Token::spl_token(distributed_tokens, spl_token_args.decimals),
                Token::spl_token(undistributed_tokens, spl_token_args.decimals),
            )
        } else {
            (
                Token::sol(distributed_tokens),
                Token::sol(undistributed_tokens),
            )
        };
    println!("{} {}", style("Distributed:").bold(), distributed_tokens,);
    println!(
        "{} {}",
        style("Undistributed:").bold(),
        undistributed_tokens,
    );
    println!(
        "{} {}",
        style("Total:").bold(),
        distributed_tokens + undistributed_tokens,
    );

    println!(
        "{}",
        style(format!("{:<44}  {:>24}", "Recipient", "Expected Balance",)).bold()
    );

    distribute_allocations(client, &mut db, &allocations, args, exit.clone())?;

    let opt_confirmations = finalize_transactions(client, &mut db, args.dry_run, exit)?;

    if !args.dry_run {
        if let Some(output_path) = &args.output_path {
            db::write_transaction_log(&db, &output_path)?;
        }
    }

    Ok(opt_confirmations)
}

fn finalize_transactions(
    client: &RpcClient,
    db: &mut PickleDb,
    dry_run: bool,
    exit: Arc<AtomicBool>,
) -> Result<Option<usize>, Error> {
    if dry_run {
        return Ok(None);
    }

    let mut opt_confirmations = update_finalized_transactions(client, db, exit.clone())?;

    let progress_bar = new_spinner_progress_bar();

    while opt_confirmations.is_some() {
        if let Some(confirmations) = opt_confirmations {
            progress_bar.set_message(format!(
                "[{}/{}] Finalizing transactions",
                confirmations, 32,
            ));
        }

        // Sleep for about 1 slot
        sleep(Duration::from_millis(500));
        let opt_conf = update_finalized_transactions(client, db, exit.clone())?;
        opt_confirmations = opt_conf;
    }

    Ok(opt_confirmations)
}

// Update the finalized bit on any transactions that are now rooted
// Return the lowest number of confirmations on the unfinalized transactions or None if all are finalized.
fn update_finalized_transactions(
    client: &RpcClient,
    db: &mut PickleDb,
    exit: Arc<AtomicBool>,
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
    let mut statuses = vec![];
    for unconfirmed_signatures_chunk in
        unconfirmed_signatures.chunks(MAX_GET_SIGNATURE_STATUSES_QUERY_ITEMS - 1)
    {
        statuses.extend(
            client
                .get_signature_statuses(unconfirmed_signatures_chunk)?
                .value
                .into_iter(),
        );
    }

    let mut confirmations = None;
    log_transaction_confirmations(
        client,
        db,
        exit,
        unconfirmed_transactions,
        statuses,
        &mut confirmations,
    )?;
    db.dump()?;
    Ok(confirmations)
}

fn log_transaction_confirmations(
    client: &RpcClient,
    db: &mut PickleDb,
    exit: Arc<AtomicBool>,
    unconfirmed_transactions: Vec<(&Transaction, Slot)>,
    statuses: Vec<Option<TransactionStatus>>,
    confirmations: &mut Option<usize>,
) -> Result<(), Error> {
    let root_slot = client.get_slot()?;
    for ((transaction, last_valid_slot), opt_transaction_status) in unconfirmed_transactions
        .into_iter()
        .zip(statuses.into_iter())
    {
        match db::update_finalized_transaction(
            db,
            &transaction.signatures[0],
            opt_transaction_status,
            last_valid_slot,
            root_slot,
        ) {
            Ok(Some(confs)) => {
                *confirmations = Some(cmp::min(confs, confirmations.unwrap_or(usize::MAX)));
            }
            result => {
                result?;
            }
        }
        if exit.load(Ordering::SeqCst) {
            db.dump()?;
            return Err(Error::ExitSignal);
        }
    }
    Ok(())
}

fn check_payer_balances(
    num_signatures: usize,
    allocations: &[Allocation],
    client: &RpcClient,
    args: &DistributeTokensArgs,
) -> Result<(), Error> {
    let mut undistributed_tokens: u64 = allocations.iter().map(|x| x.amount).sum();

    let (_blockhash, fee_calculator) = client.get_recent_blockhash()?;
    let fees = fee_calculator
        .lamports_per_signature
        .checked_mul(num_signatures as u64)
        .unwrap();

    let (distribution_source, unlocked_sol_source) = if let Some(stake_args) = &args.stake_args {
        let total_unlocked_sol = allocations.len() as u64 * stake_args.unlocked_sol;
        undistributed_tokens -= total_unlocked_sol;
        let from_pubkey = if let Some(sender_stake_args) = &stake_args.sender_stake_args {
            sender_stake_args.stake_account_address
        } else {
            args.sender_keypair.pubkey()
        };
        (
            from_pubkey,
            Some((args.sender_keypair.pubkey(), total_unlocked_sol)),
        )
    } else {
        (args.sender_keypair.pubkey(), None)
    };

    let fee_payer_balance = client.get_balance(&args.fee_payer.pubkey())?;
    if let Some((unlocked_sol_source, total_unlocked_sol)) = unlocked_sol_source {
        let staker_balance = client.get_balance(&distribution_source)?;
        if staker_balance < undistributed_tokens {
            return Err(Error::InsufficientFunds(
                vec![FundingSource::StakeAccount].into(),
                lamports_to_sol(undistributed_tokens).to_string(),
            ));
        }
        if args.fee_payer.pubkey() == unlocked_sol_source {
            if fee_payer_balance < fees + total_unlocked_sol {
                return Err(Error::InsufficientFunds(
                    vec![FundingSource::SystemAccount, FundingSource::FeePayer].into(),
                    lamports_to_sol(fees + total_unlocked_sol).to_string(),
                ));
            }
        } else {
            if fee_payer_balance < fees {
                return Err(Error::InsufficientFunds(
                    vec![FundingSource::FeePayer].into(),
                    lamports_to_sol(fees).to_string(),
                ));
            }
            let unlocked_sol_balance = client.get_balance(&unlocked_sol_source)?;
            if unlocked_sol_balance < total_unlocked_sol {
                return Err(Error::InsufficientFunds(
                    vec![FundingSource::SystemAccount].into(),
                    lamports_to_sol(total_unlocked_sol).to_string(),
                ));
            }
        }
    } else if args.fee_payer.pubkey() == distribution_source {
        if fee_payer_balance < fees + undistributed_tokens {
            return Err(Error::InsufficientFunds(
                vec![FundingSource::SystemAccount, FundingSource::FeePayer].into(),
                lamports_to_sol(fees + undistributed_tokens).to_string(),
            ));
        }
    } else {
        if fee_payer_balance < fees {
            return Err(Error::InsufficientFunds(
                vec![FundingSource::FeePayer].into(),
                lamports_to_sol(fees).to_string(),
            ));
        }
        let sender_balance = client.get_balance(&distribution_source)?;
        if sender_balance < undistributed_tokens {
            return Err(Error::InsufficientFunds(
                vec![FundingSource::SystemAccount].into(),
                lamports_to_sol(undistributed_tokens).to_string(),
            ));
        }
    }
    Ok(())
}

pub fn process_balances(client: &RpcClient, args: &BalancesArgs) -> Result<(), Error> {
    let allocations: Vec<Allocation> =
        read_allocations(&args.input_csv, None, false, args.spl_token_args.is_some())?;
    let allocations = merge_allocations(&allocations);

    let token = if let Some(spl_token_args) = &args.spl_token_args {
        spl_token_args.mint.to_string()
    } else {
        "◎".to_string()
    };
    println!("{} {}", style("Token:").bold(), token);

    println!(
        "{}",
        style(format!(
            "{:<44}  {:>24}  {:>24}  {:>24}",
            "Recipient", "Expected Balance", "Actual Balance", "Difference"
        ))
        .bold()
    );

    for allocation in &allocations {
        if let Some(spl_token_args) = &args.spl_token_args {
            print_token_balances(client, allocation, spl_token_args)?;
        } else {
            let address: Pubkey = allocation.recipient.parse().unwrap();
            let expected = lamports_to_sol(allocation.amount);
            let actual = lamports_to_sol(client.get_balance(&address).unwrap());
            println!(
                "{:<44}  {:>24.9}  {:>24.9}  {:>24.9}",
                allocation.recipient,
                expected,
                actual,
                actual - expected,
            );
        }
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
pub fn test_process_distribute_tokens_with_client(
    client: &RpcClient,
    sender_keypair: Keypair,
    transfer_amount: Option<u64>,
) {
    let exit = Arc::new(AtomicBool::default());
    let fee_payer = Keypair::new();
    let transaction = transfer(
        client,
        sol_to_lamports(1.0),
        &sender_keypair,
        &fee_payer.pubkey(),
    )
    .unwrap();
    client
        .send_and_confirm_transaction_with_spinner(&transaction)
        .unwrap();
    assert_eq!(
        client.get_balance(&fee_payer.pubkey()).unwrap(),
        sol_to_lamports(1.0),
    );

    let expected_amount = if let Some(amount) = transfer_amount {
        amount
    } else {
        sol_to_lamports(1000.0)
    };
    let alice_pubkey = solana_sdk::pubkey::new_rand();
    let allocations_file = NamedTempFile::new().unwrap();
    let input_csv = allocations_file.path().to_str().unwrap().to_string();
    let mut wtr = csv::WriterBuilder::new().from_writer(allocations_file);
    wtr.write_record(&["recipient", "amount"]).unwrap();
    wtr.write_record(&[
        alice_pubkey.to_string(),
        lamports_to_sol(expected_amount).to_string(),
    ])
    .unwrap();
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
        spl_token_args: None,
        transfer_amount,
    };
    let confirmations = process_allocations(client, &args, exit.clone()).unwrap();
    assert_eq!(confirmations, None);

    let transaction_infos =
        db::read_transaction_infos(&db::open_db(&transaction_db, true).unwrap());
    assert_eq!(transaction_infos.len(), 1);
    assert_eq!(transaction_infos[0].recipient, alice_pubkey);
    assert_eq!(transaction_infos[0].amount, expected_amount);

    assert_eq!(client.get_balance(&alice_pubkey).unwrap(), expected_amount);

    check_output_file(&output_path, &db::open_db(&transaction_db, true).unwrap());

    // Now, run it again, and check there's no double-spend.
    process_allocations(client, &args, exit).unwrap();
    let transaction_infos =
        db::read_transaction_infos(&db::open_db(&transaction_db, true).unwrap());
    assert_eq!(transaction_infos.len(), 1);
    assert_eq!(transaction_infos[0].recipient, alice_pubkey);
    assert_eq!(transaction_infos[0].amount, expected_amount);

    assert_eq!(client.get_balance(&alice_pubkey).unwrap(), expected_amount);

    check_output_file(&output_path, &db::open_db(&transaction_db, true).unwrap());
}

pub fn test_process_create_stake_with_client(client: &RpcClient, sender_keypair: Keypair) {
    let exit = Arc::new(AtomicBool::default());
    let fee_payer = Keypair::new();
    let transaction = transfer(
        client,
        sol_to_lamports(1.0),
        &sender_keypair,
        &fee_payer.pubkey(),
    )
    .unwrap();
    client
        .send_and_confirm_transaction_with_spinner(&transaction)
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
    let (blockhash, _fees) = client.get_recent_blockhash().unwrap();
    let transaction = Transaction::new(&signers, message, blockhash);
    client
        .send_and_confirm_transaction_with_spinner(&transaction)
        .unwrap();

    let expected_amount = sol_to_lamports(1000.0);
    let alice_pubkey = solana_sdk::pubkey::new_rand();
    let file = NamedTempFile::new().unwrap();
    let input_csv = file.path().to_str().unwrap().to_string();
    let mut wtr = csv::WriterBuilder::new().from_writer(file);
    wtr.write_record(&["recipient", "amount", "lockup_date"])
        .unwrap();
    wtr.write_record(&[
        alice_pubkey.to_string(),
        lamports_to_sol(expected_amount).to_string(),
        "".to_string(),
    ])
    .unwrap();
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
        lockup_authority: None,
        unlocked_sol: sol_to_lamports(1.0),
        sender_stake_args: None,
    };
    let args = DistributeTokensArgs {
        fee_payer: Box::new(fee_payer),
        dry_run: false,
        input_csv,
        transaction_db: transaction_db.clone(),
        output_path: Some(output_path.clone()),
        stake_args: Some(stake_args),
        spl_token_args: None,
        sender_keypair: Box::new(sender_keypair),
        transfer_amount: None,
    };
    let confirmations = process_allocations(client, &args, exit.clone()).unwrap();
    assert_eq!(confirmations, None);

    let transaction_infos =
        db::read_transaction_infos(&db::open_db(&transaction_db, true).unwrap());
    assert_eq!(transaction_infos.len(), 1);
    assert_eq!(transaction_infos[0].recipient, alice_pubkey);
    assert_eq!(transaction_infos[0].amount, expected_amount);

    assert_eq!(
        client.get_balance(&alice_pubkey).unwrap(),
        sol_to_lamports(1.0),
    );
    let new_stake_account_address = transaction_infos[0].new_stake_account_address.unwrap();
    assert_eq!(
        client.get_balance(&new_stake_account_address).unwrap(),
        expected_amount - sol_to_lamports(1.0),
    );

    check_output_file(&output_path, &db::open_db(&transaction_db, true).unwrap());

    // Now, run it again, and check there's no double-spend.
    process_allocations(client, &args, exit).unwrap();
    let transaction_infos =
        db::read_transaction_infos(&db::open_db(&transaction_db, true).unwrap());
    assert_eq!(transaction_infos.len(), 1);
    assert_eq!(transaction_infos[0].recipient, alice_pubkey);
    assert_eq!(transaction_infos[0].amount, expected_amount);

    assert_eq!(
        client.get_balance(&alice_pubkey).unwrap(),
        sol_to_lamports(1.0),
    );
    assert_eq!(
        client.get_balance(&new_stake_account_address).unwrap(),
        expected_amount - sol_to_lamports(1.0),
    );

    check_output_file(&output_path, &db::open_db(&transaction_db, true).unwrap());
}

pub fn test_process_distribute_stake_with_client(client: &RpcClient, sender_keypair: Keypair) {
    let exit = Arc::new(AtomicBool::default());
    let fee_payer = Keypair::new();
    let transaction = transfer(
        client,
        sol_to_lamports(1.0),
        &sender_keypair,
        &fee_payer.pubkey(),
    )
    .unwrap();
    client
        .send_and_confirm_transaction_with_spinner(&transaction)
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
    let (blockhash, _fees) = client.get_recent_blockhash().unwrap();
    let transaction = Transaction::new(&signers, message, blockhash);
    client
        .send_and_confirm_transaction_with_spinner(&transaction)
        .unwrap();

    let expected_amount = sol_to_lamports(1000.0);
    let alice_pubkey = solana_sdk::pubkey::new_rand();
    let file = NamedTempFile::new().unwrap();
    let input_csv = file.path().to_str().unwrap().to_string();
    let mut wtr = csv::WriterBuilder::new().from_writer(file);
    wtr.write_record(&["recipient", "amount", "lockup_date"])
        .unwrap();
    wtr.write_record(&[
        alice_pubkey.to_string(),
        lamports_to_sol(expected_amount).to_string(),
        "".to_string(),
    ])
    .unwrap();
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

    let sender_stake_args = SenderStakeArgs {
        stake_account_address,
        stake_authority: Box::new(stake_authority),
        withdraw_authority: Box::new(withdraw_authority),
        lockup_authority: None,
    };
    let stake_args = StakeArgs {
        unlocked_sol: sol_to_lamports(1.0),
        lockup_authority: None,
        sender_stake_args: Some(sender_stake_args),
    };
    let args = DistributeTokensArgs {
        fee_payer: Box::new(fee_payer),
        dry_run: false,
        input_csv,
        transaction_db: transaction_db.clone(),
        output_path: Some(output_path.clone()),
        stake_args: Some(stake_args),
        spl_token_args: None,
        sender_keypair: Box::new(sender_keypair),
        transfer_amount: None,
    };
    let confirmations = process_allocations(client, &args, exit.clone()).unwrap();
    assert_eq!(confirmations, None);

    let transaction_infos =
        db::read_transaction_infos(&db::open_db(&transaction_db, true).unwrap());
    assert_eq!(transaction_infos.len(), 1);
    assert_eq!(transaction_infos[0].recipient, alice_pubkey);
    assert_eq!(transaction_infos[0].amount, expected_amount);

    assert_eq!(
        client.get_balance(&alice_pubkey).unwrap(),
        sol_to_lamports(1.0),
    );
    let new_stake_account_address = transaction_infos[0].new_stake_account_address.unwrap();
    assert_eq!(
        client.get_balance(&new_stake_account_address).unwrap(),
        expected_amount - sol_to_lamports(1.0),
    );

    check_output_file(&output_path, &db::open_db(&transaction_db, true).unwrap());

    // Now, run it again, and check there's no double-spend.
    process_allocations(client, &args, exit).unwrap();
    let transaction_infos =
        db::read_transaction_infos(&db::open_db(&transaction_db, true).unwrap());
    assert_eq!(transaction_infos.len(), 1);
    assert_eq!(transaction_infos[0].recipient, alice_pubkey);
    assert_eq!(transaction_infos[0].amount, expected_amount);

    assert_eq!(
        client.get_balance(&alice_pubkey).unwrap(),
        sol_to_lamports(1.0),
    );
    assert_eq!(
        client.get_balance(&new_stake_account_address).unwrap(),
        expected_amount - sol_to_lamports(1.0),
    );

    check_output_file(&output_path, &db::open_db(&transaction_db, true).unwrap());
}

#[cfg(test)]
mod tests {
    use super::*;
    use solana_core::test_validator::TestValidator;
    use solana_sdk::{
        signature::{read_keypair_file, write_keypair_file, Signer},
        stake::instruction::StakeInstruction,
    };
    use solana_streamer::socket::SocketAddrSpace;
    use solana_transaction_status::TransactionConfirmationStatus;

    #[test]
    fn test_process_token_allocations() {
        let alice = Keypair::new();
        let test_validator =
            TestValidator::with_no_fees(alice.pubkey(), None, SocketAddrSpace::Unspecified);
        let url = test_validator.rpc_url();

        let client = RpcClient::new_with_commitment(url, CommitmentConfig::processed());
        test_process_distribute_tokens_with_client(&client, alice, None);
    }

    #[test]
    fn test_process_transfer_amount_allocations() {
        let alice = Keypair::new();
        let test_validator =
            TestValidator::with_no_fees(alice.pubkey(), None, SocketAddrSpace::Unspecified);
        let url = test_validator.rpc_url();

        let client = RpcClient::new_with_commitment(url, CommitmentConfig::processed());
        test_process_distribute_tokens_with_client(&client, alice, Some(sol_to_lamports(1.5)));
    }

    #[test]
    fn test_create_stake_allocations() {
        let alice = Keypair::new();
        let test_validator =
            TestValidator::with_no_fees(alice.pubkey(), None, SocketAddrSpace::Unspecified);
        let url = test_validator.rpc_url();

        let client = RpcClient::new_with_commitment(url, CommitmentConfig::processed());
        test_process_create_stake_with_client(&client, alice);
    }

    #[test]
    fn test_process_stake_allocations() {
        let alice = Keypair::new();
        let test_validator =
            TestValidator::with_no_fees(alice.pubkey(), None, SocketAddrSpace::Unspecified);
        let url = test_validator.rpc_url();

        let client = RpcClient::new_with_commitment(url, CommitmentConfig::processed());
        test_process_distribute_stake_with_client(&client, alice);
    }

    #[test]
    fn test_read_allocations() {
        let alice_pubkey = solana_sdk::pubkey::new_rand();
        let allocation = Allocation {
            recipient: alice_pubkey.to_string(),
            amount: 42,
            lockup_date: "".to_string(),
        };
        let file = NamedTempFile::new().unwrap();
        let input_csv = file.path().to_str().unwrap().to_string();
        let mut wtr = csv::WriterBuilder::new().from_writer(file);
        wtr.serialize(&allocation).unwrap();
        wtr.flush().unwrap();

        assert_eq!(
            read_allocations(&input_csv, None, false, true).unwrap(),
            vec![allocation]
        );

        let allocation_sol = Allocation {
            recipient: alice_pubkey.to_string(),
            amount: sol_to_lamports(42.0),
            lockup_date: "".to_string(),
        };

        assert_eq!(
            read_allocations(&input_csv, None, true, true).unwrap(),
            vec![allocation_sol.clone()]
        );
        assert_eq!(
            read_allocations(&input_csv, None, false, false).unwrap(),
            vec![allocation_sol.clone()]
        );
        assert_eq!(
            read_allocations(&input_csv, None, true, false).unwrap(),
            vec![allocation_sol]
        );
    }

    #[test]
    fn test_read_allocations_no_lockup() {
        let pubkey0 = solana_sdk::pubkey::new_rand();
        let pubkey1 = solana_sdk::pubkey::new_rand();
        let file = NamedTempFile::new().unwrap();
        let input_csv = file.path().to_str().unwrap().to_string();
        let mut wtr = csv::WriterBuilder::new().from_writer(file);
        wtr.serialize(("recipient".to_string(), "amount".to_string()))
            .unwrap();
        wtr.serialize((&pubkey0.to_string(), 42.0)).unwrap();
        wtr.serialize((&pubkey1.to_string(), 43.0)).unwrap();
        wtr.flush().unwrap();

        let expected_allocations = vec![
            Allocation {
                recipient: pubkey0.to_string(),
                amount: sol_to_lamports(42.0),
                lockup_date: "".to_string(),
            },
            Allocation {
                recipient: pubkey1.to_string(),
                amount: sol_to_lamports(43.0),
                lockup_date: "".to_string(),
            },
        ];
        assert_eq!(
            read_allocations(&input_csv, None, false, false).unwrap(),
            expected_allocations
        );
    }

    #[test]
    #[should_panic]
    fn test_read_allocations_malformed() {
        let pubkey0 = solana_sdk::pubkey::new_rand();
        let pubkey1 = solana_sdk::pubkey::new_rand();
        let file = NamedTempFile::new().unwrap();
        let input_csv = file.path().to_str().unwrap().to_string();
        let mut wtr = csv::WriterBuilder::new().from_writer(file);
        wtr.serialize(("recipient".to_string(), "amount".to_string()))
            .unwrap();
        wtr.serialize((&pubkey0.to_string(), 42.0)).unwrap();
        wtr.serialize((&pubkey1.to_string(), 43.0)).unwrap();
        wtr.flush().unwrap();

        let expected_allocations = vec![
            Allocation {
                recipient: pubkey0.to_string(),
                amount: sol_to_lamports(42.0),
                lockup_date: "".to_string(),
            },
            Allocation {
                recipient: pubkey1.to_string(),
                amount: sol_to_lamports(43.0),
                lockup_date: "".to_string(),
            },
        ];
        assert_eq!(
            read_allocations(&input_csv, None, true, false).unwrap(),
            expected_allocations
        );
    }

    #[test]
    fn test_read_allocations_transfer_amount() {
        let pubkey0 = solana_sdk::pubkey::new_rand();
        let pubkey1 = solana_sdk::pubkey::new_rand();
        let pubkey2 = solana_sdk::pubkey::new_rand();
        let file = NamedTempFile::new().unwrap();
        let input_csv = file.path().to_str().unwrap().to_string();
        let mut wtr = csv::WriterBuilder::new().from_writer(file);
        wtr.serialize("recipient".to_string()).unwrap();
        wtr.serialize(&pubkey0.to_string()).unwrap();
        wtr.serialize(&pubkey1.to_string()).unwrap();
        wtr.serialize(&pubkey2.to_string()).unwrap();
        wtr.flush().unwrap();

        let amount = sol_to_lamports(1.5);

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
            read_allocations(&input_csv, Some(amount), false, false).unwrap(),
            expected_allocations
        );
    }

    #[test]
    fn test_apply_previous_transactions() {
        let alice = solana_sdk::pubkey::new_rand();
        let bob = solana_sdk::pubkey::new_rand();
        let mut allocations = vec![
            Allocation {
                recipient: alice.to_string(),
                amount: sol_to_lamports(1.0),
                lockup_date: "".to_string(),
            },
            Allocation {
                recipient: bob.to_string(),
                amount: sol_to_lamports(1.0),
                lockup_date: "".to_string(),
            },
        ];
        let transaction_infos = vec![TransactionInfo {
            recipient: bob,
            amount: sol_to_lamports(1.0),
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
        let alice_pubkey = solana_sdk::pubkey::new_rand();
        let bob_pubkey = solana_sdk::pubkey::new_rand();
        let lockup0 = "2021-01-07T00:00:00Z".to_string();
        let lockup1 = "9999-12-31T23:59:59Z".to_string();
        let alice_alloc = Allocation {
            recipient: alice_pubkey.to_string(),
            amount: sol_to_lamports(1.0),
            lockup_date: "".to_string(),
        };
        let alice_alloc_lockup0 = Allocation {
            recipient: alice_pubkey.to_string(),
            amount: sol_to_lamports(1.0),
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

    const SET_LOCKUP_INDEX: usize = 5;

    #[test]
    fn test_set_split_stake_lockup() {
        let lockup_date_str = "2021-01-07T00:00:00Z";
        let allocation = Allocation {
            recipient: Pubkey::default().to_string(),
            amount: sol_to_lamports(1.0),
            lockup_date: lockup_date_str.to_string(),
        };
        let stake_account_address = solana_sdk::pubkey::new_rand();
        let new_stake_account_address = solana_sdk::pubkey::new_rand();
        let lockup_authority = Keypair::new();
        let lockup_authority_address = lockup_authority.pubkey();
        let sender_stake_args = SenderStakeArgs {
            stake_account_address,
            stake_authority: Box::new(Keypair::new()),
            withdraw_authority: Box::new(Keypair::new()),
            lockup_authority: Some(Box::new(lockup_authority)),
        };
        let stake_args = StakeArgs {
            lockup_authority: Some(lockup_authority_address),
            unlocked_sol: sol_to_lamports(1.0),
            sender_stake_args: Some(sender_stake_args),
        };
        let args = DistributeTokensArgs {
            fee_payer: Box::new(Keypair::new()),
            dry_run: false,
            input_csv: "".to_string(),
            transaction_db: "".to_string(),
            output_path: None,
            stake_args: Some(stake_args),
            spl_token_args: None,
            sender_keypair: Box::new(Keypair::new()),
            transfer_amount: None,
        };
        let lockup_date = lockup_date_str.parse().unwrap();
        let instructions = distribution_instructions(
            &allocation,
            &new_stake_account_address,
            &args,
            Some(lockup_date),
            false,
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

    fn tmp_file_path(name: &str, pubkey: &Pubkey) -> String {
        use std::env;
        let out_dir = env::var("FARF_DIR").unwrap_or_else(|_| "farf".to_string());

        format!("{}/tmp/{}-{}", out_dir, name, pubkey.to_string())
    }

    fn initialize_check_payer_balances_inputs(
        allocation_amount: u64,
        sender_keypair_file: &str,
        fee_payer: &str,
        stake_args: Option<StakeArgs>,
    ) -> (Vec<Allocation>, DistributeTokensArgs) {
        let recipient = solana_sdk::pubkey::new_rand();
        let allocations = vec![Allocation {
            recipient: recipient.to_string(),
            amount: allocation_amount,
            lockup_date: "".to_string(),
        }];
        let args = DistributeTokensArgs {
            sender_keypair: read_keypair_file(sender_keypair_file).unwrap().into(),
            fee_payer: read_keypair_file(fee_payer).unwrap().into(),
            dry_run: false,
            input_csv: "".to_string(),
            transaction_db: "".to_string(),
            output_path: None,
            stake_args,
            spl_token_args: None,
            transfer_amount: None,
        };
        (allocations, args)
    }

    #[test]
    fn test_check_payer_balances_distribute_tokens_single_payer() {
        let fees = 10_000;
        let fees_in_sol = lamports_to_sol(fees);

        let alice = Keypair::new();
        let test_validator = TestValidator::with_custom_fees(
            alice.pubkey(),
            fees,
            None,
            SocketAddrSpace::Unspecified,
        );
        let url = test_validator.rpc_url();

        let client = RpcClient::new_with_commitment(url, CommitmentConfig::processed());
        let sender_keypair_file = tmp_file_path("keypair_file", &alice.pubkey());
        write_keypair_file(&alice, &sender_keypair_file).unwrap();

        let allocation_amount = 1000.0;

        // Fully funded payer
        let (allocations, mut args) = initialize_check_payer_balances_inputs(
            sol_to_lamports(allocation_amount),
            &sender_keypair_file,
            &sender_keypair_file,
            None,
        );
        check_payer_balances(1, &allocations, &client, &args).unwrap();

        // Unfunded payer
        let unfunded_payer = Keypair::new();
        let unfunded_payer_keypair_file = tmp_file_path("keypair_file", &unfunded_payer.pubkey());
        write_keypair_file(&unfunded_payer, &unfunded_payer_keypair_file).unwrap();
        args.sender_keypair = read_keypair_file(&unfunded_payer_keypair_file)
            .unwrap()
            .into();
        args.fee_payer = read_keypair_file(&unfunded_payer_keypair_file)
            .unwrap()
            .into();

        let err_result = check_payer_balances(1, &allocations, &client, &args).unwrap_err();
        if let Error::InsufficientFunds(sources, amount) = err_result {
            assert_eq!(
                sources,
                vec![FundingSource::SystemAccount, FundingSource::FeePayer].into()
            );
            assert_eq!(amount, (allocation_amount + fees_in_sol).to_string());
        } else {
            panic!("check_payer_balances should have errored");
        }

        // Payer funded enough for distribution only
        let partially_funded_payer = Keypair::new();
        let partially_funded_payer_keypair_file =
            tmp_file_path("keypair_file", &partially_funded_payer.pubkey());
        write_keypair_file(
            &partially_funded_payer,
            &partially_funded_payer_keypair_file,
        )
        .unwrap();
        let transaction = transfer(
            &client,
            sol_to_lamports(allocation_amount),
            &alice,
            &partially_funded_payer.pubkey(),
        )
        .unwrap();
        client
            .send_and_confirm_transaction_with_spinner(&transaction)
            .unwrap();

        args.sender_keypair = read_keypair_file(&partially_funded_payer_keypair_file)
            .unwrap()
            .into();
        args.fee_payer = read_keypair_file(&partially_funded_payer_keypair_file)
            .unwrap()
            .into();
        let err_result = check_payer_balances(1, &allocations, &client, &args).unwrap_err();
        if let Error::InsufficientFunds(sources, amount) = err_result {
            assert_eq!(
                sources,
                vec![FundingSource::SystemAccount, FundingSource::FeePayer].into()
            );
            assert_eq!(amount, (allocation_amount + fees_in_sol).to_string());
        } else {
            panic!("check_payer_balances should have errored");
        }
    }

    #[test]
    fn test_check_payer_balances_distribute_tokens_separate_payers() {
        let fees = 10_000;
        let fees_in_sol = lamports_to_sol(fees);
        let alice = Keypair::new();
        let test_validator = TestValidator::with_custom_fees(
            alice.pubkey(),
            fees,
            None,
            SocketAddrSpace::Unspecified,
        );
        let url = test_validator.rpc_url();

        let client = RpcClient::new_with_commitment(url, CommitmentConfig::processed());

        let sender_keypair_file = tmp_file_path("keypair_file", &alice.pubkey());
        write_keypair_file(&alice, &sender_keypair_file).unwrap();

        let allocation_amount = 1000.0;

        let funded_payer = Keypair::new();
        let funded_payer_keypair_file = tmp_file_path("keypair_file", &funded_payer.pubkey());
        write_keypair_file(&funded_payer, &funded_payer_keypair_file).unwrap();
        let transaction = transfer(
            &client,
            sol_to_lamports(allocation_amount),
            &alice,
            &funded_payer.pubkey(),
        )
        .unwrap();
        client
            .send_and_confirm_transaction_with_spinner(&transaction)
            .unwrap();

        // Fully funded payers
        let (allocations, mut args) = initialize_check_payer_balances_inputs(
            sol_to_lamports(allocation_amount),
            &funded_payer_keypair_file,
            &sender_keypair_file,
            None,
        );
        check_payer_balances(1, &allocations, &client, &args).unwrap();

        // Unfunded sender
        let unfunded_payer = Keypair::new();
        let unfunded_payer_keypair_file = tmp_file_path("keypair_file", &unfunded_payer.pubkey());
        write_keypair_file(&unfunded_payer, &unfunded_payer_keypair_file).unwrap();
        args.sender_keypair = read_keypair_file(&unfunded_payer_keypair_file)
            .unwrap()
            .into();
        args.fee_payer = read_keypair_file(&sender_keypair_file).unwrap().into();

        let err_result = check_payer_balances(1, &allocations, &client, &args).unwrap_err();
        if let Error::InsufficientFunds(sources, amount) = err_result {
            assert_eq!(sources, vec![FundingSource::SystemAccount].into());
            assert_eq!(amount, allocation_amount.to_string());
        } else {
            panic!("check_payer_balances should have errored");
        }

        // Unfunded fee payer
        args.sender_keypair = read_keypair_file(&sender_keypair_file).unwrap().into();
        args.fee_payer = read_keypair_file(&unfunded_payer_keypair_file)
            .unwrap()
            .into();

        let err_result = check_payer_balances(1, &allocations, &client, &args).unwrap_err();
        if let Error::InsufficientFunds(sources, amount) = err_result {
            assert_eq!(sources, vec![FundingSource::FeePayer].into());
            assert_eq!(amount, fees_in_sol.to_string());
        } else {
            panic!("check_payer_balances should have errored");
        }
    }

    fn initialize_stake_account(
        stake_account_amount: u64,
        unlocked_sol: u64,
        sender_keypair: &Keypair,
        client: &RpcClient,
    ) -> StakeArgs {
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
            stake_account_amount,
        );
        let message = Message::new(&instructions, Some(&sender_keypair.pubkey()));
        let signers = [sender_keypair, &stake_account_keypair];
        let (blockhash, _fees) = client.get_recent_blockhash().unwrap();
        let transaction = Transaction::new(&signers, message, blockhash);
        client
            .send_and_confirm_transaction_with_spinner(&transaction)
            .unwrap();

        let sender_stake_args = SenderStakeArgs {
            stake_account_address,
            stake_authority: Box::new(stake_authority),
            withdraw_authority: Box::new(withdraw_authority),
            lockup_authority: None,
        };

        StakeArgs {
            lockup_authority: None,
            unlocked_sol,
            sender_stake_args: Some(sender_stake_args),
        }
    }

    #[test]
    fn test_check_payer_balances_distribute_stakes_single_payer() {
        let fees = 10_000;
        let fees_in_sol = lamports_to_sol(fees);
        let alice = Keypair::new();
        let test_validator = TestValidator::with_custom_fees(
            alice.pubkey(),
            fees,
            None,
            SocketAddrSpace::Unspecified,
        );
        let url = test_validator.rpc_url();
        let client = RpcClient::new_with_commitment(url, CommitmentConfig::processed());

        let sender_keypair_file = tmp_file_path("keypair_file", &alice.pubkey());
        write_keypair_file(&alice, &sender_keypair_file).unwrap();

        let allocation_amount = 1000.0;
        let unlocked_sol = 1.0;
        let stake_args = initialize_stake_account(
            sol_to_lamports(allocation_amount),
            sol_to_lamports(unlocked_sol),
            &alice,
            &client,
        );

        // Fully funded payer & stake account
        let (allocations, mut args) = initialize_check_payer_balances_inputs(
            sol_to_lamports(allocation_amount),
            &sender_keypair_file,
            &sender_keypair_file,
            Some(stake_args),
        );
        check_payer_balances(1, &allocations, &client, &args).unwrap();

        // Underfunded stake-account
        let expensive_allocation_amount = 5000.0;
        let expensive_allocations = vec![Allocation {
            recipient: solana_sdk::pubkey::new_rand().to_string(),
            amount: sol_to_lamports(expensive_allocation_amount),
            lockup_date: "".to_string(),
        }];
        let err_result =
            check_payer_balances(1, &expensive_allocations, &client, &args).unwrap_err();
        if let Error::InsufficientFunds(sources, amount) = err_result {
            assert_eq!(sources, vec![FundingSource::StakeAccount].into());
            assert_eq!(
                amount,
                (expensive_allocation_amount - unlocked_sol).to_string()
            );
        } else {
            panic!("check_payer_balances should have errored");
        }

        // Unfunded payer
        let unfunded_payer = Keypair::new();
        let unfunded_payer_keypair_file = tmp_file_path("keypair_file", &unfunded_payer.pubkey());
        write_keypair_file(&unfunded_payer, &unfunded_payer_keypair_file).unwrap();
        args.sender_keypair = read_keypair_file(&unfunded_payer_keypair_file)
            .unwrap()
            .into();
        args.fee_payer = read_keypair_file(&unfunded_payer_keypair_file)
            .unwrap()
            .into();

        let err_result = check_payer_balances(1, &allocations, &client, &args).unwrap_err();
        if let Error::InsufficientFunds(sources, amount) = err_result {
            assert_eq!(
                sources,
                vec![FundingSource::SystemAccount, FundingSource::FeePayer].into()
            );
            assert_eq!(amount, (unlocked_sol + fees_in_sol).to_string());
        } else {
            panic!("check_payer_balances should have errored");
        }

        // Payer funded enough for distribution only
        let partially_funded_payer = Keypair::new();
        let partially_funded_payer_keypair_file =
            tmp_file_path("keypair_file", &partially_funded_payer.pubkey());
        write_keypair_file(
            &partially_funded_payer,
            &partially_funded_payer_keypair_file,
        )
        .unwrap();
        let transaction = transfer(
            &client,
            sol_to_lamports(unlocked_sol),
            &alice,
            &partially_funded_payer.pubkey(),
        )
        .unwrap();
        client
            .send_and_confirm_transaction_with_spinner(&transaction)
            .unwrap();

        args.sender_keypair = read_keypair_file(&partially_funded_payer_keypair_file)
            .unwrap()
            .into();
        args.fee_payer = read_keypair_file(&partially_funded_payer_keypair_file)
            .unwrap()
            .into();
        let err_result = check_payer_balances(1, &allocations, &client, &args).unwrap_err();
        if let Error::InsufficientFunds(sources, amount) = err_result {
            assert_eq!(
                sources,
                vec![FundingSource::SystemAccount, FundingSource::FeePayer].into()
            );
            assert_eq!(amount, (unlocked_sol + fees_in_sol).to_string());
        } else {
            panic!("check_payer_balances should have errored");
        }
    }

    #[test]
    fn test_check_payer_balances_distribute_stakes_separate_payers() {
        let fees = 10_000;
        let fees_in_sol = lamports_to_sol(fees);
        let alice = Keypair::new();
        let test_validator = TestValidator::with_custom_fees(
            alice.pubkey(),
            fees,
            None,
            SocketAddrSpace::Unspecified,
        );
        let url = test_validator.rpc_url();

        let client = RpcClient::new_with_commitment(url, CommitmentConfig::processed());

        let sender_keypair_file = tmp_file_path("keypair_file", &alice.pubkey());
        write_keypair_file(&alice, &sender_keypair_file).unwrap();

        let allocation_amount = 1000.0;
        let unlocked_sol = 1.0;
        let stake_args = initialize_stake_account(
            sol_to_lamports(allocation_amount),
            sol_to_lamports(unlocked_sol),
            &alice,
            &client,
        );

        let funded_payer = Keypair::new();
        let funded_payer_keypair_file = tmp_file_path("keypair_file", &funded_payer.pubkey());
        write_keypair_file(&funded_payer, &funded_payer_keypair_file).unwrap();
        let transaction = transfer(
            &client,
            sol_to_lamports(unlocked_sol),
            &alice,
            &funded_payer.pubkey(),
        )
        .unwrap();
        client
            .send_and_confirm_transaction_with_spinner(&transaction)
            .unwrap();

        // Fully funded payers
        let (allocations, mut args) = initialize_check_payer_balances_inputs(
            sol_to_lamports(allocation_amount),
            &funded_payer_keypair_file,
            &sender_keypair_file,
            Some(stake_args),
        );
        check_payer_balances(1, &allocations, &client, &args).unwrap();

        // Unfunded sender
        let unfunded_payer = Keypair::new();
        let unfunded_payer_keypair_file = tmp_file_path("keypair_file", &unfunded_payer.pubkey());
        write_keypair_file(&unfunded_payer, &unfunded_payer_keypair_file).unwrap();
        args.sender_keypair = read_keypair_file(&unfunded_payer_keypair_file)
            .unwrap()
            .into();
        args.fee_payer = read_keypair_file(&sender_keypair_file).unwrap().into();

        let err_result = check_payer_balances(1, &allocations, &client, &args).unwrap_err();
        if let Error::InsufficientFunds(sources, amount) = err_result {
            assert_eq!(sources, vec![FundingSource::SystemAccount].into());
            assert_eq!(amount, unlocked_sol.to_string());
        } else {
            panic!("check_payer_balances should have errored");
        }

        // Unfunded fee payer
        args.sender_keypair = read_keypair_file(&sender_keypair_file).unwrap().into();
        args.fee_payer = read_keypair_file(&unfunded_payer_keypair_file)
            .unwrap()
            .into();

        let err_result = check_payer_balances(1, &allocations, &client, &args).unwrap_err();
        if let Error::InsufficientFunds(sources, amount) = err_result {
            assert_eq!(sources, vec![FundingSource::FeePayer].into());
            assert_eq!(amount, fees_in_sol.to_string());
        } else {
            panic!("check_payer_balances should have errored");
        }
    }

    #[test]
    fn test_build_messages_dump_db() {
        let client = RpcClient::new_mock("mock_client".to_string());
        let dir = tempdir().unwrap();
        let db_file = dir
            .path()
            .join("build_messages.db")
            .to_str()
            .unwrap()
            .to_string();
        let mut db = db::open_db(&db_file, false).unwrap();

        let sender = Keypair::new();
        let recipient = Pubkey::new_unique();
        let amount = sol_to_lamports(1.0);
        let last_valid_slot = 222;
        let transaction = transfer(&client, amount, &sender, &recipient).unwrap();

        // Queue db data
        db::set_transaction_info(
            &mut db,
            &recipient,
            amount,
            &transaction,
            None,
            false,
            last_valid_slot,
            None,
        )
        .unwrap();

        // Check that data has not been dumped
        let read_db = db::open_db(&db_file, true).unwrap();
        assert!(db::read_transaction_infos(&read_db).is_empty());

        // This is just dummy data; Args will not affect messages built
        let args = DistributeTokensArgs {
            sender_keypair: Box::new(Keypair::new()),
            fee_payer: Box::new(Keypair::new()),
            dry_run: true,
            input_csv: "".to_string(),
            transaction_db: "".to_string(),
            output_path: None,
            stake_args: None,
            spl_token_args: None,
            transfer_amount: None,
        };
        let allocation = Allocation {
            recipient: recipient.to_string(),
            amount: sol_to_lamports(1.0),
            lockup_date: "".to_string(),
        };

        let mut messages: Vec<Message> = vec![];
        let mut stake_extras: StakeExtras = vec![];
        let mut created_accounts = 0;

        // Exit false will not dump data
        build_messages(
            &client,
            &mut db,
            &[allocation.clone()],
            &args,
            Arc::new(AtomicBool::new(false)),
            &mut messages,
            &mut stake_extras,
            &mut created_accounts,
        )
        .unwrap();
        let read_db = db::open_db(&db_file, true).unwrap();
        assert!(db::read_transaction_infos(&read_db).is_empty());
        assert_eq!(messages.len(), 1);

        // Empty allocations will not dump data
        let mut messages: Vec<Message> = vec![];
        let exit = Arc::new(AtomicBool::new(true));
        build_messages(
            &client,
            &mut db,
            &[],
            &args,
            exit.clone(),
            &mut messages,
            &mut stake_extras,
            &mut created_accounts,
        )
        .unwrap();
        let read_db = db::open_db(&db_file, true).unwrap();
        assert!(db::read_transaction_infos(&read_db).is_empty());
        assert!(messages.is_empty());

        // Any allocation should prompt data dump
        let mut messages: Vec<Message> = vec![];
        build_messages(
            &client,
            &mut db,
            &[allocation],
            &args,
            exit,
            &mut messages,
            &mut stake_extras,
            &mut created_accounts,
        )
        .unwrap_err();
        let read_db = db::open_db(&db_file, true).unwrap();
        let transaction_info = db::read_transaction_infos(&read_db);
        assert_eq!(transaction_info.len(), 1);
        assert_eq!(
            transaction_info[0],
            TransactionInfo {
                recipient,
                amount,
                new_stake_account_address: None,
                finalized_date: None,
                transaction,
                last_valid_slot,
                lockup_date: None,
            }
        );
        assert_eq!(messages.len(), 0);
    }

    #[test]
    fn test_send_messages_dump_db() {
        let client = RpcClient::new_mock("mock_client".to_string());
        let dir = tempdir().unwrap();
        let db_file = dir
            .path()
            .join("send_messages.db")
            .to_str()
            .unwrap()
            .to_string();
        let mut db = db::open_db(&db_file, false).unwrap();

        let sender = Keypair::new();
        let recipient = Pubkey::new_unique();
        let amount = sol_to_lamports(1.0);
        let last_valid_slot = 222;
        let transaction = transfer(&client, amount, &sender, &recipient).unwrap();

        // Queue db data
        db::set_transaction_info(
            &mut db,
            &recipient,
            amount,
            &transaction,
            None,
            false,
            last_valid_slot,
            None,
        )
        .unwrap();

        // Check that data has not been dumped
        let read_db = db::open_db(&db_file, true).unwrap();
        assert!(db::read_transaction_infos(&read_db).is_empty());

        // This is just dummy data; Args will not affect messages
        let args = DistributeTokensArgs {
            sender_keypair: Box::new(Keypair::new()),
            fee_payer: Box::new(Keypair::new()),
            dry_run: true,
            input_csv: "".to_string(),
            transaction_db: "".to_string(),
            output_path: None,
            stake_args: None,
            spl_token_args: None,
            transfer_amount: None,
        };
        let allocation = Allocation {
            recipient: recipient.to_string(),
            amount: sol_to_lamports(1.0),
            lockup_date: "".to_string(),
        };
        let message = transaction.message.clone();

        // Exit false will not dump data
        send_messages(
            &client,
            &mut db,
            &[allocation.clone()],
            &args,
            Arc::new(AtomicBool::new(false)),
            vec![message.clone()],
            vec![(Keypair::new(), None)],
        )
        .unwrap();
        let read_db = db::open_db(&db_file, true).unwrap();
        assert!(db::read_transaction_infos(&read_db).is_empty());
        // The method above will, however, write a record to the in-memory db
        // Grab that expected value to test successful dump
        let num_records = db::read_transaction_infos(&db).len();

        // Empty messages/allocations will not dump data
        let exit = Arc::new(AtomicBool::new(true));
        send_messages(&client, &mut db, &[], &args, exit.clone(), vec![], vec![]).unwrap();
        let read_db = db::open_db(&db_file, true).unwrap();
        assert!(db::read_transaction_infos(&read_db).is_empty());

        // Message/allocation should prompt data dump at start of loop
        send_messages(
            &client,
            &mut db,
            &[allocation],
            &args,
            exit,
            vec![message.clone()],
            vec![(Keypair::new(), None)],
        )
        .unwrap_err();
        let read_db = db::open_db(&db_file, true).unwrap();
        let transaction_info = db::read_transaction_infos(&read_db);
        assert_eq!(transaction_info.len(), num_records);
        assert!(transaction_info.contains(&TransactionInfo {
            recipient,
            amount,
            new_stake_account_address: None,
            finalized_date: None,
            transaction,
            last_valid_slot,
            lockup_date: None,
        }));
        assert!(transaction_info.contains(&TransactionInfo {
            recipient,
            amount,
            new_stake_account_address: None,
            finalized_date: None,
            transaction: Transaction::new_unsigned(message),
            last_valid_slot: std::u64::MAX,
            lockup_date: None,
        }));

        // Next dump should write record written in last send_messages call
        let num_records = db::read_transaction_infos(&db).len();
        db.dump().unwrap();
        let read_db = db::open_db(&db_file, true).unwrap();
        let transaction_info = db::read_transaction_infos(&read_db);
        assert_eq!(transaction_info.len(), num_records);
    }

    #[test]
    fn test_distribute_allocations_dump_db() {
        let sender_keypair = Keypair::new();
        let test_validator = TestValidator::with_no_fees(
            sender_keypair.pubkey(),
            None,
            SocketAddrSpace::Unspecified,
        );
        let url = test_validator.rpc_url();
        let client = RpcClient::new_with_commitment(url, CommitmentConfig::processed());

        let fee_payer = Keypair::new();
        let transaction = transfer(
            &client,
            sol_to_lamports(1.0),
            &sender_keypair,
            &fee_payer.pubkey(),
        )
        .unwrap();
        client
            .send_and_confirm_transaction_with_spinner(&transaction)
            .unwrap();

        let dir = tempdir().unwrap();
        let db_file = dir
            .path()
            .join("dist_allocations.db")
            .to_str()
            .unwrap()
            .to_string();
        let mut db = db::open_db(&db_file, false).unwrap();
        let recipient = Pubkey::new_unique();
        let allocation = Allocation {
            recipient: recipient.to_string(),
            amount: sol_to_lamports(1.0),
            lockup_date: "".to_string(),
        };
        // This is just dummy data; Args will not affect messages
        let args = DistributeTokensArgs {
            sender_keypair: Box::new(sender_keypair),
            fee_payer: Box::new(fee_payer),
            dry_run: true,
            input_csv: "".to_string(),
            transaction_db: "".to_string(),
            output_path: None,
            stake_args: None,
            spl_token_args: None,
            transfer_amount: None,
        };

        let exit = Arc::new(AtomicBool::new(false));

        // Ensure data is always dumped after distribute_allocations
        distribute_allocations(&client, &mut db, &[allocation], &args, exit).unwrap();
        let read_db = db::open_db(&db_file, true).unwrap();
        let transaction_info = db::read_transaction_infos(&read_db);
        assert_eq!(transaction_info.len(), 1);
    }

    #[test]
    fn test_log_transaction_confirmations_dump_db() {
        let client = RpcClient::new_mock("mock_client".to_string());
        let dir = tempdir().unwrap();
        let db_file = dir
            .path()
            .join("log_transaction_confirmations.db")
            .to_str()
            .unwrap()
            .to_string();
        let mut db = db::open_db(&db_file, false).unwrap();

        let sender = Keypair::new();
        let recipient = Pubkey::new_unique();
        let amount = sol_to_lamports(1.0);
        let last_valid_slot = 222;
        let transaction = transfer(&client, amount, &sender, &recipient).unwrap();

        // Queue unconfirmed transaction into db
        db::set_transaction_info(
            &mut db,
            &recipient,
            amount,
            &transaction,
            None,
            false,
            last_valid_slot,
            None,
        )
        .unwrap();

        // Check that data has not been dumped
        let read_db = db::open_db(&db_file, true).unwrap();
        assert!(db::read_transaction_infos(&read_db).is_empty());

        // Empty unconfirmed_transactions will not dump data
        let mut confirmations = None;
        let exit = Arc::new(AtomicBool::new(true));
        log_transaction_confirmations(
            &client,
            &mut db,
            exit.clone(),
            vec![],
            vec![],
            &mut confirmations,
        )
        .unwrap();
        let read_db = db::open_db(&db_file, true).unwrap();
        assert!(db::read_transaction_infos(&read_db).is_empty());
        assert_eq!(confirmations, None);

        // Exit false will not dump data
        log_transaction_confirmations(
            &client,
            &mut db,
            Arc::new(AtomicBool::new(false)),
            vec![(&transaction, 111)],
            vec![Some(TransactionStatus {
                slot: 40,
                confirmations: Some(15),
                status: Ok(()),
                err: None,
                confirmation_status: Some(TransactionConfirmationStatus::Finalized),
            })],
            &mut confirmations,
        )
        .unwrap();
        let read_db = db::open_db(&db_file, true).unwrap();
        assert!(db::read_transaction_infos(&read_db).is_empty());
        assert_eq!(confirmations, Some(15));

        // Exit true should dump data
        log_transaction_confirmations(
            &client,
            &mut db,
            exit,
            vec![(&transaction, 111)],
            vec![Some(TransactionStatus {
                slot: 55,
                confirmations: None,
                status: Ok(()),
                err: None,
                confirmation_status: Some(TransactionConfirmationStatus::Finalized),
            })],
            &mut confirmations,
        )
        .unwrap_err();
        let read_db = db::open_db(&db_file, true).unwrap();
        let transaction_info = db::read_transaction_infos(&read_db);
        assert_eq!(transaction_info.len(), 1);
        assert!(transaction_info[0].finalized_date.is_some());
    }

    #[test]
    fn test_update_finalized_transactions_dump_db() {
        let client = RpcClient::new_mock("mock_client".to_string());
        let dir = tempdir().unwrap();
        let db_file = dir
            .path()
            .join("update_finalized_transactions.db")
            .to_str()
            .unwrap()
            .to_string();
        let mut db = db::open_db(&db_file, false).unwrap();

        let sender = Keypair::new();
        let recipient = Pubkey::new_unique();
        let amount = sol_to_lamports(1.0);
        let last_valid_slot = 222;
        let transaction = transfer(&client, amount, &sender, &recipient).unwrap();

        // Queue unconfirmed transaction into db
        db::set_transaction_info(
            &mut db,
            &recipient,
            amount,
            &transaction,
            None,
            false,
            last_valid_slot,
            None,
        )
        .unwrap();

        // Ensure data is always dumped after update_finalized_transactions
        let confs =
            update_finalized_transactions(&client, &mut db, Arc::new(AtomicBool::new(false)))
                .unwrap();
        let read_db = db::open_db(&db_file, true).unwrap();
        let transaction_info = db::read_transaction_infos(&read_db);
        assert_eq!(transaction_info.len(), 1);
        assert_eq!(confs, None);
    }
}
