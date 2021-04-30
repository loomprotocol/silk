---
title: Interest-Bearing Tokens
---

This document describes a solution for interest-bearing tokens as an on-chain
program. Interest-bearing tokens continuously accrue interest as time passes.

## Problem

Yield on blockchain assets is a crucial feature of many decentralized finance
applications, as token holders want to be compensated for providing their tokens
as liquidity to applications. The concept of "yield farming" on tokens has even
spawned very successful tongue-in-cheek projects, such as
[$MEME farming](https://dontbuymeme.com/).

Tokens that accrue interest are an attractive and popular version of yield farming,
as seen most prominently with Aave's [aTokens](https://aave.com/atokens).

### How do these work?

Let's explain through an example: assume there's an interest-bearing token mint
with a total supply of 1,000 tokens
and a 10% annual interest rate. This means that one year later, the total supply
will be 1,100 tokens, and all token holders will have 10% more tokens in their
holding account.

Since interest is accrued continuously, however, just 1 second later, the total
supply has again increased. A 10% annualized rate gives us a continuous interest
rate of ~9.5% (`ln(1.1)`), which allows us to calculate the new instantaneous supply.

```
new_supply = previous_supply * exp(continuous_rate / seconds_per_year)
new_supply = 1100 * exp(0.095 / 31536000)
new_supply = 1100.0000033...
```

Since interest-bearing tokens are constantly accruing interest, they do not have the same
properties as normal tokens. Most importantly, interest-bearing tokens held in two different
accounts are only fungible if they have been updated to the same time. For example,
10 interest-bearing tokens in the year 1999 are worth much more than 10 interest-bearing tokens in the year 3000, since
those tokens from 1999 have over 1,000 years of interest (see Futurama). Because
of this, we have to compound interest before any minting, burning, and
transferring.

### Initial Attempt

The concept seems simple to implement. We can create an SPL token, allow
tokens to be minted by a new "interest accruing" program, and then make sure to
call it regularly.

This solution has a few problems. First, how do we compound interest regularly?
We can create a separate service which periodically sends transactions to update
all of the accounts. Unfortunately, this will cost the service a lot, especially if
there are thousands of accounts to update.

We can enforce compounding before any transfer / mint / burn, but then all programs
and web consumers need to change to make these update calls. How do we change programs
and wallets to properly update accounts without creating a big `if is_i_token` case
all over the code?

This solution creates friction for almost all SPL token users: on-chain programs,
wallets, applications, and end-users.

## Proposed Solution

Following the approach taken by Aave, we can lean on a crucial feature of
interest-bearing tokens: although the holding's token amount is constantly
changing, each token holder's proportion of total supply stays the same.

Therefore, we can satisfy the fungibility requirement for tokens on proportional
ownership. The token interface stays exactly the same, operating on shares instead
of tokens, and the concept of "token amount" is actually the "UI amount" to be
displayed by frontends.

Let's go through an example. Imagine we have an interest-bearing token mint with a total of
10,000 proportional shares, a 20% annual interest rate, and 1 share is currently
worth 100 tokens. If I transfer 1,000 tokens today, the program converts that
using the interest-bearing token program and asks it to move
10 shares. One year later, 1 share is worth 120 tokens, so if I transfer
1,000 tokens, the program converts and asks to move 8.33 shares. The same concept
applies to mints and burns.

Following this model, we never have to worry about compounding interest. We
simply need to call the right program to perform instructions.

## Required Changes

A new interest-bearing token program is only one component of the overall solution,
which also comprises an interface for tokens (i-tokens) to be easily integrated in
on-chain programs, web apps, and wallets.

### Token Program Conformance

Since our solution entails the creation of a new token program, we're opening the
floodgates for other app developers to do the same. To ensure safe and easy integration
of new token programs, we need a set of conformance tests based on the current
SPL token test suite.

#### Instructions

A new token program must include all of the same instructions as SPL token,
but the "unchecked" variants should return errors. Concretely, this means the
program must implement:

* `InitializeMint`
* `InitializeHolding` (previously `InitializeAccount`)
* `Revoke`
* `SetAuthority`
* `CloseHolding` (previously `CloseAccount`)
* `FreezeHolding` (previously `FreezeAccount`)
* `ThawHolding` (previously `ThawAccount`)
* `TransferChecked`
* `ApproveChecked`
* `MintToChecked`
* `BurnChecked`
* `InitializeHoldingWithOwnerAddress` (previously `InitializeAccount2`)

And the program should throw errors for:

* `Transfer`
* `Approve`
* `MintTo`
* `Burn`
* `InitializeMultisig` (use a more general multisig)

New instructions required:

* `CreateHolding`: only performs `Allocate` and `Assign` to self,
useful for creating a holding when you don't know the program that you're
interacting with. See the Associated Token Account section for how to use this.

There are also new read-only instructions, which write data to a provided account
buffer:

* `AmountToUiAmount`: convert the given share amount to a UI token amount
* `UiAmountToAmount`: convert the given UI token amount to an exact share amount

See the Runtime section for more information on how these are used.

Programs and wallets that wish to support i-tokens must update the instructions
to use all `Checked` variants and use the new read-only instructions to fetch
information.

#### Conversions and Wrapper Library

The SPL token library provides wrappers compatible with any conforming program.
The wrapper simply checks the program id and deserializes the account data
as an `spl_token::Holding` or `spl_token::Mint`. If a client program requires access
to data specific to an i-token program, it needs to use that program's serializers.

To convert between amount and UI amount, the wrapper library calls the `AmountToUiAmount`
instruction on the i-token program, or for SPL token accounts, it calls
`spl_token::amount_to_ui_amount` inline.

For interest-bearing tokens, after deserialization into a `spl_token::Holding`,
the `amount` field will be in terms of shares, and not token amount, so UIs and
on-chain programs that need token amounts must convert using `AmountToUiAmount`.

#### Struct Conformance

The structure for new holding and mint accounts must follow the layout of the
existing SPL token accounts to ensure compatibility in RPC when fetching pre and
post token balances.

For a holding, the first 165 bytes must contain the same information
as a normal SPL token holding. The following byte must be the type of
account (ie. `Holding`), and after that, any data is allowed as required by the
new token program.

The same applies for mints, but only for the first 82 bytes.

### Token Program Registry

We need the [token-list](https://github.com/solana-labs/token-list) to include
vetted SPL token-conforming programs and group known mints by their program id.

The token list will publish a file of known program ids at
[token-list releases](https://github.com/solana-labs/token-list/releases/latest),
to be used by RPC and programs.

The entries in the token-list are signed using a managed key. As wallets or
Ledgers are asked to send transactions using a particular token program, they can
cross-check the mint or program information using a signature check on the
token-list data.

The token-list client libraries can fetch, cache, and query all registry info,
to be used for simple program and mint validation.

TODO consider detailing the process of including a new program

TODO figure out how the token-list signing key will be managed

### Runtime

#### Ephemeral Accounts

In order to reduce friction when using read-only instructions on other programs,
we need the ability for on-chain programs to dynamically create "ephemeral
accounts", which only contain data, can be passed through CPI to other programs,
and disappear after the instruction.

For a program using i-tokens, to get the UI amount of a holding, the program creates
an ephemeral account with 8 bytes of data, passes the `amount` to the i-token program's
`AmountToUiAmount` instruction, then deserializes the data back into an `f64`.
At the end of the instruction, the account disappears.

This solution avoids the need to pass additional scratch accounts to almost all
programs that use tokens.

TODO what limits do we consider for these? A certain number of accounts?
A total amount of bytes? Regarding security, this breaks the previous
check that a CPI must use a subset of accounts provided to the calling program.

#### Dynamic sysvars

The interest-bearing token program also requires the use of dynamic sysvars. For example,
during the `AmountToUiAmount` instruction, the program needs to know the current
time in order to properly convert from token amount to share amount.

Without dynamic sysvars, we need to create a system to tell what sysvars are
needed for different instructions on each program, which creates complications
for all users.

### RPC

The validator's RPC server needs to properly handle new token program accounts
for read-only instructions and pre / post balances.

Performance will be a concern when it comes to calling into the bank to perform
conversions into an ephemeral account.

The BPF compute cost of read-only transactions may need to be lowered specially
for read-only RPC calls. The interest-bearing token program will probably be one
of the more cost-intensive computations possible since it is performing present value
discounting.

#### `getTokenAccountBalance` and `getTokenSupply`

Instead of deserializing SPL token accounts and returning the data, the
RPC server runs read-only transactions on the i-token program to convert between
amount and UI amount. The current SPL token program is unaffected.

RPC simply uses the same flow as on-chain programs: create an ephemeral account
then runs the read-only transaction on the bank requested.

#### `preTokenBalances` and `postTokenBalances`

As mentioned earlier in the Token Program Conformance section, the i-token
`Holding` and `Mint` types must follow the layout for `spl_token::Holding` and
`spl_token::Mint`, so that RPC can deserialize mints and holdings into the SPL format.

For UI amounts, RPC calls the `AmountToUiAmount` instruction before and
after the transaction to generate `preTokenBalances` and `postTokenBalances`.

#### New Secondary Indexes

Token-specific RPC calls need smarter secondary indexes to pick up accounts from
new token programs.  These include:

* `getTokenAccountBalance`
* `getTokenAccountsByDelegate`
* `getTokenAccountsByOwner`
* `getTokenLargestAccounts`
* `getTokenSupply`

#### Vetted Token Programs

RPC will learn about the known program ids through a file published in the
[token-list releases](https://github.com/solana-labs/token-list/releases/latest),
downloaded automatically at startup.

### Associated Token Program

The Associated Token Program needs to support all token programs seamlessly.
Currently, it performs the following sequence:

* `SystemInstruction::Transfer` the required lamports to make the SPL holding account rent-exempt
* `SystemInstruction::Allocate` space for the SPL holding account
* `SystemInstruction::Assign` the holding account to the SPL token program
* `InitializeHolding` on SPL token program

This does not work for an opaque token program, because we do not know the size required
from the outside. Conversely, if we allow a token program to take the lamports it wants,
a malicious token program could take too much from users.

To get around this, the Token Program interface has a new `CreateHolding`
instruction, which only allocates space and assigns the account to the program.
Once the space is allocated, the Associated Token Program transfers the
required lamports, and returns an error if that number is too large. The process
becomes:

* call `CreateHolding` on the token program (allocate and assign)
* calculate rent requirement based on the data size of holding account
* `SystemInstruction::Transfer` the required lamports to make the holding account rent-exempt
* call `InitializeHolding` on the token program

### Web3 / Wallets

To properly support i-tokens, wallets and applications will need to make the
following changes:

* validate program: using the token-list, check that the given program id is legit
* get holdings: query for holdings owned by the public key, for all official
token programs listed in the registry (not just `Tokenkeg...`)
* get balances: avoid deserializing holding data, and instead use
`getBalance` from RPC to get UI amounts
* transfer tokens: only use `ApproveChecked` and `TransferChecked` on the token
program, and always convert UI amount to amount before calling instructions,
using `UiAmountToAmount`
* create holding: avoid directly creating an account and initializing,
and use the associated token account program instead

### On-chain Programs

To properly support i-tokens on-chain, programs will need to make these changes:

* multiple token programs: all programs that support SPL Tokens must always
include the program id of the token(s) holdings they're passing in.
For example, token-swap needs to accept separate token programs for token A and B
* hard-coded token program: avoid using `Tokenkeg...` directly, and delete `id()`
the SPL token code
* get holding data: instead of deserializing the account data into an SPL holding, use
the new wrapper to directly deserialize if the i-token program is "valid"
* get mint data: same as above
* UI amount vs token amount: know when the program should use amounts, which
could be share amounts, and when to use UI amounts
* transfer: only use `TransferChecked`, which requires passing mints

### Ledger

Ledger integrates with token-list in order to provide verified token program ids
and token tickers to the Ledger SDK and eliminate the need to store static
information in the Ledger Solana App.

A wallet or CLI with Ledger integration fetches the token-list and signing
information. When the user wishes to transfer tokens, the wallet first builds an
on-device token registry by pushing signed entries from the token-list for the subset
of tokens referenced by the transaction.

Ledger host applications provide token-list entry metadata is pushed to the device
via a new Ledger APDU command accepting **TBD_DATA_FORMAT** signed by the
token-list authority, whose public key is embedded in the Ledger app binary. Entries
whose signature verification passes are stored in a registry on the device that is
then referenced by a subsequent SIGN_SOLANA_TRANSACTION APDU command.

## Other New Token Programs

For awhile now, people have been coming up with ideas that likely require a new
token program to implement. These include:

### Voting Tokens

Outlined in a [GitHub issue](https://github.com/solana-labs/solana-program-library/issues/131),
voting tokens use additional data about how long a token has been held.

### Non-Fungible Tokens

It will be possible to add a lot of metadata regarding owners and traders, either
on a new mint or NFT account.

### Allow-list Tokens

Only a certain list keys are allowed (or not) to transfer to tokens, controlled
by the mint.

## Terminology

* token amount: the amount of tokens as seen from the outside. For example, an
interest-bearing token holding with 10 in token amount is converted to a proportional
ownership of 0.5% of the total supply. This amount should always be shown in UIs
using the `AmountToUiAmount` conversion instruction.
* share amount: the proportional ownership of a holding in terms of the total
supply of shares. Internally, the interest-bearing token program uses shares to
mint / transfer / burn.
* spl-token: the base on-chain program for tokens at address `Tokenkeg...` on
[all networks](https://explorer.solana.com/address/TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA)
which also defines the ABI standard required for other token programs that wish
to work "out of the box" with programs and wallets
* i-token: a program implementing the spl-token ABI, all of the instructions mentioned
in the "Token Program Conformance" section

## Proposed Layout

### Mint

```rust
pub struct Mint {
    // SPL Mint Fields
    pub mint_authority: COption<Pubkey>,
    pub supply: u64, // The total amount of shares outstanding
    pub decimals: u8,
    pub is_initialized: bool,
    pub freeze_authority: COption<Pubkey>,

    // Interest-bearing Mint Fields
    pub initialized_slot: Slot, // When the mint was created, used to discount present value (in terms of token amount) into share amount
}
```

### Holding

```rust
pub struct Holding {
    // SPL Holding Fields
    pub mint: Pubkey,
    pub owner: Pubkey,
    pub amount: u64, // This holding's proportional share of the total
    pub delegate: COption<Pubkey>,
    pub state: AccountState,
    pub is_native: COption<u64>,
    pub delegated_amount: u64,
    pub close_authority: COption<Pubkey>,
}
```
