// TODO: Merge this implementation with the one at `banks-server/src/send_transaction_service.rs`
use {
    log::*,
    solana_gossip::cluster_info::ClusterInfo,
    solana_metrics::{datapoint_warn, inc_new_counter_info},
    solana_poh::poh_recorder::PohRecorder,
    solana_runtime::{bank::Bank, bank_forks::BankForks},
    solana_sdk::{
        clock::{Slot, NUM_CONSECUTIVE_LEADER_SLOTS},
        hash::Hash,
        nonce_account,
        pubkey::Pubkey,
        signature::Signature,
    },
    std::{
        collections::HashMap,
        net::{SocketAddr, UdpSocket},
        sync::{
            mpsc::{Receiver, RecvTimeoutError},
            Arc, Mutex, RwLock,
        },
        thread::{self, Builder, JoinHandle},
        time::{Duration, Instant},
    },
};

/// Maximum size of the transaction queue
const MAX_TRANSACTION_QUEUE_SIZE: usize = 10_000; // This seems like a lot but maybe it needs to be bigger one day

pub struct SendTransactionService {
    thread: JoinHandle<()>,
}

pub struct TransactionInfo {
    pub signature: Signature,
    pub wire_transaction: Vec<u8>,
    pub last_valid_slot: Slot,
    pub durable_nonce_info: Option<(Pubkey, Hash)>,
}

impl TransactionInfo {
    pub fn new(
        signature: Signature,
        wire_transaction: Vec<u8>,
        last_valid_slot: Slot,
        durable_nonce_info: Option<(Pubkey, Hash)>,
    ) -> Self {
        Self {
            signature,
            wire_transaction,
            last_valid_slot,
            durable_nonce_info,
        }
    }
}

pub struct LeaderInfo {
    cluster_info: Arc<ClusterInfo>,
    poh_recorder: Arc<Mutex<PohRecorder>>,
    recent_peers: HashMap<Pubkey, SocketAddr>,
}

impl LeaderInfo {
    pub fn new(cluster_info: Arc<ClusterInfo>, poh_recorder: Arc<Mutex<PohRecorder>>) -> Self {
        Self {
            cluster_info,
            poh_recorder,
            recent_peers: HashMap::new(),
        }
    }

    pub fn refresh_recent_peers(&mut self) {
        self.recent_peers = self
            .cluster_info
            .tpu_peers()
            .into_iter()
            .map(|ci| (ci.id, ci.tpu))
            .collect();
    }

    pub fn get_leader_tpus(&self, max_count: u64) -> Vec<&SocketAddr> {
        let recorder = self.poh_recorder.lock().unwrap();
        let leaders: Vec<_> = (0..max_count)
            .filter_map(|i| recorder.leader_after_n_slots(i * NUM_CONSECUTIVE_LEADER_SLOTS))
            .collect();
        drop(recorder);
        let mut unique_leaders = vec![];
        for leader in leaders.iter() {
            if let Some(addr) = self.recent_peers.get(leader) {
                if !unique_leaders.contains(&addr) {
                    unique_leaders.push(addr);
                }
            }
        }
        unique_leaders
    }
}

#[derive(Default, Debug, PartialEq)]
struct ProcessTransactionsResult {
    rooted: u64,
    expired: u64,
    retried: u64,
    failed: u64,
    retained: u64,
}

impl SendTransactionService {
    pub fn new(
        tpu_address: SocketAddr,
        bank_forks: &Arc<RwLock<BankForks>>,
        leader_info: Option<LeaderInfo>,
        receiver: Receiver<TransactionInfo>,
        retry_rate_ms: u64,
        leader_forward_count: u64,
    ) -> Self {
        let thread = Self::retry_thread(
            tpu_address,
            receiver,
            bank_forks.clone(),
            leader_info,
            retry_rate_ms,
            leader_forward_count,
        );
        Self { thread }
    }

    fn retry_thread(
        tpu_address: SocketAddr,
        receiver: Receiver<TransactionInfo>,
        bank_forks: Arc<RwLock<BankForks>>,
        mut leader_info: Option<LeaderInfo>,
        retry_rate_ms: u64,
        leader_forward_count: u64,
    ) -> JoinHandle<()> {
        let mut last_status_check = Instant::now();
        let mut last_leader_refresh = Instant::now();
        let mut transactions = HashMap::new();
        let send_socket = UdpSocket::bind("0.0.0.0:0").unwrap();

        if let Some(leader_info) = leader_info.as_mut() {
            leader_info.refresh_recent_peers();
        }

        Builder::new()
            .name("send-tx-sv2".to_string())
            .spawn(move || loop {
                match receiver.recv_timeout(Duration::from_millis(1000.min(retry_rate_ms))) {
                    Err(RecvTimeoutError::Disconnected) => break,
                    Err(RecvTimeoutError::Timeout) => {}
                    Ok(transaction_info) => {
                        let addresses = leader_info
                            .as_ref()
                            .map(|leader_info| leader_info.get_leader_tpus(leader_forward_count));
                        let addresses = addresses
                            .map(|address_list| {
                                if address_list.is_empty() {
                                    vec![&tpu_address]
                                } else {
                                    address_list
                                }
                            })
                            .unwrap_or_else(|| vec![&tpu_address]);
                        for address in addresses {
                            Self::send_transaction(
                                &send_socket,
                                address,
                                &transaction_info.wire_transaction,
                            );
                        }
                        if transactions.len() < MAX_TRANSACTION_QUEUE_SIZE {
                            transactions.insert(transaction_info.signature, transaction_info);
                        } else {
                            datapoint_warn!("send_transaction_service-queue-overflow");
                        }
                    }
                }

                if last_status_check.elapsed().as_millis() as u64 >= retry_rate_ms {
                    if !transactions.is_empty() {
                        datapoint_info!(
                            "send_transaction_service-queue-size",
                            ("len", transactions.len(), i64)
                        );
                        let (root_bank, working_bank) = {
                            let bank_forks = bank_forks.read().unwrap();
                            (
                                bank_forks.root_bank().clone(),
                                bank_forks.working_bank().clone(),
                            )
                        };

                        let _result = Self::process_transactions(
                            &working_bank,
                            &root_bank,
                            &send_socket,
                            &tpu_address,
                            &mut transactions,
                            &leader_info,
                            leader_forward_count,
                        );
                    }
                    last_status_check = Instant::now();
                    if last_leader_refresh.elapsed().as_millis() > 1000 {
                        if let Some(leader_info) = leader_info.as_mut() {
                            leader_info.refresh_recent_peers();
                        }
                        last_leader_refresh = Instant::now();
                    }
                }
            })
            .unwrap()
    }

    fn process_transactions(
        working_bank: &Arc<Bank>,
        root_bank: &Arc<Bank>,
        send_socket: &UdpSocket,
        tpu_address: &SocketAddr,
        transactions: &mut HashMap<Signature, TransactionInfo>,
        leader_info: &Option<LeaderInfo>,
        leader_forward_count: u64,
    ) -> ProcessTransactionsResult {
        let mut result = ProcessTransactionsResult::default();

        transactions.retain(|signature, transaction_info| {
            if transaction_info.durable_nonce_info.is_some() {
                inc_new_counter_info!("send_transaction_service-nonced", 1);
            }
            if root_bank.has_signature(signature) {
                info!("Transaction is rooted: {}", signature);
                result.rooted += 1;
                inc_new_counter_info!("send_transaction_service-rooted", 1);
                return false;
            }
            if let Some((nonce_pubkey, durable_nonce)) = transaction_info.durable_nonce_info {
                let nonce_account = working_bank.get_account(&nonce_pubkey).unwrap_or_default();
                if !nonce_account::verify_nonce_account(&nonce_account, &durable_nonce)
                    && working_bank.get_signature_status_slot(signature).is_none()
                {
                    info!("Dropping expired durable-nonce transaction: {}", signature);
                    result.expired += 1;
                    inc_new_counter_info!("send_transaction_service-expired", 1);
                    return false;
                }
            }
            if transaction_info.last_valid_slot < root_bank.slot() {
                info!("Dropping expired transaction: {}", signature);
                result.expired += 1;
                inc_new_counter_info!("send_transaction_service-expired", 1);
                return false;
            }

            match working_bank.get_signature_status_slot(signature) {
                None => {
                    // Transaction is unknown to the working bank, it might have been
                    // dropped or landed in another fork.  Re-send it
                    info!("Retrying transaction: {}", signature);
                    result.retried += 1;
                    inc_new_counter_info!("send_transaction_service-retry", 1);
                    let addresses = leader_info
                        .as_ref()
                        .map(|leader_info| leader_info.get_leader_tpus(leader_forward_count));
                    let addresses = addresses
                        .map(|address_list| {
                            if address_list.is_empty() {
                                vec![tpu_address]
                            } else {
                                address_list
                            }
                        })
                        .unwrap_or_else(|| vec![tpu_address]);
                    for address in addresses {
                        Self::send_transaction(
                            send_socket,
                            address,
                            &transaction_info.wire_transaction,
                        );
                    }
                    true
                }
                Some((_slot, status)) => {
                    if status.is_err() {
                        info!("Dropping failed transaction: {}", signature);
                        result.failed += 1;
                        inc_new_counter_info!("send_transaction_service-failed", 1);
                        false
                    } else {
                        result.retained += 1;
                        true
                    }
                }
            }
        });

        result
    }

    fn send_transaction(
        send_socket: &UdpSocket,
        tpu_address: &SocketAddr,
        wire_transaction: &[u8],
    ) {
        if let Err(err) = send_socket.send_to(wire_transaction, tpu_address) {
            warn!("Failed to send transaction to {}: {:?}", tpu_address, err);
        }
    }

    pub fn join(self) -> thread::Result<()> {
        self.thread.join()
    }
}

#[cfg(test)]
mod test {
    use {
        super::*,
        solana_gossip::contact_info::ContactInfo,
        solana_ledger::{
            blockstore::Blockstore, get_tmp_ledger_path, leader_schedule_cache::LeaderScheduleCache,
        },
        solana_runtime::genesis_utils::{
            create_genesis_config_with_vote_accounts, GenesisConfigInfo, ValidatorVoteKeypairs,
        },
        solana_sdk::{
            account::AccountSharedData,
            fee_calculator::FeeCalculator,
            genesis_config::create_genesis_config,
            nonce,
            poh_config::PohConfig,
            pubkey::Pubkey,
            signature::{Keypair, Signer},
            system_program, system_transaction,
            timing::timestamp,
        },
        solana_streamer::socket::SocketAddrSpace,
        std::sync::{atomic::AtomicBool, mpsc::channel},
    };

    #[test]
    fn service_exit() {
        let tpu_address = "127.0.0.1:0".parse().unwrap();
        let bank = Bank::default();
        let bank_forks = Arc::new(RwLock::new(BankForks::new(bank)));
        let (sender, receiver) = channel();

        let send_tranaction_service =
            SendTransactionService::new(tpu_address, &bank_forks, None, receiver, 1000, 1);

        drop(sender);
        send_tranaction_service.join().unwrap();
    }

    #[test]
    fn process_transactions() {
        solana_logger::setup();

        let (genesis_config, mint_keypair) = create_genesis_config(4);
        let bank = Bank::new(&genesis_config);
        let bank_forks = Arc::new(RwLock::new(BankForks::new(bank)));
        let send_socket = UdpSocket::bind("0.0.0.0:0").unwrap();
        let tpu_address = "127.0.0.1:0".parse().unwrap();
        let leader_forward_count = 1;

        let root_bank = Arc::new(Bank::new_from_parent(
            &bank_forks.read().unwrap().working_bank(),
            &Pubkey::default(),
            1,
        ));
        let rooted_signature = root_bank
            .transfer(1, &mint_keypair, &mint_keypair.pubkey())
            .unwrap();

        let working_bank = Arc::new(Bank::new_from_parent(&root_bank, &Pubkey::default(), 2));

        let non_rooted_signature = working_bank
            .transfer(2, &mint_keypair, &mint_keypair.pubkey())
            .unwrap();

        let failed_signature = {
            let blockhash = working_bank.last_blockhash();
            let transaction =
                system_transaction::transfer(&mint_keypair, &Pubkey::default(), 1, blockhash);
            let signature = transaction.signatures[0];
            working_bank.process_transaction(&transaction).unwrap_err();
            signature
        };

        let mut transactions = HashMap::new();

        info!("Expired transactions are dropped...");
        transactions.insert(
            Signature::default(),
            TransactionInfo::new(Signature::default(), vec![], root_bank.slot() - 1, None),
        );
        let result = SendTransactionService::process_transactions(
            &working_bank,
            &root_bank,
            &send_socket,
            &tpu_address,
            &mut transactions,
            &None,
            leader_forward_count,
        );
        assert!(transactions.is_empty());
        assert_eq!(
            result,
            ProcessTransactionsResult {
                expired: 1,
                ..ProcessTransactionsResult::default()
            }
        );

        info!("Rooted transactions are dropped...");
        transactions.insert(
            rooted_signature,
            TransactionInfo::new(rooted_signature, vec![], working_bank.slot(), None),
        );
        let result = SendTransactionService::process_transactions(
            &working_bank,
            &root_bank,
            &send_socket,
            &tpu_address,
            &mut transactions,
            &None,
            leader_forward_count,
        );
        assert!(transactions.is_empty());
        assert_eq!(
            result,
            ProcessTransactionsResult {
                rooted: 1,
                ..ProcessTransactionsResult::default()
            }
        );

        info!("Failed transactions are dropped...");
        transactions.insert(
            failed_signature,
            TransactionInfo::new(failed_signature, vec![], working_bank.slot(), None),
        );
        let result = SendTransactionService::process_transactions(
            &working_bank,
            &root_bank,
            &send_socket,
            &tpu_address,
            &mut transactions,
            &None,
            leader_forward_count,
        );
        assert!(transactions.is_empty());
        assert_eq!(
            result,
            ProcessTransactionsResult {
                failed: 1,
                ..ProcessTransactionsResult::default()
            }
        );

        info!("Non-rooted transactions are kept...");
        transactions.insert(
            non_rooted_signature,
            TransactionInfo::new(non_rooted_signature, vec![], working_bank.slot(), None),
        );
        let result = SendTransactionService::process_transactions(
            &working_bank,
            &root_bank,
            &send_socket,
            &tpu_address,
            &mut transactions,
            &None,
            leader_forward_count,
        );
        assert_eq!(transactions.len(), 1);
        assert_eq!(
            result,
            ProcessTransactionsResult {
                retained: 1,
                ..ProcessTransactionsResult::default()
            }
        );
        transactions.clear();

        info!("Unknown transactions are retried...");
        transactions.insert(
            Signature::default(),
            TransactionInfo::new(Signature::default(), vec![], working_bank.slot(), None),
        );
        let result = SendTransactionService::process_transactions(
            &working_bank,
            &root_bank,
            &send_socket,
            &tpu_address,
            &mut transactions,
            &None,
            leader_forward_count,
        );
        assert_eq!(transactions.len(), 1);
        assert_eq!(
            result,
            ProcessTransactionsResult {
                retried: 1,
                ..ProcessTransactionsResult::default()
            }
        );
    }

    #[test]
    fn test_retry_durable_nonce_transactions() {
        solana_logger::setup();

        let (genesis_config, mint_keypair) = create_genesis_config(4);
        let bank = Bank::new(&genesis_config);
        let bank_forks = Arc::new(RwLock::new(BankForks::new(bank)));
        let send_socket = UdpSocket::bind("0.0.0.0:0").unwrap();
        let tpu_address = "127.0.0.1:0".parse().unwrap();
        let leader_forward_count = 1;

        let root_bank = Arc::new(Bank::new_from_parent(
            &bank_forks.read().unwrap().working_bank(),
            &Pubkey::default(),
            1,
        ));
        let rooted_signature = root_bank
            .transfer(1, &mint_keypair, &mint_keypair.pubkey())
            .unwrap();

        let nonce_address = Pubkey::new_unique();
        let durable_nonce = Hash::new_unique();
        let nonce_state =
            nonce::state::Versions::new_current(nonce::State::Initialized(nonce::state::Data {
                authority: Pubkey::default(),
                blockhash: durable_nonce,
                fee_calculator: FeeCalculator::new(42),
            }));
        let nonce_account =
            AccountSharedData::new_data(43, &nonce_state, &system_program::id()).unwrap();
        root_bank.store_account(&nonce_address, &nonce_account);

        let working_bank = Arc::new(Bank::new_from_parent(&root_bank, &Pubkey::default(), 2));
        let non_rooted_signature = working_bank
            .transfer(2, &mint_keypair, &mint_keypair.pubkey())
            .unwrap();

        let last_valid_slot = working_bank.slot() + 300;

        let failed_signature = {
            let blockhash = working_bank.last_blockhash();
            let transaction =
                system_transaction::transfer(&mint_keypair, &Pubkey::default(), 1, blockhash);
            let signature = transaction.signatures[0];
            working_bank.process_transaction(&transaction).unwrap_err();
            signature
        };

        let mut transactions = HashMap::new();

        info!("Rooted durable-nonce transactions are dropped...");
        transactions.insert(
            rooted_signature,
            TransactionInfo::new(
                rooted_signature,
                vec![],
                last_valid_slot,
                Some((nonce_address, durable_nonce)),
            ),
        );
        let result = SendTransactionService::process_transactions(
            &working_bank,
            &root_bank,
            &send_socket,
            &tpu_address,
            &mut transactions,
            &None,
            leader_forward_count,
        );
        assert!(transactions.is_empty());
        assert_eq!(
            result,
            ProcessTransactionsResult {
                rooted: 1,
                ..ProcessTransactionsResult::default()
            }
        );
        // Nonce expired case
        transactions.insert(
            rooted_signature,
            TransactionInfo::new(
                rooted_signature,
                vec![],
                last_valid_slot,
                Some((nonce_address, Hash::new_unique())),
            ),
        );
        let result = SendTransactionService::process_transactions(
            &working_bank,
            &root_bank,
            &send_socket,
            &tpu_address,
            &mut transactions,
            &None,
            leader_forward_count,
        );
        assert!(transactions.is_empty());
        assert_eq!(
            result,
            ProcessTransactionsResult {
                rooted: 1,
                ..ProcessTransactionsResult::default()
            }
        );

        // Expired durable-nonce transactions are dropped; nonce has advanced...
        info!("Expired durable-nonce transactions are dropped...");
        transactions.insert(
            Signature::default(),
            TransactionInfo::new(
                Signature::default(),
                vec![],
                last_valid_slot,
                Some((nonce_address, Hash::new_unique())),
            ),
        );
        let result = SendTransactionService::process_transactions(
            &working_bank,
            &root_bank,
            &send_socket,
            &tpu_address,
            &mut transactions,
            &None,
            leader_forward_count,
        );
        assert!(transactions.is_empty());
        assert_eq!(
            result,
            ProcessTransactionsResult {
                expired: 1,
                ..ProcessTransactionsResult::default()
            }
        );
        // ... or last_valid_slot timeout has passed
        transactions.insert(
            Signature::default(),
            TransactionInfo::new(
                Signature::default(),
                vec![],
                root_bank.slot() - 1,
                Some((nonce_address, durable_nonce)),
            ),
        );
        let result = SendTransactionService::process_transactions(
            &working_bank,
            &root_bank,
            &send_socket,
            &tpu_address,
            &mut transactions,
            &None,
            leader_forward_count,
        );
        assert!(transactions.is_empty());
        assert_eq!(
            result,
            ProcessTransactionsResult {
                expired: 1,
                ..ProcessTransactionsResult::default()
            }
        );

        info!("Failed durable-nonce transactions are dropped...");
        transactions.insert(
            failed_signature,
            TransactionInfo::new(
                failed_signature,
                vec![],
                last_valid_slot,
                Some((nonce_address, Hash::new_unique())), // runtime should advance nonce on failed transactions
            ),
        );
        let result = SendTransactionService::process_transactions(
            &working_bank,
            &root_bank,
            &send_socket,
            &tpu_address,
            &mut transactions,
            &None,
            leader_forward_count,
        );
        assert!(transactions.is_empty());
        assert_eq!(
            result,
            ProcessTransactionsResult {
                failed: 1,
                ..ProcessTransactionsResult::default()
            }
        );

        info!("Non-rooted durable-nonce transactions are kept...");
        transactions.insert(
            non_rooted_signature,
            TransactionInfo::new(
                non_rooted_signature,
                vec![],
                last_valid_slot,
                Some((nonce_address, Hash::new_unique())), // runtime advances nonce when transaction lands
            ),
        );
        let result = SendTransactionService::process_transactions(
            &working_bank,
            &root_bank,
            &send_socket,
            &tpu_address,
            &mut transactions,
            &None,
            leader_forward_count,
        );
        assert_eq!(transactions.len(), 1);
        assert_eq!(
            result,
            ProcessTransactionsResult {
                retained: 1,
                ..ProcessTransactionsResult::default()
            }
        );
        transactions.clear();

        info!("Unknown durable-nonce transactions are retried until nonce advances...");
        transactions.insert(
            Signature::default(),
            TransactionInfo::new(
                Signature::default(),
                vec![],
                last_valid_slot,
                Some((nonce_address, durable_nonce)),
            ),
        );
        let result = SendTransactionService::process_transactions(
            &working_bank,
            &root_bank,
            &send_socket,
            &tpu_address,
            &mut transactions,
            &None,
            leader_forward_count,
        );
        assert_eq!(transactions.len(), 1);
        assert_eq!(
            result,
            ProcessTransactionsResult {
                retried: 1,
                ..ProcessTransactionsResult::default()
            }
        );
        // Advance nonce
        let new_durable_nonce = Hash::new_unique();
        let new_nonce_state =
            nonce::state::Versions::new_current(nonce::State::Initialized(nonce::state::Data {
                authority: Pubkey::default(),
                blockhash: new_durable_nonce,
                fee_calculator: FeeCalculator::new(42),
            }));
        let nonce_account =
            AccountSharedData::new_data(43, &new_nonce_state, &system_program::id()).unwrap();
        working_bank.store_account(&nonce_address, &nonce_account);
        let result = SendTransactionService::process_transactions(
            &working_bank,
            &root_bank,
            &send_socket,
            &tpu_address,
            &mut transactions,
            &None,
            leader_forward_count,
        );
        assert_eq!(transactions.len(), 0);
        assert_eq!(
            result,
            ProcessTransactionsResult {
                expired: 1,
                ..ProcessTransactionsResult::default()
            }
        );
    }

    #[test]
    fn test_get_leader_tpus() {
        let ledger_path = get_tmp_ledger_path!();
        {
            let blockstore = Blockstore::open(&ledger_path).unwrap();

            let validator_vote_keypairs0 = ValidatorVoteKeypairs::new_rand();
            let validator_vote_keypairs1 = ValidatorVoteKeypairs::new_rand();
            let validator_vote_keypairs2 = ValidatorVoteKeypairs::new_rand();
            let validator_keypairs = vec![
                &validator_vote_keypairs0,
                &validator_vote_keypairs1,
                &validator_vote_keypairs2,
            ];
            let GenesisConfigInfo {
                genesis_config,
                mint_keypair: _,
                voting_keypair: _,
            } = create_genesis_config_with_vote_accounts(
                1_000_000_000,
                &validator_keypairs,
                vec![10_000; 3],
            );
            let bank = Arc::new(Bank::new(&genesis_config));

            let (poh_recorder, _entry_receiver, _record_receiver) = PohRecorder::new(
                0,
                bank.last_blockhash(),
                0,
                Some((2, 2)),
                bank.ticks_per_slot(),
                &Pubkey::default(),
                &Arc::new(blockstore),
                &Arc::new(LeaderScheduleCache::new_from_bank(&bank)),
                &Arc::new(PohConfig::default()),
                Arc::new(AtomicBool::default()),
            );

            let node_keypair = Arc::new(Keypair::new());
            let cluster_info = Arc::new(ClusterInfo::new(
                ContactInfo::new_localhost(&node_keypair.pubkey(), timestamp()),
                node_keypair,
                SocketAddrSpace::Unspecified,
            ));

            let validator0_socket = SocketAddr::from(([127, 0, 0, 1], 1111));
            let validator1_socket = SocketAddr::from(([127, 0, 0, 1], 2222));
            let validator2_socket = SocketAddr::from(([127, 0, 0, 1], 3333));
            let recent_peers: HashMap<_, _> = vec![
                (
                    validator_vote_keypairs0.node_keypair.pubkey(),
                    validator0_socket,
                ),
                (
                    validator_vote_keypairs1.node_keypair.pubkey(),
                    validator1_socket,
                ),
                (
                    validator_vote_keypairs2.node_keypair.pubkey(),
                    validator2_socket,
                ),
            ]
            .iter()
            .cloned()
            .collect();
            let leader_info = LeaderInfo {
                cluster_info,
                poh_recorder: Arc::new(Mutex::new(poh_recorder)),
                recent_peers: recent_peers.clone(),
            };

            let slot = bank.slot();
            let first_leader =
                solana_ledger::leader_schedule_utils::slot_leader_at(slot, &bank).unwrap();
            assert_eq!(
                leader_info.get_leader_tpus(1),
                vec![recent_peers.get(&first_leader).unwrap()]
            );

            let second_leader = solana_ledger::leader_schedule_utils::slot_leader_at(
                slot + NUM_CONSECUTIVE_LEADER_SLOTS,
                &bank,
            )
            .unwrap();
            let mut expected_leader_sockets = vec![
                recent_peers.get(&first_leader).unwrap(),
                recent_peers.get(&second_leader).unwrap(),
            ];
            expected_leader_sockets.dedup();
            assert_eq!(leader_info.get_leader_tpus(2), expected_leader_sockets);

            let third_leader = solana_ledger::leader_schedule_utils::slot_leader_at(
                slot + (2 * NUM_CONSECUTIVE_LEADER_SLOTS),
                &bank,
            )
            .unwrap();
            let mut expected_leader_sockets = vec![
                recent_peers.get(&first_leader).unwrap(),
                recent_peers.get(&second_leader).unwrap(),
                recent_peers.get(&third_leader).unwrap(),
            ];
            expected_leader_sockets.dedup();
            assert_eq!(leader_info.get_leader_tpus(3), expected_leader_sockets);

            for x in 4..8 {
                assert!(leader_info.get_leader_tpus(x).len() <= recent_peers.len());
            }
        }
        Blockstore::destroy(&ledger_path).unwrap();
    }
}
