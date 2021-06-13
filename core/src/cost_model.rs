//! 'cost_model` provides service to estimate a transaction's cost
//! It does so by analyzing accounts the transaction touches, and instructions
//! it includes. Using historical data as guideline, it estimates cost of
//! reading/writing account, the sum of that comes up to "account access cost";
//! Instructions take time to execute, both historical and runtime data are
//! used to determine each instruction's execution time, the sum of that
//! is transaction's "execution cost"
//! The main function is `calculate_cost` which returns a TransactionCost struct.
//!
use log::*;
use solana_sdk::{
    bpf_loader, bpf_loader_deprecated, bpf_loader_upgradeable, feature, incinerator,
    message::Message, native_loader, pubkey::Pubkey, secp256k1_program, system_program,
    transaction::Transaction,
};
use std::collections::HashMap;

// from mainnet-beta data, taking `vote program` as 1 COST_UNIT to load and execute
// amount all type programs, the costs are:
// min: 0.9 COST_UNIT
// max: 110 COST UNIT
// Median: 12 COST_UNIT
// Average: 19 COST_UNIT
const COST_UNIT: u32 = 1;
const DEFAULT_PROGRAM_COST: u32 = COST_UNIT * 100;
// re-adjust these numbers if needed
const SIGNED_WRITABLE_ACCOUNT_ACCESS_COST: u32 = COST_UNIT * 10;
const SIGNED_READONLY_ACCOUNT_ACCESS_COST: u32 = COST_UNIT * 2;
const NON_SIGNED_WRITABLE_ACCOUNT_ACCESS_COST: u32 = COST_UNIT * 5;
const NON_SIGNED_READONLY_ACCOUNT_ACCESS_COST: u32 = COST_UNIT;
// running 'ledger-tool compute-cost' over mainnet ledger, the largest block cost
// is 575_687, and the largest chain cost (eg account cost) is 559_000
// Configuring cost model to have larger block limit and smaller account limit
// to encourage packing parallelizable transactions in block.
pub const ACCOUNT_MAX_COST: u32 = COST_UNIT * 10_000;
pub const BLOCK_MAX_COST: u32 = COST_UNIT * 10_000_000;

// cost of transaction is made of account_access_cost and instruction execution_cost
// where
// account_access_cost is the sum of read/write/sign all accounts included in the transaction
//     read is cheaper than write.
// execution_cost is the sum of all instructions execution cost, which is
//     observed during runtime and feedback by Replay
#[derive(Default, Debug)]
pub struct TransactionCost {
    pub writable_accounts: Vec<Pubkey>,
    pub account_access_cost: u32,
    pub execution_cost: u32,
}

// instruction execution code table is initialized with default values, and
// updated with realtime information (by Replay)
#[derive(Debug)]
struct InstructionExecutionCostTable {
    pub table: HashMap<Pubkey, u32>,
}

macro_rules! costmetrics {
    ($( $key: expr => $val: expr ),*) => {{
        let mut hashmap: HashMap< Pubkey, u32 > = HashMap::new();
        $( hashmap.insert( $key, $val); )*
        hashmap
    }}
}

impl InstructionExecutionCostTable {
    // build cost table with default value
    pub fn new() -> Self {
        Self {
            table: costmetrics![
                solana_config_program::id()        => COST_UNIT,
                feature::id()                      => COST_UNIT * 2,
                incinerator::id()                  => COST_UNIT * 2,
                native_loader::id()                => COST_UNIT * 2,
                solana_stake_program::id()         => COST_UNIT * 2,
                solana_stake_program::config::id() => COST_UNIT,
                solana_vote_program::id()          => COST_UNIT,
                secp256k1_program::id()            => COST_UNIT,
                system_program::id()               => COST_UNIT * 8,
                bpf_loader::id()                   => COST_UNIT * 500,
                bpf_loader_deprecated::id()        => COST_UNIT * 500,
                bpf_loader_upgradeable::id()       => COST_UNIT * 500
            ],
        }
    }
}

#[derive(Debug)]
pub struct CostModel {
    account_cost_limit: u32,
    block_cost_limit: u32,
    instruction_execution_cost_table: InstructionExecutionCostTable,
}

impl Default for CostModel {
    fn default() -> Self {
        CostModel::new(ACCOUNT_MAX_COST, BLOCK_MAX_COST)
    }
}

impl CostModel {
    pub fn new(chain_max: u32, block_max: u32) -> Self {
        Self {
            account_cost_limit: chain_max,
            block_cost_limit: block_max,
            instruction_execution_cost_table: InstructionExecutionCostTable::new(),
        }
    }

    pub fn get_account_cost_limit(&self) -> u32 {
        self.account_cost_limit
    }

    pub fn get_block_cost_limit(&self) -> u32 {
        self.block_cost_limit
    }

    pub fn calculate_cost(&self, transaction: &Transaction) -> TransactionCost {
        let (
            signed_writable_accounts,
            signed_readonly_accounts,
            non_signed_writable_accounts,
            non_signed_readonly_accounts,
        ) = CostModel::sort_accounts_by_type(transaction.message());

        let mut cost = TransactionCost {
            writable_accounts: vec![],
            account_access_cost: CostModel::find_account_access_cost(
                &signed_writable_accounts,
                &signed_readonly_accounts,
                &non_signed_writable_accounts,
                &non_signed_readonly_accounts,
            ),
            execution_cost: self.find_transaction_cost(&transaction),
        };
        cost.writable_accounts.extend(&signed_writable_accounts);
        cost.writable_accounts.extend(&non_signed_writable_accounts);
        debug!("transaction {:?} has cost {:?}", transaction, cost);
        cost
    }

    // To update or insert instruction cost to table.
    // When updating, uses the average of new and old values to smooth out outliers
    pub fn upsert_instruction_cost(
        &mut self,
        program_key: &Pubkey,
        cost: &u32,
    ) -> Result<u32, &'static str> {
        let instruction_cost = self
            .instruction_execution_cost_table
            .table
            .entry(*program_key)
            .or_insert(*cost);
        *instruction_cost = (*instruction_cost + *cost) / 2;
        Ok(*instruction_cost)
    }

    fn find_instruction_cost(&self, program_key: &Pubkey) -> u32 {
        match self
            .instruction_execution_cost_table
            .table
            .get(&program_key)
        {
            Some(cost) => *cost,
            None => {
                debug!(
                    "Program key {:?} does not have assigned cost, using default {}",
                    program_key, DEFAULT_PROGRAM_COST
                );
                DEFAULT_PROGRAM_COST
            }
        }
    }

    fn find_transaction_cost(&self, transaction: &Transaction) -> u32 {
        let mut cost: u32 = 0;

        for instruction in &transaction.message().instructions {
            let program_id =
                transaction.message().account_keys[instruction.program_id_index as usize];
            let instruction_cost = self.find_instruction_cost(&program_id);
            trace!(
                "instruction {:?} has cost of {}",
                instruction,
                instruction_cost
            );
            cost += instruction_cost;
        }
        cost
    }

    fn find_account_access_cost(
        signed_writable_accounts: &[Pubkey],
        signed_readonly_accounts: &[Pubkey],
        non_signed_writable_accounts: &[Pubkey],
        non_signed_readonly_accounts: &[Pubkey],
    ) -> u32 {
        let mut cost = 0;
        cost += signed_writable_accounts.len() as u32 * SIGNED_WRITABLE_ACCOUNT_ACCESS_COST;
        cost += signed_readonly_accounts.len() as u32 * SIGNED_READONLY_ACCOUNT_ACCESS_COST;
        cost += non_signed_writable_accounts.len() as u32 * NON_SIGNED_WRITABLE_ACCOUNT_ACCESS_COST;
        cost += non_signed_readonly_accounts.len() as u32 * NON_SIGNED_READONLY_ACCOUNT_ACCESS_COST;
        cost
    }

    fn sort_accounts_by_type(
        message: &Message,
    ) -> (Vec<Pubkey>, Vec<Pubkey>, Vec<Pubkey>, Vec<Pubkey>) {
        let demote_sysvar_write_locks = true;
        let mut signer_writable: Vec<Pubkey> = vec![];
        let mut signer_readonly: Vec<Pubkey> = vec![];
        let mut non_signer_writable: Vec<Pubkey> = vec![];
        let mut non_signer_readonly: Vec<Pubkey> = vec![];
        message.account_keys.iter().enumerate().for_each(|(i, k)| {
            let is_signer = message.is_signer(i);
            let is_writable = message.is_writable(i, demote_sysvar_write_locks);

            if is_signer && is_writable {
                signer_writable.push(*k);
            } else if is_signer && !is_writable {
                signer_readonly.push(*k);
            } else if !is_signer && is_writable {
                non_signer_writable.push(*k);
            } else {
                non_signer_readonly.push(*k);
            }
        });
        (
            signer_writable,
            signer_readonly,
            non_signer_writable,
            non_signer_readonly,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use solana_runtime::{
        bank::Bank,
        genesis_utils::{create_genesis_config, GenesisConfigInfo},
    };
    use solana_sdk::{
        hash::Hash,
        instruction::CompiledInstruction,
        message::Message,
        signature::{Keypair, Signer},
        system_instruction::{self},
        system_transaction,
    };
    use std::{
        str::FromStr,
        sync::{Arc, RwLock},
        thread::{self, JoinHandle},
    };

    fn test_setup() -> (Keypair, Hash) {
        solana_logger::setup();
        let GenesisConfigInfo {
            genesis_config,
            mint_keypair,
            ..
        } = create_genesis_config(10);
        let bank = Arc::new(Bank::new_no_wallclock_throttle(&genesis_config));
        let start_hash = bank.last_blockhash();
        (mint_keypair, start_hash)
    }

    #[test]
    fn test_cost_model_instruction_cost() {
        let testee = CostModel::default();

        // find cost for known programs
        assert_eq!(
            COST_UNIT,
            testee.find_instruction_cost(
                &Pubkey::from_str("Vote111111111111111111111111111111111111111").unwrap()
            )
        );
        assert_eq!(
            COST_UNIT * 500,
            testee.find_instruction_cost(&bpf_loader::id())
        );

        // unknown program is assigned with default cost
        assert_eq!(
            DEFAULT_PROGRAM_COST,
            testee.find_instruction_cost(
                &Pubkey::from_str("unknown111111111111111111111111111111111111").unwrap()
            )
        );
    }

    #[test]
    fn test_cost_model_simple_transaction() {
        let (mint_keypair, start_hash) = test_setup();

        let keypair = Keypair::new();
        let simple_transaction =
            system_transaction::transfer(&mint_keypair, &keypair.pubkey(), 2, start_hash);
        debug!(
            "system_transaction simple_transaction {:?}",
            simple_transaction
        );

        // expected cost for one system transfer instructions
        let expected_cost = COST_UNIT * 8;

        let testee = CostModel::default();
        assert_eq!(
            expected_cost,
            testee.find_transaction_cost(&simple_transaction)
        );
    }

    #[test]
    fn test_cost_model_transaction_many_transfer_instructions() {
        let (mint_keypair, start_hash) = test_setup();

        let key1 = solana_sdk::pubkey::new_rand();
        let key2 = solana_sdk::pubkey::new_rand();
        let instructions =
            system_instruction::transfer_many(&mint_keypair.pubkey(), &[(key1, 1), (key2, 1)]);
        let message = Message::new(&instructions, Some(&mint_keypair.pubkey()));
        let tx = Transaction::new(&[&mint_keypair], message, start_hash);
        debug!("many transfer transaction {:?}", tx);

        // expected cost for two system transfer instructions
        let expected_cost = COST_UNIT * 8 * 2;

        let testee = CostModel::default();
        assert_eq!(expected_cost, testee.find_transaction_cost(&tx));
    }

    #[test]
    fn test_cost_model_message_many_different_instructions() {
        let (mint_keypair, start_hash) = test_setup();

        // construct a transaction with multiple random instructions
        let key1 = solana_sdk::pubkey::new_rand();
        let key2 = solana_sdk::pubkey::new_rand();
        let prog1 = solana_sdk::pubkey::new_rand();
        let prog2 = solana_sdk::pubkey::new_rand();
        let instructions = vec![
            CompiledInstruction::new(3, &(), vec![0, 1]),
            CompiledInstruction::new(4, &(), vec![0, 2]),
        ];
        let tx = Transaction::new_with_compiled_instructions(
            &[&mint_keypair],
            &[key1, key2],
            start_hash,
            vec![prog1, prog2],
            instructions,
        );
        debug!("many random transaction {:?}", tx);

        // expected cost for two random/unknown program is
        let expected_cost = DEFAULT_PROGRAM_COST * 2;

        let testee = CostModel::default();
        assert_eq!(expected_cost, testee.find_transaction_cost(&tx));
    }

    #[test]
    fn test_cost_model_sort_message_accounts_by_type() {
        // construct a transaction with two random instructions with same signer
        let signer1 = Keypair::new();
        let signer2 = Keypair::new();
        let key1 = Pubkey::new_unique();
        let key2 = Pubkey::new_unique();
        let prog1 = Pubkey::new_unique();
        let prog2 = Pubkey::new_unique();
        let instructions = vec![
            CompiledInstruction::new(4, &(), vec![0, 2]),
            CompiledInstruction::new(5, &(), vec![1, 3]),
        ];
        let tx = Transaction::new_with_compiled_instructions(
            &[&signer1, &signer2],
            &[key1, key2],
            Hash::new_unique(),
            vec![prog1, prog2],
            instructions,
        );
        debug!("many random transaction {:?}", tx);

        let (
            signed_writable_accounts,
            signed_readonly_accounts,
            non_signed_writable_accounts,
            non_signed_readonly_accounts,
        ) = CostModel::sort_accounts_by_type(tx.message());

        assert_eq!(2, signed_writable_accounts.len());
        assert_eq!(signer1.pubkey(), signed_writable_accounts[0]);
        assert_eq!(signer2.pubkey(), signed_writable_accounts[1]);
        assert_eq!(0, signed_readonly_accounts.len());
        assert_eq!(2, non_signed_writable_accounts.len());
        assert_eq!(key1, non_signed_writable_accounts[0]);
        assert_eq!(key2, non_signed_writable_accounts[1]);
        assert_eq!(2, non_signed_readonly_accounts.len());
        assert_eq!(prog1, non_signed_readonly_accounts[0]);
        assert_eq!(prog2, non_signed_readonly_accounts[1]);
    }

    #[test]
    fn test_cost_model_insert_instruction_cost() {
        let key1 = Pubkey::new_unique();
        let cost1 = 100;

        let mut cost_model = CostModel::default();
        // Using default cost for unknown instruction
        assert_eq!(
            DEFAULT_PROGRAM_COST,
            cost_model.find_instruction_cost(&key1)
        );

        // insert instruction cost to table
        assert!(cost_model.upsert_instruction_cost(&key1, &cost1).is_ok());

        // now it is known insturction with known cost
        assert_eq!(cost1, cost_model.find_instruction_cost(&key1));
    }

    #[test]
    fn test_cost_model_calculate_cost() {
        let (mint_keypair, start_hash) = test_setup();
        let tx =
            system_transaction::transfer(&mint_keypair, &Keypair::new().pubkey(), 2, start_hash);

        let expected_account_cost = SIGNED_WRITABLE_ACCOUNT_ACCESS_COST
            + NON_SIGNED_WRITABLE_ACCOUNT_ACCESS_COST
            + NON_SIGNED_READONLY_ACCOUNT_ACCESS_COST;
        let expected_execution_cost = COST_UNIT * 8;

        let cost_model = CostModel::default();
        let tx_cost = cost_model.calculate_cost(&tx);
        assert_eq!(expected_account_cost, tx_cost.account_access_cost);
        assert_eq!(expected_execution_cost, tx_cost.execution_cost);
        assert_eq!(2, tx_cost.writable_accounts.len());
    }

    #[test]
    fn test_cost_model_update_instruction_cost() {
        let key1 = Pubkey::new_unique();
        let cost1 = 100;
        let cost2 = 200;
        let updated_cost = (cost1 + cost2) / 2;

        let mut cost_model = CostModel::default();

        // insert instruction cost to table
        assert!(cost_model.upsert_instruction_cost(&key1, &cost1).is_ok());
        assert_eq!(cost1, cost_model.find_instruction_cost(&key1));

        // update instruction cost
        assert!(cost_model.upsert_instruction_cost(&key1, &cost2).is_ok());
        assert_eq!(updated_cost, cost_model.find_instruction_cost(&key1));
    }

    #[test]
    fn test_cost_model_can_be_shared_concurrently_as_immutable() {
        let (mint_keypair, start_hash) = test_setup();
        let number_threads = 10;
        let expected_account_cost = SIGNED_WRITABLE_ACCOUNT_ACCESS_COST
            + NON_SIGNED_WRITABLE_ACCOUNT_ACCESS_COST
            + NON_SIGNED_READONLY_ACCOUNT_ACCESS_COST;
        let expected_execution_cost = COST_UNIT * 8;

        let cost_model = Arc::new(CostModel::default());

        let thread_handlers: Vec<JoinHandle<()>> = (0..number_threads)
            .map(|_| {
                // each thread creates its own simple transaction
                let simple_transaction = system_transaction::transfer(
                    &mint_keypair,
                    &Keypair::new().pubkey(),
                    2,
                    start_hash,
                );
                let cost_model = cost_model.clone();
                thread::spawn(move || {
                    let tx_cost = cost_model.calculate_cost(&simple_transaction);
                    assert_eq!(2, tx_cost.writable_accounts.len());
                    assert_eq!(expected_account_cost, tx_cost.account_access_cost);
                    assert_eq!(expected_execution_cost, tx_cost.execution_cost);
                })
            })
            .collect();

        for th in thread_handlers {
            th.join().unwrap();
        }
    }

    #[test]
    fn test_cost_model_can_be_shared_concurrently_with_rwlock() {
        let (mint_keypair, start_hash) = test_setup();
        // construct a transaction with multiple random instructions
        let key1 = solana_sdk::pubkey::new_rand();
        let key2 = solana_sdk::pubkey::new_rand();
        let prog1 = solana_sdk::pubkey::new_rand();
        let prog2 = solana_sdk::pubkey::new_rand();
        let instructions = vec![
            CompiledInstruction::new(3, &(), vec![0, 1]),
            CompiledInstruction::new(4, &(), vec![0, 2]),
        ];
        let tx = Arc::new(Transaction::new_with_compiled_instructions(
            &[&mint_keypair],
            &[key1, key2],
            start_hash,
            vec![prog1, prog2],
            instructions,
        ));

        let number_threads = 10;
        let expected_account_cost = SIGNED_WRITABLE_ACCOUNT_ACCESS_COST
            + NON_SIGNED_WRITABLE_ACCOUNT_ACCESS_COST * 2
            + NON_SIGNED_READONLY_ACCOUNT_ACCESS_COST * 2;
        let cost1 = 100;
        let cost2 = 200;
        // execution cost can be either 2 * Default (before write) or cost1+cost2 (after write)
        let expected_execution_cost = Arc::new(vec![cost1 + cost2, DEFAULT_PROGRAM_COST * 2]);

        let cost_model: Arc<RwLock<CostModel>> = Arc::new(RwLock::new(CostModel::default()));

        let thread_handlers: Vec<JoinHandle<()>> = (0..number_threads)
            .map(|i| {
                let cost_model = cost_model.clone();
                let tx = tx.clone();
                let expected_execution_cost = expected_execution_cost.clone();

                if i == 5 {
                    thread::spawn(move || {
                        let mut cost_model = cost_model.write().unwrap();
                        assert!(cost_model.upsert_instruction_cost(&prog1, &cost1).is_ok());
                        assert!(cost_model.upsert_instruction_cost(&prog2, &cost2).is_ok());
                    })
                } else {
                    thread::spawn(move || {
                        let tx_cost = cost_model.read().unwrap().calculate_cost(&tx);
                        assert_eq!(3, tx_cost.writable_accounts.len());
                        assert_eq!(expected_account_cost, tx_cost.account_access_cost);
                        assert!(expected_execution_cost.contains(&tx_cost.execution_cost));
                    })
                }
            })
            .collect();

        for th in thread_handlers {
            th.join().unwrap();
        }
    }
}
