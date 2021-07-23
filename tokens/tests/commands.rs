use solana_client::rpc_client::RpcClient;
use solana_core::test_validator::TestValidator;
use solana_sdk::signature::{Keypair, Signer};
use solana_streamer::socket::SocketAddrSpace;
use solana_tokens::commands::test_process_distribute_tokens_with_client;

#[test]
fn test_process_distribute_with_rpc_client() {
    solana_logger::setup();

    let mint_keypair = Keypair::new();
    let test_validator =
        TestValidator::with_no_fees(mint_keypair.pubkey(), None, SocketAddrSpace::Unspecified);

    let client = RpcClient::new(test_validator.rpc_url());
    test_process_distribute_tokens_with_client(&client, mint_keypair, None);
}
