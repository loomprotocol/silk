use crate::{
    entry::{Entry, EntrySlice, VerifyOption, VerifyRecyclers},
    unverified_blocks::UnverifiedBlocks,
};
use solana_runtime::bank_forks::BankForks;
use solana_sdk::clock::Slot;
use std::{
    collections::HashMap,
    sync::atomic::{AtomicBool, Ordering},
    sync::mpsc::{Receiver, RecvTimeoutError, Sender},
    sync::Arc,
    sync::RwLock,
    thread::{self, Builder, JoinHandle},
};

pub type VerifySlotSender = Sender<Vec<(Slot, Vec<Entry>, u128)>>;
pub type VerifySlotReceiver = Receiver<Vec<(Slot, Vec<Entry>, u128)>>;

pub struct EntryVerifyService {
    t_verify: JoinHandle<()>,
}

impl EntryVerifyService {
    pub fn new(
        slot_receiver: VerifySlotReceiver,
        bank_forks: Arc<RwLock<BankForks>>,
        slot_verify_results: Arc<RwLock<HashMap<Slot, bool>>>,
        exit: &Arc<AtomicBool>,
    ) -> Self {
        let exit = exit.clone();

        let t_verify = Builder::new()
            .name("solana-entry-verify".to_string())
            .spawn(move || {
                let mut unverified_blocks = UnverifiedBlocks::default();
                loop {
                    if exit.load(Ordering::Relaxed) {
                        break;
                    }

                    if let Err(e) = Self::verify_entries(
                        &slot_receiver,
                        &bank_forks,
                        &slot_verify_results,
                        &mut unverified_blocks,
                    ) {
                        match e {
                            RecvTimeoutError::Disconnected => break,
                            RecvTimeoutError::Timeout => (),
                        }
                    }
                }
            })
            .unwrap();
        Self { t_verify }
    }

    fn verify_entries(
        slot_receiver: &VerifySlotReceiver,
        bank_forks: &Arc<RwLock<BankForks>>,
        slot_verify_results: &Arc<RwLock<HashMap<Slot, bool>>>,
        unverified_blocks: &mut UnverifiedBlocks,
    ) -> Result<(), RecvTimeoutError> {
        unverified_blocks.set_root(&bank_forks);

        while let Ok(slot_entries) = slot_receiver.try_recv() {
            for (slot, entries, weight) in slot_entries {
                // If slot bank doesn't exist, then it must have been
                // pruned by `set_root` and verification is no longer necessary
                {
                    // Hold the lock so that `set_root` doesn't get called
                    // in the middle of this logic
                    let w_bank_forks = bank_forks.write().unwrap();
                    if let Some(bank) = w_bank_forks.get(slot) {
                        let parent_bank = bank.parent().expect(
                            "Unverified slot can't be the root, so
                        parent must exist",
                        );
                        let parent_slot = parent_bank.slot();
                        let parent_hash = parent_bank.last_blockhash();
                        unverified_blocks.add_unverified_block(
                            slot,
                            parent_slot,
                            entries,
                            weight,
                            parent_hash,
                        );
                    }
                }
            }
        }

        if let Some((heaviest_slot, heaviest_block_info)) =
            unverified_blocks.pop_heaviest_ancestor()
        {
            let mut verifier = heaviest_block_info.entries.start_verify(
                &heaviest_block_info.parent_hash,
                VerifyRecyclers::default(),
                VerifyOption::PohOnly,
            );

            let verify_result = verifier.finish_verify(&heaviest_block_info.entries);
            datapoint_info!(
                "verify_poh_elapsed",
                ("slot", heaviest_slot, i64),
                ("elapsed", verifier.poh_duration_us(), i64)
            );
            slot_verify_results
                .write()
                .unwrap()
                .insert(heaviest_slot, verify_result);
            info!(
                "Verifying slot: {}, num_entries: {}, start_hash: {}, result: {}",
                heaviest_slot,
                heaviest_block_info.entries.len(),
                heaviest_block_info.parent_hash,
                verify_result,
            );
        }
        Ok(())
    }

    pub fn join(self) -> thread::Result<()> {
        self.t_verify.join()
    }
}
