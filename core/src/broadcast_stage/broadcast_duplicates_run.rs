use super::*;
use solana_entry::entry::Entry;
use solana_ledger::shred::Shredder;
use solana_runtime::blockhash_queue::BlockhashQueue;
use solana_sdk::{
    hash::Hash,
    signature::{Keypair, Signer},
    system_transaction,
};

pub const MINIMUM_DUPLICATE_SLOT: Slot = 20;
pub const DUPLICATE_RATE: usize = 10;

#[derive(PartialEq, Clone, Debug)]
pub struct BroadcastDuplicatesConfig {
    /// Amount of stake (excluding the leader) to send different version of slots to.
    /// Note this is sampled from a list of stakes sorted least to greatest.
    pub stake_partition: u64,
}

#[derive(Clone)]
pub(super) struct BroadcastDuplicatesRun {
    config: BroadcastDuplicatesConfig,
    // Local queue for broadcast to track which duplicate blockhashes we've sent
    duplicate_queue: BlockhashQueue,
    // Buffer for duplicate entries
    duplicate_entries_buffer: Vec<Entry>,
    last_duplicate_entry_hash: Hash,
    current_slot: Slot,
    next_shred_index: u32,
    shred_version: u16,
    recent_blockhash: Option<Hash>,
    prev_entry_hash: Option<Hash>,
    num_slots_broadcasted: usize,
}

impl BroadcastDuplicatesRun {
    pub(super) fn new(shred_version: u16, config: BroadcastDuplicatesConfig) -> Self {
        Self {
            config,
            duplicate_queue: BlockhashQueue::default(),
            duplicate_entries_buffer: vec![],
            next_shred_index: u32::MAX,
            last_duplicate_entry_hash: Hash::default(),
            shred_version,
            current_slot: 0,
            recent_blockhash: None,
            prev_entry_hash: None,
            num_slots_broadcasted: 0,
        }
    }

    fn get_non_partitioned_batches(
        &self,
        my_pubkey: &Pubkey,
        bank: &Bank,
        data_shreds: Arc<Vec<Shred>>,
    ) -> TransmitShreds {
        let bank_epoch = bank.get_leader_schedule_epoch(bank.slot());
        let mut stakes: HashMap<Pubkey, u64> = bank.epoch_staked_nodes(bank_epoch).unwrap();
        stakes.retain(|pubkey, _stake| pubkey != my_pubkey);
        (Some(Arc::new(stakes)), data_shreds)
    }

    fn get_partitioned_batches(
        &self,
        my_pubkey: &Pubkey,
        bank: &Bank,
        original_shreds: Arc<Vec<Shred>>,
        partition_shreds: Arc<Vec<Shred>>,
    ) -> (TransmitShreds, TransmitShreds) {
        // On the last shred, partition network with duplicate and real shreds
        let bank_epoch = bank.get_leader_schedule_epoch(bank.slot());
        let mut original_recipients = HashMap::new();
        let mut partition_recipients = HashMap::new();

        let mut stakes: Vec<(Pubkey, u64)> = bank
            .epoch_staked_nodes(bank_epoch)
            .unwrap()
            .into_iter()
            .filter(|(pubkey, _)| pubkey != my_pubkey)
            .collect();
        stakes.sort_by(|(l_key, l_stake), (r_key, r_stake)| {
            if r_stake == l_stake {
                l_key.cmp(r_key)
            } else {
                l_stake.cmp(r_stake)
            }
        });

        let mut cumulative_stake: u64 = 0;
        for (pubkey, stake) in stakes.into_iter() {
            cumulative_stake += stake;
            if cumulative_stake <= self.config.stake_partition {
                partition_recipients.insert(pubkey, stake);
            } else {
                original_recipients.insert(pubkey, stake);
            }
        }

        warn!(
            "{} sent duplicate slot {} to nodes: {:?}",
            my_pubkey,
            bank.slot(),
            &partition_recipients,
        );

        let original_recipients = Arc::new(original_recipients);
        let original_transmit_shreds = (Some(original_recipients), original_shreds);

        let partition_recipients = Arc::new(partition_recipients);
        let partition_transmit_shreds = (Some(partition_recipients), partition_shreds);

        (original_transmit_shreds, partition_transmit_shreds)
    }
}

impl BroadcastRun for BroadcastDuplicatesRun {
    fn run(
        &mut self,
        keypair: &Keypair,
        _blockstore: &Arc<Blockstore>,
        receiver: &Receiver<WorkingBankEntry>,
        socket_sender: &Sender<(TransmitShreds, Option<BroadcastShredBatchInfo>)>,
        blockstore_sender: &Sender<(Arc<Vec<Shred>>, Option<BroadcastShredBatchInfo>)>,
    ) -> Result<()> {
        // 1) Pull entries from banking stage
        let mut receive_results = broadcast_utils::recv_slot_entries(receiver)?;
        let bank = receive_results.bank.clone();
        let last_tick_height = receive_results.last_tick_height;

        if bank.slot() != self.current_slot {
            self.next_shred_index = 0;
            self.current_slot = bank.slot();
            self.prev_entry_hash = None;
            self.num_slots_broadcasted += 1;
        }

        if receive_results.entries.is_empty() {
            return Ok(());
        }

        // Update the recent blockhash based on transactions in the entries
        for entry in &receive_results.entries {
            if !entry.transactions.is_empty() {
                self.recent_blockhash = Some(entry.transactions[0].message.recent_blockhash);
                break;
            }
        }

        // 2) Convert entries to shreds + generate coding shreds. Set a garbage PoH on the last entry
        // in the slot to make verification fail on validators
        let last_entries = {
            if last_tick_height == bank.max_tick_height()
                && bank.slot() > MINIMUM_DUPLICATE_SLOT
                && self.num_slots_broadcasted % DUPLICATE_RATE == 0
                && self.recent_blockhash.is_some()
            {
                let entry_batch_len = receive_results.entries.len();
                let prev_entry_hash =
                    // Try to get second-to-last entry before last tick
                    if entry_batch_len > 1 {
                        Some(receive_results.entries[entry_batch_len - 2].hash)
                    } else {
                        self.prev_entry_hash
                    };

                if let Some(prev_entry_hash) = prev_entry_hash {
                    let original_last_entry = receive_results.entries.pop().unwrap();

                    // Last entry has to be a tick
                    assert!(original_last_entry.is_tick());

                    // Inject an extra entry before the last tick
                    let extra_tx = system_transaction::transfer(
                        keypair,
                        &Pubkey::new_unique(),
                        1,
                        self.recent_blockhash.unwrap(),
                    );
                    let new_extra_entry = Entry::new(&prev_entry_hash, 1, vec![extra_tx]);

                    // This will only work with sleepy tick producer where the hashing
                    // checks in replay are turned off, because we're introducing an extra
                    // hash for the last tick in the `new_extra_entry`.
                    let new_last_entry = Entry::new(
                        &new_extra_entry.hash,
                        original_last_entry.num_hashes,
                        vec![],
                    );

                    Some((original_last_entry, vec![new_extra_entry, new_last_entry]))
                } else {
                    None
                }
            } else {
                None
            }
        };

        self.prev_entry_hash = last_entries
            .as_ref()
            .map(|(original_last_entry, _)| original_last_entry.hash)
            .or_else(|| Some(receive_results.entries.last().unwrap().hash));

        let shredder = Shredder::new(
            bank.slot(),
            bank.parent().unwrap().slot(),
            (bank.tick_height() % bank.ticks_per_slot()) as u8,
            self.shred_version,
        )
        .expect("Expected to create a new shredder");

        let (data_shreds, _, _) = shredder.entries_to_shreds(
            keypair,
            &receive_results.entries,
            last_tick_height == bank.max_tick_height() && last_entries.is_none(),
            self.next_shred_index,
        );

        self.next_shred_index += data_shreds.len() as u32;
        let last_shreds = last_entries.map(|(original_last_entry, duplicate_extra_last_entries)| {
            let (original_last_data_shred, _, _) =
                shredder.entries_to_shreds(keypair, &[original_last_entry], true, self.next_shred_index);

            let (partition_last_data_shred, _, _) =
                // Don't mark the last shred as last so that validators won't know that
                // they've gotten all the shreds, and will continue trying to repair
                shredder.entries_to_shreds(keypair, &duplicate_extra_last_entries, true, self.next_shred_index);

                let sigs: Vec<_> = partition_last_data_shred.iter().map(|s| (s.signature(), s.index())).collect();
                info!(
                    "duplicate signatures for slot {}, sigs: {:?}",
                    bank.slot(),
                    sigs,
                );

            self.next_shred_index += 1;
            (original_last_data_shred, partition_last_data_shred)
        });

        let data_shreds = Arc::new(data_shreds);
        blockstore_sender.send((data_shreds.clone(), None))?;

        // 3) Start broadcast step
        let transmit_shreds =
            self.get_non_partitioned_batches(&keypair.pubkey(), &bank, data_shreds.clone());
        info!(
            "{} Sending good shreds for slot {} to network",
            keypair.pubkey(),
            data_shreds.first().unwrap().slot()
        );
        socket_sender.send((transmit_shreds, None))?;

        // Special handling of last shred to cause partition
        if let Some((original_last_data_shred, partition_last_data_shred)) = last_shreds {
            let original_last_data_shred = Arc::new(original_last_data_shred);
            let partition_last_data_shred = Arc::new(partition_last_data_shred);

            // Store the original shreds that this node replayed
            blockstore_sender.send((original_last_data_shred.clone(), None))?;

            let (original_transmit_shreds, partition_transmit_shreds) = self
                .get_partitioned_batches(
                    &keypair.pubkey(),
                    &bank,
                    original_last_data_shred,
                    partition_last_data_shred,
                );

            socket_sender.send((original_transmit_shreds, None))?;
            socket_sender.send((partition_transmit_shreds, None))?;
        }
        Ok(())
    }

    fn transmit(
        &mut self,
        receiver: &Arc<Mutex<TransmitReceiver>>,
        cluster_info: &ClusterInfo,
        sock: &UdpSocket,
        bank_forks: &Arc<RwLock<BankForks>>,
    ) -> Result<()> {
        let ((stakes, shreds), _) = receiver.lock().unwrap().recv()?;
        // Broadcast data
        let cluster_nodes = ClusterNodes::<BroadcastStage>::new(
            cluster_info,
            stakes.as_deref().unwrap_or(&HashMap::default()),
        );
        broadcast_shreds(
            sock,
            &shreds,
            &cluster_nodes,
            &Arc::new(AtomicU64::new(0)),
            &mut TransmitShredsStats::default(),
            cluster_info.id(),
            bank_forks,
            cluster_info.socket_addr_space(),
        )?;

        Ok(())
    }

    fn record(
        &mut self,
        receiver: &Arc<Mutex<RecordReceiver>>,
        blockstore: &Arc<Blockstore>,
    ) -> Result<()> {
        let (all_shreds, _) = receiver.lock().unwrap().recv()?;
        blockstore
            .insert_shreds(all_shreds.to_vec(), None, true)
            .expect("Failed to insert shreds in blockstore");
        Ok(())
    }
}
