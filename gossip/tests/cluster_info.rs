#![allow(clippy::integer_arithmetic)]
use {
    rayon::{iter::ParallelIterator, prelude::*},
    serial_test::serial,
    solana_gossip::{
        cluster_info::{compute_retransmit_peers, ClusterInfo},
        contact_info::ContactInfo,
        deprecated::{shuffle_peers_and_index, sorted_retransmit_peers_and_stakes},
    },
    solana_sdk::{pubkey::Pubkey, signer::keypair::Keypair},
    solana_streamer::socket::SocketAddrSpace,
    std::{
        collections::{HashMap, HashSet},
        sync::{
            mpsc::{channel, Receiver, Sender, TryRecvError},
            Arc, Mutex,
        },
        time::Instant,
    },
};

type Nodes = HashMap<Pubkey, (bool, HashSet<i32>, Receiver<(i32, bool)>)>;

fn num_threads() -> usize {
    num_cpus::get()
}

/// Search for the a node with the given balance
fn find_insert_shred(id: &Pubkey, shred: i32, batches: &mut [Nodes]) {
    batches.par_iter_mut().for_each(|batch| {
        if batch.contains_key(id) {
            let _ = batch.get_mut(id).unwrap().1.insert(shred);
        }
    });
}

fn retransmit(
    mut shuffled_nodes: Vec<ContactInfo>,
    senders: &HashMap<Pubkey, Sender<(i32, bool)>>,
    cluster: &ClusterInfo,
    fanout: usize,
    shred: i32,
    retransmit: bool,
) -> i32 {
    let mut seed = [0; 32];
    let mut my_index = 0;
    let mut index = 0;
    shuffled_nodes.retain(|c| {
        if c.id == cluster.id() {
            my_index = index;
            false
        } else {
            index += 1;
            true
        }
    });
    seed[0..4].copy_from_slice(&shred.to_le_bytes());
    let shuffled_indices: Vec<_> = (0..shuffled_nodes.len()).collect();
    let (neighbors, children) = compute_retransmit_peers(fanout, my_index, &shuffled_indices);
    children.into_iter().for_each(|i| {
        let s = senders.get(&shuffled_nodes[i].id).unwrap();
        let _ = s.send((shred, retransmit));
    });

    if retransmit {
        neighbors.into_iter().for_each(|i| {
            let s = senders.get(&shuffled_nodes[i].id).unwrap();
            let _ = s.send((shred, false));
        });
    }

    shred
}

#[allow(clippy::type_complexity)]
fn run_simulation(stakes: &[u64], fanout: usize) {
    let num_threads = num_threads();
    // set timeout to 5 minutes
    let timeout = 60 * 5;

    // describe the leader
    let leader_info = ContactInfo::new_localhost(&solana_sdk::pubkey::new_rand(), 0);
    let cluster_info = ClusterInfo::new(
        leader_info.clone(),
        Arc::new(Keypair::new()),
        SocketAddrSpace::Unspecified,
    );

    // setup staked nodes
    let mut staked_nodes = HashMap::new();

    // setup accounts for all nodes (leader has 0 bal)
    let (s, r) = channel();
    let senders: Arc<Mutex<HashMap<Pubkey, Sender<(i32, bool)>>>> =
        Arc::new(Mutex::new(HashMap::new()));
    senders.lock().unwrap().insert(leader_info.id, s);
    let mut batches: Vec<Nodes> = Vec::with_capacity(num_threads);
    (0..num_threads).for_each(|_| batches.push(HashMap::new()));
    batches
        .get_mut(0)
        .unwrap()
        .insert(leader_info.id, (false, HashSet::new(), r));
    let range: Vec<_> = (1..=stakes.len()).collect();
    let chunk_size = (stakes.len() + num_threads - 1) / num_threads;
    range.chunks(chunk_size).for_each(|chunk| {
        chunk.iter().for_each(|i| {
            //distribute neighbors across threads to maximize parallel compute
            let batch_ix = *i as usize % batches.len();
            let node = ContactInfo::new_localhost(&solana_sdk::pubkey::new_rand(), 0);
            staked_nodes.insert(node.id, stakes[*i - 1]);
            cluster_info.insert_info(node.clone());
            let (s, r) = channel();
            batches
                .get_mut(batch_ix)
                .unwrap()
                .insert(node.id, (false, HashSet::new(), r));
            senders.lock().unwrap().insert(node.id, s);
        })
    });
    let c_info = cluster_info.clone_with_id(&cluster_info.id());

    let shreds_len = 100;
    let shuffled_peers: Vec<Vec<ContactInfo>> = (0..shreds_len as i32)
        .map(|i| {
            let mut seed = [0; 32];
            seed[0..4].copy_from_slice(&i.to_le_bytes());
            // TODO: Ideally these should use the new methods in
            // solana_core::cluster_nodes, however that would add build
            // dependency on solana_core which is not desired.
            let (peers, stakes_and_index) =
                sorted_retransmit_peers_and_stakes(&cluster_info, Some(&staked_nodes));
            let (_, shuffled_stakes_and_indexes) =
                shuffle_peers_and_index(&cluster_info.id(), &peers, &stakes_and_index, seed);
            shuffled_stakes_and_indexes
                .into_iter()
                .map(|(_, i)| peers[i].clone())
                .collect()
        })
        .collect();

    // create some "shreds".
    (0..shreds_len).for_each(|i| {
        let broadcast_table = &shuffled_peers[i];
        find_insert_shred(&broadcast_table[0].id, i as i32, &mut batches);
    });

    assert!(!batches.is_empty());

    // start turbine simulation
    let now = Instant::now();
    batches.par_iter_mut().for_each(|batch| {
        let mut remaining = batch.len();
        let senders: HashMap<_, _> = senders.lock().unwrap().clone();
        while remaining > 0 {
            for (id, (layer1_done, recv, r)) in batch.iter_mut() {
                assert!(
                    now.elapsed().as_secs() < timeout,
                    "Timed out with {:?} remaining nodes",
                    remaining
                );
                let cluster = c_info.clone_with_id(id);
                if !*layer1_done {
                    recv.iter().for_each(|i| {
                        retransmit(
                            shuffled_peers[*i as usize].clone(),
                            &senders,
                            &cluster,
                            fanout,
                            *i,
                            true,
                        );
                    });
                    *layer1_done = true;
                }

                //send and recv
                if recv.len() < shreds_len {
                    loop {
                        match r.try_recv() {
                            Ok((data, retx)) => {
                                if recv.insert(data) {
                                    let _ = retransmit(
                                        shuffled_peers[data as usize].clone(),
                                        &senders,
                                        &cluster,
                                        fanout,
                                        data,
                                        retx,
                                    );
                                }
                                if recv.len() == shreds_len {
                                    remaining -= 1;
                                    break;
                                }
                            }
                            Err(TryRecvError::Disconnected) => break,
                            Err(TryRecvError::Empty) => break,
                        };
                    }
                }
            }
        }
    });
}

// Recommended to not run these tests in parallel (they are resource heavy and want all the compute)

//todo add tests with network failures

// Run with a single layer
#[test]
#[serial]
fn test_retransmit_small() {
    let stakes: Vec<_> = (0..200).collect();
    run_simulation(&stakes, 200);
}

// Make sure at least 2 layers are used
#[test]
#[serial]
fn test_retransmit_medium() {
    let num_nodes = 2000;
    let stakes: Vec<_> = (0..num_nodes).collect();
    run_simulation(&stakes, 200);
}

// Make sure at least 2 layers are used but with equal stakes
#[test]
#[serial]
fn test_retransmit_medium_equal_stakes() {
    let num_nodes = 2000;
    let stakes: Vec<_> = (0..num_nodes).map(|_| 10).collect();
    run_simulation(&stakes, 200);
}

// Scale down the network and make sure many layers are used
#[test]
#[serial]
fn test_retransmit_large() {
    let num_nodes = 4000;
    let stakes: Vec<_> = (0..num_nodes).collect();
    run_simulation(&stakes, 2);
}
