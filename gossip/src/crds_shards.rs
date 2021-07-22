use {
    crate::{crds::VersionedCrdsValue, crds_gossip_pull::CrdsFilter},
    indexmap::map::IndexMap,
    std::{
        cmp::Ordering,
        ops::{Index, IndexMut},
    },
};

#[derive(Clone)]
pub struct CrdsShards {
    // shards[k] includes crds values which the first shard_bits of their hash
    // value is equal to k. Each shard is a mapping from crds values indices to
    // their hash value.
    shards: Vec<IndexMap<usize, u64>>,
    shard_bits: u32,
}

impl CrdsShards {
    pub fn new(shard_bits: u32) -> Self {
        CrdsShards {
            shards: vec![IndexMap::new(); 1 << shard_bits],
            shard_bits,
        }
    }

    pub fn insert(&mut self, index: usize, value: &VersionedCrdsValue) -> bool {
        let hash = CrdsFilter::hash_as_u64(&value.value_hash);
        self.shard_mut(hash).insert(index, hash).is_none()
    }

    pub fn remove(&mut self, index: usize, value: &VersionedCrdsValue) -> bool {
        let hash = CrdsFilter::hash_as_u64(&value.value_hash);
        self.shard_mut(hash).swap_remove(&index).is_some()
    }

    /// Returns indices of all crds values which the first 'mask_bits' of their
    /// hash value is equal to 'mask'.
    pub fn find(&self, mask: u64, mask_bits: u32) -> impl Iterator<Item = usize> + '_ {
        let ones = (!0u64).checked_shr(mask_bits).unwrap_or(0);
        let mask = mask | ones;
        match self.shard_bits.cmp(&mask_bits) {
            Ordering::Less => {
                let pred = move |(&index, hash)| {
                    if hash | ones == mask {
                        Some(index)
                    } else {
                        None
                    }
                };
                Iter::Less(self.shard(mask).iter().filter_map(pred))
            }
            Ordering::Equal => Iter::Equal(self.shard(mask).keys().cloned()),
            Ordering::Greater => {
                let count = 1 << (self.shard_bits - mask_bits);
                let end = self.shard_index(mask) + 1;
                Iter::Greater(
                    self.shards[end - count..end]
                        .iter()
                        .flat_map(IndexMap::keys)
                        .cloned(),
                )
            }
        }
    }

    #[inline]
    fn shard_index(&self, hash: u64) -> usize {
        hash.checked_shr(64 - self.shard_bits).unwrap_or(0) as usize
    }

    #[inline]
    fn shard(&self, hash: u64) -> &IndexMap<usize, u64> {
        let shard_index = self.shard_index(hash);
        self.shards.index(shard_index)
    }

    #[inline]
    fn shard_mut(&mut self, hash: u64) -> &mut IndexMap<usize, u64> {
        let shard_index = self.shard_index(hash);
        self.shards.index_mut(shard_index)
    }

    // Checks invariants in the shards tables against the crds table.
    #[cfg(test)]
    pub fn check(&self, crds: &[VersionedCrdsValue]) {
        let mut indices: Vec<_> = self
            .shards
            .iter()
            .flat_map(IndexMap::keys)
            .cloned()
            .collect();
        indices.sort_unstable();
        assert_eq!(indices, (0..crds.len()).collect::<Vec<_>>());
        for (shard_index, shard) in self.shards.iter().enumerate() {
            for (&index, &hash) in shard {
                assert_eq!(hash, CrdsFilter::hash_as_u64(&crds[index].value_hash));
                assert_eq!(
                    shard_index as u64,
                    hash.checked_shr(64 - self.shard_bits).unwrap_or(0)
                );
            }
        }
    }
}

// Wrapper for 3 types of iterators we get when comparing shard_bits and
// mask_bits in find method. This is to avoid Box<dyn Iterator<Item =...>>
// which involves dynamic dispatch and is relatively slow.
enum Iter<R, S, T> {
    Less(R),
    Equal(S),
    Greater(T),
}

impl<R, S, T> Iterator for Iter<R, S, T>
where
    R: Iterator<Item = usize>,
    S: Iterator<Item = usize>,
    T: Iterator<Item = usize>,
{
    type Item = usize;

    fn next(&mut self) -> Option<Self::Item> {
        match self {
            Self::Greater(iter) => iter.next(),
            Self::Less(iter) => iter.next(),
            Self::Equal(iter) => iter.next(),
        }
    }
}

#[cfg(test)]
mod test {
    use {
        super::*,
        crate::{crds::Crds, crds_value::CrdsValue},
        rand::{thread_rng, Rng},
        solana_sdk::timing::timestamp,
        std::{collections::HashSet, iter::repeat_with, ops::Index},
    };

    fn new_test_crds_value<R: Rng>(rng: &mut R) -> VersionedCrdsValue {
        let value = CrdsValue::new_rand(rng, None);
        let label = value.label();
        let mut crds = Crds::default();
        crds.insert(value, timestamp()).unwrap();
        crds.get::<&VersionedCrdsValue>(&label).cloned().unwrap()
    }

    // Returns true if the first mask_bits most significant bits of hash is the
    // same as the given bit mask.
    fn check_mask(value: &VersionedCrdsValue, mask: u64, mask_bits: u32) -> bool {
        let hash = CrdsFilter::hash_as_u64(&value.value_hash);
        let ones = (!0u64).checked_shr(mask_bits).unwrap_or(0u64);
        (hash | ones) == (mask | ones)
    }

    // Manual filtering by scanning all the values.
    fn filter_crds_values(
        values: &[VersionedCrdsValue],
        mask: u64,
        mask_bits: u32,
    ) -> HashSet<usize> {
        values
            .iter()
            .enumerate()
            .filter_map(|(index, value)| {
                if check_mask(value, mask, mask_bits) {
                    Some(index)
                } else {
                    None
                }
            })
            .collect()
    }

    #[test]
    fn test_crds_shards_round_trip() {
        let mut rng = thread_rng();
        // Generate some random hash and crds value labels.
        let mut values: Vec<_> = repeat_with(|| new_test_crds_value(&mut rng))
            .take(4096)
            .collect();
        // Insert everything into the crds shards.
        let mut shards = CrdsShards::new(5);
        for (index, value) in values.iter().enumerate() {
            assert!(shards.insert(index, value));
        }
        shards.check(&values);
        // Remove some of the values.
        for _ in 0..512 {
            let index = rng.gen_range(0, values.len());
            let value = values.swap_remove(index);
            assert!(shards.remove(index, &value));
            if index < values.len() {
                let value = values.index(index);
                assert!(shards.remove(values.len(), value));
                assert!(shards.insert(index, value));
            }
            shards.check(&values);
        }
        // Random masks.
        for _ in 0..10 {
            let mask = rng.gen();
            for mask_bits in 0..12 {
                let mut set = filter_crds_values(&values, mask, mask_bits);
                for index in shards.find(mask, mask_bits) {
                    assert!(set.remove(&index));
                }
                assert!(set.is_empty());
            }
        }
        // Existing hash values.
        for (index, value) in values.iter().enumerate() {
            let mask = CrdsFilter::hash_as_u64(&value.value_hash);
            let hits: Vec<_> = shards.find(mask, 64).collect();
            assert_eq!(hits, vec![index]);
        }
        // Remove everything.
        while !values.is_empty() {
            let index = rng.gen_range(0, values.len());
            let value = values.swap_remove(index);
            assert!(shards.remove(index, &value));
            if index < values.len() {
                let value = values.index(index);
                assert!(shards.remove(values.len(), value));
                assert!(shards.insert(index, value));
            }
            if index % 5 == 0 {
                shards.check(&values);
            }
        }
    }
}
