//! `window_service` handles the data plane incoming blobs, storing them in
//!   blocktree and retransmitting where required
//!
use crate::blocktree::Blocktree;
use crate::cluster_info::ClusterInfo;
use crate::leader_schedule_cache::LeaderScheduleCache;
use crate::packet::{Blob, SharedBlob};
use crate::repair_service::{RepairService, RepairStrategy};
use crate::result::{Error, Result};
use crate::service::Service;
use crate::streamer::{BlobReceiver, BlobSender};
use rayon::prelude::*;
use rayon::ThreadPool;
use solana_metrics::{inc_new_counter_debug, inc_new_counter_error};
use solana_runtime::bank::Bank;
use solana_sdk::pubkey::Pubkey;
use solana_sdk::signature::Signable;
use solana_sdk::timing::duration_as_ms;
use std::net::UdpSocket;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::RecvTimeoutError;
use std::sync::{Arc, RwLock};
use std::thread::{self, Builder, JoinHandle};
use std::time::{Duration, Instant};

pub const NUM_THREADS: u32 = 10;

fn retransmit_blobs(blobs: &[SharedBlob], retransmit: &BlobSender, id: &Pubkey) -> Result<()> {
    let mut retransmit_queue: Vec<SharedBlob> = Vec::new();
    for blob in blobs {
        let mut blob_guard = blob.write().unwrap();
        // Don't add blobs generated by this node to the retransmit queue
        if blob_guard.id() != *id && !blob_guard.is_coding() {
            //let mut w_blob = blob.write().unwrap();
            blob_guard.meta.forward = blob_guard.should_forward();
            blob_guard.set_forwarded(false);
            retransmit_queue.push(blob.clone());
        }
    }

    if !retransmit_queue.is_empty() {
        inc_new_counter_debug!(
            "streamer-recv_window-retransmit",
            retransmit_queue.len(),
            0,
            1000
        );
        retransmit.send(retransmit_queue)?;
    }
    Ok(())
}

/// Process a blob: Add blob to the ledger window.
pub fn process_blobs(blobs: &[SharedBlob], blocktree: &Arc<Blocktree>) -> Result<()> {
    // make an iterator for insert_data_blobs()
    //let blobs: Vec<_> = blobs.iter().map(move |blob| blob.read().unwrap()).collect();

    blocktree.write_shared_blobs(
        blobs
            .iter()
            .filter(|blob| !blob.read().unwrap().is_coding()),
    )?;

    blocktree
        .put_shared_coding_blobs(blobs.iter().filter(|blob| blob.read().unwrap().is_coding()))?;

    Ok(())
}

/// drop blobs that are from myself or not from the correct leader for the
/// blob's slot
pub fn should_retransmit_and_persist(
    blob: &Blob,
    bank: Option<Arc<Bank>>,
    leader_schedule_cache: &Arc<LeaderScheduleCache>,
    my_pubkey: &Pubkey,
) -> bool {
    let slot_leader_pubkey = match bank {
        None => leader_schedule_cache.slot_leader_at(blob.slot(), None),
        Some(bank) => leader_schedule_cache.slot_leader_at(blob.slot(), Some(&bank)),
    };

    if !blob.verify() {
        inc_new_counter_debug!("streamer-recv_window-invalid_signature", 1);
        false
    } else if blob.id() == *my_pubkey {
        inc_new_counter_debug!("streamer-recv_window-circular_transmission", 1);
        false
    } else if slot_leader_pubkey == None {
        inc_new_counter_debug!("streamer-recv_window-unknown_leader", 1);
        false
    } else if slot_leader_pubkey != Some(blob.id()) {
        inc_new_counter_debug!("streamer-recv_window-wrong_leader", 1);
        false
    } else {
        // At this point, slot_leader_id == blob.id() && blob.id() != *my_id, so
        // the blob is valid to process
        true
    }
}

fn recv_window<F>(
    blocktree: &Arc<Blocktree>,
    my_pubkey: &Pubkey,
    r: &BlobReceiver,
    retransmit: &BlobSender,
    blob_filter: F,
    thread_pool: &ThreadPool,
) -> Result<()>
where
    F: Fn(&Blob) -> bool,
    F: Sync,
{
    let timer = Duration::from_millis(200);
    let mut blobs = r.recv_timeout(timer)?;

    while let Ok(mut blob) = r.try_recv() {
        blobs.append(&mut blob)
    }
    let now = Instant::now();
    inc_new_counter_debug!("streamer-recv_window-recv", blobs.len(), 0, 1000);

    let blobs: Vec<_> = thread_pool.install(|| {
        blobs
            .into_par_iter()
            .filter(|b| blob_filter(&b.read().unwrap()))
            .collect()
    });

    match retransmit_blobs(&blobs, retransmit, my_pubkey) {
        Ok(_) => Ok(()),
        Err(Error::SendError) => Ok(()),
        Err(e) => Err(e),
    }?;

    trace!("{} num blobs received: {}", my_pubkey, blobs.len());

    process_blobs(&blobs, blocktree)?;

    trace!(
        "Elapsed processing time in recv_window(): {}",
        duration_as_ms(&now.elapsed())
    );

    Ok(())
}

// Implement a destructor for the window_service thread to signal it exited
// even on panics
struct Finalizer {
    exit_sender: Arc<AtomicBool>,
}

impl Finalizer {
    fn new(exit_sender: Arc<AtomicBool>) -> Self {
        Finalizer { exit_sender }
    }
}
// Implement a destructor for Finalizer.
impl Drop for Finalizer {
    fn drop(&mut self) {
        self.exit_sender.clone().store(true, Ordering::Relaxed);
    }
}

pub struct WindowService {
    t_window: JoinHandle<()>,
    repair_service: RepairService,
}

impl WindowService {
    #[allow(clippy::too_many_arguments)]
    pub fn new<F>(
        blocktree: Arc<Blocktree>,
        cluster_info: Arc<RwLock<ClusterInfo>>,
        r: BlobReceiver,
        retransmit: BlobSender,
        repair_socket: Arc<UdpSocket>,
        exit: &Arc<AtomicBool>,
        repair_strategy: RepairStrategy,
        blob_filter: F,
    ) -> WindowService
    where
        F: 'static
            + Fn(&Pubkey, &Blob, Option<Arc<Bank>>) -> bool
            + std::marker::Send
            + std::marker::Sync,
    {
        let bank_forks = match repair_strategy {
            RepairStrategy::RepairRange(_) => None,

            RepairStrategy::RepairAll { ref bank_forks, .. } => Some(bank_forks.clone()),
        };

        let repair_service = RepairService::new(
            blocktree.clone(),
            exit.clone(),
            repair_socket,
            cluster_info.clone(),
            repair_strategy,
        );
        let exit = exit.clone();
        let blob_filter = Arc::new(blob_filter);
        let bank_forks = bank_forks.clone();
        let t_window = Builder::new()
            .name("solana-window".to_string())
            // TODO: Mark: Why is it overflowing
            .stack_size(8 * 1024 * 1024)
            .spawn(move || {
                let _exit = Finalizer::new(exit.clone());
                let id = cluster_info.read().unwrap().id();
                trace!("{}: RECV_WINDOW started", id);
                let thread_pool = rayon::ThreadPoolBuilder::new()
                    .num_threads(sys_info::cpu_num().unwrap_or(NUM_THREADS) as usize)
                    .build()
                    .unwrap();
                loop {
                    if exit.load(Ordering::Relaxed) {
                        break;
                    }

                    if let Err(e) = recv_window(
                        &blocktree,
                        &id,
                        &r,
                        &retransmit,
                        |blob| {
                            blob_filter(
                                &id,
                                blob,
                                bank_forks
                                    .as_ref()
                                    .map(|bank_forks| bank_forks.read().unwrap().working_bank()),
                            )
                        },
                        &thread_pool,
                    ) {
                        match e {
                            Error::RecvTimeoutError(RecvTimeoutError::Disconnected) => break,
                            Error::RecvTimeoutError(RecvTimeoutError::Timeout) => (),
                            _ => {
                                inc_new_counter_error!("streamer-window-error", 1, 1);
                                error!("window error: {:?}", e);
                            }
                        }
                    }
                }
            })
            .unwrap();

        WindowService {
            t_window,
            repair_service,
        }
    }
}

impl Service for WindowService {
    type JoinReturnType = ();

    fn join(self) -> thread::Result<()> {
        self.t_window.join()?;
        self.repair_service.join()
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::bank_forks::BankForks;
    use crate::blocktree::{get_tmp_ledger_path, Blocktree};
    use crate::cluster_info::{ClusterInfo, Node};
    use crate::entry::{make_consecutive_blobs, make_tiny_test_entries, Entry, EntrySlice};
    use crate::erasure::ErasureConfig;
    use crate::genesis_utils::create_genesis_block_with_leader;
    use crate::packet::index_blobs;
    use crate::service::Service;
    use crate::streamer::{blob_receiver, responder};
    use solana_runtime::epoch_schedule::MINIMUM_SLOTS_PER_EPOCH;
    use solana_sdk::hash::Hash;
    use solana_sdk::signature::{Keypair, KeypairUtil};
    use std::fs::remove_dir_all;
    use std::net::UdpSocket;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::mpsc::channel;
    use std::sync::{Arc, RwLock};
    use std::time::Duration;

    #[test]
    fn test_process_blob() {
        let blocktree_path = get_tmp_ledger_path!();
        let blocktree = Arc::new(Blocktree::open(&blocktree_path).unwrap());
        let num_entries = 10;
        let original_entries = make_tiny_test_entries(num_entries);
        let shared_blobs = original_entries.clone().to_shared_blobs();

        index_blobs(&shared_blobs, &Pubkey::new_rand(), 0, 0, 0);

        for blob in shared_blobs.into_iter().rev() {
            process_blobs(&[blob], &blocktree).expect("Expect successful processing of blob");
        }

        assert_eq!(
            blocktree.get_slot_entries(0, 0, None).unwrap(),
            original_entries
        );

        drop(blocktree);
        Blocktree::destroy(&blocktree_path).expect("Expected successful database destruction");
    }

    #[test]
    fn test_should_retransmit_and_persist() {
        let me_id = Pubkey::new_rand();
        let leader_keypair = Keypair::new();
        let leader_pubkey = leader_keypair.pubkey();
        let bank = Arc::new(Bank::new(
            &create_genesis_block_with_leader(100, &leader_pubkey, 10).genesis_block,
        ));
        let cache = Arc::new(LeaderScheduleCache::new_from_bank(&bank));

        let entry = Entry::default();
        let mut blob = entry.to_blob();
        blob.set_id(&leader_pubkey);
        blob.sign(&leader_keypair);

        // with a Bank for slot 0, blob continues
        assert_eq!(
            should_retransmit_and_persist(&blob, Some(bank.clone()), &cache, &me_id),
            true
        );

        // set the blob to have come from the wrong leader
        blob.set_id(&Pubkey::new_rand());
        assert_eq!(
            should_retransmit_and_persist(&blob, Some(bank.clone()), &cache, &me_id),
            false
        );

        // with a Bank and no idea who leader is, blob gets thrown out
        blob.set_slot(MINIMUM_SLOTS_PER_EPOCH as u64 * 3);
        assert_eq!(
            should_retransmit_and_persist(&blob, Some(bank), &cache, &me_id),
            false
        );

        // if the blob came back from me, it doesn't continue, whether or not I have a bank
        blob.set_id(&me_id);
        assert_eq!(
            should_retransmit_and_persist(&blob, None, &cache, &me_id),
            false
        );
    }

    #[test]
    pub fn window_send_test() {
        solana_logger::setup();
        // setup a leader whose id is used to generates blobs and a validator
        // node whose window service will retransmit leader blobs.
        let leader_node = Node::new_localhost();
        let validator_node = Node::new_localhost();
        let exit = Arc::new(AtomicBool::new(false));
        let cluster_info_me = ClusterInfo::new_with_invalid_keypair(validator_node.info.clone());
        let me_id = leader_node.info.id;
        let subs = Arc::new(RwLock::new(cluster_info_me));

        let (s_reader, r_reader) = channel();
        let t_receiver = blob_receiver(Arc::new(leader_node.sockets.gossip), &exit, s_reader);
        let (s_retransmit, r_retransmit) = channel();
        let blocktree_path = get_tmp_ledger_path!();
        let (blocktree, _, completed_slots_receiver) =
            Blocktree::open_with_signal(&blocktree_path, &ErasureConfig::default())
                .expect("Expected to be able to open database ledger");
        let blocktree = Arc::new(blocktree);

        let bank = Bank::new(&create_genesis_block_with_leader(100, &me_id, 10).genesis_block);
        let bank_forks = Arc::new(RwLock::new(BankForks::new(0, bank)));
        let repair_strategy = RepairStrategy::RepairAll {
            bank_forks: bank_forks.clone(),
            completed_slots_receiver,
            epoch_schedule: bank_forks
                .read()
                .unwrap()
                .working_bank()
                .epoch_schedule()
                .clone(),
        };
        let t_window = WindowService::new(
            blocktree,
            subs,
            r_reader,
            s_retransmit,
            Arc::new(leader_node.sockets.repair),
            &exit,
            repair_strategy,
            |_, _, _| true,
        );
        let t_responder = {
            let (s_responder, r_responder) = channel();
            let blob_sockets: Vec<Arc<UdpSocket>> =
                leader_node.sockets.tvu.into_iter().map(Arc::new).collect();

            let t_responder = responder("window_send_test", blob_sockets[0].clone(), r_responder);
            let num_blobs_to_make = 10;
            let gossip_address = &leader_node.info.gossip;
            let msgs = make_consecutive_blobs(
                &me_id,
                num_blobs_to_make,
                0,
                Hash::default(),
                &gossip_address,
            )
            .into_iter()
            .rev()
            .collect();;
            s_responder.send(msgs).expect("send");
            t_responder
        };

        let max_attempts = 10;
        let mut num_attempts = 0;
        let mut q = Vec::new();
        loop {
            assert!(num_attempts != max_attempts);
            while let Ok(mut nq) = r_retransmit.recv_timeout(Duration::from_millis(500)) {
                q.append(&mut nq);
            }
            if q.len() == 10 {
                break;
            }
            num_attempts += 1;
        }

        exit.store(true, Ordering::Relaxed);
        t_receiver.join().expect("join");
        t_responder.join().expect("join");
        t_window.join().expect("join");
        Blocktree::destroy(&blocktree_path).expect("Expected successful database destruction");
        let _ignored = remove_dir_all(&blocktree_path);
    }

    #[test]
    pub fn window_send_leader_test2() {
        solana_logger::setup();
        // setup a leader whose id is used to generates blobs and a validator
        // node whose window service will retransmit leader blobs.
        let leader_node = Node::new_localhost();
        let validator_node = Node::new_localhost();
        let exit = Arc::new(AtomicBool::new(false));
        let cluster_info_me = ClusterInfo::new_with_invalid_keypair(validator_node.info.clone());
        let me_id = leader_node.info.id;
        let subs = Arc::new(RwLock::new(cluster_info_me));

        let (s_reader, r_reader) = channel();
        let t_receiver = blob_receiver(Arc::new(leader_node.sockets.gossip), &exit, s_reader);
        let (s_retransmit, r_retransmit) = channel();
        let blocktree_path = get_tmp_ledger_path!();
        let (blocktree, _, completed_slots_receiver) =
            Blocktree::open_with_signal(&blocktree_path, &ErasureConfig::default())
                .expect("Expected to be able to open database ledger");

        let blocktree = Arc::new(blocktree);
        let bank = Bank::new(&create_genesis_block_with_leader(100, &me_id, 10).genesis_block);
        let bank_forks = Arc::new(RwLock::new(BankForks::new(0, bank)));
        let epoch_schedule = *bank_forks.read().unwrap().working_bank().epoch_schedule();
        let repair_strategy = RepairStrategy::RepairAll {
            bank_forks,
            completed_slots_receiver,
            epoch_schedule,
        };
        let t_window = WindowService::new(
            blocktree,
            subs.clone(),
            r_reader,
            s_retransmit,
            Arc::new(leader_node.sockets.repair),
            &exit,
            repair_strategy,
            |_, _, _| true,
        );
        let t_responder = {
            let (s_responder, r_responder) = channel();
            let blob_sockets: Vec<Arc<UdpSocket>> =
                leader_node.sockets.tvu.into_iter().map(Arc::new).collect();
            let t_responder = responder("window_send_test", blob_sockets[0].clone(), r_responder);
            let mut msgs = Vec::new();
            let blobs =
                make_consecutive_blobs(&me_id, 14u64, 0, Hash::default(), &leader_node.info.gossip);

            for v in 0..10 {
                let i = 9 - v;
                msgs.push(blobs[i].clone());
            }
            s_responder.send(msgs).expect("send");

            let mut msgs1 = Vec::new();
            for v in 1..5 {
                let i = 9 + v;
                msgs1.push(blobs[i].clone());
            }
            s_responder.send(msgs1).expect("send");
            t_responder
        };
        let mut q = Vec::new();
        while let Ok(mut nq) = r_retransmit.recv_timeout(Duration::from_millis(500)) {
            q.append(&mut nq);
        }
        assert!(q.len() > 10);
        exit.store(true, Ordering::Relaxed);
        t_receiver.join().expect("join");
        t_responder.join().expect("join");
        t_window.join().expect("join");
        Blocktree::destroy(&blocktree_path).expect("Expected successful database destruction");
        let _ignored = remove_dir_all(&blocktree_path);
    }
}
