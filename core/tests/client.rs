use solana_client::{pubsub_client::PubsubClient, rpc_client::RpcClient, rpc_response::SlotInfo};
use solana_core::test_validator::TestValidator;
use solana_rpc::{
    optimistically_confirmed_bank_tracker::OptimisticallyConfirmedBank,
    rpc_pubsub_service::{PubSubConfig, PubSubService},
    rpc_subscriptions::RpcSubscriptions,
};
use solana_runtime::{
    bank::Bank,
    bank_forks::BankForks,
    commitment::BlockCommitmentCache,
    genesis_utils::{create_genesis_config, GenesisConfigInfo},
};
use solana_sdk::{
    commitment_config::CommitmentConfig,
    native_token::sol_to_lamports,
    rpc_port,
    signature::{Keypair, Signer},
    system_transaction,
};
use solana_streamer::socket::SocketAddrSpace;
use std::{
    net::{IpAddr, SocketAddr},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, RwLock,
    },
    thread::sleep,
    time::{Duration, Instant},
};
use systemstat::Ipv4Addr;

#[test]
fn test_rpc_client() {
    solana_logger::setup();

    let alice = Keypair::new();
    let test_validator =
        TestValidator::with_no_fees(alice.pubkey(), None, SocketAddrSpace::Unspecified);

    let bob_pubkey = solana_sdk::pubkey::new_rand();

    let client = RpcClient::new(test_validator.rpc_url());

    assert_eq!(
        client.get_version().unwrap().solana_core,
        solana_version::semver!()
    );

    assert!(client.get_account(&bob_pubkey).is_err());

    assert_eq!(client.get_balance(&bob_pubkey).unwrap(), 0);

    let original_alice_balance = client.get_balance(&alice.pubkey()).unwrap();

    let (blockhash, _fee_calculator) = client.get_recent_blockhash().unwrap();

    let tx = system_transaction::transfer(&alice, &bob_pubkey, sol_to_lamports(20.0), blockhash);
    let signature = client.send_transaction(&tx).unwrap();

    let mut confirmed_tx = false;

    let now = Instant::now();
    while now.elapsed().as_secs() <= 20 {
        let response = client
            .confirm_transaction_with_commitment(&signature, CommitmentConfig::default())
            .unwrap();

        if response.value {
            confirmed_tx = true;
            break;
        }

        sleep(Duration::from_millis(500));
    }

    assert!(confirmed_tx);

    assert_eq!(
        client.get_balance(&bob_pubkey).unwrap(),
        sol_to_lamports(20.0)
    );
    assert_eq!(
        client.get_balance(&alice.pubkey()).unwrap(),
        original_alice_balance - sol_to_lamports(20.0)
    );
}

#[test]
fn test_slot_subscription() {
    let pubsub_addr = SocketAddr::new(
        IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)),
        rpc_port::DEFAULT_RPC_PUBSUB_PORT,
    );
    let exit = Arc::new(AtomicBool::new(false));
    let GenesisConfigInfo { genesis_config, .. } = create_genesis_config(10_000);
    let bank = Bank::new(&genesis_config);
    let bank_forks = Arc::new(RwLock::new(BankForks::new(bank)));
    let optimistically_confirmed_bank =
        OptimisticallyConfirmedBank::locked_from_bank_forks_root(&bank_forks);
    let subscriptions = Arc::new(RpcSubscriptions::new(
        &exit,
        bank_forks,
        Arc::new(RwLock::new(BlockCommitmentCache::default())),
        optimistically_confirmed_bank,
    ));
    let pubsub_service =
        PubSubService::new(PubSubConfig::default(), &subscriptions, pubsub_addr, &exit);
    std::thread::sleep(Duration::from_millis(400));

    let (mut client, receiver) =
        PubsubClient::slot_subscribe(&format!("ws://0.0.0.0:{}/", pubsub_addr.port())).unwrap();

    let mut errors: Vec<(SlotInfo, SlotInfo)> = Vec::new();

    for i in 0..3 {
        subscriptions.notify_slot(i + 1, i, i);

        let maybe_actual = receiver.recv_timeout(Duration::from_millis(400));

        match maybe_actual {
            Ok(actual) => {
                let expected = SlotInfo {
                    slot: i + 1,
                    parent: i,
                    root: i,
                };

                if actual != expected {
                    errors.push((actual, expected));
                }
            }
            Err(_err) => {
                eprintln!("unexpected websocket receive timeout");
                break;
            }
        }
    }

    exit.store(true, Ordering::Relaxed);
    client.shutdown().unwrap();
    pubsub_service.close().unwrap();

    assert_eq!(errors, [].to_vec());
}
