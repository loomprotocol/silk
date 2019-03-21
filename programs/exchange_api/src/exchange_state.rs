use serde_derive::{Deserialize, Serialize};
use solana_sdk::pubkey::Pubkey;
use std::fmt;

/// Fixed-point scaler, 10 = one base 10 digit to the right of the decimal, 100 = 2, ...
/// Used by both price and amount in their fixed point representation
pub const SCALER: u64 = 1000;

/// Supported token types
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum Token {
    A,
    B,
    C,
    D,
}
impl Default for Token {
    fn default() -> Token {
        Token::A
    }
}

// Values of tokens, could be quantities, prices, etc...
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[allow(non_snake_case)]
pub struct Tokens {
    pub A: u64,
    pub B: u64,
    pub C: u64,
    pub D: u64,
}
impl Tokens {
    pub fn new(a: u64, b: u64, c: u64, d: u64) -> Self {
        Tokens {
            A: a,
            B: b,
            C: c,
            D: d,
        }
    }
}
impl std::ops::Index<Token> for Tokens {
    type Output = u64;
    fn index(&self, t: Token) -> &u64 {
        match t {
            Token::A => &self.A,
            Token::B => &self.B,
            Token::C => &self.C,
            Token::D => &self.D,
        }
    }
}
impl std::ops::IndexMut<Token> for Tokens {
    fn index_mut(&mut self, t: Token) -> &mut u64 {
        match t {
            Token::A => &mut self.A,
            Token::B => &mut self.B,
            Token::C => &mut self.C,
            Token::D => &mut self.D,
        }
    }
}

/// Token accounts are populated with this structure
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct TokenAccountInfo {
    /// Investor who owns this account
    pub owner: Pubkey,
    /// Current number of tokens this account holds
    pub tokens: Tokens,
}
impl TokenAccountInfo {
    pub fn owner(mut self, owner: Pubkey) -> Self {
        self.owner = owner;
        self
    }
    pub fn tokens(mut self, a: u64, b: u64, c: u64, d: u64) -> Self {
        self.tokens = Tokens {
            A: a,
            B: b,
            C: c,
            D: d,
        };
        self
    }
}

/// Direction of the exchange between two tokens in a pair
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum Direction {
    /// Trade first token type (primary) in the pair 'To' the second
    To,
    /// Trade first token type in the pair 'From' the second (secondary)
    From,
}
impl fmt::Display for Direction {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Direction::To => write!(f, "T")?,
            Direction::From => write!(f, "F")?,
        }
        Ok(())
    }
}

/// Trade accounts are populated with this structure
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct TradeOrderInfo {
    /// Owner of the trade order
    pub owner: Pubkey,
    /// Direction of the exchange
    pub direction: Direction,
    /// Token pair indicating two tokens to exchange, first is primary
    pub pair: (Token, Token),
    /// Number of tokens to exchange; primary or secondary depending on direction
    pub tokens: u64,
    /// Scaled price of the secondary token given the primary is equal to the scale value
    /// If scale is 1 and price is 2 then ratio is 1:2 or 1 primary token for 2 secondary tokens
    pub price: u64,
    /// account which the tokens were source from.  The trade account holds the tokens in escrow
    /// until either one or more part of a swap or the trade is cancelled.
    pub src_account: Pubkey,
    /// account which the tokens the tokens will be deposited into on a successful trade
    pub dst_account: Pubkey,
}
impl Default for TradeOrderInfo {
    fn default() -> TradeOrderInfo {
        TradeOrderInfo {
            owner: Pubkey::default(),
            pair: (Token::A, Token::B),
            direction: Direction::To,
            tokens: 0,
            price: 0,
            src_account: Pubkey::default(),
            dst_account: Pubkey::default(),
        }
    }
}
impl TradeOrderInfo {
    pub fn pair(mut self, primary: Token, secondary: Token) -> Self {
        self.pair = (primary, secondary);
        self
    }
    pub fn direction(mut self, direction: Direction) -> Self {
        self.direction = direction;
        self
    }
    pub fn tokens(mut self, tokens: u64) -> Self {
        self.tokens = tokens;
        self
    }
    pub fn price(mut self, price: u64) -> Self {
        self.price = price;
        self
    }
}

/// Swap accounts are populated with this structure
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct TradeSwapInfo {
    /// `To` trade order
    pub to_trade_order: Pubkey,
    /// `From` trade order
    pub from_trade_order: Pubkey,
    /// Type of primary token
    pub primary_token: Token,
    /// Number of primary tokens exchanged
    pub primary_tokens: u64,
    /// Price the primary tokens were exchanged for
    pub primary_price: u64,
    /// Type of secondary token
    pub secondary_token: Token,
    /// Number of secondary tokens exchanged
    pub secondary_tokens: u64,
    /// Price the secondary tokens were exchanged for
    pub secondary_price: u64,
}

/// Type of exchange account, account's user data is populated with this enum
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum ExchangeState {
    /// Account's Userdata is unallocated
    Unallocated,
    // Token account
    Account(TokenAccountInfo),
    // Trade order account
    Trade(TradeOrderInfo),
    // Swap account
    Swap(TradeSwapInfo),
    Invalid,
}
