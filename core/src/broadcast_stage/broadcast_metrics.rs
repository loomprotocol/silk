use super::*;

pub(crate) trait BroadcastStats {
    fn update(&mut self, new_stats: &Self);
    fn report_stats(&mut self, slot: Slot, slot_start: Instant);
}

#[derive(Clone)]
pub(crate) struct BroadcastShredBatchInfo {
    pub(crate) slot: Slot,
    pub(crate) num_expected_batches: Option<usize>,
    pub(crate) slot_start_ts: Instant,
}

#[derive(Default, Clone)]
pub struct TransmitShredsStats {
    pub transmit_elapsed: u64,
    pub send_mmsg_elapsed: u64,
    pub get_peers_elapsed: u64,
    pub shred_select: u64,
    pub num_shreds: usize,
    pub dropped_packets: usize,
}

impl BroadcastStats for TransmitShredsStats {
    fn update(&mut self, new_stats: &TransmitShredsStats) {
        self.transmit_elapsed += new_stats.transmit_elapsed;
        self.send_mmsg_elapsed += new_stats.send_mmsg_elapsed;
        self.get_peers_elapsed += new_stats.get_peers_elapsed;
        self.num_shreds += new_stats.num_shreds;
        self.shred_select += new_stats.shred_select;
        self.dropped_packets += new_stats.dropped_packets;
    }
    fn report_stats(&mut self, slot: Slot, slot_start: Instant) {
        datapoint_info!(
            "broadcast-transmit-shreds-stats",
            ("slot", slot as i64, i64),
            (
                "end_to_end_elapsed",
                // `slot_start` signals when the first batch of shreds was
                // received, used to measure duration of broadcast
                slot_start.elapsed().as_micros() as i64,
                i64
            ),
            ("transmit_elapsed", self.transmit_elapsed as i64, i64),
            ("send_mmsg_elapsed", self.send_mmsg_elapsed as i64, i64),
            ("get_peers_elapsed", self.get_peers_elapsed as i64, i64),
            ("num_shreds", self.num_shreds as i64, i64),
            ("shred_select", self.shred_select as i64, i64),
            ("dropped_packets", self.dropped_packets as i64, i64),
        );
    }
}

#[derive(Default, Clone)]
pub(crate) struct InsertShredsStats {
    pub(crate) insert_shreds_elapsed: u64,
    pub(crate) num_shreds: usize,
}
impl BroadcastStats for InsertShredsStats {
    fn update(&mut self, new_stats: &InsertShredsStats) {
        self.insert_shreds_elapsed += new_stats.insert_shreds_elapsed;
        self.num_shreds += new_stats.num_shreds;
    }
    fn report_stats(&mut self, slot: Slot, slot_start: Instant) {
        datapoint_info!(
            "broadcast-insert-shreds-stats",
            ("slot", slot as i64, i64),
            (
                "end_to_end_elapsed",
                // `slot_start` signals when the first batch of shreds was
                // received, used to measure duration of broadcast
                slot_start.elapsed().as_micros() as i64,
                i64
            ),
            (
                "insert_shreds_elapsed",
                self.insert_shreds_elapsed as i64,
                i64
            ),
            ("num_shreds", self.num_shreds as i64, i64),
        );
    }
}

// Tracks metrics of type `T` across multiple threads
#[derive(Default)]
pub(crate) struct BatchCounter<T: BroadcastStats + Default> {
    // The number of batches processed across all threads so far
    num_batches: usize,
    // Filled in when the last batch of shreds is received,
    // signals how many batches of shreds to expect
    num_expected_batches: Option<usize>,
    broadcast_shred_stats: T,
}

impl<T: BroadcastStats + Default> BatchCounter<T> {
    #[cfg(test)]
    pub(crate) fn num_batches(&self) -> usize {
        self.num_batches
    }
}

#[derive(Default)]
pub(crate) struct SlotBroadcastStats<T: BroadcastStats + Default>(HashMap<Slot, BatchCounter<T>>);

impl<T: BroadcastStats + Default> SlotBroadcastStats<T> {
    #[cfg(test)]
    pub(crate) fn get(&self, slot: Slot) -> Option<&BatchCounter<T>> {
        self.0.get(&slot)
    }
    pub(crate) fn update(&mut self, new_stats: &T, batch_info: &Option<BroadcastShredBatchInfo>) {
        if let Some(batch_info) = batch_info {
            let mut should_delete = false;
            {
                let slot_batch_counter = self.0.entry(batch_info.slot).or_default();
                slot_batch_counter.broadcast_shred_stats.update(new_stats);
                // Only count the ones where `broadcast_shred_batch_info`.is_some(), because
                // there could potentially be other `retransmit` slots inserted into the
                // transmit pipeline (signaled by ReplayStage) that are not created by the
                // main shredding/broadcast pipeline
                slot_batch_counter.num_batches += 1;
                if let Some(num_expected_batches) = batch_info.num_expected_batches {
                    slot_batch_counter.num_expected_batches = Some(num_expected_batches);
                }
                if let Some(num_expected_batches) = slot_batch_counter.num_expected_batches {
                    if slot_batch_counter.num_batches == num_expected_batches {
                        slot_batch_counter
                            .broadcast_shred_stats
                            .report_stats(batch_info.slot, batch_info.slot_start_ts);
                        should_delete = true;
                    }
                }
            }
            if should_delete {
                self.0
                    .remove(&batch_info.slot)
                    .expect("delete should be successful");
            }
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[derive(Default)]
    struct TestStats {
        sender: Option<Sender<(usize, Slot, Instant)>>,
        count: usize,
    }

    impl BroadcastStats for TestStats {
        fn update(&mut self, new_stats: &TestStats) {
            self.count += new_stats.count;
            self.sender = new_stats.sender.clone();
        }
        fn report_stats(&mut self, slot: Slot, slot_start: Instant) {
            self.sender
                .as_ref()
                .unwrap()
                .send((self.count, slot, slot_start))
                .unwrap()
        }
    }

    #[test]
    fn test_update_broadcast() {
        let start = Instant::now();
        let mut slot_broadcast_stats = SlotBroadcastStats::default();
        slot_broadcast_stats.update(
            &TransmitShredsStats {
                transmit_elapsed: 1,
                get_peers_elapsed: 2,
                send_mmsg_elapsed: 3,
                shred_select: 4,
                num_shreds: 5,
                dropped_packets: 6,
            },
            &Some(BroadcastShredBatchInfo {
                slot: 0,
                num_expected_batches: Some(2),
                slot_start_ts: start,
            }),
        );

        // Singular update
        let slot_0_stats = slot_broadcast_stats.0.get(&0).unwrap();
        assert_eq!(slot_0_stats.num_batches, 1);
        assert_eq!(slot_0_stats.num_expected_batches.unwrap(), 2);
        assert_eq!(slot_0_stats.broadcast_shred_stats.transmit_elapsed, 1);
        assert_eq!(slot_0_stats.broadcast_shred_stats.get_peers_elapsed, 2);
        assert_eq!(slot_0_stats.broadcast_shred_stats.send_mmsg_elapsed, 3);
        assert_eq!(slot_0_stats.broadcast_shred_stats.shred_select, 4);
        assert_eq!(slot_0_stats.broadcast_shred_stats.num_shreds, 5);
        assert_eq!(slot_0_stats.broadcast_shred_stats.dropped_packets, 6);

        slot_broadcast_stats.update(
            &TransmitShredsStats {
                transmit_elapsed: 7,
                get_peers_elapsed: 8,
                send_mmsg_elapsed: 9,
                shred_select: 10,
                num_shreds: 11,
                dropped_packets: 12,
            },
            &None,
        );

        // If BroadcastShredBatchInfo == None, then update should be ignored
        let slot_0_stats = slot_broadcast_stats.0.get(&0).unwrap();
        assert_eq!(slot_0_stats.num_batches, 1);
        assert_eq!(slot_0_stats.num_expected_batches.unwrap(), 2);
        assert_eq!(slot_0_stats.broadcast_shred_stats.transmit_elapsed, 1);
        assert_eq!(slot_0_stats.broadcast_shred_stats.get_peers_elapsed, 2);
        assert_eq!(slot_0_stats.broadcast_shred_stats.send_mmsg_elapsed, 3);
        assert_eq!(slot_0_stats.broadcast_shred_stats.shred_select, 4);
        assert_eq!(slot_0_stats.broadcast_shred_stats.num_shreds, 5);
        assert_eq!(slot_0_stats.broadcast_shred_stats.dropped_packets, 6);

        // If another batch is given, then total number of batches == num_expected_batches == 2,
        // so the batch should be purged from the HashMap
        slot_broadcast_stats.update(
            &TransmitShredsStats {
                transmit_elapsed: 1,
                get_peers_elapsed: 1,
                send_mmsg_elapsed: 1,
                shred_select: 1,
                num_shreds: 1,
                dropped_packets: 1,
            },
            &Some(BroadcastShredBatchInfo {
                slot: 0,
                num_expected_batches: None,
                slot_start_ts: start,
            }),
        );

        assert!(slot_broadcast_stats.0.get(&0).is_none());
    }

    #[test]
    fn test_update_multi_threaded() {
        for round in 0..50 {
            let start = Instant::now();
            let slot_broadcast_stats = Arc::new(Mutex::new(SlotBroadcastStats::default()));
            let num_threads = 5;
            let slot = 0;
            let (sender, receiver) = channel();
            let thread_handles: Vec<_> = (0..num_threads)
                .map(|i| {
                    let slot_broadcast_stats = slot_broadcast_stats.clone();
                    let sender = Some(sender.clone());
                    let test_stats = TestStats { sender, count: 1 };
                    let mut broadcast_batch_info = BroadcastShredBatchInfo {
                        slot,
                        num_expected_batches: None,
                        slot_start_ts: start,
                    };
                    if i == round % num_threads {
                        broadcast_batch_info.num_expected_batches = Some(num_threads);
                    }
                    Builder::new()
                        .name("test_update_multi_threaded".to_string())
                        .spawn(move || {
                            slot_broadcast_stats
                                .lock()
                                .unwrap()
                                .update(&test_stats, &Some(broadcast_batch_info))
                        })
                        .unwrap()
                })
                .collect();

            for t in thread_handles {
                t.join().unwrap();
            }

            assert!(slot_broadcast_stats.lock().unwrap().0.get(&slot).is_none());
            let (returned_count, returned_slot, _returned_instant) = receiver.recv().unwrap();
            assert_eq!(returned_count, num_threads);
            assert_eq!(returned_slot, slot);
        }
    }
}
