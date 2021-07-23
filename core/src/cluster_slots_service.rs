use crate::cluster_slots::ClusterSlots;
use crossbeam_channel::{Receiver, RecvTimeoutError, Sender};
use solana_gossip::cluster_info::ClusterInfo;
use solana_ledger::blockstore::Blockstore;
use solana_measure::measure::Measure;
use solana_runtime::bank_forks::BankForks;
use solana_sdk::clock::Slot;
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        {Arc, RwLock},
    },
    thread::{self, Builder, JoinHandle},
    time::{Duration, Instant},
};

pub type ClusterSlotsUpdateReceiver = Receiver<Vec<Slot>>;
pub type ClusterSlotsUpdateSender = Sender<Vec<Slot>>;

#[derive(Default, Debug)]
struct ClusterSlotsServiceTiming {
    pub lowest_slot_elapsed: u64,
    pub process_cluster_slots_updates_elapsed: u64,
}

impl ClusterSlotsServiceTiming {
    fn update(&mut self, lowest_slot_elapsed: u64, process_cluster_slots_updates_elapsed: u64) {
        self.lowest_slot_elapsed += lowest_slot_elapsed;
        self.process_cluster_slots_updates_elapsed += process_cluster_slots_updates_elapsed;
    }
}

pub struct ClusterSlotsService {
    t_cluster_slots_service: JoinHandle<()>,
}

impl ClusterSlotsService {
    pub fn new(
        blockstore: Arc<Blockstore>,
        cluster_slots: Arc<ClusterSlots>,
        bank_forks: Arc<RwLock<BankForks>>,
        cluster_info: Arc<ClusterInfo>,
        cluster_slots_update_receiver: ClusterSlotsUpdateReceiver,
        exit: Arc<AtomicBool>,
    ) -> Self {
        Self::initialize_lowest_slot(&blockstore, &cluster_info);
        Self::initialize_epoch_slots(&bank_forks, &cluster_info);
        let t_cluster_slots_service = Builder::new()
            .name("solana-cluster-slots-service".to_string())
            .spawn(move || {
                Self::run(
                    blockstore,
                    cluster_slots,
                    bank_forks,
                    cluster_info,
                    cluster_slots_update_receiver,
                    exit,
                )
            })
            .unwrap();

        ClusterSlotsService {
            t_cluster_slots_service,
        }
    }

    pub fn join(self) -> thread::Result<()> {
        self.t_cluster_slots_service.join()
    }

    fn run(
        blockstore: Arc<Blockstore>,
        cluster_slots: Arc<ClusterSlots>,
        bank_forks: Arc<RwLock<BankForks>>,
        cluster_info: Arc<ClusterInfo>,
        cluster_slots_update_receiver: ClusterSlotsUpdateReceiver,
        exit: Arc<AtomicBool>,
    ) {
        let mut cluster_slots_service_timing = ClusterSlotsServiceTiming::default();
        let mut last_stats = Instant::now();
        loop {
            if exit.load(Ordering::Relaxed) {
                break;
            }
            let slots = match cluster_slots_update_receiver.recv_timeout(Duration::from_millis(200))
            {
                Ok(slots) => Some(slots),
                Err(RecvTimeoutError::Timeout) => None,
                Err(RecvTimeoutError::Disconnected) => {
                    warn!("Cluster slots service - sender disconnected");
                    break;
                }
            };
            let new_root = bank_forks.read().unwrap().root();
            let mut lowest_slot_elapsed = Measure::start("lowest_slot_elapsed");
            let lowest_slot = blockstore.lowest_slot();
            Self::update_lowest_slot(lowest_slot, &cluster_info);
            lowest_slot_elapsed.stop();
            let mut process_cluster_slots_updates_elapsed =
                Measure::start("process_cluster_slots_updates_elapsed");
            if let Some(slots) = slots {
                Self::process_cluster_slots_updates(
                    slots,
                    &cluster_slots_update_receiver,
                    &cluster_info,
                );
            }
            cluster_slots.update(new_root, &cluster_info, &bank_forks);
            process_cluster_slots_updates_elapsed.stop();

            cluster_slots_service_timing.update(
                lowest_slot_elapsed.as_us(),
                process_cluster_slots_updates_elapsed.as_us(),
            );

            if last_stats.elapsed().as_secs() > 2 {
                datapoint_info!(
                    "cluster_slots_service-timing",
                    (
                        "lowest_slot_elapsed",
                        cluster_slots_service_timing.lowest_slot_elapsed,
                        i64
                    ),
                    (
                        "process_cluster_slots_updates_elapsed",
                        cluster_slots_service_timing.process_cluster_slots_updates_elapsed,
                        i64
                    ),
                );
                cluster_slots_service_timing = ClusterSlotsServiceTiming::default();
                last_stats = Instant::now();
            }
        }
    }

    fn process_cluster_slots_updates(
        mut slots: Vec<Slot>,
        cluster_slots_update_receiver: &ClusterSlotsUpdateReceiver,
        cluster_info: &ClusterInfo,
    ) {
        while let Ok(mut more) = cluster_slots_update_receiver.try_recv() {
            slots.append(&mut more);
        }
        #[allow(clippy::stable_sort_primitive)]
        slots.sort();

        if !slots.is_empty() {
            cluster_info.push_epoch_slots(&slots);
        }
    }

    fn initialize_lowest_slot(blockstore: &Blockstore, cluster_info: &ClusterInfo) {
        // Safe to set into gossip because by this time, the leader schedule cache should
        // also be updated with the latest root (done in blockstore_processor) and thus
        // will provide a schedule to window_service for any incoming shreds up to the
        // last_confirmed_epoch.
        cluster_info.push_lowest_slot(blockstore.lowest_slot());
    }

    fn update_lowest_slot(lowest_slot: Slot, cluster_info: &ClusterInfo) {
        cluster_info.push_lowest_slot(lowest_slot);
    }

    fn initialize_epoch_slots(bank_forks: &RwLock<BankForks>, cluster_info: &ClusterInfo) {
        // TODO: Should probably incorporate slots that were replayed on startup,
        // and maybe some that were frozen < snapshot root in case validators restart
        // from newer snapshots and lose history.
        let frozen_banks = bank_forks.read().unwrap().frozen_banks();
        let mut frozen_bank_slots: Vec<Slot> = frozen_banks.keys().cloned().collect();
        frozen_bank_slots.sort_unstable();

        if !frozen_bank_slots.is_empty() {
            cluster_info.push_epoch_slots(&frozen_bank_slots);
        }
    }
}

#[cfg(test)]
mod test {
    use {
        super::*,
        solana_gossip::{cluster_info::Node, crds_value::LowestSlot},
        solana_sdk::{pubkey::Pubkey, signature::Keypair},
        solana_streamer::socket::SocketAddrSpace,
    };

    #[test]
    pub fn test_update_lowest_slot() {
        let pubkey = Pubkey::new_unique();
        let node_info = Node::new_localhost_with_pubkey(&pubkey);
        let cluster_info = ClusterInfo::new(
            node_info.info,
            Arc::new(Keypair::new()),
            SocketAddrSpace::Unspecified,
        );
        ClusterSlotsService::update_lowest_slot(5, &cluster_info);
        cluster_info.flush_push_queue();
        let lowest = {
            let gossip_crds = cluster_info.gossip.crds.read().unwrap();
            gossip_crds.get::<&LowestSlot>(pubkey).unwrap().clone()
        };
        assert_eq!(lowest.lowest, 5);
    }
}
