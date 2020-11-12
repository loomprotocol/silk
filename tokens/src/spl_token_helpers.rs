use crate::{
    args::{DistributeTokensArgs, SplTokenArgs},
    commands::{Allocation, Error, FundingSource},
};
use solana_account_decoder::parse_token::{pubkey_from_spl_token_v2_0, token_amount_to_ui_amount};
use solana_client::rpc_client::RpcClient;
use solana_sdk::native_token::lamports_to_sol;
use spl_token_v2_0::{
    solana_program::program_pack::Pack,
    state::{Account as SplTokenAccount, Mint},
};

pub fn update_token_args(client: &RpcClient, args: &mut Option<SplTokenArgs>) -> Result<(), Error> {
    if let Some(spl_token_args) = args {
        let sender_account = client
            .get_account(&spl_token_args.token_account_address)
            .unwrap_or_default();
        let mint_address =
            pubkey_from_spl_token_v2_0(&SplTokenAccount::unpack(&sender_account.data)?.mint);
        spl_token_args.mint = mint_address;
        update_decimals(client, args)?;
    }
    Ok(())
}

pub fn update_decimals(client: &RpcClient, args: &mut Option<SplTokenArgs>) -> Result<(), Error> {
    if let Some(spl_token_args) = args {
        let mint_account = client.get_account(&spl_token_args.mint).unwrap_or_default();
        let mint = Mint::unpack(&mint_account.data)?;
        spl_token_args.decimals = mint.decimals;
    }
    Ok(())
}

pub fn spl_token_amount(amount: f64, decimals: u8) -> u64 {
    (amount * 10_usize.pow(decimals as u32) as f64) as u64
}

pub fn check_spl_token_balances(
    num_signatures: usize,
    allocations: &[Allocation],
    client: &RpcClient,
    args: &DistributeTokensArgs,
    created_accounts: u64,
) -> Result<(), Error> {
    let spl_token_args = args
        .spl_token_args
        .as_ref()
        .expect("spl_token_args must be some");
    let undistributed_tokens: f64 = allocations.iter().map(|x| x.amount).sum();
    let allocation_amount = spl_token_amount(undistributed_tokens, spl_token_args.decimals);

    let fee_calculator = client.get_recent_blockhash()?.1;
    let fees = fee_calculator
        .lamports_per_signature
        .checked_mul(num_signatures as u64)
        .unwrap();

    let token_account_rent_exempt_balance =
        client.get_minimum_balance_for_rent_exemption(SplTokenAccount::LEN)?;
    let account_creation_amount = created_accounts * token_account_rent_exempt_balance;
    let fee_payer_balance = client.get_balance(&args.fee_payer.pubkey())?;
    if fee_payer_balance < fees + account_creation_amount {
        return Err(Error::InsufficientFunds(
            vec![FundingSource::FeePayer].into(),
            lamports_to_sol(fees + account_creation_amount),
        ));
    }
    let source_token_account = client
        .get_account(&spl_token_args.token_account_address)
        .unwrap_or_default();
    let source_token = SplTokenAccount::unpack(&source_token_account.data)?;
    if source_token.amount < allocation_amount {
        return Err(Error::InsufficientFunds(
            vec![FundingSource::SplTokenAccount].into(),
            token_amount_to_ui_amount(allocation_amount, spl_token_args.decimals).ui_amount,
        ));
    }
    Ok(())
}
