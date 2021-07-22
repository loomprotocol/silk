use {
    crate::{
        cluster_info::ClusterInfo, contact_info::ContactInfo, weighted_shuffle::weighted_shuffle,
    },
    itertools::Itertools,
    solana_sdk::{clock::Slot, pubkey::Pubkey},
    std::collections::HashMap,
};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, AbiExample, AbiEnumVisitor)]
enum CompressionType {
    Uncompressed,
    GZip,
    BZip2,
}

impl Default for CompressionType {
    fn default() -> Self {
        Self::Uncompressed
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, AbiExample)]
pub(crate) struct EpochIncompleteSlots {
    first: Slot,
    compression: CompressionType,
    compressed_list: Vec<u8>,
}

// Legacy methods copied for testing backward compatibility.

pub fn sorted_retransmit_peers_and_stakes(
    cluster_info: &ClusterInfo,
    stakes: Option<&HashMap<Pubkey, u64>>,
) -> (Vec<ContactInfo>, Vec<(u64, usize)>) {
    let mut peers = cluster_info.tvu_peers();
    // insert "self" into this list for the layer and neighborhood computation
    peers.push(cluster_info.my_contact_info());
    let stakes_and_index = sorted_stakes_with_index(&peers, stakes);
    (peers, stakes_and_index)
}

pub fn sorted_stakes_with_index(
    peers: &[ContactInfo],
    stakes: Option<&HashMap<Pubkey, u64>>,
) -> Vec<(u64, usize)> {
    let stakes_and_index: Vec<_> = peers
        .iter()
        .enumerate()
        .map(|(i, c)| {
            // For stake weighted shuffle a valid weight is atleast 1. Weight 0 is
            // assumed to be missing entry. So let's make sure stake weights are atleast 1
            let stake = 1.max(
                stakes
                    .as_ref()
                    .map_or(1, |stakes| *stakes.get(&c.id).unwrap_or(&1)),
            );
            (stake, i)
        })
        .sorted_by(|(l_stake, l_info), (r_stake, r_info)| {
            if r_stake == l_stake {
                peers[*r_info].id.cmp(&peers[*l_info].id)
            } else {
                r_stake.cmp(l_stake)
            }
        })
        .collect();

    stakes_and_index
}

pub fn shuffle_peers_and_index(
    id: &Pubkey,
    peers: &[ContactInfo],
    stakes_and_index: &[(u64, usize)],
    seed: [u8; 32],
) -> (usize, Vec<(u64, usize)>) {
    let shuffled_stakes_and_index = stake_weighted_shuffle(stakes_and_index, seed);
    let self_index = shuffled_stakes_and_index
        .iter()
        .enumerate()
        .find_map(|(i, (_stake, index))| {
            if peers[*index].id == *id {
                Some(i)
            } else {
                None
            }
        })
        .unwrap();
    (self_index, shuffled_stakes_and_index)
}

fn stake_weighted_shuffle(stakes_and_index: &[(u64, usize)], seed: [u8; 32]) -> Vec<(u64, usize)> {
    let stake_weights = stakes_and_index.iter().map(|(w, _)| *w);

    let shuffle = weighted_shuffle(stake_weights, seed);

    shuffle.iter().map(|x| stakes_and_index[*x]).collect()
}
