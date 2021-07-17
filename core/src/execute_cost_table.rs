/// ExecuteCostTable is aggregated by Cost Model, it keeps each program's
/// average cost in its HashMap, with fixed capacity to avoid from growing
/// unchecked.
/// When its capacity limit is reached, it prunes old and less-used programs
/// to make room for new ones.
use log::*;
use solana_sdk::pubkey::Pubkey;
use std::{collections::HashMap, time::SystemTime};

// prune is rather expensive op, free up bulk space in each operation
// would be more efficient. PRUNE_RATIO defines the after prune table
// size will be original_size * PRUNE_RATIO.
const PRUNE_RATIO: f64 = 0.75;
// with 50_000 TPS as norm, weights occurrences '100' per microsec
const OCCURRENCES_WEIGHT: i64 = 100;

const DEFAULT_CAPACITY: usize = 1024;

#[derive(Debug)]
pub struct ExecuteCostTable {
    capacity: usize,
    table: HashMap<Pubkey, u64>,
    occurrences: HashMap<Pubkey, (usize, SystemTime)>,
}

impl Default for ExecuteCostTable {
    fn default() -> Self {
        ExecuteCostTable::new(DEFAULT_CAPACITY)
    }
}

impl ExecuteCostTable {
    pub fn new(cap: usize) -> Self {
        Self {
            capacity: cap,
            table: HashMap::with_capacity(cap),
            occurrences: HashMap::with_capacity(cap),
        }
    }

    pub fn get_cost_table(&self) -> &HashMap<Pubkey, u64> {
        &self.table
    }

    pub fn get_count(&self) -> usize {
        self.table.len()
    }

    // instead of assigning unknown program with a configured/hard-coded cost
    // use average or mode function to make a educated guess.
    pub fn get_average(&self) -> u64 {
        if self.table.is_empty() {
            0
        } else {
            self.table.iter().map(|(_, value)| value).sum::<u64>() / self.get_count() as u64
        }
    }

    pub fn get_mode(&self) -> u64 {
        if self.occurrences.is_empty() {
            0
        } else {
            let key = self
                .occurrences
                .iter()
                .max_by_key(|&(_, count)| count)
                .map(|(key, _)| key)
                .expect("cannot find mode from cost table");

            *self.table.get(&key).unwrap()
        }
    }

    // returns None if program doesn't exist in table. In this case,
    // client is advised to call `get_average()` or `get_mode()` to
    // assign a 'default' value for new program.
    pub fn get_cost(&self, key: &Pubkey) -> Option<&u64> {
        self.table.get(&key)
    }

    pub fn upsert(&mut self, key: &Pubkey, value: &u64) {
        let need_to_add = self.table.get(&key).is_none();
        let current_size = self.get_count();
        if current_size == self.capacity && need_to_add {
            self.prune_to(&((current_size as f64 * PRUNE_RATIO) as usize));
        }

        let program_cost = self.table.entry(*key).or_insert(*value);
        *program_cost = (*program_cost + *value) / 2;

        let (count, timestamp) = self
            .occurrences
            .entry(*key)
            .or_insert((0, SystemTime::now()));
        *count += 1;
        *timestamp = SystemTime::now();
    }

    // prune the old programs so the table contains `new_size` of records,
    // where `old` is defined as weighted age, which is negatively correlated
    // with program's age and
    // positively correlated with how frequently the program
    // is executed (eg. occurrence),
    fn prune_to(&mut self, new_size: &usize) {
        debug!(
            "prune cost table, current size {}, new size {}",
            self.get_count(),
            new_size
        );

        if *new_size == self.get_count() {
            return;
        }

        if *new_size == 0 {
            self.table.clear();
            self.occurrences.clear();
            return;
        }

        let now = SystemTime::now();
        let mut sorted_by_weighted_age: Vec<_> = self
            .occurrences
            .iter()
            .map(|(key, (count, timestamp))| {
                let age = now.duration_since(*timestamp).unwrap().as_micros();
                let weighted_age = *count as i64 * OCCURRENCES_WEIGHT + -(age as i64);
                (weighted_age, *key)
            })
            .collect();
        sorted_by_weighted_age.sort_by(|x, y| x.0.partial_cmp(&y.0).unwrap());

        for i in sorted_by_weighted_age.iter() {
            self.table.remove(&i.1);
            self.occurrences.remove(&i.1);
            if *new_size == self.get_count() {
                break;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_execute_cost_table_prune_simple_table() {
        solana_logger::setup();
        let capacity: usize = 3;
        let mut testee = ExecuteCostTable::new(capacity);

        let key1 = Pubkey::new_unique();
        let key2 = Pubkey::new_unique();
        let key3 = Pubkey::new_unique();

        testee.upsert(&key1, &1);
        testee.upsert(&key2, &2);
        testee.upsert(&key3, &3);

        testee.prune_to(&(capacity - 1));

        // the oldest, key1, should be pruned
        assert!(testee.get_cost(&key1).is_none());
        assert!(testee.get_cost(&key2).is_some());
        assert!(testee.get_cost(&key2).is_some());
    }

    #[test]
    fn test_execute_cost_table_prune_weighted_table() {
        solana_logger::setup();
        let capacity: usize = 3;
        let mut testee = ExecuteCostTable::new(capacity);

        let key1 = Pubkey::new_unique();
        let key2 = Pubkey::new_unique();
        let key3 = Pubkey::new_unique();

        testee.upsert(&key1, &1);
        testee.upsert(&key1, &1);
        testee.upsert(&key2, &2);
        testee.upsert(&key3, &3);

        testee.prune_to(&(capacity - 1));

        // the oldest, key1, has 2 counts; 2nd oldest Key2 has 1 count;
        // expect key2 to be pruned.
        assert!(testee.get_cost(&key1).is_some());
        assert!(testee.get_cost(&key2).is_none());
        assert!(testee.get_cost(&key3).is_some());
    }

    #[test]
    fn test_execute_cost_table_upsert_within_capacity() {
        solana_logger::setup();
        let mut testee = ExecuteCostTable::default();

        let key1 = Pubkey::new_unique();
        let key2 = Pubkey::new_unique();
        let cost1: u64 = 100;
        let cost2: u64 = 110;

        // query empty table
        assert!(testee.get_cost(&key1).is_none());

        // insert one record
        testee.upsert(&key1, &cost1);
        assert_eq!(1, testee.get_count());
        assert_eq!(cost1, testee.get_average());
        assert_eq!(cost1, testee.get_mode());
        assert_eq!(&cost1, testee.get_cost(&key1).unwrap());

        // insert 2nd record
        testee.upsert(&key2, &cost2);
        assert_eq!(2, testee.get_count());
        assert_eq!((cost1 + cost2) / 2_u64, testee.get_average());
        assert_eq!(cost2, testee.get_mode());
        assert_eq!(&cost1, testee.get_cost(&key1).unwrap());
        assert_eq!(&cost2, testee.get_cost(&key2).unwrap());

        // update 1st record
        testee.upsert(&key1, &cost2);
        assert_eq!(2, testee.get_count());
        assert_eq!(((cost1 + cost2) / 2 + cost2) / 2, testee.get_average());
        assert_eq!((cost1 + cost2) / 2, testee.get_mode());
        assert_eq!(&((cost1 + cost2) / 2), testee.get_cost(&key1).unwrap());
        assert_eq!(&cost2, testee.get_cost(&key2).unwrap());
    }

    #[test]
    fn test_execute_cost_table_upsert_exceeds_capacity() {
        solana_logger::setup();
        let capacity: usize = 2;
        let mut testee = ExecuteCostTable::new(capacity);

        let key1 = Pubkey::new_unique();
        let key2 = Pubkey::new_unique();
        let key3 = Pubkey::new_unique();
        let key4 = Pubkey::new_unique();
        let cost1: u64 = 100;
        let cost2: u64 = 110;
        let cost3: u64 = 120;
        let cost4: u64 = 130;

        // insert one record
        testee.upsert(&key1, &cost1);
        assert_eq!(1, testee.get_count());
        assert_eq!(&cost1, testee.get_cost(&key1).unwrap());

        // insert 2nd record
        testee.upsert(&key2, &cost2);
        assert_eq!(2, testee.get_count());
        assert_eq!(&cost1, testee.get_cost(&key1).unwrap());
        assert_eq!(&cost2, testee.get_cost(&key2).unwrap());

        // insert 3rd record, pushes out the oldest (eg 1st) record
        testee.upsert(&key3, &cost3);
        assert_eq!(2, testee.get_count());
        assert_eq!((cost2 + cost3) / 2_u64, testee.get_average());
        assert_eq!(cost3, testee.get_mode());
        assert!(testee.get_cost(&key1).is_none());
        assert_eq!(&cost2, testee.get_cost(&key2).unwrap());
        assert_eq!(&cost3, testee.get_cost(&key3).unwrap());

        // update 2nd record, so the 3rd becomes the oldest
        // add 4th record, pushes out 3rd key
        testee.upsert(&key2, &cost1);
        testee.upsert(&key4, &cost4);
        assert_eq!(((cost1 + cost2) / 2 + cost4) / 2_u64, testee.get_average());
        assert_eq!((cost1 + cost2) / 2, testee.get_mode());
        assert_eq!(2, testee.get_count());
        assert!(testee.get_cost(&key1).is_none());
        assert_eq!(&((cost1 + cost2) / 2), testee.get_cost(&key2).unwrap());
        assert!(testee.get_cost(&key3).is_none());
        assert_eq!(&cost4, testee.get_cost(&key4).unwrap());
    }
}
