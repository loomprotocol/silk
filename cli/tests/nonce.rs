use solana_cli::{
    cli::{process_command, request_and_confirm_airdrop, CliCommand, CliConfig},
    spend_utils::SpendAmount,
    test_utils::{check_ready, check_recent_balance},
};
use solana_cli_output::{parse_sign_only_reply_string, OutputFormat};
use solana_client::{
    blockhash_query::{self, BlockhashQuery},
    nonce_utils,
    rpc_client::RpcClient,
};
use solana_core::test_validator::TestValidator;
use solana_faucet::faucet::run_local_faucet;
use solana_sdk::{
    commitment_config::CommitmentConfig,
    hash::Hash,
    pubkey::Pubkey,
    signature::{keypair_from_seed, Keypair, Signer},
    system_program,
};
use solana_streamer::socket::SocketAddrSpace;

#[test]
fn test_nonce() {
    let mint_keypair = Keypair::new();
    let mint_pubkey = mint_keypair.pubkey();
    let faucet_addr = run_local_faucet(mint_keypair, None);
    let test_validator =
        TestValidator::with_no_fees(mint_pubkey, Some(faucet_addr), SocketAddrSpace::Unspecified);

    full_battery_tests(test_validator, None, false);
}

#[test]
fn test_nonce_with_seed() {
    let mint_keypair = Keypair::new();
    let mint_pubkey = mint_keypair.pubkey();
    let faucet_addr = run_local_faucet(mint_keypair, None);
    let test_validator =
        TestValidator::with_no_fees(mint_pubkey, Some(faucet_addr), SocketAddrSpace::Unspecified);

    full_battery_tests(test_validator, Some(String::from("seed")), false);
}

#[test]
fn test_nonce_with_authority() {
    let mint_keypair = Keypair::new();
    let mint_pubkey = mint_keypair.pubkey();
    let faucet_addr = run_local_faucet(mint_keypair, None);
    let test_validator =
        TestValidator::with_no_fees(mint_pubkey, Some(faucet_addr), SocketAddrSpace::Unspecified);

    full_battery_tests(test_validator, None, true);
}

fn full_battery_tests(
    test_validator: TestValidator,
    seed: Option<String>,
    use_nonce_authority: bool,
) {
    let rpc_client =
        RpcClient::new_with_commitment(test_validator.rpc_url(), CommitmentConfig::processed());
    let json_rpc_url = test_validator.rpc_url();

    let mut config_payer = CliConfig::recent_for_tests();
    config_payer.json_rpc_url = json_rpc_url.clone();
    let payer = Keypair::new();
    config_payer.signers = vec![&payer];

    request_and_confirm_airdrop(
        &rpc_client,
        &config_payer,
        &config_payer.signers[0].pubkey(),
        2000,
    )
    .unwrap();
    check_recent_balance(2000, &rpc_client, &config_payer.signers[0].pubkey());

    let mut config_nonce = CliConfig::recent_for_tests();
    config_nonce.json_rpc_url = json_rpc_url;
    let nonce_keypair = keypair_from_seed(&[0u8; 32]).unwrap();
    config_nonce.signers = vec![&nonce_keypair];

    let nonce_account = if let Some(seed) = seed.as_ref() {
        Pubkey::create_with_seed(
            &config_nonce.signers[0].pubkey(),
            seed,
            &system_program::id(),
        )
        .unwrap()
    } else {
        nonce_keypair.pubkey()
    };

    let nonce_authority = Keypair::new();
    let optional_authority = if use_nonce_authority {
        Some(nonce_authority.pubkey())
    } else {
        None
    };

    // Create nonce account
    config_payer.signers.push(&nonce_keypair);
    config_payer.command = CliCommand::CreateNonceAccount {
        nonce_account: 1,
        seed,
        nonce_authority: optional_authority,
        memo: None,
        amount: SpendAmount::Some(1000),
    };

    process_command(&config_payer).unwrap();
    check_recent_balance(1000, &rpc_client, &config_payer.signers[0].pubkey());
    check_recent_balance(1000, &rpc_client, &nonce_account);

    // Get nonce
    config_payer.signers.pop();
    config_payer.command = CliCommand::GetNonce(nonce_account);
    let first_nonce_string = process_command(&config_payer).unwrap();
    let first_nonce = first_nonce_string.parse::<Hash>().unwrap();

    // Get nonce
    config_payer.command = CliCommand::GetNonce(nonce_account);
    let second_nonce_string = process_command(&config_payer).unwrap();
    let second_nonce = second_nonce_string.parse::<Hash>().unwrap();

    assert_eq!(first_nonce, second_nonce);

    let mut authorized_signers: Vec<&dyn Signer> = vec![&payer];
    let index = if use_nonce_authority {
        authorized_signers.push(&nonce_authority);
        1
    } else {
        0
    };

    // New nonce
    config_payer.signers = authorized_signers.clone();
    config_payer.command = CliCommand::NewNonce {
        nonce_account,
        nonce_authority: index,
        memo: None,
    };
    process_command(&config_payer).unwrap();

    // Get nonce
    config_payer.signers = vec![&payer];
    config_payer.command = CliCommand::GetNonce(nonce_account);
    let third_nonce_string = process_command(&config_payer).unwrap();
    let third_nonce = third_nonce_string.parse::<Hash>().unwrap();

    assert_ne!(first_nonce, third_nonce);

    // Withdraw from nonce account
    let payee_pubkey = solana_sdk::pubkey::new_rand();
    config_payer.signers = authorized_signers;
    config_payer.command = CliCommand::WithdrawFromNonceAccount {
        nonce_account,
        nonce_authority: index,
        memo: None,
        destination_account_pubkey: payee_pubkey,
        lamports: 100,
    };
    process_command(&config_payer).unwrap();
    check_recent_balance(1000, &rpc_client, &config_payer.signers[0].pubkey());
    check_recent_balance(900, &rpc_client, &nonce_account);
    check_recent_balance(100, &rpc_client, &payee_pubkey);

    // Show nonce account
    config_payer.command = CliCommand::ShowNonceAccount {
        nonce_account_pubkey: nonce_account,
        use_lamports_unit: true,
    };
    process_command(&config_payer).unwrap();

    // Set new authority
    let new_authority = Keypair::new();
    config_payer.command = CliCommand::AuthorizeNonceAccount {
        nonce_account,
        nonce_authority: index,
        memo: None,
        new_authority: new_authority.pubkey(),
    };
    process_command(&config_payer).unwrap();

    // Old authority fails now
    config_payer.command = CliCommand::NewNonce {
        nonce_account,
        nonce_authority: index,
        memo: None,
    };
    process_command(&config_payer).unwrap_err();

    // New authority can advance nonce
    config_payer.signers = vec![&payer, &new_authority];
    config_payer.command = CliCommand::NewNonce {
        nonce_account,
        nonce_authority: 1,
        memo: None,
    };
    process_command(&config_payer).unwrap();

    // New authority can withdraw from nonce account
    config_payer.command = CliCommand::WithdrawFromNonceAccount {
        nonce_account,
        nonce_authority: 1,
        memo: None,
        destination_account_pubkey: payee_pubkey,
        lamports: 100,
    };
    process_command(&config_payer).unwrap();
    check_recent_balance(1000, &rpc_client, &config_payer.signers[0].pubkey());
    check_recent_balance(800, &rpc_client, &nonce_account);
    check_recent_balance(200, &rpc_client, &payee_pubkey);
}

#[test]
fn test_create_account_with_seed() {
    solana_logger::setup();
    let mint_keypair = Keypair::new();
    let mint_pubkey = mint_keypair.pubkey();
    let faucet_addr = run_local_faucet(mint_keypair, None);
    let test_validator = TestValidator::with_custom_fees(
        mint_pubkey,
        1,
        Some(faucet_addr),
        SocketAddrSpace::Unspecified,
    );

    let offline_nonce_authority_signer = keypair_from_seed(&[1u8; 32]).unwrap();
    let online_nonce_creator_signer = keypair_from_seed(&[2u8; 32]).unwrap();
    let to_address = Pubkey::new(&[3u8; 32]);

    // Setup accounts
    let rpc_client =
        RpcClient::new_with_commitment(test_validator.rpc_url(), CommitmentConfig::processed());
    request_and_confirm_airdrop(
        &rpc_client,
        &CliConfig::recent_for_tests(),
        &offline_nonce_authority_signer.pubkey(),
        42,
    )
    .unwrap();
    request_and_confirm_airdrop(
        &rpc_client,
        &CliConfig::recent_for_tests(),
        &online_nonce_creator_signer.pubkey(),
        4242,
    )
    .unwrap();
    check_recent_balance(42, &rpc_client, &offline_nonce_authority_signer.pubkey());
    check_recent_balance(4242, &rpc_client, &online_nonce_creator_signer.pubkey());
    check_recent_balance(0, &rpc_client, &to_address);

    check_ready(&rpc_client);

    // Create nonce account
    let creator_pubkey = online_nonce_creator_signer.pubkey();
    let authority_pubkey = offline_nonce_authority_signer.pubkey();
    let seed = authority_pubkey.to_string()[0..32].to_string();
    let nonce_address =
        Pubkey::create_with_seed(&creator_pubkey, &seed, &system_program::id()).unwrap();
    check_recent_balance(0, &rpc_client, &nonce_address);

    let mut creator_config = CliConfig::recent_for_tests();
    creator_config.json_rpc_url = test_validator.rpc_url();
    creator_config.signers = vec![&online_nonce_creator_signer];
    creator_config.command = CliCommand::CreateNonceAccount {
        nonce_account: 0,
        seed: Some(seed),
        nonce_authority: Some(authority_pubkey),
        memo: None,
        amount: SpendAmount::Some(241),
    };
    process_command(&creator_config).unwrap();
    check_recent_balance(241, &rpc_client, &nonce_address);
    check_recent_balance(42, &rpc_client, &offline_nonce_authority_signer.pubkey());
    check_recent_balance(4000, &rpc_client, &online_nonce_creator_signer.pubkey());
    check_recent_balance(0, &rpc_client, &to_address);

    // Fetch nonce hash
    let nonce_hash = nonce_utils::get_account_with_commitment(
        &rpc_client,
        &nonce_address,
        CommitmentConfig::processed(),
    )
    .and_then(|ref a| nonce_utils::data_from_account(a))
    .unwrap()
    .blockhash;

    // Test by creating transfer TX with nonce, fully offline
    let mut authority_config = CliConfig::recent_for_tests();
    authority_config.json_rpc_url = String::default();
    authority_config.signers = vec![&offline_nonce_authority_signer];
    // Verify we cannot contact the cluster
    authority_config.command = CliCommand::ClusterVersion;
    process_command(&authority_config).unwrap_err();
    authority_config.command = CliCommand::Transfer {
        amount: SpendAmount::Some(10),
        to: to_address,
        from: 0,
        sign_only: true,
        dump_transaction_message: true,
        allow_unfunded_recipient: true,
        no_wait: false,
        blockhash_query: BlockhashQuery::None(nonce_hash),
        nonce_account: Some(nonce_address),
        nonce_authority: 0,
        memo: None,
        fee_payer: 0,
        derived_address_seed: None,
        derived_address_program_id: None,
    };
    authority_config.output_format = OutputFormat::JsonCompact;
    let sign_only_reply = process_command(&authority_config).unwrap();
    let sign_only = parse_sign_only_reply_string(&sign_only_reply);
    let authority_presigner = sign_only.presigner_of(&authority_pubkey).unwrap();
    assert_eq!(sign_only.blockhash, nonce_hash);

    // And submit it
    let mut submit_config = CliConfig::recent_for_tests();
    submit_config.json_rpc_url = test_validator.rpc_url();
    submit_config.signers = vec![&authority_presigner];
    submit_config.command = CliCommand::Transfer {
        amount: SpendAmount::Some(10),
        to: to_address,
        from: 0,
        sign_only: false,
        dump_transaction_message: true,
        allow_unfunded_recipient: true,
        no_wait: false,
        blockhash_query: BlockhashQuery::FeeCalculator(
            blockhash_query::Source::NonceAccount(nonce_address),
            sign_only.blockhash,
        ),
        nonce_account: Some(nonce_address),
        nonce_authority: 0,
        memo: None,
        fee_payer: 0,
        derived_address_seed: None,
        derived_address_program_id: None,
    };
    process_command(&submit_config).unwrap();
    check_recent_balance(241, &rpc_client, &nonce_address);
    check_recent_balance(31, &rpc_client, &offline_nonce_authority_signer.pubkey());
    check_recent_balance(4000, &rpc_client, &online_nonce_creator_signer.pubkey());
    check_recent_balance(10, &rpc_client, &to_address);
}
