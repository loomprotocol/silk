//! The `bank_forks` module implments BankForks a DAG of checkpointed Banks

use bincode::{deserialize_from, serialize_into};
use solana_metrics::counter::Counter;
use solana_runtime::bank::Bank;
use solana_sdk::timing;
use std::collections::{HashMap, HashSet};
use std::env;
use std::fs;
use std::fs::File;
use std::io::{BufReader, BufWriter, Error, ErrorKind};
use std::ops::Index;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

pub struct BankForks {
    banks: HashMap<u64, Arc<Bank>>,
    working_bank: Arc<Bank>,
    root: u64,
    slots: HashSet<u64>,
    use_snapshot: bool,
}

impl Index<u64> for BankForks {
    type Output = Arc<Bank>;
    fn index(&self, bank_slot: u64) -> &Arc<Bank> {
        &self.banks[&bank_slot]
    }
}

impl BankForks {
    pub fn new(bank_slot: u64, bank: Bank) -> Self {
        let mut banks = HashMap::new();
        let working_bank = Arc::new(bank);
        banks.insert(bank_slot, working_bank.clone());
        Self {
            banks,
            working_bank,
            root: 0,
            slots: HashSet::new(),
            use_snapshot: false,
        }
    }

    /// Create a map of bank slot id to the set of ancestors for the bank slot.
    pub fn ancestors(&self) -> HashMap<u64, HashSet<u64>> {
        let mut ancestors = HashMap::new();
        for bank in self.banks.values() {
            let mut set: HashSet<u64> = bank.ancestors.keys().cloned().collect();
            set.remove(&bank.slot());
            ancestors.insert(bank.slot(), set);
        }
        ancestors
    }

    /// Create a map of bank slot id to the set of all of its descendants
    #[allow(clippy::or_fun_call)]
    pub fn descendants(&self) -> HashMap<u64, HashSet<u64>> {
        let mut descendants = HashMap::new();
        for bank in self.banks.values() {
            let _ = descendants.entry(bank.slot()).or_insert(HashSet::new());
            let mut set: HashSet<u64> = bank.ancestors.keys().cloned().collect();
            set.remove(&bank.slot());
            for parent in set {
                descendants
                    .entry(parent)
                    .or_insert(HashSet::new())
                    .insert(bank.slot());
            }
        }
        descendants
    }

    pub fn frozen_banks(&self) -> HashMap<u64, Arc<Bank>> {
        self.banks
            .iter()
            .filter(|(_, b)| b.is_frozen())
            .map(|(k, b)| (*k, b.clone()))
            .collect()
    }

    pub fn active_banks(&self) -> Vec<u64> {
        self.banks
            .iter()
            .filter(|(_, v)| !v.is_frozen())
            .map(|(k, _v)| *k)
            .collect()
    }

    pub fn get(&self, bank_slot: u64) -> Option<&Arc<Bank>> {
        self.banks.get(&bank_slot)
    }

    pub fn new_from_banks(initial_banks: &[Arc<Bank>], root: u64) -> Self {
        let mut banks = HashMap::new();
        let working_bank = initial_banks[0].clone();
        for bank in initial_banks {
            banks.insert(bank.slot(), bank.clone());
        }
        Self {
            root,
            banks,
            working_bank,
            slots: HashSet::new(),
            use_snapshot: false,
        }
    }

    pub fn insert(&mut self, bank: Bank) {
        let bank = Arc::new(bank);
        let prev = self.banks.insert(bank.slot(), bank.clone());
        assert!(prev.is_none());

        self.working_bank = bank.clone();
    }

    // TODO: really want to kill this...
    pub fn working_bank(&self) -> Arc<Bank> {
        self.working_bank.clone()
    }

    pub fn set_root(&mut self, root: u64) {
        self.root = root;
        let set_root_start = Instant::now();
        let root_bank = self
            .banks
            .get(&root)
            .expect("root bank didn't exist in bank_forks");
        root_bank.squash();
        self.prune_non_root(root);

        inc_new_counter_info!(
            "bank-forks_set_root_ms",
            timing::duration_as_ms(&set_root_start.elapsed()) as usize
        );
    }

    pub fn root(&self) -> u64 {
        self.root
    }

    fn prune_non_root(&mut self, root: u64) {
        let slots: HashSet<u64> = self
            .banks
            .iter()
            .filter(|(_, b)| b.is_frozen())
            .map(|(k, _)| *k)
            .collect();
        let descendants = self.descendants();
        self.banks
            .retain(|slot, _| descendants[&root].contains(slot));
        let diff: HashSet<_> = slots.symmetric_difference(&self.slots).collect();
        for slot in diff.iter() {
            if **slot > root {
                let _ = self.add_snapshot(**slot);
            } else {
                self.remove_snapshot(**slot);
            }
        }
        self.slots = slots.clone();
    }

    fn get_io_error(error: &str) -> Error {
        Error::new(ErrorKind::Other, error)
    }

    fn get_snapshot_path() -> PathBuf {
        let out_dir = env::var("OUT_DIR").unwrap_or_else(|_| "target".to_string());
        let snapshot_dir = format!("{}/snapshots/", out_dir);
        Path::new(&snapshot_dir).to_path_buf()
    }

    pub fn add_snapshot(&self, slot: u64) -> Result<(), Error> {
        let path = BankForks::get_snapshot_path();
        fs::create_dir_all(path.clone())?;
        let bank_file = format!("{}", slot);
        let bank_file_path = path.join(bank_file);
        let file = File::create(bank_file_path)?;
        let mut stream = BufWriter::new(file);
        serialize_into(&mut stream, self.get(slot).unwrap())
            .map_err(|_| BankForks::get_io_error("serialize bank error"))?;
        Ok(())
    }

    pub fn remove_snapshot(&self, slot: u64) {
        let path = BankForks::get_snapshot_path();
        let bank_file = format!("{}", slot);
        let bank_file_path = path.join(bank_file);
        let _ = fs::remove_file(bank_file_path);
    }

    pub fn set_snapshot_config(&mut self, use_snapshot: bool) {
        self.use_snapshot = use_snapshot;
    }

    pub fn load_from_snapshot() -> Result<Self, Error> {
        let path = BankForks::get_snapshot_path();
        let paths = fs::read_dir(path.clone())?;
        let mut names = paths
            .filter_map(|entry| {
                entry.ok().and_then(|e| {
                    e.path()
                        .file_name()
                        .and_then(|n| n.to_str().map(|s| s.parse::<u64>().unwrap()))
                })
            })
            .collect::<Vec<u64>>();

        names.sort();
        let mut banks: HashMap<u64, Arc<Bank>> = HashMap::new();
        let mut slots = HashSet::new();
        let mut last_slot: u64 = 0;
        for bank_slot in names.clone() {
            let bank_path = format!("{}", bank_slot);
            let bank_file_path = path.join(bank_path.clone());
            info!("Load from {:?}", bank_file_path);
            let file = File::open(bank_file_path)?;
            let mut stream = BufReader::new(file);
            let bank: Result<Bank, std::io::Error> = deserialize_from(&mut stream)
                .map_err(|_| BankForks::get_io_error("deserialize bank error"));
            match bank {
                Ok(v) => {
                    banks.insert(bank_slot, Arc::new(v));
                    slots.insert(bank_slot);
                    last_slot = bank_slot;
                }
                Err(_) => warn!("Load snapshot failed for {}", bank_slot),
            }
        }
        info!("last slot: {}", last_slot);
        let working_bank = banks[&last_slot].clone();
        Ok(BankForks {
            banks,
            working_bank,
            slots,
            use_snapshot: true,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bincode::{deserialize, serialize};
    use crate::genesis_utils::create_genesis_block;
    use solana_sdk::hash::Hash;
    use solana_sdk::pubkey::Pubkey;
    use solana_sdk::signature::{Keypair, KeypairUtil};
    use solana_sdk::system_transaction;
    use std::env;
    use std::fs::remove_dir_all;

    #[test]
    fn test_bank_forks() {
        let (genesis_block, _) = create_genesis_block(10_000);
        let bank = Bank::new(&genesis_block);
        let mut bank_forks = BankForks::new(0, bank);
        let child_bank = Bank::new_from_parent(&bank_forks[0u64], &Pubkey::default(), 1);
        child_bank.register_tick(&Hash::default());
        bank_forks.insert(child_bank);
        assert_eq!(bank_forks[1u64].tick_height(), 1);
        assert_eq!(bank_forks.working_bank().tick_height(), 1);
    }

    #[test]
    fn test_bank_forks_descendants() {
        let (genesis_block, _) = create_genesis_block(10_000);
        let bank = Bank::new(&genesis_block);
        let mut bank_forks = BankForks::new(0, bank);
        let bank0 = bank_forks[0].clone();
        let bank = Bank::new_from_parent(&bank0, &Pubkey::default(), 1);
        bank_forks.insert(bank);
        let bank = Bank::new_from_parent(&bank0, &Pubkey::default(), 2);
        bank_forks.insert(bank);
        let descendants = bank_forks.descendants();
        let children: HashSet<u64> = [1u64, 2u64].to_vec().into_iter().collect();
        assert_eq!(children, *descendants.get(&0).unwrap());
        assert!(descendants[&1].is_empty());
        assert!(descendants[&2].is_empty());
    }

    #[test]
    fn test_bank_forks_ancestors() {
        let (genesis_block, _) = create_genesis_block(10_000);
        let bank = Bank::new(&genesis_block);
        let mut bank_forks = BankForks::new(0, bank);
        let bank0 = bank_forks[0].clone();
        let bank = Bank::new_from_parent(&bank0, &Pubkey::default(), 1);
        bank_forks.insert(bank);
        let bank = Bank::new_from_parent(&bank0, &Pubkey::default(), 2);
        bank_forks.insert(bank);
        let ancestors = bank_forks.ancestors();
        assert!(ancestors[&0].is_empty());
        let parents: Vec<u64> = ancestors[&1].iter().cloned().collect();
        assert_eq!(parents, vec![0]);
        let parents: Vec<u64> = ancestors[&2].iter().cloned().collect();
        assert_eq!(parents, vec![0]);
    }

    #[test]
    fn test_bank_forks_frozen_banks() {
        let (genesis_block, _) = create_genesis_block(10_000);
        let bank = Bank::new(&genesis_block);
        let mut bank_forks = BankForks::new(0, bank);
        let child_bank = Bank::new_from_parent(&bank_forks[0u64], &Pubkey::default(), 1);
        bank_forks.insert(child_bank);
        assert!(bank_forks.frozen_banks().get(&0).is_some());
        assert!(bank_forks.frozen_banks().get(&1).is_none());
    }

    #[test]
    fn test_bank_forks_active_banks() {
        let (genesis_block, _) = create_genesis_block(10_000);
        let bank = Bank::new(&genesis_block);
        let mut bank_forks = BankForks::new(0, bank);
        let child_bank = Bank::new_from_parent(&bank_forks[0u64], &Pubkey::default(), 1);
        bank_forks.insert(child_bank);
        assert_eq!(bank_forks.active_banks(), vec![1]);
    }

    struct TempPaths {
        pub paths: String,
    }

    #[macro_export]
    macro_rules! tmp_bank_accounts_name {
        () => {
            &format!("{}-{}", file!(), line!())
        };
    }

    #[macro_export]
    macro_rules! get_tmp_bank_accounts_path {
        () => {
            get_tmp_bank_accounts_path(tmp_bank_accounts_name!())
        };
    }

    impl Drop for TempPaths {
        fn drop(&mut self) {
            let paths: Vec<String> = self.paths.split(',').map(|s| s.to_string()).collect();
            paths.iter().for_each(|p| {
                let _ignored = remove_dir_all(p);
            });
        }
    }

    fn get_paths_vec(paths: &str) -> Vec<String> {
        paths.split(',').map(|s| s.to_string()).collect()
    }

    fn get_tmp_bank_accounts_path(paths: &str) -> TempPaths {
        let vpaths = get_paths_vec(paths);
        let out_dir = env::var("OUT_DIR").unwrap_or_else(|_| "target".to_string());
        let vpaths: Vec<_> = vpaths
            .iter()
            .map(|path| format!("{}/{}", out_dir, path))
            .collect();
        TempPaths {
            paths: vpaths.join(","),
        }
    }

    fn save_and_load_snapshot(bank_forks: &BankForks) {
        let bank = bank_forks.banks.get(&0).unwrap();
        let tick_height = bank.tick_height();
        let bank_ser = serialize(&bank).unwrap();
        let child_bank = bank_forks.banks.get(&1).unwrap();
        let child_bank_ser = serialize(&child_bank).unwrap();
        for (slot, _) in bank_forks.banks.iter() {
            bank_forks.add_snapshot(*slot).unwrap();
        }
        drop(bank_forks);

        let new = BankForks::load_from_snapshot().unwrap();
        assert_eq!(new[0].tick_height(), tick_height);
        let bank: Bank = deserialize(&bank_ser).unwrap();
        let new_bank = new.banks.get(&0).unwrap();
        bank.compare_bank(&new_bank);
        let child_bank: Bank = deserialize(&child_bank_ser).unwrap();
        let new_bank = new.banks.get(&1).unwrap();
        child_bank.compare_bank(&new_bank);
        for (slot, _) in new.banks.iter() {
            new.remove_snapshot(*slot);
        }
        drop(new);
    }

    #[test]
    fn test_bank_forks_snapshot_n() {
        solana_logger::setup();
        let path = get_tmp_bank_accounts_path!();
        let (genesis_block, mint_keypair) = GenesisBlock::new(10_000);
        let bank0 = Bank::new_with_paths(&genesis_block, Some(path.paths.clone()));
        bank0.freeze();
        let mut bank_forks = BankForks::new(0, bank0);
        for index in 0..10 {
            let bank = Bank::new_from_parent(&bank_forks[index], &Pubkey::default(), index + 1);
            let key1 = Keypair::new().pubkey();
            let tx = system_transaction::create_user_account(
                &mint_keypair,
                &key1,
                1,
                genesis_block.hash(),
                0,
            );
            assert_eq!(bank.process_transaction(&tx), Ok(()));
            bank.freeze();
            bank_forks.insert(bank);
            save_and_load_snapshot(&bank_forks);
        }
        assert_eq!(bank_forks.working_bank().slot(), 10);
    }
}
