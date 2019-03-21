//! Exchange program

use bincode;
use log::*;
use solana_exchange_api::exchange_instruction::*;
use solana_exchange_api::exchange_state::*;
use solana_sdk::account::KeyedAccount;
use solana_sdk::pubkey::Pubkey;
use solana_sdk::solana_entrypoint;
use solana_sdk::transaction::InstructionError;
use std::cmp;

pub struct ExchangeProgram {}

impl ExchangeProgram {
    #[allow(clippy::needless_pass_by_value)]
    fn map_to_invalid_arg(err: std::boxed::Box<bincode::ErrorKind>) -> InstructionError {
        warn!("Deserialze failed: {:?}", err);
        InstructionError::InvalidArgument
    }

    fn is_account_unallocated(data: &[u8]) -> Result<(), InstructionError> {
        let state: ExchangeState = bincode::deserialize(data).map_err(Self::map_to_invalid_arg)?;
        match state {
            ExchangeState::Unallocated => Ok(()),
            _ => {
                error!("New account is already in use");
                Err(InstructionError::InvalidAccountData)?
            }
        }
    }

    fn deserialize_account(data: &[u8]) -> Result<(TokenAccountInfo), InstructionError> {
        let state: ExchangeState = bincode::deserialize(data).map_err(Self::map_to_invalid_arg)?;
        match state {
            ExchangeState::Account(account) => Ok(account),
            _ => {
                error!("Not a valid account");
                Err(InstructionError::InvalidAccountData)?
            }
        }
    }

    fn deserialize_trade(data: &[u8]) -> Result<(TradeOrderInfo), InstructionError> {
        let state: ExchangeState = bincode::deserialize(data).map_err(Self::map_to_invalid_arg)?;
        match state {
            ExchangeState::Trade(info) => Ok(info),
            _ => {
                error!("Not a valid trade");
                Err(InstructionError::InvalidAccountData)?
            }
        }
    }

    fn serialize(state: &ExchangeState, data: &mut [u8]) -> Result<(), InstructionError> {
        let writer = std::io::BufWriter::new(data);
        match bincode::serialize_into(writer, state) {
            Ok(_) => Ok(()),
            Err(e) => {
                error!("Serialize failed: {:?}", e);
                Err(InstructionError::GenericError)?
            }
        }
    }

    fn calculate_swap(
        scaler: u64,
        swap: &mut TradeSwapInfo,
        to_trade: &mut TradeOrderInfo,
        from_trade: &mut TradeOrderInfo,
        to_trade_account: &mut TokenAccountInfo,
        from_trade_account: &mut TokenAccountInfo,
        profit_account: &mut TokenAccountInfo,
    ) -> Result<(), InstructionError> {
        if to_trade.tokens == 0 || from_trade.tokens == 0 {
            error!("Inactive Trade, balance is zero");
            Err(InstructionError::InvalidArgument)?
        }
        if to_trade.price == 0 || from_trade.price == 0 {
            error!("Inactive Trade, price is zero");
            Err(InstructionError::InvalidArgument)?
        }
        if to_trade.price > from_trade.price {
            error!("From trade price greater then to price");
            Err(InstructionError::InvalidArgument)?
        }

        // Calc swap

        let max_from_primary = from_trade.tokens * scaler / from_trade.price;
        let max_to_secondary = to_trade.tokens * to_trade.price / scaler;

        trace!("mfp {} mts {}", max_from_primary, max_to_secondary);

        let max_primary = cmp::min(max_from_primary, to_trade.tokens);
        let max_secondary = cmp::min(max_to_secondary, from_trade.tokens);

        trace!("mp {} ms {}", max_primary, max_secondary);

        let primary_tokens = if max_secondary < max_primary {
            max_secondary * scaler / from_trade.price
        } else {
            max_primary
        };
        let secondary_tokens = if max_secondary < max_primary {
            max_secondary
        } else {
            max_primary * to_trade.price / scaler
        };

        if primary_tokens == 0 || secondary_tokens == 0 {
            error!("Trade quantities to low to be fulfilled");
            Err(InstructionError::InvalidArgument)?
        }

        trace!("pt {} st {}", primary_tokens, secondary_tokens);

        let primary_cost = cmp::max(primary_tokens, secondary_tokens * scaler / to_trade.price);
        let secondary_cost = cmp::max(secondary_tokens, primary_tokens * from_trade.price / scaler);

        trace!("pc {} sc {}", primary_cost, secondary_cost);

        let primary_profit = primary_cost - primary_tokens;
        let secondary_profit = secondary_cost - secondary_tokens;

        trace!("pp {} sp {}", primary_profit, secondary_profit);

        let primary_token = from_trade.pair.0;
        let secondary_token = from_trade.pair.1;

        // Update tokens/accounts

        from_trade.tokens -= secondary_cost;
        to_trade.tokens -= primary_cost;

        to_trade_account.tokens[secondary_token] += secondary_tokens;
        from_trade_account.tokens[primary_token] += primary_tokens;

        profit_account.tokens[primary_token] += primary_profit;
        profit_account.tokens[secondary_token] += secondary_profit;

        swap.primary_token = primary_token;
        swap.primary_tokens = primary_cost;
        swap.primary_price = to_trade.price;
        swap.secondary_token = secondary_token;
        swap.secondary_tokens = secondary_cost;
        swap.secondary_price = from_trade.price;

        Ok(())
    }

    fn do_account_request(ka: &mut [KeyedAccount]) -> Result<(), InstructionError> {
        if ka.len() < 2 {
            error!("Not enough accounts");
            Err(InstructionError::InvalidArgument)?
        }

        Self::is_account_unallocated(&ka[1].account.data[..])?;
        Self::serialize(
            &ExchangeState::Account(
                TokenAccountInfo::default().owner(ka[0].unsigned_key().clone()),
            ),
            &mut ka[1].account.data[..],
        )
    }

    fn do_transfer_request(
        ka: &mut [KeyedAccount],
        token: Token,
        tokens: u64,
    ) -> Result<(), InstructionError> {
        if ka.len() < 3 {
            error!("Not enough accounts");
            Err(InstructionError::InvalidArgument)?
        }

        let mut to_account = Self::deserialize_account(&ka[1].account.data[..])?;

        if &solana_exchange_api::id() == ka[2].unsigned_key() {
            to_account.tokens[token] += tokens;
        } else {
            let mut from_account = Self::deserialize_account(&ka[2].account.data[..])?;

            if &from_account.owner != ka[0].unsigned_key() {
                error!("Signer does not own from account");
                Err(InstructionError::GenericError)?
            }

            if from_account.tokens[token] < tokens {
                error!("From account balance too low");
                Err(InstructionError::GenericError)?
            }

            from_account.tokens[token] -= tokens;
            to_account.tokens[token] += tokens;

            Self::serialize(
                &ExchangeState::Account(from_account),
                &mut ka[1].account.data[..],
            )?;
        }

        Self::serialize(
            &ExchangeState::Account(to_account),
            &mut ka[1].account.data[..],
        )
    }

    fn do_trade_request(
        ka: &mut [KeyedAccount],
        info: TradeRequestInfo,
    ) -> Result<(), InstructionError> {
        if ka.len() < 3 {
            error!("Not enough accounts");
            Err(InstructionError::InvalidArgument)?
        }

        Self::is_account_unallocated(&ka[1].account.data[..])?;

        let mut account = Self::deserialize_account(&ka[2].account.data[..])?;

        if info.primary_token == info.secondary_token {
            error!("Cannot trade like tokens");
            Err(InstructionError::GenericError)?
        }
        if &account.owner != ka[0].unsigned_key() {
            error!("Signer does not own To/From account");
            Err(InstructionError::GenericError)?
        }
        let from_token = match info.direction {
            Direction::To => info.primary_token,
            Direction::From => info.secondary_token,
        };
        if account.tokens[from_token] < info.tokens {
            error!("From token balance is too low");
            Err(InstructionError::GenericError)?
        }

        // Trade holds the tokens in escrow
        account.tokens[from_token] -= info.tokens;

        Self::serialize(
            &ExchangeState::Trade(TradeOrderInfo {
                owner: *ka[0].unsigned_key(),
                direction: info.direction,
                pair: (info.primary_token, info.secondary_token),
                tokens: info.tokens,
                price: info.price,
                src_account: *ka[2].unsigned_key(),
                dst_account: info.dst_account,
            }),
            &mut ka[1].account.data[..],
        )?;
        Self::serialize(
            &ExchangeState::Account(account),
            &mut ka[2].account.data[..],
        )
    }

    fn do_trade_cancellation(ka: &mut [KeyedAccount]) -> Result<(), InstructionError> {
        if ka.len() < 3 {
            error!("Not enough accounts");
            Err(InstructionError::InvalidArgument)?
        }
        let mut trade = Self::deserialize_trade(&ka[1].account.data[..])?;
        let mut account = Self::deserialize_account(&ka[2].account.data[..])?;

        if &trade.owner != ka[0].unsigned_key() {
            error!("Signer does not own trade");
            Err(InstructionError::GenericError)?
        }

        if &account.owner != ka[0].unsigned_key() {
            error!("Signer does not own account");
            Err(InstructionError::GenericError)?
        }

        let token = match trade.direction {
            Direction::To => trade.pair.0,
            Direction::From => trade.pair.1,
        };

        // Outstanding tokens transferred back to account
        account.tokens[token] += trade.tokens;
        // Trade becomes invalid
        trade.tokens = 0;

        Self::serialize(&ExchangeState::Trade(trade), &mut ka[1].account.data[..])?;
        Self::serialize(
            &ExchangeState::Account(account),
            &mut ka[2].account.data[..],
        )
    }

    fn do_swap_request(ka: &mut [KeyedAccount]) -> Result<(), InstructionError> {
        if ka.len() < 7 {
            error!("Not enough accounts");
            Err(InstructionError::InvalidArgument)?
        }
        Self::is_account_unallocated(&ka[1].account.data[..])?;
        let mut to_trade = Self::deserialize_trade(&ka[2].account.data[..])?;
        let mut from_trade = Self::deserialize_trade(&ka[3].account.data[..])?;
        let mut to_trade_account = Self::deserialize_account(&ka[4].account.data[..])?;
        let mut from_trade_account = Self::deserialize_account(&ka[5].account.data[..])?;
        let mut profit_account = Self::deserialize_account(&ka[6].account.data[..])?;

        if &to_trade.dst_account != ka[4].unsigned_key() {
            error!("To trade account and to account differ");
            Err(InstructionError::InvalidArgument)?
        }
        if &from_trade.dst_account != ka[5].unsigned_key() {
            error!("From trade account and from account differ");
            Err(InstructionError::InvalidArgument)?
        }
        if to_trade.direction != Direction::To {
            error!("To trade is not a To");
            Err(InstructionError::InvalidArgument)?
        }
        if from_trade.direction != Direction::From {
            error!("From trade is not a From");
            Err(InstructionError::InvalidArgument)?
        }
        if to_trade.pair != from_trade.pair {
            error!("Mismatched token pairs");
            Err(InstructionError::InvalidArgument)?
        }
        if to_trade.direction == from_trade.direction {
            error!("Matching trade directions");
            Err(InstructionError::InvalidArgument)?
        }

        let mut swap = TradeSwapInfo::default();
        swap.to_trade_order = *ka[2].unsigned_key();
        swap.from_trade_order = *ka[3].unsigned_key();

        if let Err(e) = ExchangeProgram::calculate_swap(
            SCALER,
            &mut swap,
            &mut from_trade,
            &mut to_trade,
            &mut from_trade_account,
            &mut to_trade_account,
            &mut profit_account,
        ) {
            error!(
                "Swap calculation failed from {} for {} to {} for {}",
                from_trade.tokens, from_trade.price, to_trade.tokens, to_trade.price,
            );
            Err(e)?
        }

        Self::serialize(&ExchangeState::Swap(swap), &mut ka[1].account.data[..])?;
        Self::serialize(&ExchangeState::Trade(to_trade), &mut ka[2].account.data[..])?;
        Self::serialize(
            &ExchangeState::Trade(from_trade),
            &mut ka[3].account.data[..],
        )?;
        Self::serialize(
            &ExchangeState::Account(to_trade_account),
            &mut ka[4].account.data[..],
        )?;
        Self::serialize(
            &ExchangeState::Account(from_trade_account),
            &mut ka[5].account.data[..],
        )?;
        Self::serialize(
            &ExchangeState::Account(profit_account),
            &mut ka[6].account.data[..],
        )
    }

    pub fn process_instruction(
        _program_id: &Pubkey,
        ka: &mut [KeyedAccount],
        data: &[u8],
    ) -> Result<(), InstructionError> {
        let command =
            bincode::deserialize::<ExchangeInstruction>(data).map_err(Self::map_to_invalid_arg)?;

        match command {
            ExchangeInstruction::AccountRequest => Self::do_account_request(ka),
            ExchangeInstruction::TransferRequest(token, tokens) => {
                Self::do_transfer_request(ka, token, tokens)
            }
            ExchangeInstruction::TradeRequest(info) => Self::do_trade_request(ka, info),
            ExchangeInstruction::TradeCancellation => Self::do_trade_cancellation(ka),
            ExchangeInstruction::SwapRequest => Self::do_swap_request(ka),
        }
    }
}

solana_entrypoint!(entrypoint);
fn entrypoint(
    program_id: &Pubkey,
    keyed_accounts: &mut [KeyedAccount],
    data: &[u8],
    _tick_height: u64,
) -> Result<(), InstructionError> {
    solana_logger::setup();

    // TODO swap does not require this, plus isn't this always enforced by
    // the layers above?
    // All exchange instructions require that accounts_keys[0] be a signer
    if keyed_accounts[0].signer_key().is_none() {
        error!("account[0] is unsigned");
        Err(InstructionError::InvalidArgument)?;
    }

    ExchangeProgram::process_instruction(program_id, keyed_accounts, data)
        .map_err(|_| InstructionError::GenericError)
}

#[cfg(test)]
mod test {
    use super::*;

    fn try_calc(
        scaler: u64,
        primary_tokens: u64,
        primary_price: u64,
        secondary_tokens: u64,
        secondary_price: u64,
        primary_tokens_expect: u64,
        secondary_tokens_expect: u64,
        primary_tokens: Tokens,
        secondary_tokens: Tokens,
        profit_tokens: Tokens,
    ) -> Result<(), InstructionError> {
        trace!(
            "Swap {} {} for {} to {} for {}",
            direction,
            primary_tokens,
            primary_price,
            secondary_tokens,
            secondary_price,
        );
        let mut swap = TradeSwapInfo::default();
        let mut to_trade = TradeOrderInfo::default();
        let mut from_trade = TradeOrderInfo::default().direction(Direction::From);
        let mut to_account = TokenAccountInfo::default();
        let mut from_account = TokenAccountInfo::default();
        let mut profit_account = TokenAccountInfo::default();

        to_trade.tokens = primary_tokens;
        to_trade.price = primary_price;
        from_trade.tokens = secondary_tokens;
        from_trade.price = secondary_price;
        ExchangeProgram::calculate_swap(
            scaler,
            &mut swap,
            &mut to_trade,
            &mut from_trade,
            &mut to_trade_account,
            &mut from_trade_account,
            &mut profit_account,
        )?;

        trace!(
            "{:?} {:?} {:?} {:?}\n{:?}\n{:?}\n{:?}\n{:?}\n{:?}\n{:?}",
            to_trade.tokens,
            primary_tokens_expect,
            from_trade.tokens,
            secondary_tokens_expect,
            to_trade_account.tokens,
            primary_tokens,
            from_trade_account.tokens,
            secondary_tokens,
            profit_account.tokens,
            profit_tokens
        );

        assert_eq!(to_trade.tokens, primary_tokens_expect);
        assert_eq!(from_trade.tokens, secondary_tokens_expect);
        assert_eq!(to_trade_account.tokens, primary_tokens);
        assert_eq!(from_trade_account.tokens, secondary_tokens);
        assert_eq!(profit_account.tokens, profit_tokens);
        assert_eq!(swap.primary_tokens, primary_tokens - to_trade.tokens);
        assert_eq!(swap.primary_price, to_trade.price);
        assert_eq!(swap.secondary_tokens, secondary_tokens - from_trade.tokens);
        assert_eq!(swap.secondary_price, from_trade.price);
        Ok(())
    }

    #[test]
    #[rustfmt::skip]
    fn test_calculate_swap() {
        solana_logger::setup();

        try_calc(1,     50,     2,   50,    1,  0, 0, Tokens::new(0,  50, 0, 0), Tokens::new(  50, 0, 0, 0), Tokens::new(   0, 0, 0, 0)).unwrap_err();
        try_calc(1,     50,     1,    0,    1,  0, 0, Tokens::new(0,  50, 0, 0), Tokens::new(  50, 0, 0, 0), Tokens::new(   0, 0, 0, 0)).unwrap_err();
        try_calc(1,      0,     1,   50,    1,  0, 0, Tokens::new(0,  50, 0, 0), Tokens::new(  50, 0, 0, 0), Tokens::new(   0, 0, 0, 0)).unwrap_err();
        try_calc(1,     50,     1,   50,    0,  0, 0, Tokens::new(0,  50, 0, 0), Tokens::new(  50, 0, 0, 0), Tokens::new(   0, 0, 0, 0)).unwrap_err();
        try_calc(1,     50,     0,   50,    1,  0, 0, Tokens::new(0,  50, 0, 0), Tokens::new(  50, 0, 0, 0), Tokens::new(   0, 0, 0, 0)).unwrap_err();
        try_calc(1,       1,    2,    2,    3,  1, 2, Tokens::new(0,   0, 0, 0), Tokens::new(   0, 0, 0, 0), Tokens::new(   0, 0, 0, 0)).unwrap_err();

        try_calc(1,     50,     1,   50,    1,  0, 0, Tokens::new(0,  50, 0, 0), Tokens::new(  50, 0, 0, 0), Tokens::new(   0, 0, 0, 0)).unwrap();
        try_calc(1,       1,    2,    3,    3,  0, 0, Tokens::new(0,   2, 0, 0), Tokens::new(   1, 0, 0, 0), Tokens::new(   0, 1, 0, 0)).unwrap();
        try_calc(1,       2,    2,    3,    3,  1, 0, Tokens::new(0,   2, 0, 0), Tokens::new(   1, 0, 0, 0), Tokens::new(   0, 1, 0, 0)).unwrap();
        try_calc(1,       3,    2,    3,    3,  2, 0, Tokens::new(0,   2, 0, 0), Tokens::new(   1, 0, 0, 0), Tokens::new(   0, 1, 0, 0)).unwrap();
        try_calc(1,       3,    2,    6,    3,  1, 0, Tokens::new(0,   4, 0, 0), Tokens::new(   2, 0, 0, 0), Tokens::new(   0, 2, 0, 0)).unwrap();
        try_calc(1000,    1, 2000,    3, 3000,  0, 0, Tokens::new(0,   2, 0, 0), Tokens::new(   1, 0, 0, 0), Tokens::new(   0, 1, 0, 0)).unwrap();
        try_calc(1,       3,    2,    7,    3,  1, 1, Tokens::new(0,   4, 0, 0), Tokens::new(   2, 0, 0, 0), Tokens::new(   0, 2, 0, 0)).unwrap();
        try_calc(1000, 3000,  333, 1000,  500,  0, 1, Tokens::new(0, 999, 0, 0), Tokens::new(1998, 0, 0, 0), Tokens::new(1002, 0, 0, 0)).unwrap();
        try_calc(1000,   50,  100,   50,  101,  0,45, Tokens::new(0,   5, 0, 0), Tokens::new(  49, 0, 0, 0), Tokens::new(   1, 0, 0, 0)).unwrap();
    }
}
