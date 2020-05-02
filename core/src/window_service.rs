//! `window_service` handles the data plane incoming shreds, storing them in
//!   blockstore and retransmitting where required
//!
use crate::{
    cluster_info::ClusterInfo,
    cluster_slots::ClusterSlots,
    outstanding_requests::OutstandingRequests,
    repair_service::{RepairService, RepairStrategy},
    result::{Error, Result},
};
use crossbeam_channel::{
    unbounded, Receiver as CrossbeamReceiver, RecvTimeoutError, Sender as CrossbeamSender,
};
use rayon::iter::IntoParallelRefMutIterator;
use rayon::iter::ParallelIterator;
use rayon::ThreadPool;
use solana_ledger::bank_forks::BankForks;
use solana_ledger::blockstore::{
    self, Blockstore, BlockstoreInsertionMetrics, MAX_DATA_SHREDS_PER_SLOT,
};
use solana_ledger::leader_schedule_cache::LeaderScheduleCache;
use solana_ledger::shred::Shred;
use solana_metrics::{inc_new_counter_debug, inc_new_counter_error};
use solana_perf::packet::Packets;
use solana_rayon_threadlimit::get_thread_count;
use solana_runtime::bank::Bank;
use solana_sdk::pubkey::Pubkey;
use solana_sdk::timing::duration_as_ms;
use solana_streamer::streamer::PacketSender;
use std::net::UdpSocket;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};
use std::thread::{self, Builder, JoinHandle};
use std::time::{Duration, Instant};

fn verify_shred_slot(shred: &Shred, root: u64) -> bool {
    if shred.is_data() {
        // Only data shreds have parent information
        blockstore::verify_shred_slots(shred.slot(), shred.parent(), root)
    } else {
        // Filter out outdated coding shreds
        shred.slot() >= root
    }
}

/// drop shreds that are from myself or not from the correct leader for the
/// shred's slot
pub fn should_retransmit_and_persist(
    shred: &Shred,
    bank: Option<Arc<Bank>>,
    leader_schedule_cache: &Arc<LeaderScheduleCache>,
    my_pubkey: &Pubkey,
    root: u64,
    shred_version: u16,
) -> bool {
    let slot_leader_pubkey = match bank {
        None => leader_schedule_cache.slot_leader_at(shred.slot(), None),
        Some(bank) => leader_schedule_cache.slot_leader_at(shred.slot(), Some(&bank)),
    };
    if let Some(leader_id) = slot_leader_pubkey {
        if leader_id == *my_pubkey {
            inc_new_counter_debug!("streamer-recv_window-circular_transmission", 1);
            false
        } else if !verify_shred_slot(shred, root) {
            inc_new_counter_debug!("streamer-recv_window-outdated_transmission", 1);
            false
        } else if shred.version() != shred_version {
            inc_new_counter_debug!("streamer-recv_window-incorrect_shred_version", 1);
            false
        } else if shred.index() >= MAX_DATA_SHREDS_PER_SLOT as u32 {
            inc_new_counter_warn!("streamer-recv_window-shred_index_overrun", 1);
            false
        } else {
            true
        }
    } else {
        inc_new_counter_debug!("streamer-recv_window-unknown_leader", 1);
        false
    }
}

fn run_check_duplicate(
    blockstore: &Arc<Blockstore>,
    shred_receiver: &CrossbeamReceiver<Shred>,
) -> Result<()> {
    let check_duplicate = |shred: Shred| -> Result<()> {
        if !blockstore.has_duplicate_shreds_in_slot(shred.slot()) {
            if let Some(existing_shred_payload) =
                blockstore.is_shred_duplicate(shred.slot(), shred.index(), &shred.payload)
            {
                blockstore.store_duplicate_slot(
                    shred.slot(),
                    existing_shred_payload,
                    shred.payload,
                )?;
            }
        }

        Ok(())
    };
    let timer = Duration::from_millis(200);
    let shred = shred_receiver.recv_timeout(timer)?;
    check_duplicate(shred)?;
    while let Ok(shred) = shred_receiver.try_recv() {
        check_duplicate(shred)?;
    }

    Ok(())
}

fn run_insert<F>(
    shred_receiver: &CrossbeamReceiver<Vec<Shred>>,
    blockstore: &Arc<Blockstore>,
    leader_schedule_cache: &Arc<LeaderScheduleCache>,
    handle_duplicate: F,
    metrics: &mut BlockstoreInsertionMetrics,
) -> Result<()>
where
    F: Fn(Shred) -> (),
{
    let timer = Duration::from_millis(200);
    let mut shreds = shred_receiver.recv_timeout(timer)?;

    while let Ok(mut more_shreds) = shred_receiver.try_recv() {
        shreds.append(&mut more_shreds)
    }

    blockstore.insert_shreds_handle_duplicate(
        shreds,
        Some(leader_schedule_cache),
        false,
        &handle_duplicate,
        metrics,
    )?;
    Ok(())
}

fn recv_window<F>(
    blockstore: &Arc<Blockstore>,
    insert_shred_sender: &CrossbeamSender<Vec<Shred>>,
    my_pubkey: &Pubkey,
    verified_receiver: &CrossbeamReceiver<Vec<Packets>>,
    retransmit: &PacketSender,
    shred_filter: F,
    thread_pool: &ThreadPool,
) -> Result<()>
where
    F: Fn(&Shred, u64) -> bool + Sync,
{
    let timer = Duration::from_millis(200);
    let mut packets = verified_receiver.recv_timeout(timer)?;
    let mut total_packets: usize = packets.iter().map(|p| p.packets.len()).sum();

    while let Ok(mut more_packets) = verified_receiver.try_recv() {
        let count: usize = more_packets.iter().map(|p| p.packets.len()).sum();
        total_packets += count;
        packets.append(&mut more_packets)
    }

    let now = Instant::now();
    inc_new_counter_debug!("streamer-recv_window-recv", total_packets);

    let last_root = blockstore.last_root();
    let shreds: Vec<_> = thread_pool.install(|| {
        packets
            .par_iter_mut()
            .flat_map(|packets| {
                packets
                    .packets
                    .iter_mut()
                    .filter_map(|packet| {
                        if packet.meta.discard {
                            inc_new_counter_debug!(
                                "streamer-recv_window-invalid_or_unnecessary_packet",
                                1
                            );
                            None
                        } else if let Ok(shred) =
                            Shred::new_from_serialized_shred(packet.data.to_vec())
                        {
                            if shred_filter(&shred, last_root) {
                                // Mark slot as dead if the current shred is on the boundary
                                // of max shreds per slot. However, let the current shred
                                // get retransmitted. It'll allow peer nodes to see this shred
                                // and trigger them to mark the slot as dead.
                                if shred.index() >= (MAX_DATA_SHREDS_PER_SLOT - 1) as u32 {
                                    let _ = blockstore.set_dead_slot(shred.slot());
                                }
                                packet.meta.slot = shred.slot();
                                packet.meta.seed = shred.seed();
                                Some(shred)
                            } else {
                                packet.meta.discard = true;
                                None
                            }
                        } else {
                            packet.meta.discard = true;
                            None
                        }
                    })
                    .collect::<Vec<_>>()
            })
            .collect()
    });

    trace!("{:?} shreds from packets", shreds.len());

    trace!("{} num total shreds received: {}", my_pubkey, total_packets);

    for packets in packets.into_iter() {
        if !packets.is_empty() {
            // Ignore the send error, as the retransmit is optional (e.g. archivers don't retransmit)
            let _ = retransmit.send(packets);
        }
    }

    insert_shred_sender.send(shreds)?;

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
    t_insert: JoinHandle<()>,
    t_check_duplicate: JoinHandle<()>,
    repair_service: RepairService,
}

impl WindowService {
    #[allow(clippy::too_many_arguments)]
    pub fn new<F>(
        blockstore: Arc<Blockstore>,
        cluster_info: Arc<ClusterInfo>,
        verified_receiver: CrossbeamReceiver<Vec<Packets>>,
        retransmit: PacketSender,
        repair_socket: Arc<UdpSocket>,
        exit: &Arc<AtomicBool>,
        repair_strategy: RepairStrategy,
        leader_schedule_cache: &Arc<LeaderScheduleCache>,
        shred_filter: F,
        cluster_slots: Arc<ClusterSlots>,
        outstanding_requests: Arc<RwLock<OutstandingRequests>>,
    ) -> WindowService
    where
        F: 'static
            + Fn(&Pubkey, &Shred, Option<Arc<Bank>>, u64) -> bool
            + std::marker::Send
            + std::marker::Sync,
    {
        let bank_forks = match repair_strategy {
            RepairStrategy::RepairRange(_) => None,
            RepairStrategy::RepairAll { ref bank_forks, .. } => Some(bank_forks.clone()),
        };

        let repair_service = RepairService::new(
            blockstore.clone(),
            exit.clone(),
            repair_socket,
            cluster_info.clone(),
            repair_strategy,
            cluster_slots,
            outstanding_requests,
        );

        let (insert_sender, insert_receiver) = unbounded();
        let (duplicate_sender, duplicate_receiver) = unbounded();

        let t_check_duplicate =
            Self::start_check_duplicate_thread(exit, &blockstore, duplicate_receiver);

        let t_insert = Self::start_window_insert_thread(
            exit,
            &blockstore,
            leader_schedule_cache,
            insert_receiver,
            duplicate_sender,
        );

        let t_window = Self::start_recv_window_thread(
            cluster_info.id(),
            exit,
            &blockstore,
            insert_sender,
            verified_receiver,
            shred_filter,
            bank_forks,
            retransmit,
        );

        WindowService {
            t_window,
            t_insert,
            t_check_duplicate,
            repair_service,
        }
    }

    fn start_check_duplicate_thread(
        exit: &Arc<AtomicBool>,
        blockstore: &Arc<Blockstore>,
        duplicate_receiver: CrossbeamReceiver<Shred>,
    ) -> JoinHandle<()> {
        let exit = exit.clone();
        let blockstore = blockstore.clone();
        let handle_error = || {
            inc_new_counter_error!("solana-check-duplicate-error", 1, 1);
        };
        Builder::new()
            .name("solana-check-duplicate".to_string())
            .spawn(move || loop {
                if exit.load(Ordering::Relaxed) {
                    break;
                }

                let mut noop = || {};
                if let Err(e) = run_check_duplicate(&blockstore, &duplicate_receiver) {
                    if Self::should_exit_on_error(e, &mut noop, &handle_error) {
                        break;
                    }
                }
            })
            .unwrap()
    }

    fn start_window_insert_thread(
        exit: &Arc<AtomicBool>,
        blockstore: &Arc<Blockstore>,
        leader_schedule_cache: &Arc<LeaderScheduleCache>,
        insert_receiver: CrossbeamReceiver<Vec<Shred>>,
        duplicate_sender: CrossbeamSender<Shred>,
    ) -> JoinHandle<()> {
        let exit = exit.clone();
        let blockstore = blockstore.clone();
        let leader_schedule_cache = leader_schedule_cache.clone();
        let mut handle_timeout = || {};
        let handle_error = || {
            inc_new_counter_error!("solana-window-insert-error", 1, 1);
        };

        Builder::new()
            .name("solana-window-insert".to_string())
            .spawn(move || {
                let handle_duplicate = |shred| {
                    let _ = duplicate_sender.send(shred);
                };
                let mut metrics = BlockstoreInsertionMetrics::default();
                let mut last_print = Instant::now();
                loop {
                    if exit.load(Ordering::Relaxed) {
                        break;
                    }

                    if let Err(e) = run_insert(
                        &insert_receiver,
                        &blockstore,
                        &leader_schedule_cache,
                        &handle_duplicate,
                        &mut metrics,
                    ) {
                        if Self::should_exit_on_error(e, &mut handle_timeout, &handle_error) {
                            break;
                        }
                    }

                    if last_print.elapsed().as_secs() > 2 {
                        metrics.report_metrics("recv-window-insert-shreds");
                        metrics = BlockstoreInsertionMetrics::default();
                        last_print = Instant::now();
                    }
                }
            })
            .unwrap()
    }

    fn start_recv_window_thread<F>(
        id: Pubkey,
        exit: &Arc<AtomicBool>,
        blockstore: &Arc<Blockstore>,
        insert_sender: CrossbeamSender<Vec<Shred>>,
        verified_receiver: CrossbeamReceiver<Vec<Packets>>,
        shred_filter: F,
        bank_forks: Option<Arc<RwLock<BankForks>>>,
        retransmit: PacketSender,
    ) -> JoinHandle<()>
    where
        F: 'static
            + Fn(&Pubkey, &Shred, Option<Arc<Bank>>, u64) -> bool
            + std::marker::Send
            + std::marker::Sync,
    {
        let exit = exit.clone();
        let blockstore = blockstore.clone();
        Builder::new()
            .name("solana-window".to_string())
            .spawn(move || {
                let _exit = Finalizer::new(exit.clone());
                trace!("{}: RECV_WINDOW started", id);
                let thread_pool = rayon::ThreadPoolBuilder::new()
                    .num_threads(get_thread_count())
                    .build()
                    .unwrap();
                let mut now = Instant::now();
                let handle_error = || {
                    inc_new_counter_error!("solana-window-error", 1, 1);
                };

                loop {
                    if exit.load(Ordering::Relaxed) {
                        break;
                    }

                    let mut handle_timeout = || {
                        if now.elapsed() > Duration::from_secs(30) {
                            warn!("Window does not seem to be receiving data. Ensure port configuration is correct...");
                            now = Instant::now();
                        }
                    };
                    if let Err(e) = recv_window(
                        &blockstore,
                        &insert_sender,
                        &id,
                        &verified_receiver,
                        &retransmit,
                        |shred, last_root| {
                            shred_filter(
                                &id,
                                shred,
                                bank_forks
                                    .as_ref()
                                    .map(|bank_forks| bank_forks.read().unwrap().working_bank()),
                                last_root,
                            )
                        },
                        &thread_pool,
                    ) {
                        if Self::should_exit_on_error(e, &mut handle_timeout, &handle_error) {
                            break;
                        }
                    } else {
                        now = Instant::now();
                    }
                }
            })
            .unwrap()
    }

    fn should_exit_on_error<F, H>(e: Error, handle_timeout: &mut F, handle_error: &H) -> bool
    where
        F: FnMut() -> (),
        H: Fn() -> (),
    {
        match e {
            Error::CrossbeamRecvTimeoutError(RecvTimeoutError::Disconnected) => true,
            Error::CrossbeamRecvTimeoutError(RecvTimeoutError::Timeout) => {
                handle_timeout();
                false
            }
            _ => {
                handle_error();
                error!("thread {:?} error {:?}", thread::current().name(), e);
                false
            }
        }
    }

    pub fn join(self) -> thread::Result<()> {
        self.t_window.join()?;
        self.t_insert.join()?;
        self.t_check_duplicate.join()?;
        self.repair_service.join()
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::{
        cluster_info::ClusterInfo, contact_info::ContactInfo, repair_service::RepairSlotRange,
    };
    use rand::thread_rng;
    use solana_ledger::shred::DataShredHeader;
    use solana_ledger::{
        blockstore::{make_many_slot_entries, Blockstore},
        entry::{create_ticks, Entry},
        genesis_utils::create_genesis_config_with_leader,
        get_tmp_ledger_path,
        shred::Shredder,
    };
    use solana_perf::packet::Packet;
    use solana_sdk::{
        clock::Slot,
        epoch_schedule::MINIMUM_SLOTS_PER_EPOCH,
        hash::Hash,
        signature::{Keypair, Signer},
    };
    use std::{
        net::UdpSocket,
        sync::atomic::{AtomicBool, Ordering},
        sync::mpsc::channel,
        sync::Arc,
        thread::sleep,
        time::Duration,
    };

    fn local_entries_to_shred(
        entries: &[Entry],
        slot: Slot,
        parent: Slot,
        keypair: &Arc<Keypair>,
    ) -> Vec<Shred> {
        let shredder = Shredder::new(slot, parent, 0.0, keypair.clone(), 0, 0)
            .expect("Failed to create entry shredder");
        shredder.entries_to_shreds(&entries, true, 0).0
    }

    #[test]
    fn test_process_shred() {
        let blockstore_path = get_tmp_ledger_path!();
        let blockstore = Arc::new(Blockstore::open(&blockstore_path).unwrap());
        let num_entries = 10;
        let original_entries = create_ticks(num_entries, 0, Hash::default());
        let mut shreds = local_entries_to_shred(&original_entries, 0, 0, &Arc::new(Keypair::new()));
        shreds.reverse();
        blockstore
            .insert_shreds(shreds, None, false)
            .expect("Expect successful processing of shred");

        assert_eq!(blockstore.get_slot_entries(0, 0).unwrap(), original_entries);

        drop(blockstore);
        Blockstore::destroy(&blockstore_path).expect("Expected successful database destruction");
    }

    #[test]
    fn test_should_retransmit_and_persist() {
        let me_id = Pubkey::new_rand();
        let leader_keypair = Arc::new(Keypair::new());
        let leader_pubkey = leader_keypair.pubkey();
        let bank = Arc::new(Bank::new(
            &create_genesis_config_with_leader(100, &leader_pubkey, 10).genesis_config,
        ));
        let cache = Arc::new(LeaderScheduleCache::new_from_bank(&bank));

        let mut shreds = local_entries_to_shred(&[Entry::default()], 0, 0, &leader_keypair);

        // with a Bank for slot 0, shred continues
        assert_eq!(
            should_retransmit_and_persist(&shreds[0], Some(bank.clone()), &cache, &me_id, 0, 0),
            true
        );
        // with the wrong shred_version, shred gets thrown out
        assert_eq!(
            should_retransmit_and_persist(&shreds[0], Some(bank.clone()), &cache, &me_id, 0, 1),
            false
        );

        // If it's a coding shred, test that slot >= root
        let (common, coding) = Shredder::new_coding_shred_header(5, 5, 5, 6, 6, 0, 0);
        let mut coding_shred =
            Shred::new_empty_from_header(common, DataShredHeader::default(), coding);
        Shredder::sign_shred(&leader_keypair, &mut coding_shred);
        assert_eq!(
            should_retransmit_and_persist(&coding_shred, Some(bank.clone()), &cache, &me_id, 0, 0),
            true
        );
        assert_eq!(
            should_retransmit_and_persist(&coding_shred, Some(bank.clone()), &cache, &me_id, 5, 0),
            true
        );
        assert_eq!(
            should_retransmit_and_persist(&coding_shred, Some(bank.clone()), &cache, &me_id, 6, 0),
            false
        );

        // with a Bank and no idea who leader is, shred gets thrown out
        shreds[0].set_slot(MINIMUM_SLOTS_PER_EPOCH as u64 * 3);
        assert_eq!(
            should_retransmit_and_persist(&shreds[0], Some(bank.clone()), &cache, &me_id, 0, 0),
            false
        );

        // with a shred where shred.slot() == root, shred gets thrown out
        let slot = MINIMUM_SLOTS_PER_EPOCH as u64 * 3;
        let shreds = local_entries_to_shred(&[Entry::default()], slot, slot - 1, &leader_keypair);
        assert_eq!(
            should_retransmit_and_persist(&shreds[0], Some(bank.clone()), &cache, &me_id, slot, 0),
            false
        );

        // with a shred where shred.parent() < root, shred gets thrown out
        let slot = MINIMUM_SLOTS_PER_EPOCH as u64 * 3;
        let shreds =
            local_entries_to_shred(&[Entry::default()], slot + 1, slot - 1, &leader_keypair);
        assert_eq!(
            should_retransmit_and_persist(&shreds[0], Some(bank.clone()), &cache, &me_id, slot, 0),
            false
        );

        // if the shred came back from me, it doesn't continue, whether or not I have a bank
        assert_eq!(
            should_retransmit_and_persist(&shreds[0], None, &cache, &me_id, 0, 0),
            false
        );
    }

    fn make_test_window(
        verified_receiver: CrossbeamReceiver<Vec<Packets>>,
        exit: Arc<AtomicBool>,
    ) -> WindowService {
        let blockstore_path = get_tmp_ledger_path!();
        let (blockstore, _, _) = Blockstore::open_with_signal(&blockstore_path)
            .expect("Expected to be able to open database ledger");

        let blockstore = Arc::new(blockstore);
        let (retransmit_sender, _retransmit_receiver) = channel();
        let cluster_info = Arc::new(ClusterInfo::new_with_invalid_keypair(
            ContactInfo::new_localhost(&Pubkey::default(), 0),
        ));
        let cluster_slots = Arc::new(ClusterSlots::default());
        let repair_sock = Arc::new(UdpSocket::bind(socketaddr_any!()).unwrap());
        let window = WindowService::new(
            blockstore,
            cluster_info,
            verified_receiver,
            retransmit_sender,
            repair_sock,
            &exit,
            RepairStrategy::RepairRange(RepairSlotRange { start: 0, end: 0 }),
            &Arc::new(LeaderScheduleCache::default()),
            |_, _, _, _| true,
            cluster_slots,
        );
        window
    }

    #[test]
    fn test_recv_window() {
        let (packet_sender, packet_receiver) = unbounded();
        let exit = Arc::new(AtomicBool::new(false));
        let window = make_test_window(packet_receiver, exit.clone());
        // send 5 slots worth of data to the window
        let (shreds, _) = make_many_slot_entries(0, 5, 10);
        let packets: Vec<_> = shreds
            .into_iter()
            .map(|mut s| {
                let mut p = Packet::default();
                p.data.copy_from_slice(&mut s.payload);
                p
            })
            .collect();
        let mut packets = Packets::new(packets);
        packet_sender.send(vec![packets.clone()]).unwrap();
        sleep(Duration::from_millis(500));

        // add some empty packets to the data set. These should fail to deserialize
        packets.packets.append(&mut vec![Packet::default(); 10]);
        packets.packets.shuffle(&mut thread_rng());
        packet_sender.send(vec![packets.clone()]).unwrap();
        sleep(Duration::from_millis(500));

        // send 1 empty packet that cannot deserialize into a shred
        packet_sender
            .send(vec![Packets::new(vec![Packet::default(); 1])])
            .unwrap();
        sleep(Duration::from_millis(500));

        exit.store(true, Ordering::Relaxed);
        window.join().unwrap();
    }

    #[test]
    fn test_run_check_duplicate() {
        let blockstore_path = get_tmp_ledger_path!();
        let blockstore = Arc::new(Blockstore::open(&blockstore_path).unwrap());
        let (sender, receiver) = unbounded();
        let (shreds, _) = make_many_slot_entries(5, 5, 10);
        blockstore
            .insert_shreds(shreds.clone(), None, false)
            .unwrap();
        let mut duplicate_shred = shreds[1].clone();
        duplicate_shred.set_slot(shreds[0].slot());
        let duplicate_shred_slot = duplicate_shred.slot();
        sender.send(duplicate_shred).unwrap();
        assert!(!blockstore.has_duplicate_shreds_in_slot(duplicate_shred_slot));
        run_check_duplicate(&blockstore, &receiver).unwrap();
        assert!(blockstore.has_duplicate_shreds_in_slot(duplicate_shred_slot));
    }
}
