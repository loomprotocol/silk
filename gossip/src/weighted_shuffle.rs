//! The `weighted_shuffle` module provides an iterator over shuffled weights.

use {
    itertools::Itertools,
    num_traits::{CheckedAdd, FromPrimitive, ToPrimitive},
    rand::{
        distributions::uniform::{SampleUniform, UniformSampler},
        Rng, SeedableRng,
    },
    rand_chacha::ChaChaRng,
    std::{
        borrow::Borrow,
        iter,
        ops::{AddAssign, Div, Sub, SubAssign},
    },
};

#[derive(Debug)]
pub enum WeightedShuffleError<T> {
    NegativeWeight(T),
    SumOverflow,
}

/// Implements an iterator where indices are shuffled according to their
/// weights:
///   - Returned indices are unique in the range [0, weights.len()).
///   - Higher weighted indices tend to appear earlier proportional to their
///     weight.
///   - Zero weighted indices are excluded. Therefore the iterator may have
///     count less than weights.len().
pub struct WeightedShuffle<'a, R, T> {
    arr: Vec<T>,    // Underlying array implementing binary indexed tree.
    sum: T,         // Current sum of weights, excluding already selected indices.
    rng: &'a mut R, // Random number generator.
}

// The implementation uses binary indexed tree:
// https://en.wikipedia.org/wiki/Fenwick_tree
// to maintain cumulative sum of weights excluding already selected indices
// over self.arr.
impl<'a, R: Rng, T> WeightedShuffle<'a, R, T>
where
    T: Copy + Default + PartialOrd + AddAssign + CheckedAdd,
{
    /// Returns error if:
    ///   - any of the weights are negative.
    ///   - sum of weights overflows.
    pub fn new(rng: &'a mut R, weights: &[T]) -> Result<Self, WeightedShuffleError<T>> {
        let size = weights.len() + 1;
        let zero = <T as Default>::default();
        let mut arr = vec![zero; size];
        let mut sum = zero;
        for (mut k, &weight) in (1usize..).zip(weights) {
            #[allow(clippy::neg_cmp_op_on_partial_ord)]
            // weight < zero does not work for NaNs.
            if !(weight >= zero) {
                return Err(WeightedShuffleError::NegativeWeight(weight));
            }
            sum = sum
                .checked_add(&weight)
                .ok_or(WeightedShuffleError::SumOverflow)?;
            while k < size {
                arr[k] += weight;
                k += k & k.wrapping_neg();
            }
        }
        Ok(Self { arr, sum, rng })
    }
}

impl<'a, R, T> WeightedShuffle<'a, R, T>
where
    T: Copy + Default + PartialOrd + AddAssign + SubAssign + Sub<Output = T>,
{
    // Returns cumulative sum of current weights upto index k (inclusive).
    fn cumsum(&self, mut k: usize) -> T {
        let mut out = <T as Default>::default();
        while k != 0 {
            out += self.arr[k];
            k ^= k & k.wrapping_neg();
        }
        out
    }

    // Removes given weight at index k.
    fn remove(&mut self, mut k: usize, weight: T) {
        self.sum -= weight;
        let size = self.arr.len();
        while k < size {
            self.arr[k] -= weight;
            k += k & k.wrapping_neg();
        }
    }

    // Returns smallest index such that self.cumsum(k) > val,
    // along with its respective weight.
    fn search(&self, val: T) -> (/*index:*/ usize, /*weight:*/ T) {
        let zero = <T as Default>::default();
        debug_assert!(val >= zero);
        debug_assert!(val < self.sum);
        let mut lo = (/*index:*/ 0, /*cumsum:*/ zero);
        let mut hi = (self.arr.len() - 1, self.sum);
        while lo.0 + 1 < hi.0 {
            let k = lo.0 + (hi.0 - lo.0) / 2;
            let sum = self.cumsum(k);
            if sum <= val {
                lo = (k, sum);
            } else {
                hi = (k, sum);
            }
        }
        debug_assert!(lo.1 <= val);
        debug_assert!(hi.1 > val);
        (hi.0, hi.1 - lo.1)
    }
}

impl<'a, R: Rng, T> Iterator for WeightedShuffle<'a, R, T>
where
    T: Copy + Default + PartialOrd + AddAssign + SampleUniform + SubAssign + Sub<Output = T>,
{
    type Item = usize;

    fn next(&mut self) -> Option<Self::Item> {
        let zero = <T as Default>::default();
        #[allow(clippy::neg_cmp_op_on_partial_ord)]
        // self.sum <= zero does not work for NaNs.
        if !(self.sum > zero) {
            return None;
        }
        let sample = <T as SampleUniform>::Sampler::sample_single(zero, self.sum, &mut self.rng);
        let (index, weight) = WeightedShuffle::search(self, sample);
        self.remove(index, weight);
        Some(index - 1)
    }
}

/// Returns a list of indexes shuffled based on the input weights
/// Note - The sum of all weights must not exceed `u64::MAX`
pub fn weighted_shuffle<T, B, F>(weights: F, seed: [u8; 32]) -> Vec<usize>
where
    T: Copy + PartialOrd + iter::Sum + Div<T, Output = T> + FromPrimitive + ToPrimitive,
    B: Borrow<T>,
    F: Iterator<Item = B> + Clone,
{
    let total_weight: T = weights.clone().map(|x| *x.borrow()).sum();
    let mut rng = ChaChaRng::from_seed(seed);
    weights
        .enumerate()
        .map(|(i, weight)| {
            let weight = weight.borrow();
            // This generates an "inverse" weight but it avoids floating point math
            let x = (total_weight / *weight)
                .to_u64()
                .expect("values > u64::max are not supported");
            (
                i,
                // capture the u64 into u128s to prevent overflow
                rng.gen_range(1, u128::from(std::u16::MAX)) * u128::from(x),
            )
        })
        // sort in ascending order
        .sorted_by(|(_, l_val), (_, r_val)| l_val.cmp(r_val))
        .map(|x| x.0)
        .collect()
}

/// Returns the highest index after computing a weighted shuffle.
/// Saves doing any sorting for O(n) max calculation.
// TODO: Remove in favor of rand::distributions::WeightedIndex.
pub fn weighted_best(weights_and_indexes: &[(u64, usize)], seed: [u8; 32]) -> usize {
    if weights_and_indexes.is_empty() {
        return 0;
    }
    let mut rng = ChaChaRng::from_seed(seed);
    let total_weight: u64 = weights_and_indexes.iter().map(|x| x.0).sum();
    let mut lowest_weight = std::u128::MAX;
    let mut best_index = 0;
    for v in weights_and_indexes {
        // This generates an "inverse" weight but it avoids floating point math
        let x = (total_weight / v.0)
            .to_u64()
            .expect("values > u64::max are not supported");
        // capture the u64 into u128s to prevent overflow
        let computed_weight = rng.gen_range(1, u128::from(std::u16::MAX)) * u128::from(x);
        // The highest input weight maps to the lowest computed weight
        if computed_weight < lowest_weight {
            lowest_weight = computed_weight;
            best_index = v.1;
        }
    }

    best_index
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{convert::TryInto, iter::repeat_with};

    fn weighted_shuffle_slow<R>(rng: &mut R, mut weights: Vec<u64>) -> Vec<usize>
    where
        R: Rng,
    {
        let mut shuffle = Vec::with_capacity(weights.len());
        loop {
            let high: u64 = weights.iter().sum();
            if high == 0 {
                break shuffle;
            }
            let sample = rng.gen_range(0, high);
            let index = weights
                .iter()
                .scan(0, |acc, &w| {
                    *acc += w;
                    Some(*acc)
                })
                .position(|acc| sample < acc)
                .unwrap();
            shuffle.push(index);
            weights[index] = 0;
        }
    }

    #[test]
    fn test_weighted_shuffle_iterator() {
        let mut test_set = [0; 6];
        let mut count = 0;
        let shuffle = weighted_shuffle(vec![50, 10, 2, 1, 1, 1].into_iter(), [0x5a; 32]);
        shuffle.into_iter().for_each(|x| {
            assert_eq!(test_set[x], 0);
            test_set[x] = 1;
            count += 1;
        });
        assert_eq!(count, 6);
    }

    #[test]
    fn test_weighted_shuffle_iterator_large() {
        let mut test_set = [0; 100];
        let mut test_weights = vec![0; 100];
        (0..100).for_each(|i| test_weights[i] = (i + 1) as u64);
        let mut count = 0;
        let shuffle = weighted_shuffle(test_weights.into_iter(), [0xa5; 32]);
        shuffle.into_iter().for_each(|x| {
            assert_eq!(test_set[x], 0);
            test_set[x] = 1;
            count += 1;
        });
        assert_eq!(count, 100);
    }

    #[test]
    fn test_weighted_shuffle_compare() {
        let shuffle = weighted_shuffle(vec![50, 10, 2, 1, 1, 1].into_iter(), [0x5a; 32]);

        let shuffle1 = weighted_shuffle(vec![50, 10, 2, 1, 1, 1].into_iter(), [0x5a; 32]);
        shuffle1
            .into_iter()
            .zip(shuffle.into_iter())
            .for_each(|(x, y)| {
                assert_eq!(x, y);
            });
    }

    #[test]
    fn test_weighted_shuffle_imbalanced() {
        let mut weights = vec![std::u32::MAX as u64; 3];
        weights.push(1);
        let shuffle = weighted_shuffle(weights.iter().cloned(), [0x5a; 32]);
        shuffle.into_iter().for_each(|x| {
            if x == weights.len() - 1 {
                assert_eq!(weights[x], 1);
            } else {
                assert_eq!(weights[x], std::u32::MAX as u64);
            }
        });
    }

    #[test]
    fn test_weighted_best() {
        let weights_and_indexes: Vec<_> = vec![100u64, 1000, 10_000, 10]
            .into_iter()
            .enumerate()
            .map(|(i, weight)| (weight, i))
            .collect();
        let best_index = weighted_best(&weights_and_indexes, [0x5b; 32]);
        assert_eq!(best_index, 2);
    }

    // Asserts that each index is selected proportional to its weight.
    #[test]
    fn test_weighted_shuffle_sanity() {
        let seed: Vec<_> = (1..).step_by(3).take(32).collect();
        let seed: [u8; 32] = seed.try_into().unwrap();
        let mut rng = ChaChaRng::from_seed(seed);
        let weights = [1, 1000, 10, 100];
        let mut counts = [0; 4];
        for _ in 0..100000 {
            let mut shuffle = WeightedShuffle::new(&mut rng, &weights).unwrap();
            counts[shuffle.next().unwrap()] += 1;
            let _ = shuffle.count(); // consume the rest.
        }
        assert_eq!(counts, [101, 90113, 891, 8895]);
    }

    #[test]
    fn test_weighted_shuffle_hard_coded() {
        let weights = [
            78, 70, 38, 27, 21, 0, 82, 42, 21, 77, 77, 17, 4, 50, 96, 83, 33, 16, 72,
        ];
        let seed = [48u8; 32];
        let mut rng = ChaChaRng::from_seed(seed);
        let shuffle: Vec<_> = WeightedShuffle::new(&mut rng, &weights).unwrap().collect();
        assert_eq!(
            shuffle,
            [2, 11, 16, 0, 13, 14, 15, 10, 1, 9, 7, 6, 12, 18, 4, 17, 3, 8]
        );
        let seed = [37u8; 32];
        let mut rng = ChaChaRng::from_seed(seed);
        let shuffle: Vec<_> = WeightedShuffle::new(&mut rng, &weights).unwrap().collect();
        assert_eq!(
            shuffle,
            [17, 3, 14, 13, 6, 10, 15, 16, 9, 2, 4, 1, 0, 7, 8, 18, 11, 12]
        );
    }

    #[test]
    fn test_weighted_shuffle_match_slow() {
        let mut rng = rand::thread_rng();
        let weights: Vec<u64> = repeat_with(|| rng.gen_range(0, 1000)).take(997).collect();
        for _ in 0..10 {
            let mut seed = [0u8; 32];
            rng.fill(&mut seed[..]);
            let mut rng = ChaChaRng::from_seed(seed);
            let shuffle: Vec<_> = WeightedShuffle::new(&mut rng, &weights).unwrap().collect();
            let mut rng = ChaChaRng::from_seed(seed);
            let shuffle_slow = weighted_shuffle_slow(&mut rng, weights.clone());
            assert_eq!(shuffle, shuffle_slow,);
        }
    }
}
