---
title: JSON RPC API
---

Solana nodes accept HTTP requests using the [JSON-RPC 2.0](https://www.jsonrpc.org/specification) specification.

To interact with a Solana node inside a JavaScript application, use the
[solana-web3.js](https://github.com/solana-labs/solana-web3.js) library, which
gives a convenient interface for the RPC methods.

## RPC HTTP Endpoint

**Default port:** 8899 eg. [http://localhost:8899](http://localhost:8899), [http://192.168.1.88:8899](http://192.168.1.88:8899)

## RPC PubSub WebSocket Endpoint

**Default port:** 8900 eg. ws://localhost:8900, [http://192.168.1.88:8900](http://192.168.1.88:8900)

## Methods

- [getAccountInfo](jsonrpc-api.md#getaccountinfo)
- [getBalance](jsonrpc-api.md#getbalance)
- [getBlock](jsonrpc-api.md#getblock)
- [getBlockHeight](jsonrpc-api.md#getblockheight)
- [getBlockProduction](jsonrpc-api.md#getblockproduction)
- [getBlockCommitment](jsonrpc-api.md#getblockcommitment)
- [getBlocks](jsonrpc-api.md#getblocks)
- [getBlocksWithLimit](jsonrpc-api.md#getblockswithlimit)
- [getBlockTime](jsonrpc-api.md#getblocktime)
- [getClusterNodes](jsonrpc-api.md#getclusternodes)
- [getEpochInfo](jsonrpc-api.md#getepochinfo)
- [getEpochSchedule](jsonrpc-api.md#getepochschedule)
- [getFeeCalculatorForBlockhash](jsonrpc-api.md#getfeecalculatorforblockhash)
- [getFeeRateGovernor](jsonrpc-api.md#getfeerategovernor)
- [getFees](jsonrpc-api.md#getfees)
- [getFirstAvailableBlock](jsonrpc-api.md#getfirstavailableblock)
- [getGenesisHash](jsonrpc-api.md#getgenesishash)
- [getHealth](jsonrpc-api.md#gethealth)
- [getIdentity](jsonrpc-api.md#getidentity)
- [getInflationGovernor](jsonrpc-api.md#getinflationgovernor)
- [getInflationRate](jsonrpc-api.md#getinflationrate)
- [getInflationReward](jsonrpc-api.md#getinflationreward)
- [getLargestAccounts](jsonrpc-api.md#getlargestaccounts)
- [getLeaderSchedule](jsonrpc-api.md#getleaderschedule)
- [getMaxRetransmitSlot](jsonrpc-api.md#getmaxretransmitslot)
- [getMaxShredInsertSlot](jsonrpc-api.md#getmaxshredinsertslot)
- [getMinimumBalanceForRentExemption](jsonrpc-api.md#getminimumbalanceforrentexemption)
- [getMultipleAccounts](jsonrpc-api.md#getmultipleaccounts)
- [getProgramAccounts](jsonrpc-api.md#getprogramaccounts)
- [getRecentBlockhash](jsonrpc-api.md#getrecentblockhash)
- [getRecentPerformanceSamples](jsonrpc-api.md#getrecentperformancesamples)
- [getSignaturesForAddress](jsonrpc-api.md#getsignaturesforaddress)
- [getSignatureStatuses](jsonrpc-api.md#getsignaturestatuses)
- [getSlot](jsonrpc-api.md#getslot)
- [getSlotLeader](jsonrpc-api.md#getslotleader)
- [getSlotLeaders](jsonrpc-api.md#getslotleaders)
- [getStakeActivation](jsonrpc-api.md#getstakeactivation)
- [getSupply](jsonrpc-api.md#getsupply)
- [getTokenAccountBalance](jsonrpc-api.md#gettokenaccountbalance)
- [getTokenAccountsByDelegate](jsonrpc-api.md#gettokenaccountsbydelegate)
- [getTokenAccountsByOwner](jsonrpc-api.md#gettokenaccountsbyowner)
- [getTokenLargestAccounts](jsonrpc-api.md#gettokenlargestaccounts)
- [getTokenSupply](jsonrpc-api.md#gettokensupply)
- [getTransaction](jsonrpc-api.md#gettransaction)
- [getTransactionCount](jsonrpc-api.md#gettransactioncount)
- [getVersion](jsonrpc-api.md#getversion)
- [getVoteAccounts](jsonrpc-api.md#getvoteaccounts)
- [minimumLedgerSlot](jsonrpc-api.md#minimumledgerslot)
- [requestAirdrop](jsonrpc-api.md#requestairdrop)
- [sendTransaction](jsonrpc-api.md#sendtransaction)
- [simulateTransaction](jsonrpc-api.md#simulatetransaction)
- [Subscription Websocket](jsonrpc-api.md#subscription-websocket)
  - [accountSubscribe](jsonrpc-api.md#accountsubscribe)
  - [accountUnsubscribe](jsonrpc-api.md#accountunsubscribe)
  - [logsSubscribe](jsonrpc-api.md#logssubscribe)
  - [logsUnsubscribe](jsonrpc-api.md#logsunsubscribe)
  - [programSubscribe](jsonrpc-api.md#programsubscribe)
  - [programUnsubscribe](jsonrpc-api.md#programunsubscribe)
  - [signatureSubscribe](jsonrpc-api.md#signaturesubscribe)
  - [signatureUnsubscribe](jsonrpc-api.md#signatureunsubscribe)
  - [slotSubscribe](jsonrpc-api.md#slotsubscribe)
  - [slotUnsubscribe](jsonrpc-api.md#slotunsubscribe)

### Unstable Methods

Unstable methods may see breaking changes in patch releases and may not be supported in perpetuity.

- [slotsUpdatesSubscribe](jsonrpc-api.md#slotsupdatessubscribe---unstable)
- [slotsUpdatesUnsubscribe](jsonrpc-api.md#slotsupdatesunsubscribe)
- [voteSubscribe](jsonrpc-api.md#votesubscribe---unstable-disabled-by-default)
- [voteUnsubscribe](jsonrpc-api.md#voteunsubscribe)

### Deprecated Methods

- [getConfirmedBlock](jsonrpc-api.md#getconfirmedblock)
- [getConfirmedBlocks](jsonrpc-api.md#getconfirmedblocks)
- [getConfirmedBlocksWithLimit](jsonrpc-api.md#getconfirmedblockswithlimit)
- [getConfirmedSignaturesForAddress2](jsonrpc-api.md#getconfirmedsignaturesforaddress2)
- [getConfirmedTransaction](jsonrpc-api.md#getconfirmedtransaction)

## Request Formatting

To make a JSON-RPC request, send an HTTP POST request with a `Content-Type:
application/json` header. The JSON request data should contain 4 fields:

- `jsonrpc: <string>`, set to `"2.0"`
- `id: <number>`, a unique client-generated identifying integer
- `method: <string>`, a string containing the method to be invoked
- `params: <array>`, a JSON array of ordered parameter values

Example using curl:

```bash
curl http://localhost:8899 -X POST -H "Content-Type: application/json" -d '
  {
    "jsonrpc": "2.0",
    "id": 1,
    "method": "getBalance",
    "params": [
      "83astBRguLMdt2h5U1Tpdq5tjFoJ6noeGwaY3mDLVcri"
    ]
  }
'
```

The response output will be a JSON object with the following fields:

- `jsonrpc: <string>`, matching the request specification
- `id: <number>`, matching the request identifier
- `result: <array|number|object|string>`, requested data or success confirmation

Requests can be sent in batches by sending an array of JSON-RPC request objects as the data for a single POST.

## Definitions

- Hash: A SHA-256 hash of a chunk of data.
- Pubkey: The public key of a Ed25519 key-pair.
- Transaction: A list of Solana instructions signed by a client keypair to authorize those actions.
- Signature: An Ed25519 signature of transaction's payload data including instructions. This can be used to identify transactions.

## Configuring State Commitment

For preflight checks and transaction processing, Solana nodes choose which bank
state to query based on a commitment requirement set by the client. The
commitment describes how finalized a block is at that point in time.  When
querying the ledger state, it's recommended to use lower levels of commitment
to report progress and higher levels to ensure the state will not be rolled back.

In descending order of commitment (most finalized to least finalized), clients
may specify:

- `"finalized"` - the node will query the most recent block confirmed by supermajority
of the cluster as having reached maximum lockout, meaning the cluster has
recognized this block as finalized
- `"confirmed"` - the node will query the most recent block that has been voted on by supermajority of the cluster.
  - It incorporates votes from gossip and replay.
  - It does not count votes on descendants of a block, only direct votes on that block.
  - This confirmation level also upholds "optimistic confirmation" guarantees in
    release 1.3 and onwards.
- `"processed"` - the node will query its most recent block.  Note that the block
may not be complete.

For processing many dependent transactions in series, it's recommended to use
`"confirmed"` commitment, which balances speed with rollback safety.
For total safety, it's recommended to use`"finalized"` commitment.

#### Example

The commitment parameter should be included as the last element in the `params` array:

```bash
curl http://localhost:8899 -X POST -H "Content-Type: application/json" -d '
  {
    "jsonrpc": "2.0",
    "id": 1,
    "method": "getBalance",
    "params": [
      "83astBRguLMdt2h5U1Tpdq5tjFoJ6noeGwaY3mDLVcri",
      {
        "commitment": "finalized"
      }
    ]
  }
'
```

#### Default:

If commitment configuration is not provided, the node will default to `"finalized"` commitment

Only methods that query bank state accept the commitment parameter. They are indicated in the API Reference below.

#### RpcResponse Structure

Many methods that take a commitment parameter return an RpcResponse JSON object comprised of two parts:

- `context` : An RpcResponseContext JSON structure including a `slot` field at which the operation was evaluated.
- `value` : The value returned by the operation itself.

## Health Check

Although not a JSON RPC API, a `GET /health` at the RPC HTTP Endpoint provides a
health-check mechanism for use by load balancers or other network
infrastructure. This request will always return a HTTP 200 OK response with a body of
"ok", "behind" or "unknown" based on the following conditions:

1. If one or more `--trusted-validator` arguments are provided to `solana-validator`, "ok" is returned
   when the node has within `HEALTH_CHECK_SLOT_DISTANCE` slots of the highest
   trusted validator, otherwise "behind". "unknown" is returned when no slot
   information from trusted validators is not yet available.
2. "ok" is always returned if no trusted validators are provided.

## JSON RPC API Reference

### getAccountInfo

Returns all information associated with the account of provided Pubkey

#### Parameters:

- `<string>` - Pubkey of account to query, as base-58 encoded string
- `<object>` - (optional) Configuration object containing the following optional
fields:
  - (optional) [Commitment](jsonrpc-api.md#configuring-state-commitment)
  - `encoding: <string>` - encoding for Account data, either "base58" (*slow*), "base64", "base64+zstd", or "jsonParsed".
    "base58" is limited to Account data of less than 129 bytes.
    "base64" will return base64 encoded data for Account data of any size.
    "base64+zstd" compresses the Account data using [Zstandard](https://facebook.github.io/zstd/) and base64-encodes the result.
    "jsonParsed" encoding attempts to use program-specific state parsers to return more human-readable and explicit account state data. If "jsonParsed" is requested but a parser cannot be found, the field falls back to "base64" encoding, detectable when the `data` field is type `<string>`.
  - (optional) `dataSlice: <object>` - limit the returned account data using the provided `offset: <usize>` and `length: <usize>` fields; only available for "base58", "base64" or "base64+zstd" encodings.

#### Results:

The result will be an RpcResponse JSON object with `value` equal to:

- `<null>` - if the requested account doesn't exist
- `<object>` - otherwise, a JSON object containing:
  - `lamports: <u64>`, number of lamports assigned to this account, as a u64
  - `owner: <string>`, base-58 encoded Pubkey of the program this account has been assigned to
  - `data: <[string, encoding]|object>`, data associated with the account, either as encoded binary data or JSON format `{<program>: <state>}`, depending on encoding parameter
  - `executable: <bool>`, boolean indicating if the account contains a program \(and is strictly read-only\)
  - `rentEpoch: <u64>`, the epoch at which this account will next owe rent, as u64

#### Example:

Request:
```bash
curl http://localhost:8899 -X POST -H "Content-Type: application/json" -d '
  {
    "jsonrpc": "2.0",
    "id": 1,
    "method": "getAccountInfo",
    "params": [
      "vines1vzrYbzLMRdu58ou5XTby4qAqVRLmqo36NKPTg",
      {
        "encoding": "base58"
      }
    ]
  }
'
```
Response:
```json
{
  "jsonrpc": "2.0",
  "result": {
    "context": {
      "slot": 1
    },
    "value": {
      "data": [
        "11116bv5nS2h3y12kD1yUKeMZvGcKLSjQgX6BeV7u1FrjeJcKfsHRTPuR3oZ1EioKtYGiYxpxMG5vpbZLsbcBYBEmZZcMKaSoGx9JZeAuWf",
        "base58"
      ],
      "executable": false,
      "lamports": 1000000000,
      "owner": "11111111111111111111111111111111",
      "rentEpoch": 2
    }
  },
  "id": 1
}
```

#### Example:
Request:
```bash
curl http://localhost:8899 -X POST -H "Content-Type: application/json" -d '
  {
    "jsonrpc": "2.0",
    "id": 1,
    "method": "getAccountInfo",
    "params": [
      "4fYNw3dojWmQ4dXtSGE9epjRGy9pFSx62YypT7avPYvA",
      {
        "encoding": "jsonParsed"
      }
    ]
  }
'
```
Response:
```json
{
  "jsonrpc": "2.0",
  "result": {
    "context": {
      "slot": 1
    },
    "value": {
      "data": {
        "nonce": {
          "initialized": {
            "authority": "Bbqg1M4YVVfbhEzwA9SpC9FhsaG83YMTYoR4a8oTDLX",
            "blockhash": "3xLP3jK6dVJwpeGeTDYTwdDK3TKchUf1gYYGHa4sF3XJ",
            "feeCalculator": {
              "lamportsPerSignature": 5000
            }
          }
        }
      },
      "executable": false,
      "lamports": 1000000000,
      "owner": "11111111111111111111111111111111",
      "rentEpoch": 2
    }
  },
  "id": 1
}
```

### getBalance

Returns the balance of the account of provided Pubkey

#### Parameters:

- `<string>` - Pubkey of account to query, as base-58 encoded string
- `<object>` - (optional) [Commitment](jsonrpc-api.md#configuring-state-commitment)

#### Results:

- `RpcResponse<u64>` - RpcResponse JSON object with `value` field set to the balance

#### Example:

Request:
```bash
curl http://localhost:8899 -X POST -H "Content-Type: application/json" -d '
  {"jsonrpc":"2.0", "id":1, "method":"getBalance", "params":["83astBRguLMdt2h5U1Tpdq5tjFoJ6noeGwaY3mDLVcri"]}
'
```

Result:
```json
{"jsonrpc":"2.0","result":{"context":{"slot":1},"value":0},"id":1}
```

### getBlock

**NEW: This method is only available in solana-core v1.7 or newer. Please use
[getConfirmedBlock](jsonrpc-api.md#getconfirmedblock) for solana-core v1.6**

Returns identity and transaction information about a confirmed block in the ledger

#### Parameters:

- `<u64>` - slot, as u64 integer
- `<object>` - (optional) Configuration object containing the following optional fields:
  - (optional) `encoding: <string>` - encoding for each returned Transaction, either "json", "jsonParsed", "base58" (*slow*), "base64". If parameter not provided, the default encoding is "json".
  "jsonParsed" encoding attempts to use program-specific instruction parsers to return more human-readable and explicit data in the `transaction.message.instructions` list. If "jsonParsed" is requested but a parser cannot be found, the instruction falls back to regular JSON encoding (`accounts`, `data`, and `programIdIndex` fields).
  - (optional) `transactionDetails: <string>` - level of transaction detail to return, either "full", "signatures", or "none". If parameter not provided, the default detail level is "full".
  - (optional) `rewards: bool` - whether to populate the `rewards` array. If parameter not provided, the default includes rewards.
  - (optional) [Commitment](jsonrpc-api.md#configuring-state-commitment); "processed" is not supported. If parameter not provided, the default is "finalized".

#### Results:

The result field will be an object with the following fields:

- `<null>` - if specified block is not confirmed
- `<object>` - if block is confirmed, an object with the following fields:
  - `blockhash: <string>` - the blockhash of this block, as base-58 encoded string
  - `previousBlockhash: <string>` - the blockhash of this block's parent, as base-58 encoded string; if the parent block is not available due to ledger cleanup, this field will return "11111111111111111111111111111111"
  - `parentSlot: <u64>` - the slot index of this block's parent
  - `transactions: <array>` - present if "full" transaction details are requested; an array of JSON objects containing:
    - `transaction: <object|[string,encoding]>` - [Transaction](#transaction-structure) object, either in JSON format or encoded binary data, depending on encoding parameter
    - `meta: <object>` - transaction status metadata object, containing `null` or:
      - `err: <object | null>` - Error if transaction failed, null if transaction succeeded. [TransactionError definitions](https://github.com/solana-labs/solana/blob/master/sdk/src/transaction.rs#L24)
      - `fee: <u64>` - fee this transaction was charged, as u64 integer
      - `preBalances: <array>` - array of u64 account balances from before the transaction was processed
      - `postBalances: <array>` - array of u64 account balances after the transaction was processed
      - `innerInstructions: <array|undefined>` - List of [inner instructions](#inner-instructions-structure) or omitted if inner instruction recording was not yet enabled during this transaction
      - `preTokenBalances: <array|undefined>` - List of [token balances](#token-balances-structure) from before the transaction was processed or omitted if token balance recording was not yet enabled during this transaction
      - `postTokenBalances: <array|undefined>` - List of [token balances](#token-balances-structure) from after the transaction was processed or omitted if token balance recording was not yet enabled during this transaction
      - `logMessages: <array>` - array of string log messages or omitted if log message recording was not yet enabled during this transaction
      - DEPRECATED: `status: <object>` - Transaction status
        - `"Ok": <null>` - Transaction was successful
        - `"Err": <ERR>` - Transaction failed with TransactionError
  - `signatures: <array>` - present if "signatures" are requested for transaction details; an array of signatures strings, corresponding to the transaction order in the block
  - `rewards: <array>` - present if rewards are requested; an array of JSON objects containing:
    - `pubkey: <string>` - The public key, as base-58 encoded string, of the account that received the reward
    - `lamports: <i64>`- number of reward lamports credited or debited by the account, as a i64
    - `postBalance: <u64>` - account balance in lamports after the reward was applied
    - `rewardType: <string|undefined>` - type of reward: "fee", "rent", "voting", "staking"
    - `commission: <u8|undefined>` - vote account commission when the reward was credited, only present for voting and staking rewards
  - `blockTime: <i64 | null>` - estimated production time, as Unix timestamp (seconds since the Unix epoch). null if not available
  - `blockHeight: <u64 | null>` - the number of blocks beneath this block

#### Example:

Request:
```bash
curl http://localhost:8899 -X POST -H "Content-Type: application/json" -d '
  {"jsonrpc": "2.0","id":1,"method":"getBlock","params":[430, {"encoding": "json","transactionDetails":"full","rewards":false}]}
'
```

Result:
```json
{
  "jsonrpc": "2.0",
  "result": {
    "blockHeight": 428,
    "blockTime": null,
    "blockhash": "3Eq21vXNB5s86c62bVuUfTeaMif1N2kUqRPBmGRJhyTA",
    "parentSlot": 429,
    "previousBlockhash": "mfcyqEXB3DnHXki6KjjmZck6YjmZLvpAByy2fj4nh6B",
    "transactions": [
      {
        "meta": {
          "err": null,
          "fee": 5000,
          "innerInstructions": [],
          "logMessages": [],
          "postBalances": [
            499998932500,
            26858640,
            1,
            1,
            1
          ],
          "postTokenBalances": [],
          "preBalances": [
            499998937500,
            26858640,
            1,
            1,
            1
          ],
          "preTokenBalances": [],
          "status": {
            "Ok": null
          }
        },
        "transaction": {
          "message": {
            "accountKeys": [
              "3UVYmECPPMZSCqWKfENfuoTv51fTDTWicX9xmBD2euKe",
              "AjozzgE83A3x1sHNUR64hfH7zaEBWeMaFuAN9kQgujrc",
              "SysvarS1otHashes111111111111111111111111111",
              "SysvarC1ock11111111111111111111111111111111",
              "Vote111111111111111111111111111111111111111"
            ],
            "header": {
              "numReadonlySignedAccounts": 0,
              "numReadonlyUnsignedAccounts": 3,
              "numRequiredSignatures": 1
            },
            "instructions": [
              {
                "accounts": [
                  1,
                  2,
                  3,
                  0
                ],
                "data": "37u9WtQpcm6ULa3WRQHmj49EPs4if7o9f1jSRVZpm2dvihR9C8jY4NqEwXUbLwx15HBSNcP1",
                "programIdIndex": 4
              }
            ],
            "recentBlockhash": "mfcyqEXB3DnHXki6KjjmZck6YjmZLvpAByy2fj4nh6B"
          },
          "signatures": [
            "2nBhEBYYvfaAe16UMNqRHre4YNSskvuYgx3M6E4JP1oDYvZEJHvoPzyUidNgNX5r9sTyN1J9UxtbCXy2rqYcuyuv"
          ]
        }
      }
    ]
  },
  "id": 1
}
```

#### Example:
Request:
```bash
curl http://localhost:8899 -X POST -H "Content-Type: application/json" -d '
  {"jsonrpc": "2.0","id":1,"method":"getBlock","params":[430, "base64"]}
'
```

Result:
```json
{
  "jsonrpc": "2.0",
  "result": {
    "blockHeight": 428,
    "blockTime": null,
    "blockhash": "3Eq21vXNB5s86c62bVuUfTeaMif1N2kUqRPBmGRJhyTA",
    "parentSlot": 429,
    "previousBlockhash": "mfcyqEXB3DnHXki6KjjmZck6YjmZLvpAByy2fj4nh6B",
    "rewards": [],
    "transactions": [
      {
        "meta": {
          "err": null,
          "fee": 5000,
          "innerInstructions": [],
          "logMessages": [],
          "postBalances": [
            499998932500,
            26858640,
            1,
            1,
            1
          ],
          "postTokenBalances": [],
          "preBalances": [
            499998937500,
            26858640,
            1,
            1,
            1
          ],
          "preTokenBalances": [],
          "status": {
            "Ok": null
          }
        },
        "transaction": [
          "AVj7dxHlQ9IrvdYVIjuiRFs1jLaDMHixgrv+qtHBwz51L4/ImLZhszwiyEJDIp7xeBSpm/TX5B7mYzxa+fPOMw0BAAMFJMJVqLw+hJYheizSoYlLm53KzgT82cDVmazarqQKG2GQsLgiqktA+a+FDR4/7xnDX7rsusMwryYVUdixfz1B1Qan1RcZLwqvxvJl4/t3zHragsUp0L47E24tAFUgAAAABqfVFxjHdMkoVmOYaR1etoteuKObS21cc1VbIQAAAAAHYUgdNXR0u3xNdiTr072z2DVec9EQQ/wNo1OAAAAAAAtxOUhPBp2WSjUNJEgfvy70BbxI00fZyEPvFHNfxrtEAQQEAQIDADUCAAAAAQAAAAAAAACtAQAAAAAAAAdUE18R96XTJCe+YfRfUp6WP+YKCy/72ucOL8AoBFSpAA==",
          "base64"
        ]
      }
    ]
  },
  "id": 1
}
```

#### Transaction Structure

Transactions are quite different from those on other blockchains. Be sure to review [Anatomy of a Transaction](developing/programming-model/transactions.md) to learn about transactions on Solana.

The JSON structure of a transaction is defined as follows:

- `signatures: <array[string]>` - A list of base-58 encoded signatures applied to the transaction. The list is always of length `message.header.numRequiredSignatures` and not empty. The signature at index `i` corresponds to the public key at index `i` in `message.account_keys`. The first one is used as the [transaction id](../../terminology.md#transaction-id).
- `message: <object>` - Defines the content of the transaction.
  - `accountKeys: <array[string]>` - List of base-58 encoded public keys used by the transaction, including by the instructions and for signatures. The first `message.header.numRequiredSignatures` public keys must sign the transaction.
  - `header: <object>` - Details the account types and signatures required by the transaction.
    - `numRequiredSignatures: <number>` - The total number of signatures required to make the transaction valid. The signatures must match the first `numRequiredSignatures` of `message.account_keys`.
    - `numReadonlySignedAccounts: <number>` - The last `numReadonlySignedAccounts` of the signed keys are read-only accounts. Programs may process multiple transactions that load read-only accounts within a single PoH entry, but are not permitted to credit or debit lamports or modify account data. Transactions targeting the same read-write account are evaluated sequentially.
    - `numReadonlyUnsignedAccounts: <number>` - The last `numReadonlyUnsignedAccounts` of the unsigned keys are read-only accounts.
  - `recentBlockhash: <string>` - A base-58 encoded hash of a recent block in the ledger used to prevent transaction duplication and to give transactions lifetimes.
  - `instructions: <array[object]>` - List of program instructions that will be executed in sequence and committed in one atomic transaction if all succeed.
    - `programIdIndex: <number>` - Index into the `message.accountKeys` array indicating the program account that executes this instruction.
    - `accounts: <array[number]>` - List of ordered indices into the `message.accountKeys` array indicating which accounts to pass to the program.
    - `data: <string>` - The program input data encoded in a base-58 string.

#### Inner Instructions Structure

The Solana runtime records the cross-program instructions that are invoked during transaction processing and makes these available for greater transparency of what was executed on-chain per transaction instruction. Invoked instructions are grouped by the originating transaction instruction and are listed in order of processing.

The JSON structure of inner instructions is defined as a list of objects in the following structure:

- `index: number` - Index of the transaction instruction from which the inner instruction(s) originated
- `instructions: <array[object]>` - Ordered list of inner program instructions that were invoked during a single transaction instruction.
  - `programIdIndex: <number>` - Index into the `message.accountKeys` array indicating the program account that executes this instruction.
  - `accounts: <array[number]>` - List of ordered indices into the `message.accountKeys` array indicating which accounts to pass to the program.
  - `data: <string>` - The program input data encoded in a base-58 string.

#### Token Balances Structure

The JSON structure of token balances is defined as a list of objects in the following structure:

- `accountIndex: <number>` - Index of the account in which the token balance is provided for.
- `mint: <string>` - Pubkey of the token's mint.
- `uiTokenAmount: <object>` -
  - `amount: <string>` - Raw amount of tokens as a string, ignoring decimals.
  - `decimals: <number>` - Number of decimals configured for token's mint.
  - `uiAmount: <number | null>` - Token amount as a float, accounting for decimals. **DEPRECATED**
  - `uiAmountString: <string>` - Token amount as a string, accounting for decimals.


### getBlockHeight

Returns the current block height of the node

#### Parameters:

- `<object>` - (optional) [Commitment](jsonrpc-api.md#configuring-state-commitment)

#### Results:

- `<u64>` - Current block height

#### Example:

Request:
```bash
curl http://localhost:8899 -X POST -H "Content-Type: application/json" -d '
  {"jsonrpc":"2.0","id":1, "method":"getBlockHeight"}
'
```

Result:
```json
{"jsonrpc":"2.0","result":1233,"id":1}
```

### getBlockProduction

Returns recent block production information from the current or previous epoch.

#### Parameters:

- `<object>` - (optional) Configuration object containing the following optional fields:
  - (optional) [Commitment](jsonrpc-api.md#configuring-state-commitment)
  - (optional) `range: <object>` - Slot range to return block production for. If parameter not provided, defaults to current epoch.
    - `firstSlot: <u64>` - first slot to return block production information for (inclusive)
    - (optional) `lastSlot: <u64>` - last slot to return block production information for (inclusive). If parameter not provided, defaults to the highest slot
  - (optional) `identity: <string>` - Only return results for this validator identity (base-58 encoded)

#### Results:

The result will be an RpcResponse JSON object with `value` equal to:
- `<object>`
  - `byIdentity: <object>` - a dictionary of validator identities,
    as base-58 encoded strings.  Value is a two element array containing the
    number of leader slots and the number of blocks produced.
  - `range: <object>` - Block production slot range
    - `firstSlot: <u64>` - first slot of the block production information (inclusive)
    - `lastSlot: <u64>` - last slot of block production information (inclusive)

#### Example:

Request:
```bash
curl http://localhost:8899 -X POST -H "Content-Type: application/json" -d '
  {"jsonrpc":"2.0","id":1, "method":"getBlockProduction"}
'
```

Result:
```json
{
  "jsonrpc": "2.0",
  "result": {
    "context": {
      "slot": 9887
    },
    "value": {
      "byIdentity": {
        "85iYT5RuzRTDgjyRa3cP8SYhM2j21fj7NhfJ3peu1DPr": [
          9888,
          9886
        ]
      },
      "range": {
        "firstSlot": 0,
        "lastSlot": 9887,
      }
    }
  },
  "id": 1
}
```

#### Example:

Request:
```bash
curl http://localhost:8899 -X POST -H "Content-Type: application/json" -d '
  {
    "jsonrpc": "2.0",
    "id": 1,
    "method": "getBlockProduction",
    "params": [
      {
        "identity": "85iYT5RuzRTDgjyRa3cP8SYhM2j21fj7NhfJ3peu1DPr",
        "range": {
          "firstSlot": 40,
          "lastSlot": 50
        }
      }
    ]
  }
'
```

Result:
```json
{
  "jsonrpc": "2.0",
  "result": {
    "context": {
      "slot": 10102
    },
    "value": {
      "byIdentity": {
        "85iYT5RuzRTDgjyRa3cP8SYhM2j21fj7NhfJ3peu1DPr": [
          11,
          11
        ]
      },
      "range": {
        "firstSlot": 50,
        "lastSlot": 40
      }
    }
  },
  "id": 1
}
```

### getBlockCommitment

Returns commitment for particular block

#### Parameters:

- `<u64>` - block, identified by Slot

#### Results:

The result field will be a JSON object containing:

- `commitment` - commitment, comprising either:
  - `<null>` - Unknown block
  - `<array>` - commitment, array of u64 integers logging the amount of cluster stake in lamports that has voted on the block at each depth from 0 to `MAX_LOCKOUT_HISTORY` + 1
- `totalStake` - total active stake, in lamports, of the current epoch

#### Example:

Request:
```bash
curl http://localhost:8899 -X POST -H "Content-Type: application/json" -d '
  {"jsonrpc":"2.0","id":1, "method":"getBlockCommitment","params":[5]}
'
```

Result:
```json
{
  "jsonrpc":"2.0",
  "result":{
    "commitment":[0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,10,32],
    "totalStake": 42
  },
  "id":1
}
```

### getBlocks

**NEW: This method is only available in solana-core v1.7 or newer. Please use
[getConfirmedBlocks](jsonrpc-api.md#getconfirmedblocks) for solana-core v1.6**

Returns a list of confirmed blocks between two slots

#### Parameters:

- `<u64>` - start_slot, as u64 integer
- `<u64>` - (optional) end_slot, as u64 integer
- (optional) [Commitment](jsonrpc-api.md#configuring-state-commitment); "processed" is not supported. If parameter not provided, the default is "finalized".

#### Results:

The result field will be an array of u64 integers listing confirmed blocks
between `start_slot` and either `end_slot`, if provided, or latest confirmed block,
inclusive.  Max range allowed is 500,000 slots.


#### Example:

Request:
```bash
curl http://localhost:8899 -X POST -H "Content-Type: application/json" -d '
  {"jsonrpc": "2.0","id":1,"method":"getBlocks","params":[5, 10]}
'
```

Result:
```json
{"jsonrpc":"2.0","result":[5,6,7,8,9,10],"id":1}
```

### getBlocksWithLimit

**NEW: This method is only available in solana-core v1.7 or newer. Please use
[getConfirmedBlocksWithLimit](jsonrpc-api.md#getconfirmedblockswithlimit) for solana-core v1.6**

Returns a list of confirmed blocks starting at the given slot

#### Parameters:

- `<u64>` - start_slot, as u64 integer
- `<u64>` - limit, as u64 integer
- (optional) [Commitment](jsonrpc-api.md#configuring-state-commitment); "processed" is not supported. If parameter not provided, the default is "finalized".

#### Results:

The result field will be an array of u64 integers listing confirmed blocks
starting at `start_slot` for up to `limit` blocks, inclusive.

#### Example:

Request:
```bash
curl http://localhost:8899 -X POST -H "Content-Type: application/json" -d '
  {"jsonrpc": "2.0","id":1,"method":"getBlocksWithLimit","params":[5, 3]}
'
```

Result:
```json
{"jsonrpc":"2.0","result":[5,6,7],"id":1}
```

### getBlockTime

Returns the estimated production time of a block.

Each validator reports their UTC time to the ledger on a regular interval by
intermittently adding a timestamp to a Vote for a particular block. A requested
block's time is calculated from the stake-weighted mean of the Vote timestamps
in a set of recent blocks recorded on the ledger.

#### Parameters:

- `<u64>` - block, identified by Slot

#### Results:

* `<i64>` - estimated production time, as Unix timestamp (seconds since the Unix epoch)
* `<null>` - timestamp is not available for this block

#### Example:

Request:
```bash
curl http://localhost:8899 -X POST -H "Content-Type: application/json" -d '
  {"jsonrpc":"2.0","id":1, "method":"getBlockTime","params":[5]}
'
```

Result:
```json
{"jsonrpc":"2.0","result":1574721591,"id":1}
```

### getClusterNodes

Returns information about all the nodes participating in the cluster

#### Parameters:

None

#### Results:

The result field will be an array of JSON objects, each with the following sub fields:

- `pubkey: <string>` - Node public key, as base-58 encoded string
- `gossip: <string>` - Gossip network address for the node
- `tpu: <string>` - TPU network address for the node
- `rpc: <string>|null` - JSON RPC network address for the node, or `null` if the JSON RPC service is not enabled
- `version: <string>|null` - The software version of the node, or `null` if the version information is not available
- `featureSet: <number>|null` - The unique identifier of the node's feature set
- `shredVersion: <number>|null` - The shred version the node has been configured to use

#### Example:

Request:
```bash
curl http://localhost:8899 -X POST -H "Content-Type: application/json" -d '
  {"jsonrpc":"2.0", "id":1, "method":"getClusterNodes"}
'
```

Result:
```json
{
  "jsonrpc": "2.0",
  "result": [
    {
      "gossip": "10.239.6.48:8001",
      "pubkey": "9QzsJf7LPLj8GkXbYT3LFDKqsj2hHG7TA3xinJHu8epQ",
      "rpc": "10.239.6.48:8899",
      "tpu": "10.239.6.48:8856",
      "version": "1.0.0 c375ce1f"
    }
  ],
  "id": 1
}
```

### getEpochInfo

Returns information about the current epoch

#### Parameters:

- `<object>` - (optional) [Commitment](jsonrpc-api.md#configuring-state-commitment)

#### Results:

The result field will be an object with the following fields:

- `absoluteSlot: <u64>`, the current slot
- `blockHeight: <u64>`, the current block height
- `epoch: <u64>`, the current epoch
- `slotIndex: <u64>`, the current slot relative to the start of the current epoch
- `slotsInEpoch: <u64>`, the number of slots in this epoch

#### Example:

Request:
```bash
curl http://localhost:8899 -X POST -H "Content-Type: application/json" -d '
  {"jsonrpc":"2.0","id":1, "method":"getEpochInfo"}
'
```

Result:
```json
{
  "jsonrpc": "2.0",
  "result": {
    "absoluteSlot": 166598,
    "blockHeight": 166500,
    "epoch": 27,
    "slotIndex": 2790,
    "slotsInEpoch": 8192
  },
  "id": 1
}
```

### getEpochSchedule

Returns epoch schedule information from this cluster's genesis config

#### Parameters:

None

#### Results:

The result field will be an object with the following fields:

- `slotsPerEpoch: <u64>`, the maximum number of slots in each epoch
- `leaderScheduleSlotOffset: <u64>`, the number of slots before beginning of an epoch to calculate a leader schedule for that epoch
- `warmup: <bool>`, whether epochs start short and grow
- `firstNormalEpoch: <u64>`, first normal-length epoch, log2(slotsPerEpoch) - log2(MINIMUM_SLOTS_PER_EPOCH)
- `firstNormalSlot: <u64>`, MINIMUM_SLOTS_PER_EPOCH \* (2.pow(firstNormalEpoch) - 1)

#### Example:

Request:
```bash
curl http://localhost:8899 -X POST -H "Content-Type: application/json" -d '
  {"jsonrpc":"2.0","id":1, "method":"getEpochSchedule"}
'
```

Result:
```json
{
  "jsonrpc": "2.0",
  "result": {
    "firstNormalEpoch": 8,
    "firstNormalSlot": 8160,
    "leaderScheduleSlotOffset": 8192,
    "slotsPerEpoch": 8192,
    "warmup": true
  },
  "id": 1
}
```

### getFeeCalculatorForBlockhash

Returns the fee calculator associated with the query blockhash, or `null` if the blockhash has expired

#### Parameters:

- `<string>` - query blockhash as a Base58 encoded string
- `<object>` - (optional) [Commitment](jsonrpc-api.md#configuring-state-commitment)

#### Results:

The result will be an RpcResponse JSON object with `value` equal to:

- `<null>` - if the query blockhash has expired
- `<object>` - otherwise, a JSON object containing:
  - `feeCalculator: <object>`, `FeeCalculator` object describing the cluster fee rate at the queried blockhash

#### Example:

Request:
```bash
curl http://localhost:8899 -X POST -H "Content-Type: application/json" -d '
  {
    "jsonrpc": "2.0",
    "id": 1,
    "method": "getFeeCalculatorForBlockhash",
    "params": [
      "GJxqhuxcgfn5Tcj6y3f8X4FeCDd2RQ6SnEMo1AAxrPRZ"
    ]
  }
'
```

Result:
```json
{
  "jsonrpc": "2.0",
  "result": {
    "context": {
      "slot": 221
    },
    "value": {
      "feeCalculator": {
        "lamportsPerSignature": 5000
      }
    }
  },
  "id": 1
}
```

### getFeeRateGovernor

Returns the fee rate governor information from the root bank

#### Parameters:

None

#### Results:

The `result` field will be an `object` with the following fields:

- `burnPercent: <u8>`, Percentage of fees collected to be destroyed
- `maxLamportsPerSignature: <u64>`, Largest value `lamportsPerSignature` can attain for the next slot
- `minLamportsPerSignature: <u64>`, Smallest value `lamportsPerSignature` can attain for the next slot
- `targetLamportsPerSignature: <u64>`, Desired fee rate for the cluster
- `targetSignaturesPerSlot: <u64>`, Desired signature rate for the cluster

#### Example:

Request:
```bash
curl http://localhost:8899 -X POST -H "Content-Type: application/json" -d '
  {"jsonrpc":"2.0","id":1, "method":"getFeeRateGovernor"}
'
```

Result:
```json
{
  "jsonrpc": "2.0",
  "result": {
    "context": {
      "slot": 54
    },
    "value": {
      "feeRateGovernor": {
        "burnPercent": 50,
        "maxLamportsPerSignature": 100000,
        "minLamportsPerSignature": 5000,
        "targetLamportsPerSignature": 10000,
        "targetSignaturesPerSlot": 20000
      }
    }
  },
  "id": 1
}
```

### getFees

Returns a recent block hash from the ledger, a fee schedule that can be used to
compute the cost of submitting a transaction using it, and the last slot in
which the blockhash will be valid.

#### Parameters:

- `<object>` - (optional) [Commitment](jsonrpc-api.md#configuring-state-commitment)

#### Results:

The result will be an RpcResponse JSON object with `value` set to a JSON object with the following fields:

- `blockhash: <string>` - a Hash as base-58 encoded string
- `feeCalculator: <object>` - FeeCalculator object, the fee schedule for this block hash
- `lastValidSlot: <u64>` - DEPRECATED - this value is inaccurate and should not be relied upon
- `lastValidBlockHeight: <u64>` - last [block height](../../terminology.md#block-height) at which a blockhash will be valid

#### Example:

Request:
```bash
curl http://localhost:8899 -X POST -H "Content-Type: application/json" -d '
  {"jsonrpc":"2.0","id":1, "method":"getFees"}
'
```

Result:
```json
{
  "jsonrpc": "2.0",
  "result": {
    "context": {
      "slot": 1
    },
    "value": {
      "blockhash": "CSymwgTNX1j3E4qhKfJAUE41nBWEwXufoYryPbkde5RR",
      "feeCalculator": {
        "lamportsPerSignature": 5000
      },
      "lastValidSlot": 297,
      "lastValidBlockHeight": 296
    }
  },
  "id": 1
}
```

### getFirstAvailableBlock

Returns the slot of the lowest confirmed block that has not been purged from the ledger

#### Parameters:

None

#### Results:

- `<u64>` - Slot

#### Example:

Request:
```bash
curl http://localhost:8899 -X POST -H "Content-Type: application/json" -d '
  {"jsonrpc":"2.0","id":1, "method":"getFirstAvailableBlock"}
'
```

Result:
```json
{"jsonrpc":"2.0","result":250000,"id":1}
```

### getGenesisHash

Returns the genesis hash

#### Parameters:

None

#### Results:

- `<string>` - a Hash as base-58 encoded string

#### Example:

Request:
```bash
curl http://localhost:8899 -X POST -H "Content-Type: application/json" -d '
  {"jsonrpc":"2.0","id":1, "method":"getGenesisHash"}
'
```

Result:
```json
{"jsonrpc":"2.0","result":"GH7ome3EiwEr7tu9JuTh2dpYWBJK3z69Xm1ZE3MEE6JC","id":1}
```

### getHealth

Returns the current health of the node.

If one or more `--trusted-validator` arguments are provided to
`solana-validator`, "ok" is returned when the node has within
`HEALTH_CHECK_SLOT_DISTANCE` slots of the highest trusted validator, otherwise
an error is returned.  "ok" is always returned if no trusted validators are
provided.

#### Parameters:

None

#### Results:

If the node is healthy: "ok"
If the node is unhealthy, a JSON RPC error response is returned.  The specifics
of the error response are **UNSTABLE** and may change in the future


#### Example:

Request:
```bash
curl http://localhost:8899 -X POST -H "Content-Type: application/json" -d '
  {"jsonrpc":"2.0","id":1, "method":"getHealth"}
'
```

Healthy Result:
```json
{"jsonrpc":"2.0","result": "ok","id":1}
```

Unhealthy Result (generic):
```json
{
  "jsonrpc": "2.0",
  "error": {
    "code": -32005,
    "message": "Node is unhealthy",
    "data": {}
  },
  "id": 1
}
```

Unhealthy Result (if additional information is available)
```json
{
  "jsonrpc": "2.0",
  "error": {
    "code": -32005,
    "message": "Node is behind by 42 slots",
    "data": {
      "numSlotsBehind": 42
    }
  },
  "id": 1
}
```

### getIdentity

Returns the identity pubkey for the current node

#### Parameters:

None

#### Results:

The result field will be a JSON object with the following fields:

- `identity`, the identity pubkey of the current node \(as a base-58 encoded string\)

#### Example:

Request:
```bash
curl http://localhost:8899 -X POST -H "Content-Type: application/json" -d '
  {"jsonrpc":"2.0","id":1, "method":"getIdentity"}
'
```

Result:
```json
{"jsonrpc":"2.0","result":{"identity": "2r1F4iWqVcb8M1DbAjQuFpebkQHY9hcVU4WuW2DJBppN"},"id":1}
```

### getInflationGovernor

Returns the current inflation governor

#### Parameters:

- `<object>` - (optional) [Commitment](jsonrpc-api.md#configuring-state-commitment)

#### Results:

The result field will be a JSON object with the following fields:

- `initial: <f64>`, the initial inflation percentage from time 0
- `terminal: <f64>`, terminal inflation percentage
- `taper: <f64>`, rate per year at which inflation is lowered.
   Rate reduction is derived using the target slot time in genesis config
- `foundation: <f64>`, percentage of total inflation allocated to the foundation
- `foundationTerm: <f64>`, duration of foundation pool inflation in years

#### Example:

Request:
```bash
curl http://localhost:8899 -X POST -H "Content-Type: application/json" -d '
  {"jsonrpc":"2.0","id":1, "method":"getInflationGovernor"}
'
```

Result:
```json
{
  "jsonrpc": "2.0",
  "result": {
    "foundation": 0.05,
    "foundationTerm": 7,
    "initial": 0.15,
    "taper": 0.15,
    "terminal": 0.015
  },
  "id": 1
}
```

### getInflationRate

Returns the specific inflation values for the current epoch

#### Parameters:

None

#### Results:

The result field will be a JSON object with the following fields:

- `total: <f64>`, total inflation
- `validator: <f64>`, inflation allocated to validators
- `foundation: <f64>`, inflation allocated to the foundation
- `epoch: <f64>`, epoch for which these values are valid

#### Example:

Request:
```bash
curl http://localhost:8899 -X POST -H "Content-Type: application/json" -d '
  {"jsonrpc":"2.0","id":1, "method":"getInflationRate"}
'
```

Result:
```json
{"jsonrpc":"2.0","result":{"epoch":100,"foundation":0.001,"total":0.149,"validator":0.148},"id":1}
```

### getInflationReward

Returns the inflation reward for a list of addresses for an epoch

#### Parameters:
- `<array>` - An array of addresses to query, as base-58 encoded strings
* `<object>` - (optional) Configuration object containing the following optional fields:
  * (optional) [Commitment](jsonrpc-api.md#configuring-state-commitment)
  * (optional) `epoch: <u64>` - An epoch for which the reward occurs. If omitted, the previous epoch will be used

#### Results

The result field will be a JSON array with the following fields:

- `epoch: <u64>`, epoch for which reward occured
- `effectiveSlot: <u64>`, the slot in which the rewards are effective
- `amount: <u64>`, reward amount in lamports
- `postBalance: <u64>`, post balance of the account in lamports
- `commission: <u8|undefined>` - vote account commission when the reward was credited

#### Example

Request:
```bash
curl http://localhost:8899 -X POST -H "Content-Type: application/json" -d '
  {
    "jsonrpc": "2.0",
    "id": 1,
    "method": "getInflationReward",
    "params": [
       ["6dmNQ5jwLeLk5REvio1JcMshcbvkYMwy26sJ8pbkvStu", "BGsqMegLpV6n6Ve146sSX2dTjUMj3M92HnU8BbNRMhF2"], {"epoch": 2}
    ]
  }
'
```

Response:
```json
  {
    "jsonrpc": "2.0",
    "result": [
        {
            "amount": 2500,
            "effectiveSlot": 224,
            "epoch": 2,
            "postBalance": 499999442500
        },
        null
    ],
    "id": 1
  }
```

### getLargestAccounts

Returns the 20 largest accounts, by lamport balance (results may be cached up to two hours)

#### Parameters:

- `<object>` - (optional) Configuration object containing the following optional fields:
  - (optional) [Commitment](jsonrpc-api.md#configuring-state-commitment)
  - (optional) `filter: <string>` - filter results by account type; currently supported: `circulating|nonCirculating`

#### Results:

The result will be an RpcResponse JSON object with `value` equal to an array of:

- `<object>` - otherwise, a JSON object containing:
  - `address: <string>`, base-58 encoded address of the account
  - `lamports: <u64>`, number of lamports in the account, as a u64

#### Example:

Request:
```bash
curl http://localhost:8899 -X POST -H "Content-Type: application/json" -d '
  {"jsonrpc":"2.0","id":1, "method":"getLargestAccounts"}
'
```

Result:
```json
{
  "jsonrpc": "2.0",
  "result": {
    "context": {
      "slot": 54
    },
    "value": [
      {
        "lamports": 999974,
        "address": "99P8ZgtJYe1buSK8JXkvpLh8xPsCFuLYhz9hQFNw93WJ"
      },
      {
        "lamports": 42,
        "address": "uPwWLo16MVehpyWqsLkK3Ka8nLowWvAHbBChqv2FZeL"
      },
      {
        "lamports": 42,
        "address": "aYJCgU7REfu3XF8b3QhkqgqQvLizx8zxuLBHA25PzDS"
      },
      {
        "lamports": 42,
        "address": "CTvHVtQ4gd4gUcw3bdVgZJJqApXE9nCbbbP4VTS5wE1D"
      },
      {
        "lamports": 20,
        "address": "4fq3xJ6kfrh9RkJQsmVd5gNMvJbuSHfErywvEjNQDPxu"
      },
      {
        "lamports": 4,
        "address": "AXJADheGVp9cruP8WYu46oNkRbeASngN5fPCMVGQqNHa"
      },
      {
        "lamports": 2,
        "address": "8NT8yS6LiwNprgW4yM1jPPow7CwRUotddBVkrkWgYp24"
      },
      {
        "lamports": 1,
        "address": "SysvarEpochSchedu1e111111111111111111111111"
      },
      {
        "lamports": 1,
        "address": "11111111111111111111111111111111"
      },
      {
        "lamports": 1,
        "address": "Stake11111111111111111111111111111111111111"
      },
      {
        "lamports": 1,
        "address": "SysvarC1ock11111111111111111111111111111111"
      },
      {
        "lamports": 1,
        "address": "StakeConfig11111111111111111111111111111111"
      },
      {
        "lamports": 1,
        "address": "SysvarRent111111111111111111111111111111111"
      },
      {
        "lamports": 1,
        "address": "Config1111111111111111111111111111111111111"
      },
      {
        "lamports": 1,
        "address": "SysvarStakeHistory1111111111111111111111111"
      },
      {
        "lamports": 1,
        "address": "SysvarRecentB1ockHashes11111111111111111111"
      },
      {
        "lamports": 1,
        "address": "SysvarFees111111111111111111111111111111111"
      },
      {
        "lamports": 1,
        "address": "Vote111111111111111111111111111111111111111"
      }
    ]
  },
  "id": 1
}
```

### getLeaderSchedule

Returns the leader schedule for an epoch

#### Parameters:

- `<u64>` - (optional) Fetch the leader schedule for the epoch that corresponds to the provided slot.
            If unspecified, the leader schedule for the current epoch is fetched
- `<object>` - (optional) Configuration object containing the following field:
  - (optional) [Commitment](jsonrpc-api.md#configuring-state-commitment)
  - (optional) `identity: <string>` - Only return results for this validator identity (base-58 encoded)

#### Results:

- `<null>` - if requested epoch is not found
- `<object>` - otherwise, the result field will be a dictionary of validator identities,
  as base-58 encoded strings, and their corresponding leader slot indices as values
  (indices are relative to the first slot in the requested epoch)

#### Example:

Request:
```bash
curl http://localhost:8899 -X POST -H "Content-Type: application/json" -d '
  {"jsonrpc":"2.0","id":1, "method":"getLeaderSchedule"}
'
```

Result:
```json
{
  "jsonrpc":"2.0",
  "result":{
    "4Qkev8aNZcqFNSRhQzwyLMFSsi94jHqE8WNVTJzTP99F":[0,1,2,3,4,5,6,7,8,9,10,11,12,13,14,15,16,17,18,19,20,21,22,23,24,25,26,27,28,29,30,31,32,33,34,35,36,37,38,39,40,41,42,43,44,45,46,47,48,49,50,51,52,53,54,55,56,57,58,59,60,61,62,63]
  },
  "id":1
}
```

#### Example:

Request:
```bash
curl http://localhost:8899 -X POST -H "Content-Type: application/json" -d '
  {
    "jsonrpc": "2.0",
    "id": 1,
    "method": "getLeaderSchedule",
    "params": [
      null,
      {
        "identity": "4Qkev8aNZcqFNSRhQzwyLMFSsi94jHqE8WNVTJzTP99F"
      }
    ]
  }
'
```

Result:
```json
{
  "jsonrpc":"2.0",
  "result":{
    "4Qkev8aNZcqFNSRhQzwyLMFSsi94jHqE8WNVTJzTP99F":[0,1,2,3,4,5,6,7,8,9,10,11,12,13,14,15,16,17,18,19,20,21,22,23,24,25,26,27,28,29,30,31,32,33,34,35,36,37,38,39,40,41,42,43,44,45,46,47,48,49,50,51,52,53,54,55,56,57,58,59,60,61,62,63]
  },
  "id":1
}
```

### getMaxRetransmitSlot

Get the max slot seen from retransmit stage.

#### Results:

- `<u64>` - Slot

#### Example:

Request:
```bash
curl http://localhost:8899 -X POST -H "Content-Type: application/json" -d '
  {"jsonrpc":"2.0","id":1, "method":"getMaxRetransmitSlot"}
'
```

Result:
```json
{"jsonrpc":"2.0","result":1234,"id":1}
```

### getMaxShredInsertSlot

Get the max slot seen from after shred insert.

#### Results:

- `<u64>` - Slot

#### Example:

Request:
```bash
curl http://localhost:8899 -X POST -H "Content-Type: application/json" -d '
  {"jsonrpc":"2.0","id":1, "method":"getMaxShredInsertSlot"}
'
```

Result:
```json
{"jsonrpc":"2.0","result":1234,"id":1}
```

### getMinimumBalanceForRentExemption

Returns minimum balance required to make account rent exempt.

#### Parameters:

- `<usize>` - account data length
- `<object>` - (optional) [Commitment](jsonrpc-api.md#configuring-state-commitment)

#### Results:

- `<u64>` - minimum lamports required in account

#### Example:

Request:
```bash
curl http://localhost:8899 -X POST -H "Content-Type: application/json" -d '
  {"jsonrpc":"2.0", "id":1, "method":"getMinimumBalanceForRentExemption", "params":[50]}
'
```

Result:
```json
{"jsonrpc":"2.0","result":500,"id":1}
```

### getMultipleAccounts

Returns the account information for a list of Pubkeys

#### Parameters:

- `<array>` - An array of Pubkeys to query, as base-58 encoded strings
- `<object>` - (optional) Configuration object containing the following optional fields:
  - (optional) [Commitment](jsonrpc-api.md#configuring-state-commitment)
  - `encoding: <string>` - encoding for Account data, either "base58" (*slow*), "base64", "base64+zstd", or "jsonParsed".
    "base58" is limited to Account data of less than 129 bytes.
    "base64" will return base64 encoded data for Account data of any size.
    "base64+zstd" compresses the Account data using [Zstandard](https://facebook.github.io/zstd/) and base64-encodes the result.
    "jsonParsed" encoding attempts to use program-specific state parsers to return more human-readable and explicit account state data. If "jsonParsed" is requested but a parser cannot be found, the field falls back to "base64" encoding, detectable when the `data` field is type `<string>`.
  - (optional) `dataSlice: <object>` - limit the returned account data using the provided `offset: <usize>` and `length: <usize>` fields; only available for "base58", "base64" or "base64+zstd" encodings.


#### Results:

The result will be an RpcResponse JSON object with `value` equal to:

An array of:

- `<null>` - if the account at that Pubkey doesn't exist
- `<object>` - otherwise, a JSON object containing:
  - `lamports: <u64>`, number of lamports assigned to this account, as a u64
  - `owner: <string>`, base-58 encoded Pubkey of the program this account has been assigned to
  - `data: <[string, encoding]|object>`, data associated with the account, either as encoded binary data or JSON format `{<program>: <state>}`, depending on encoding parameter
  - `executable: <bool>`, boolean indicating if the account contains a program \(and is strictly read-only\)
  - `rentEpoch: <u64>`, the epoch at which this account will next owe rent, as u64

#### Example:

Request:
```bash
curl http://localhost:8899 -X POST -H "Content-Type: application/json" -d '
  {
    "jsonrpc": "2.0",
    "id": 1,
    "method": "getMultipleAccounts",
    "params": [
      [
        "vines1vzrYbzLMRdu58ou5XTby4qAqVRLmqo36NKPTg",
        "4fYNw3dojWmQ4dXtSGE9epjRGy9pFSx62YypT7avPYvA"
      ],
      {
        "dataSlice": {
          "offset": 0,
          "length": 0
        }
      }
    ]
  }
'
```

Result:
```json
{
  "jsonrpc": "2.0",
  "result": {
    "context": {
      "slot": 1
    },
    "value": [
      {
        "data": [
          "AAAAAAEAAAACtzNsyJrW0g==",
          "base64"
        ],
        "executable": false,
        "lamports": 1000000000,
        "owner": "11111111111111111111111111111111",
        "rentEpoch": 2
      },
      {
        "data": [
          "",
          "base64"
        ],
        "executable": false,
        "lamports": 5000000000,
        "owner": "11111111111111111111111111111111",
        "rentEpoch": 2
      }
    ]
  },
  "id": 1
}
```

#### Example:
Request:
```bash
curl http://localhost:8899 -X POST -H "Content-Type: application/json" -d '
  {
    "jsonrpc": "2.0",
    "id": 1,
    "method": "getMultipleAccounts",
    "params": [
      [
        "vines1vzrYbzLMRdu58ou5XTby4qAqVRLmqo36NKPTg",
        "4fYNw3dojWmQ4dXtSGE9epjRGy9pFSx62YypT7avPYvA"
      ],
      {
        "encoding": "base58"
      }
    ]
  }
'
```

Result:
```json
{
  "jsonrpc": "2.0",
  "result": {
    "context": {
      "slot": 1
    },
    "value": [
      {
        "data": [
          "11116bv5nS2h3y12kD1yUKeMZvGcKLSjQgX6BeV7u1FrjeJcKfsHRTPuR3oZ1EioKtYGiYxpxMG5vpbZLsbcBYBEmZZcMKaSoGx9JZeAuWf",
          "base58"
        ],
        "executable": false,
        "lamports": 1000000000,
        "owner": "11111111111111111111111111111111",
        "rentEpoch": 2
      },
      {
        "data": [
          "",
          "base58"
        ],
        "executable": false,
        "lamports": 5000000000,
        "owner": "11111111111111111111111111111111",
        "rentEpoch": 2
      }
    ]
  },
  "id": 1
}
```

### getProgramAccounts

Returns all accounts owned by the provided program Pubkey

#### Parameters:

- `<string>` - Pubkey of program, as base-58 encoded string
- `<object>` - (optional) Configuration object containing the following optional fields:
  - (optional) [Commitment](jsonrpc-api.md#configuring-state-commitment)
  - `encoding: <string>` - encoding for Account data, either "base58" (*slow*), "base64", "base64+zstd", or "jsonParsed".
    "base58" is limited to Account data of less than 129 bytes.
    "base64" will return base64 encoded data for Account data of any size.
    "base64+zstd" compresses the Account data using [Zstandard](https://facebook.github.io/zstd/) and base64-encodes the result.
    "jsonParsed" encoding attempts to use program-specific state parsers to return more human-readable and explicit account state data. If "jsonParsed" is requested but a parser cannot be found, the field falls back to "base64" encoding, detectable when the `data` field is type `<string>`.
  - (optional) `dataSlice: <object>` - limit the returned account data using the provided `offset: <usize>` and `length: <usize>` fields; only available for "base58", "base64" or "base64+zstd" encodings.
  - (optional) `filters: <array>` - filter results using various [filter objects](jsonrpc-api.md#filters); account must meet all filter criteria to be included in results
  - (optional) `withContext: bool` - wrap the result in an RpcResponse JSON object.
##### Filters:
- `memcmp: <object>` - compares a provided series of bytes with program account data at a particular offset. Fields:
  - `offset: <usize>` - offset into program account data to start comparison
  - `bytes: <string>` - data to match, as base-58 encoded string and limited to less than 129 bytes

- `dataSize: <u64>` - compares the program account data length with the provided data size

#### Results:

By default the result field will be an array of JSON objects. If `withContext` flag is set the array will be wrapped in an RpcResponse JSON object.

The array will contain:

- `pubkey: <string>` - the account Pubkey as base-58 encoded string
- `account: <object>` - a JSON object, with the following sub fields:
   - `lamports: <u64>`, number of lamports assigned to this account, as a u64
   - `owner: <string>`, base-58 encoded Pubkey of the program this account has been assigned to
   - `data: <[string,encoding]|object>`, data associated with the account, either as encoded binary data or JSON format `{<program>: <state>}`, depending on encoding parameter
   - `executable: <bool>`, boolean indicating if the account contains a program \(and is strictly read-only\)
   - `rentEpoch: <u64>`, the epoch at which this account will next owe rent, as u64

#### Example:
Request:
```bash
curl http://localhost:8899 -X POST -H "Content-Type: application/json" -d '
  {"jsonrpc":"2.0", "id":1, "method":"getProgramAccounts", "params":["4Nd1mBQtrMJVYVfKf2PJy9NZUZdTAsp7D4xWLs4gDB4T"]}
'
```

Result:
```json
{
  "jsonrpc": "2.0",
  "result": [
    {
      "account": {
        "data": "2R9jLfiAQ9bgdcw6h8s44439",
        "executable": false,
        "lamports": 15298080,
        "owner": "4Nd1mBQtrMJVYVfKf2PJy9NZUZdTAsp7D4xWLs4gDB4T",
        "rentEpoch": 28
      },
      "pubkey": "CxELquR1gPP8wHe33gZ4QxqGB3sZ9RSwsJ2KshVewkFY"
    }
  ],
  "id": 1
}
```

#### Example:
Request:
```bash
curl http://localhost:8899 -X POST -H "Content-Type: application/json" -d '
  {
    "jsonrpc": "2.0",
    "id": 1,
    "method": "getProgramAccounts",
    "params": [
      "4Nd1mBQtrMJVYVfKf2PJy9NZUZdTAsp7D4xWLs4gDB4T",
      {
        "filters": [
          {
            "dataSize": 17
          },
          {
            "memcmp": {
              "offset": 4,
              "bytes": "3Mc6vR"
            }
          }
        ]
      }
    ]
  }
'
```

Result:
```json
{
  "jsonrpc": "2.0",
  "result": [
    {
      "account": {
        "data": "2R9jLfiAQ9bgdcw6h8s44439",
        "executable": false,
        "lamports": 15298080,
        "owner": "4Nd1mBQtrMJVYVfKf2PJy9NZUZdTAsp7D4xWLs4gDB4T",
        "rentEpoch": 28
      },
      "pubkey": "CxELquR1gPP8wHe33gZ4QxqGB3sZ9RSwsJ2KshVewkFY"
    }
  ],
  "id": 1
}
```

### getRecentBlockhash

Returns a recent block hash from the ledger, and a fee schedule that can be used to compute the cost of submitting a transaction using it.

#### Parameters:

- `<object>` - (optional) [Commitment](jsonrpc-api.md#configuring-state-commitment)

#### Results:

An RpcResponse containing a JSON object consisting of a string blockhash and FeeCalculator JSON object.

- `RpcResponse<object>` - RpcResponse JSON object with `value` field set to a JSON object including:
- `blockhash: <string>` - a Hash as base-58 encoded string
- `feeCalculator: <object>` - FeeCalculator object, the fee schedule for this block hash

#### Example:

Request:
```bash
curl http://localhost:8899 -X POST -H "Content-Type: application/json" -d '
  {"jsonrpc":"2.0","id":1, "method":"getRecentBlockhash"}
'
```

Result:
```json
{
  "jsonrpc": "2.0",
  "result": {
    "context": {
      "slot": 1
    },
    "value": {
      "blockhash": "CSymwgTNX1j3E4qhKfJAUE41nBWEwXufoYryPbkde5RR",
      "feeCalculator": {
        "lamportsPerSignature": 5000
      }
    }
  },
  "id": 1
}
```

### getRecentPerformanceSamples

Returns a list of recent performance samples, in reverse slot order. Performance samples are taken every 60 seconds and
include the number of transactions and slots that occur in a given time window.

#### Parameters:
- `limit: <usize>` - (optional) number of samples to return (maximum 720)

#### Results:

An array of:

- `RpcPerfSample<object>`
  - `slot: <u64>` - Slot in which sample was taken at
  - `numTransactions: <u64>` - Number of transactions in sample
  - `numSlots: <u64>` - Number of slots in sample
  - `samplePeriodSecs: <u16>` - Number of seconds in a sample window

#### Example:

Request:
```bash
// Request
curl http://localhost:8899 -X POST -H "Content-Type: application/json" -d '
  {"jsonrpc":"2.0", "id":1, "method":"getRecentPerformanceSamples", "params": [4]}
'
```

Result:
```json
{
  "jsonrpc": "2.0",
  "result": [
    {
      "numSlots": 126,
      "numTransactions": 126,
      "samplePeriodSecs": 60,
      "slot": 348125
    },
    {
      "numSlots": 126,
      "numTransactions": 126,
      "samplePeriodSecs": 60,
      "slot": 347999
    },
    {
      "numSlots": 125,
      "numTransactions": 125,
      "samplePeriodSecs": 60,
      "slot": 347873
    },
    {
      "numSlots": 125,
      "numTransactions": 125,
      "samplePeriodSecs": 60,
      "slot": 347748
    }
  ],
  "id": 1
}
```


### getSnapshotSlot

Returns the highest slot that the node has a snapshot for

#### Parameters:

None

#### Results:

- `<u64>` - Snapshot slot

#### Example:

Request:
```bash
curl http://localhost:8899 -X POST -H "Content-Type: application/json" -d '
  {"jsonrpc":"2.0","id":1, "method":"getSnapshotSlot"}
'
```

Result:
```json
{"jsonrpc":"2.0","result":100,"id":1}
```

Result when the node has no snapshot:
```json
{"jsonrpc":"2.0","error":{"code":-32008,"message":"No snapshot"},"id":1}
```

### getSignaturesForAddress

**NEW: This method is only available in solana-core v1.7 or newer. Please use
[getConfirmedSignaturesForAddress2](jsonrpc-api.md#getconfirmedsignaturesforaddress2) for solana-core v1.6**


Returns confirmed signatures for transactions involving an
address backwards in time from the provided signature or most recent confirmed block

#### Parameters:
* `<string>` - account address as base-58 encoded string
* `<object>` - (optional) Configuration object containing the following fields:
  * `limit: <number>` - (optional) maximum transaction signatures to return (between 1 and 1,000, default: 1,000).
  * `before: <string>` - (optional) start searching backwards from this transaction signature.
                         If not provided the search starts from the top of the highest max confirmed block.
  * `until: <string>` - (optional) search until this transaction signature, if found before limit reached.
  * (optional) [Commitment](jsonrpc-api.md#configuring-state-commitment); "processed" is not supported. If parameter not provided, the default is "finalized".

#### Results:
The result field will be an array of transaction signature information, ordered
from newest to oldest transaction:
* `<object>`
  * `signature: <string>` - transaction signature as base-58 encoded string
  * `slot: <u64>` - The slot that contains the block with the transaction
  * `err: <object | null>` - Error if transaction failed, null if transaction succeeded. [TransactionError definitions](https://github.com/solana-labs/solana/blob/master/sdk/src/transaction.rs#L24)
  * `memo: <string |null>` - Memo associated with the transaction, null if no memo is present
  * `blockTime: <i64 | null>` - estimated production time, as Unix timestamp (seconds since the Unix epoch) of when transaction was processed. null if not available.

#### Example:
Request:
```bash
curl http://localhost:8899 -X POST -H "Content-Type: application/json" -d '
  {
    "jsonrpc": "2.0",
    "id": 1,
    "method": "getSignaturesForAddress",
    "params": [
      "Vote111111111111111111111111111111111111111",
      {
        "limit": 1
      }
    ]
  }
'
```

Result:
```json
{
  "jsonrpc": "2.0",
  "result": [
    {
      "err": null,
      "memo": null,
      "signature": "5h6xBEauJ3PK6SWCZ1PGjBvj8vDdWG3KpwATGy1ARAXFSDwt8GFXM7W5Ncn16wmqokgpiKRLuS83KUxyZyv2sUYv",
      "slot": 114,
      "blockTime": null
    }
  ],
  "id": 1
}
```

### getSignatureStatuses

Returns the statuses of a list of signatures. Unless the
`searchTransactionHistory` configuration parameter is included, this method only
searches the recent status cache of signatures, which retains statuses for all
active slots plus `MAX_RECENT_BLOCKHASHES` rooted slots.

#### Parameters:

- `<array>` - An array of transaction signatures to confirm, as base-58 encoded strings
- `<object>` - (optional) Configuration object containing the following field:
  - `searchTransactionHistory: <bool>` - if true, a Solana node will search its ledger cache for any signatures not found in the recent status cache

#### Results:

An RpcResponse containing a JSON object consisting of an array of TransactionStatus objects.

- `RpcResponse<object>` - RpcResponse JSON object with `value` field:

An array of:

- `<null>` - Unknown transaction
- `<object>`
  - `slot: <u64>` - The slot the transaction was processed
  - `confirmations: <usize | null>` - Number of blocks since signature confirmation, null if rooted, as well as finalized by a supermajority of the cluster
  - `err: <object | null>` - Error if transaction failed, null if transaction succeeded. [TransactionError definitions](https://github.com/solana-labs/solana/blob/master/sdk/src/transaction.rs#L24)
  - `confirmationStatus: <string | null>` - The transaction's cluster confirmation status; either `processed`, `confirmed`, or `finalized`. See [Commitment](jsonrpc-api.md#configuring-state-commitment) for more on optimistic confirmation.
  - DEPRECATED: `status: <object>` - Transaction status
    - `"Ok": <null>` - Transaction was successful
    - `"Err": <ERR>` - Transaction failed with TransactionError

#### Example:

Request:
```bash
curl http://localhost:8899 -X POST -H "Content-Type: application/json" -d '
  {
    "jsonrpc": "2.0",
    "id": 1,
    "method": "getSignatureStatuses",
    "params": [
      [
        "5VERv8NMvzbJMEkV8xnrLkEaWRtSz9CosKDYjCJjBRnbJLgp8uirBgmQpjKhoR4tjF3ZpRzrFmBV6UjKdiSZkQUW",
        "5j7s6NiJS3JAkvgkoc18WVAsiSaci2pxB2A6ueCJP4tprA2TFg9wSyTLeYouxPBJEMzJinENTkpA52YStRW5Dia7"
      ]
    ]
  }
'
```

Result:
```json
{
  "jsonrpc": "2.0",
  "result": {
    "context": {
      "slot": 82
    },
    "value": [
      {
        "slot": 72,
        "confirmations": 10,
        "err": null,
        "status": {
          "Ok": null
        },
        "confirmationStatus": "confirmed",
      },
      null
    ]
  },
  "id": 1
}
```

#### Example:
Request:
```bash
curl http://localhost:8899 -X POST -H "Content-Type: application/json" -d '
  {
    "jsonrpc": "2.0",
    "id": 1,
    "method": "getSignatureStatuses",
    "params": [
      [
        "5VERv8NMvzbJMEkV8xnrLkEaWRtSz9CosKDYjCJjBRnbJLgp8uirBgmQpjKhoR4tjF3ZpRzrFmBV6UjKdiSZkQUW"
      ],
      {
        "searchTransactionHistory": true
      }
    ]
  }
'
```

Result:
```json
{
  "jsonrpc": "2.0",
  "result": {
    "context": {
      "slot": 82
    },
    "value": [
      {
        "slot": 48,
        "confirmations": null,
        "err": null,
        "status": {
          "Ok": null
        },
        "confirmationStatus": "finalized",
      },
      null
    ]
  },
  "id": 1
}
```

### getSlot

Returns the current slot the node is processing

#### Parameters:

- `<object>` - (optional) [Commitment](jsonrpc-api.md#configuring-state-commitment)

#### Results:

- `<u64>` - Current slot

#### Example:

Request:
```bash
curl http://localhost:8899 -X POST -H "Content-Type: application/json" -d '
  {"jsonrpc":"2.0","id":1, "method":"getSlot"}
'
```

Result:
```json
{"jsonrpc":"2.0","result":1234,"id":1}
```

### getSlotLeader

Returns the current slot leader

#### Parameters:

- `<object>` - (optional) [Commitment](jsonrpc-api.md#configuring-state-commitment)

#### Results:

- `<string>` - Node identity Pubkey as base-58 encoded string

#### Example:

Request:
```bash
curl http://localhost:8899 -X POST -H "Content-Type: application/json" -d '
  {"jsonrpc":"2.0","id":1, "method":"getSlotLeader"}
'
```

Result:
```json
{"jsonrpc":"2.0","result":"ENvAW7JScgYq6o4zKZwewtkzzJgDzuJAFxYasvmEQdpS","id":1}
```

### getSlotLeaders

Returns the slot leaders for a given slot range

#### Parameters:

- `<u64>` - Start slot, as u64 integer
- `<u64>` - Limit, as u64 integer

#### Results:

- `<array<string>>` - Node identity public keys as base-58 encoded strings

#### Example:

If the current slot is #99, query the next 10 leaders with the following request:

Request:
```bash
curl http://localhost:8899 -X POST -H "Content-Type: application/json" -d '
  {"jsonrpc":"2.0","id":1, "method":"getSlotLeaders", "params":[100, 10]}
'
```

Result:

The first leader returned is the leader for slot #100:

```json
{
  "jsonrpc": "2.0",
  "result": [
    "ChorusmmK7i1AxXeiTtQgQZhQNiXYU84ULeaYF1EH15n",
    "ChorusmmK7i1AxXeiTtQgQZhQNiXYU84ULeaYF1EH15n",
    "ChorusmmK7i1AxXeiTtQgQZhQNiXYU84ULeaYF1EH15n",
    "ChorusmmK7i1AxXeiTtQgQZhQNiXYU84ULeaYF1EH15n",
    "Awes4Tr6TX8JDzEhCZY2QVNimT6iD1zWHzf1vNyGvpLM",
    "Awes4Tr6TX8JDzEhCZY2QVNimT6iD1zWHzf1vNyGvpLM",
    "Awes4Tr6TX8JDzEhCZY2QVNimT6iD1zWHzf1vNyGvpLM",
    "Awes4Tr6TX8JDzEhCZY2QVNimT6iD1zWHzf1vNyGvpLM",
    "DWvDTSh3qfn88UoQTEKRV2JnLt5jtJAVoiCo3ivtMwXP",
    "DWvDTSh3qfn88UoQTEKRV2JnLt5jtJAVoiCo3ivtMwXP"
  ],
  "id": 1
}
```

### getStakeActivation

Returns epoch activation information for a stake account

#### Parameters:

* `<string>` - Pubkey of stake account to query, as base-58 encoded string
* `<object>` - (optional) Configuration object containing the following optional fields:
  * (optional) [Commitment](jsonrpc-api.md#configuring-state-commitment)
  * (optional) `epoch: <u64>` - epoch for which to calculate activation details. If parameter not provided, defaults to current epoch.

#### Results:

The result will be a JSON object with the following fields:

* `state: <string` - the stake account's activation state, one of: `active`, `inactive`, `activating`, `deactivating`
* `active: <u64>` - stake active during the epoch
* `inactive: <u64>` - stake inactive during the epoch

#### Example:
Request:
```bash
curl http://localhost:8899 -X POST -H "Content-Type: application/json" -d '
  {"jsonrpc":"2.0","id":1, "method":"getStakeActivation", "params": ["CYRJWqiSjLitBAcRxPvWpgX3s5TvmN2SuRY3eEYypFvT"]}
'
```

Result:
```json
{"jsonrpc":"2.0","result":{"active":197717120,"inactive":0,"state":"active"},"id":1}
```

#### Example:
Request:
```bash
curl http://localhost:8899 -X POST -H "Content-Type: application/json" -d '
  {
    "jsonrpc": "2.0",
    "id": 1,
    "method": "getStakeActivation",
    "params": [
      "CYRJWqiSjLitBAcRxPvWpgX3s5TvmN2SuRY3eEYypFvT",
      {
        "epoch": 4
      }
    ]
  }
'
```

Result:
```json
{
  "jsonrpc": "2.0",
  "result": {
    "active": 124429280,
    "inactive": 73287840,
    "state": "activating"
  },
  "id": 1
}
```

### getSupply

Returns information about the current supply.

#### Parameters:

- `<object>` - (optional) [Commitment](jsonrpc-api.md#configuring-state-commitment)

#### Results:

The result will be an RpcResponse JSON object with `value` equal to a JSON object containing:

- `total: <u64>` - Total supply in lamports
- `circulating: <u64>` - Circulating supply in lamports
- `nonCirculating: <u64>` - Non-circulating supply in lamports
- `nonCirculatingAccounts: <array>` - an array of account addresses of non-circulating accounts, as strings

#### Example:

Request:
```bash
curl http://localhost:8899 -X POST -H "Content-Type: application/json" -d '
  {"jsonrpc":"2.0", "id":1, "method":"getSupply"}
'
```

Result:
```json
{
  "jsonrpc": "2.0",
  "result": {
    "context": {
      "slot": 1114
    },
    "value": {
      "circulating": 16000,
      "nonCirculating": 1000000,
      "nonCirculatingAccounts": [
        "FEy8pTbP5fEoqMV1GdTz83byuA8EKByqYat1PKDgVAq5",
        "9huDUZfxoJ7wGMTffUE7vh1xePqef7gyrLJu9NApncqA",
        "3mi1GmwEE3zo2jmfDuzvjSX9ovRXsDUKHvsntpkhuLJ9",
        "BYxEJTDerkaRWBem3XgnVcdhppktBXa2HbkHPKj2Ui4Z"
      ],
      "total": 1016000
    }
  },
  "id": 1
}
```

### getTokenAccountBalance

Returns the token balance of an SPL Token account.

#### Parameters:

- `<string>` - Pubkey of Token account to query, as base-58 encoded string
- `<object>` - (optional) [Commitment](jsonrpc-api.md#configuring-state-commitment)

#### Results:

The result will be an RpcResponse JSON object with `value` equal to a JSON object containing:

- `amount: <string>` - the raw balance without decimals, a string representation of u64
- `decimals: <u8>` - number of base 10 digits to the right of the decimal place
- `uiAmount: <number | null>` - the balance, using mint-prescribed decimals **DEPRECATED**
- `uiAmountString: <string>` - the balance as a string, using mint-prescribed decimals

For more details on returned data: The
[Token Balances Structure](jsonrpc-api.md#token-balances-structure) response from [getBlock](jsonrpc-api.md#getblock) follows a similar structure.

#### Example:

Request:
```bash
curl http://localhost:8899 -X POST -H "Content-Type: application/json" -d '
  {"jsonrpc":"2.0", "id":1, "method":"getTokenAccountBalance", "params": ["7fUAJdStEuGbc3sM84cKRL6yYaaSstyLSU4ve5oovLS7"]}
'
```

Result:
```json
{
  "jsonrpc": "2.0",
  "result": {
    "context": {
      "slot": 1114
    },
    "value": {
      "amount": "9864",
      "decimals": 2,
      "uiAmount": 98.64,
      "uiAmountString": "98.64",
    },
    "id": 1
  }
}
```

### getTokenAccountsByDelegate

Returns all SPL Token accounts by approved Delegate.

#### Parameters:

- `<string>` - Pubkey of account delegate to query, as base-58 encoded string
- `<object>` - Either:
  * `mint: <string>` - Pubkey of the specific token Mint to limit accounts to, as base-58 encoded string; or
  * `programId: <string>` - Pubkey of the Token program ID that owns the accounts, as base-58 encoded string
- `<object>` - (optional) Configuration object containing the following optional fields:
  - (optional) [Commitment](jsonrpc-api.md#configuring-state-commitment)
  - `encoding: <string>` - encoding for Account data, either "base58" (*slow*), "base64", "base64+zstd" or "jsonParsed".
    "jsonParsed" encoding attempts to use program-specific state parsers to return more human-readable and explicit account state data. If "jsonParsed" is requested but a valid mint cannot be found for a particular account, that account will be filtered out from results.
  - (optional) `dataSlice: <object>` - limit the returned account data using the provided `offset: <usize>` and `length: <usize>` fields; only available for "base58", "base64" or "base64+zstd" encodings.

#### Results:

The result will be an RpcResponse JSON object with `value` equal to an array of JSON objects, which will contain:

- `pubkey: <string>` - the account Pubkey as base-58 encoded string
- `account: <object>` - a JSON object, with the following sub fields:
   - `lamports: <u64>`, number of lamports assigned to this account, as a u64
   - `owner: <string>`, base-58 encoded Pubkey of the program this account has been assigned to
   - `data: <object>`, Token state data associated with the account, either as encoded binary data or in JSON format `{<program>: <state>}`
   - `executable: <bool>`, boolean indicating if the account contains a program \(and is strictly read-only\)
   - `rentEpoch: <u64>`, the epoch at which this account will next owe rent, as u64

#### Example:

```bash
curl http://localhost:8899 -X POST -H "Content-Type: application/json" -d '
  {
    "jsonrpc": "2.0",
    "id": 1,
    "method": "getTokenAccountsByDelegate",
    "params": [
      "4Nd1mBQtrMJVYVfKf2PJy9NZUZdTAsp7D4xWLs4gDB4T",
      {
        "programId": "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA"
      },
      {
        "encoding": "jsonParsed"
      }
    ]
  }
'
```

Result:
```json
{
  "jsonrpc": "2.0",
  "result": {
    "context": {
      "slot": 1114
    },
    "value": [
      {
        "data": {
          "program": "spl-token",
          "parsed": {
            "accountType": "account",
            "info": {
              "tokenAmount": {
                "amount": "1",
                "decimals": 1,
                "uiAmount": 0.1,
                "uiAmountString": "0.1",
              },
              "delegate": "4Nd1mBQtrMJVYVfKf2PJy9NZUZdTAsp7D4xWLs4gDB4T",
              "delegatedAmount": 1,
              "isInitialized": true,
              "isNative": false,
              "mint": "3wyAj7Rt1TWVPZVteFJPLa26JmLvdb1CAKEFZm3NY75E",
              "owner": "CnPoSPKXu7wJqxe59Fs72tkBeALovhsCxYeFwPCQH9TD"
            }
          }
        },
        "executable": false,
        "lamports": 1726080,
        "owner": "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA",
        "rentEpoch": 4
      }
    ]
  },
  "id": 1
}
```

### getTokenAccountsByOwner

Returns all SPL Token accounts by token owner.

#### Parameters:

- `<string>` - Pubkey of account owner to query, as base-58 encoded string
- `<object>` - Either:
  * `mint: <string>` - Pubkey of the specific token Mint to limit accounts to, as base-58 encoded string; or
  * `programId: <string>` - Pubkey of the Token program ID that owns the accounts, as base-58 encoded string
- `<object>` - (optional) Configuration object containing the following optional fields:
  - (optional) [Commitment](jsonrpc-api.md#configuring-state-commitment)
  - `encoding: <string>` - encoding for Account data, either "base58" (*slow*), "base64", "base64+zstd" or "jsonParsed".
    "jsonParsed" encoding attempts to use program-specific state parsers to return more human-readable and explicit account state data. If "jsonParsed" is requested but a valid mint cannot be found for a particular account, that account will be filtered out from results.
  - (optional) `dataSlice: <object>` - limit the returned account data using the provided `offset: <usize>` and `length: <usize>` fields; only available for "base58", "base64" or "base64+zstd" encodings.

#### Results:

The result will be an RpcResponse JSON object with `value` equal to an array of JSON objects, which will contain:

- `pubkey: <string>` - the account Pubkey as base-58 encoded string
- `account: <object>` - a JSON object, with the following sub fields:
   - `lamports: <u64>`, number of lamports assigned to this account, as a u64
   - `owner: <string>`, base-58 encoded Pubkey of the program this account has been assigned to
   - `data: <object>`, Token state data associated with the account, either as encoded binary data or in JSON format `{<program>: <state>}`
   - `executable: <bool>`, boolean indicating if the account contains a program \(and is strictly read-only\)
   - `rentEpoch: <u64>`, the epoch at which this account will next owe rent, as u64

#### Example:

```bash
curl http://localhost:8899 -X POST -H "Content-Type: application/json" -d '
  {
    "jsonrpc": "2.0",
    "id": 1,
    "method": "getTokenAccountsByOwner",
    "params": [
      "4Qkev8aNZcqFNSRhQzwyLMFSsi94jHqE8WNVTJzTP99F",
      {
        "mint": "3wyAj7Rt1TWVPZVteFJPLa26JmLvdb1CAKEFZm3NY75E"
      },
      {
        "encoding": "jsonParsed"
      }
    ]
  }
'
```

Result:
```json
{
  "jsonrpc": "2.0",
  "result": {
    "context": {
      "slot": 1114
    },
    "value": [
      {
        "data": {
          "program": "spl-token",
          "parsed": {
            "accountType": "account",
            "info": {
              "tokenAmount": {
                "amount": "1",
                "decimals": 1,
                "uiAmount": 0.1,
                "uiAmountString": "0.1",
              },
              "delegate": null,
              "delegatedAmount": 1,
              "isInitialized": true,
              "isNative": false,
              "mint": "3wyAj7Rt1TWVPZVteFJPLa26JmLvdb1CAKEFZm3NY75E",
              "owner": "4Qkev8aNZcqFNSRhQzwyLMFSsi94jHqE8WNVTJzTP99F"
            }
          }
        },
        "executable": false,
        "lamports": 1726080,
        "owner": "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA",
        "rentEpoch": 4
      }
    ]
  },
  "id": 1
}
```

### getTokenLargestAccounts

Returns the 20 largest accounts of a particular SPL Token type.

#### Parameters:

- `<string>` - Pubkey of token Mint to query, as base-58 encoded string
- `<object>` - (optional) [Commitment](jsonrpc-api.md#configuring-state-commitment)

#### Results:

The result will be an RpcResponse JSON object with `value` equal to an array of JSON objects containing:

- `address: <string>` - the address of the token account
- `amount: <string>` - the raw token account balance without decimals, a string representation of u64
- `decimals: <u8>` - number of base 10 digits to the right of the decimal place
- `uiAmount: <number | null>` - the token account balance, using mint-prescribed decimals **DEPRECATED**
- `uiAmountString: <string>` - the token account balance as a string, using mint-prescribed decimals

#### Example:

```bash
curl http://localhost:8899 -X POST -H "Content-Type: application/json" -d '
  {"jsonrpc":"2.0", "id":1, "method":"getTokenLargestAccounts", "params": ["3wyAj7Rt1TWVPZVteFJPLa26JmLvdb1CAKEFZm3NY75E"]}
'
```

Result:
```json
{
  "jsonrpc": "2.0",
  "result": {
    "context": {
      "slot": 1114
    },
    "value": [
      {
        "address": "FYjHNoFtSQ5uijKrZFyYAxvEr87hsKXkXcxkcmkBAf4r",
        "amount": "771",
        "decimals": 2,
        "uiAmount": 7.71,
        "uiAmountString": "7.71"
      },
      {
        "address": "BnsywxTcaYeNUtzrPxQUvzAWxfzZe3ZLUJ4wMMuLESnu",
        "amount": "229",
        "decimals": 2,
        "uiAmount": 2.29,
        "uiAmountString": "2.29"
      }
    ]
  },
  "id": 1
}
```

### getTokenSupply

Returns the total supply of an SPL Token type.

#### Parameters:

- `<string>` - Pubkey of token Mint to query, as base-58 encoded string
- `<object>` - (optional) [Commitment](jsonrpc-api.md#configuring-state-commitment)

#### Results:

The result will be an RpcResponse JSON object with `value` equal to a JSON object containing:

- `amount: <string>` - the raw total token supply without decimals, a string representation of u64
- `decimals: <u8>` - number of base 10 digits to the right of the decimal place
- `uiAmount: <number | null>` - the total token supply, using mint-prescribed decimals **DEPRECATED**
- `uiAmountString: <string>` - the total token supply as a string, using mint-prescribed decimals

#### Example:

```bash
curl http://localhost:8899 -X POST -H "Content-Type: application/json" -d '
  {"jsonrpc":"2.0", "id":1, "method":"getTokenSupply", "params": ["3wyAj7Rt1TWVPZVteFJPLa26JmLvdb1CAKEFZm3NY75E"]}
'
```

Result:
```json
{
  "jsonrpc": "2.0",
  "result": {
    "context": {
      "slot": 1114
    },
    "value": {
      "amount": "100000",
      "decimals": 2,
      "uiAmount": 1000,
      "uiAmountString": "1000",
    }
  },
  "id": 1
}
```

### getTransaction

**NEW: This method is only available in solana-core v1.7 or newer. Please use
[getConfirmedTransaction](jsonrpc-api.md#getconfirmedtransaction) for solana-core v1.6**

Returns transaction details for a confirmed transaction

#### Parameters:

- `<string>` - transaction signature as base-58 encoded string
- `<object>` - (optional) Configuration object containing the following optional fields:
  - (optional) `encoding: <string>` - encoding for each returned Transaction, either "json", "jsonParsed", "base58" (*slow*), "base64". If parameter not provided, the default encoding is "json".
  "jsonParsed" encoding attempts to use program-specific instruction parsers to return more human-readable and explicit data in the `transaction.message.instructions` list. If "jsonParsed" is requested but a parser cannot be found, the instruction falls back to regular JSON encoding (`accounts`, `data`, and `programIdIndex` fields).
  - (optional) [Commitment](jsonrpc-api.md#configuring-state-commitment); "processed" is not supported. If parameter not provided, the default is "finalized".

#### Results:

- `<null>` - if transaction is not found or not confirmed
- `<object>` - if transaction is confirmed, an object with the following fields:
  - `slot: <u64>` - the slot this transaction was processed in
  - `transaction: <object|[string,encoding]>` - [Transaction](#transaction-structure) object, either in JSON format or encoded binary data, depending on encoding parameter
  - `blockTime: <i64 | null>` - estimated production time, as Unix timestamp (seconds since the Unix epoch) of when the transaction was processed. null if not available
  - `meta: <object | null>` - transaction status metadata object:
    - `err: <object | null>` - Error if transaction failed, null if transaction succeeded. [TransactionError definitions](https://github.com/solana-labs/solana/blob/master/sdk/src/transaction.rs#L24)
    - `fee: <u64>` - fee this transaction was charged, as u64 integer
    - `preBalances: <array>` - array of u64 account balances from before the transaction was processed
    - `postBalances: <array>` - array of u64 account balances after the transaction was processed
    - `innerInstructions: <array|undefined>` - List of [inner instructions](#inner-instructions-structure) or omitted if inner instruction recording was not yet enabled during this transaction
    - `preTokenBalances: <array|undefined>` - List of  [token balances](#token-balances-structure) from before the transaction was processed or omitted if token balance recording was not yet enabled during this transaction
    - `postTokenBalances: <array|undefined>` - List of [token balances](#token-balances-structure) from after the transaction was processed or omitted if token balance recording was not yet enabled during this transaction
    - `logMessages: <array>` - array of string log messages or omitted if log message recording was not yet enabled during this transaction
    - DEPRECATED: `status: <object>` - Transaction status
      - `"Ok": <null>` - Transaction was successful
      - `"Err": <ERR>` - Transaction failed with TransactionError
    - `rewards: <array>` - present if rewards are requested; an array of JSON objects containing:
      - `pubkey: <string>` - The public key, as base-58 encoded string, of the account that received the reward
      - `lamports: <i64>`- number of reward lamports credited or debited by the account, as a i64
      - `postBalance: <u64>` - account balance in lamports after the reward was applied
      - `rewardType: <string>` - type of reward: currently only "rent", other types may be added in the future
      - `commission: <u8|undefined>` - vote account commission when the reward was credited, only present for voting and staking rewards


#### Example:
Request:
```bash
curl http://localhost:8899 -X POST -H "Content-Type: application/json" -d '
  {
    "jsonrpc": "2.0",
    "id": 1,
    "method": "getTransaction",
    "params": [
      "2nBhEBYYvfaAe16UMNqRHre4YNSskvuYgx3M6E4JP1oDYvZEJHvoPzyUidNgNX5r9sTyN1J9UxtbCXy2rqYcuyuv",
      "json"
    ]
  }
'
```

Result:
```json
{
  "jsonrpc": "2.0",
  "result": {
    "meta": {
      "err": null,
      "fee": 5000,
      "innerInstructions": [],
      "postBalances": [
        499998932500,
        26858640,
        1,
        1,
        1
      ],
      "postTokenBalances": [],
      "preBalances": [
        499998937500,
        26858640,
        1,
        1,
        1
      ],
      "preTokenBalances": [],
      "status": {
        "Ok": null
      }
    },
    "slot": 430,
    "transaction": {
      "message": {
        "accountKeys": [
          "3UVYmECPPMZSCqWKfENfuoTv51fTDTWicX9xmBD2euKe",
          "AjozzgE83A3x1sHNUR64hfH7zaEBWeMaFuAN9kQgujrc",
          "SysvarS1otHashes111111111111111111111111111",
          "SysvarC1ock11111111111111111111111111111111",
          "Vote111111111111111111111111111111111111111"
        ],
        "header": {
          "numReadonlySignedAccounts": 0,
          "numReadonlyUnsignedAccounts": 3,
          "numRequiredSignatures": 1
        },
        "instructions": [
          {
            "accounts": [
              1,
              2,
              3,
              0
            ],
            "data": "37u9WtQpcm6ULa3WRQHmj49EPs4if7o9f1jSRVZpm2dvihR9C8jY4NqEwXUbLwx15HBSNcP1",
            "programIdIndex": 4
          }
        ],
        "recentBlockhash": "mfcyqEXB3DnHXki6KjjmZck6YjmZLvpAByy2fj4nh6B"
      },
      "signatures": [
        "2nBhEBYYvfaAe16UMNqRHre4YNSskvuYgx3M6E4JP1oDYvZEJHvoPzyUidNgNX5r9sTyN1J9UxtbCXy2rqYcuyuv"
      ]
    }
  },
  "blockTime": null,
  "id": 1
}
```

#### Example:
Request:
```bash
curl http://localhost:8899 -X POST -H "Content-Type: application/json" -d '
  {
    "jsonrpc": "2.0",
    "id": 1,
    "method": "getTransaction",
    "params": [
      "2nBhEBYYvfaAe16UMNqRHre4YNSskvuYgx3M6E4JP1oDYvZEJHvoPzyUidNgNX5r9sTyN1J9UxtbCXy2rqYcuyuv",
      "base64"
    ]
  }
'
```

Result:
```json
{
  "jsonrpc": "2.0",
  "result": {
    "meta": {
      "err": null,
      "fee": 5000,
      "innerInstructions": [],
      "postBalances": [
        499998932500,
        26858640,
        1,
        1,
        1
      ],
      "postTokenBalances": [],
      "preBalances": [
        499998937500,
        26858640,
        1,
        1,
        1
      ],
      "preTokenBalances": [],
      "status": {
        "Ok": null
      }
    },
    "slot": 430,
    "transaction": [
      "AVj7dxHlQ9IrvdYVIjuiRFs1jLaDMHixgrv+qtHBwz51L4/ImLZhszwiyEJDIp7xeBSpm/TX5B7mYzxa+fPOMw0BAAMFJMJVqLw+hJYheizSoYlLm53KzgT82cDVmazarqQKG2GQsLgiqktA+a+FDR4/7xnDX7rsusMwryYVUdixfz1B1Qan1RcZLwqvxvJl4/t3zHragsUp0L47E24tAFUgAAAABqfVFxjHdMkoVmOYaR1etoteuKObS21cc1VbIQAAAAAHYUgdNXR0u3xNdiTr072z2DVec9EQQ/wNo1OAAAAAAAtxOUhPBp2WSjUNJEgfvy70BbxI00fZyEPvFHNfxrtEAQQEAQIDADUCAAAAAQAAAAAAAACtAQAAAAAAAAdUE18R96XTJCe+YfRfUp6WP+YKCy/72ucOL8AoBFSpAA==",
      "base64"
    ]
  },
  "id": 1
}
```

### getTransactionCount

Returns the current Transaction count from the ledger

#### Parameters:

- `<object>` - (optional) [Commitment](jsonrpc-api.md#configuring-state-commitment)

#### Results:

- `<u64>` - count

#### Example:

```bash
curl http://localhost:8899 -X POST -H "Content-Type: application/json" -d '
  {"jsonrpc":"2.0","id":1, "method":"getTransactionCount"}
'

```

Result:
```json
{"jsonrpc":"2.0","result":268,"id":1}
```

### getVersion

Returns the current solana versions running on the node

#### Parameters:

None

#### Results:

The result field will be a JSON object with the following fields:

- `solana-core`, software version of solana-core
- `feature-set`, unique identifier of the current software's feature set

#### Example:

Request:
```bash
curl http://localhost:8899 -X POST -H "Content-Type: application/json" -d '
  {"jsonrpc":"2.0","id":1, "method":"getVersion"}
'
```

Result:
```json
{"jsonrpc":"2.0","result":{"solana-core": "1.8.0"},"id":1}
```

### getVoteAccounts

Returns the account info and associated stake for all the voting accounts in the current bank.

#### Parameters:

- `<object>` - (optional) Configuration object containing the following field:
  - (optional) [Commitment](jsonrpc-api.md#configuring-state-commitment)
  - (optional) `votePubkey: <string>` - Only return results for this validator vote address (base-58 encoded)
  - (optional) `keepUnstakedDelinquents: <bool>` - Do not filter out delinquent validators with no stake
  - (optional) `delinquentSlotDistance: <u64>` - Specify the number of slots behind the tip that a validator must fall to be considered delinquent. **NOTE:** For the sake of consistency between ecosystem products, _it is **not** recommended that this argument be specified._

#### Results:

The result field will be a JSON object of `current` and `delinquent` accounts,
each containing an array of JSON objects with the following sub fields:

- `votePubkey: <string>` - Vote account address, as base-58 encoded string
- `nodePubkey: <string>` - Validator identity, as base-58 encoded string
- `activatedStake: <u64>` - the stake, in lamports, delegated to this vote account and active in this epoch
- `epochVoteAccount: <bool>` - bool, whether the vote account is staked for this epoch
- `commission: <number>`, percentage (0-100) of rewards payout owed to the vote account
- `lastVote: <u64>` - Most recent slot voted on by this vote account
- `epochCredits: <array>` - History of how many credits earned by the end of each epoch, as an array of arrays containing: `[epoch, credits, previousCredits]`

#### Example:
Request:
```bash
curl http://localhost:8899 -X POST -H "Content-Type: application/json" -d '
  {"jsonrpc":"2.0","id":1, "method":"getVoteAccounts"}
'
```

Result:
```json
{
  "jsonrpc": "2.0",
  "result": {
    "current": [
      {
        "commission": 0,
        "epochVoteAccount": true,
        "epochCredits": [
          [ 1, 64, 0 ],
          [ 2, 192, 64 ]
        ],
        "nodePubkey": "B97CCUW3AEZFGy6uUg6zUdnNYvnVq5VG8PUtb2HayTDD",
        "lastVote": 147,
        "activatedStake": 42,
        "votePubkey": "3ZT31jkAGhUaw8jsy4bTknwBMP8i4Eueh52By4zXcsVw"
      }
    ],
    "delinquent": [
      {
        "commission": 127,
        "epochVoteAccount": false,
        "epochCredits": [],
        "nodePubkey": "6ZPxeQaDo4bkZLRsdNrCzchNQr5LN9QMc9sipXv9Kw8f",
        "lastVote": 0,
        "activatedStake": 0,
        "votePubkey": "CmgCk4aMS7KW1SHX3s9K5tBJ6Yng2LBaC8MFov4wx9sm"
      }
    ]
  },
  "id": 1
}
```

#### Example: Restrict results to a single validator vote account

Request:
```bash
curl http://localhost:8899 -X POST -H "Content-Type: application/json" -d '
  {
    "jsonrpc": "2.0",
    "id": 1,
    "method": "getVoteAccounts",
    "params": [
      {
        "votePubkey": "3ZT31jkAGhUaw8jsy4bTknwBMP8i4Eueh52By4zXcsVw"
      }
    ]
  }
'
```

Result:
```json
{
  "jsonrpc": "2.0",
  "result": {
    "current": [
      {
        "commission": 0,
        "epochVoteAccount": true,
        "epochCredits": [
          [ 1, 64, 0 ],
          [ 2, 192, 64 ]
        ],
        "nodePubkey": "B97CCUW3AEZFGy6uUg6zUdnNYvnVq5VG8PUtb2HayTDD",
        "lastVote": 147,
        "activatedStake": 42,
        "votePubkey": "3ZT31jkAGhUaw8jsy4bTknwBMP8i4Eueh52By4zXcsVw"
      }
    ],
    "delinquent": []
  },
  "id": 1
}
```

### minimumLedgerSlot

Returns the lowest slot that the node has information about in its ledger. This
value may increase over time if the node is configured to purge older ledger data

#### Parameters:

None

#### Results:

- `u64` - Minimum ledger slot

#### Example:

```bash
curl http://localhost:8899 -X POST -H "Content-Type: application/json" -d '
  {"jsonrpc":"2.0","id":1, "method":"minimumLedgerSlot"}
'

```

Result:
```json
{"jsonrpc":"2.0","result":1234,"id":1}
```

### requestAirdrop

Requests an airdrop of lamports to a Pubkey

#### Parameters:

- `<string>` - Pubkey of account to receive lamports, as base-58 encoded string
- `<integer>` - lamports, as a u64
- `<object>` - (optional) [Commitment](jsonrpc-api.md#configuring-state-commitment) (used for retrieving blockhash and verifying airdrop success)

#### Results:

- `<string>` - Transaction Signature of airdrop, as base-58 encoded string

#### Example:

```bash
curl http://localhost:8899 -X POST -H "Content-Type: application/json" -d '
  {"jsonrpc":"2.0","id":1, "method":"requestAirdrop", "params":["83astBRguLMdt2h5U1Tpdq5tjFoJ6noeGwaY3mDLVcri", 1000000000]}
'

```

Result:
```json
{"jsonrpc":"2.0","result":"5VERv8NMvzbJMEkV8xnrLkEaWRtSz9CosKDYjCJjBRnbJLgp8uirBgmQpjKhoR4tjF3ZpRzrFmBV6UjKdiSZkQUW","id":1}
```

### sendTransaction

Submits a signed transaction to the cluster for processing.

This method does not alter the transaction in any way; it relays the
transaction created by clients to the node as-is.

If the node's rpc service receives the transaction, this method immediately
succeeds, without waiting for any confirmations. A successful response from
this method does not guarantee the transaction is processed or confirmed by the
cluster.

While the rpc service will reasonably retry to submit it, the transaction
could be rejected if transaction's `recent_blockhash` expires before it lands.

Use [`getSignatureStatuses`](jsonrpc-api.md#getsignaturestatuses) to ensure
a transaction is processed and confirmed.

Before submitting, the following preflight checks are performed:

1. The transaction signatures are verified
2. The transaction is simulated against the bank slot specified by the preflight
   commitment. On failure an error will be returned. Preflight checks may be
   disabled if desired. It is recommended to specify the same commitment and
   preflight commitment to avoid confusing behavior.

The returned signature is the first signature in the transaction, which
is used to identify the transaction ([transaction id](../../terminology.md#transanction-id)).
This identifier can be easily extracted from the transaction data before
submission.

#### Parameters:

- `<string>` - fully-signed Transaction, as encoded string
- `<object>` - (optional) Configuration object containing the following field:
  - `skipPreflight: <bool>` - if true, skip the preflight transaction checks (default: false)
  - `preflightCommitment: <string>` - (optional) [Commitment](jsonrpc-api.md#configuring-state-commitment) level to use for preflight (default: `"finalized"`).
  - `encoding: <string>` - (optional) Encoding used for the transaction data. Either `"base58"` (*slow*, **DEPRECATED**), or `"base64"`. (default: `"base58"`).

#### Results:

- `<string>` - First Transaction Signature embedded in the transaction, as base-58 encoded string ([transaction id](../../terminology.md#transanction-id))

#### Example:

```bash
curl http://localhost:8899 -X POST -H "Content-Type: application/json" -d '
  {
    "jsonrpc": "2.0",
    "id": 1,
    "method": "sendTransaction",
    "params": [
      "4hXTCkRzt9WyecNzV1XPgCDfGAZzQKNxLXgynz5QDuWWPSAZBZSHptvWRL3BjCvzUXRdKvHL2b7yGrRQcWyaqsaBCncVG7BFggS8w9snUts67BSh3EqKpXLUm5UMHfD7ZBe9GhARjbNQMLJ1QD3Spr6oMTBU6EhdB4RD8CP2xUxr2u3d6fos36PD98XS6oX8TQjLpsMwncs5DAMiD4nNnR8NBfyghGCWvCVifVwvA8B8TJxE1aiyiv2L429BCWfyzAme5sZW8rDb14NeCQHhZbtNqfXhcp2tAnaAT"
    ]
  }
'

```

Result:
```json
{"jsonrpc":"2.0","result":"2id3YC2jK9G5Wo2phDx4gJVAew8DcY5NAojnVuao8rkxwPYPe8cSwE5GzhEgJA2y8fVjDEo6iR6ykBvDxrTQrtpb","id":1}
```

### simulateTransaction

Simulate sending a transaction

#### Parameters:

- `<string>` - Transaction, as an encoded string. The transaction must have a valid blockhash, but is not required to be signed.
- `<object>` - (optional) Configuration object containing the following fields:
  - `sigVerify: <bool>` - if true the transaction signatures will be verified (default: false, conflicts with `replaceRecentBlockhash`)
  - `commitment: <string>` - (optional) [Commitment](jsonrpc-api.md#configuring-state-commitment) level to simulate the transaction at (default: `"finalized"`).
  - `encoding: <string>` - (optional) Encoding used for the transaction data. Either `"base58"` (*slow*, **DEPRECATED**), or `"base64"`. (default: `"base58"`).
  - `replaceRecentBlockhash: <bool>` - (optional) if true the transaction recent blockhash will be replaced with the most recent blockhash.
  (default: false, conflicts with `sigVerify`)
  - `accounts: <object>` - (optional) Accounts configuration object containing the following fields:
     - `encoding: <string>` - (optional) encoding for returned Account data, either  "base64" (default), "base64+zstd" or "jsonParsed".
        "jsonParsed" encoding attempts to use program-specific state parsers to return more human-readable and explicit account state data. If "jsonParsed" is requested but a parser cannot be found, the field falls back to binary encoding, detectable when the `data` field is type `<string>`.
     - `addresses: <array>` - An array of accounts to return, as base-58 encoded strings

#### Results:

An RpcResponse containing a TransactionStatus object
The result will be an RpcResponse JSON object with `value` set to a JSON object with the following fields:

- `err: <object | string | null>` - Error if transaction failed, null if transaction succeeded. [TransactionError definitions](https://github.com/solana-labs/solana/blob/master/sdk/src/transaction.rs#L24)
- `logs: <array | null>` - Array of log messages the transaction instructions output during execution, null if simulation failed before the transaction was able to execute (for example due to an invalid blockhash or signature verification failure)
- `accounts: <array> | null>` - array of accounts with the same length as the `accounts.addresses` array in the request
  - `<null>` - if the account doesn't exist or if `err` is not null
  - `<object>` - otherwise, a JSON object containing:
    - `lamports: <u64>`, number of lamports assigned to this account, as a u64
    - `owner: <string>`, base-58 encoded Pubkey of the program this account has been assigned to
    - `data: <[string, encoding]|object>`, data associated with the account, either as encoded binary data or JSON format `{<program>: <state>}`, depending on encoding parameter
    - `executable: <bool>`, boolean indicating if the account contains a program \(and is strictly read-only\)
    - `rentEpoch: <u64>`, the epoch at which this account will next owe rent, as u64
- `unitsConsumed: <u64 | undefined>`, The number of compute budget units consumed during the processing of this transaction

#### Example:

```bash
curl http://localhost:8899 -X POST -H "Content-Type: application/json" -d '
  {
    "jsonrpc": "2.0",
    "id": 1,
    "method": "simulateTransaction",
    "params": [
      "4hXTCkRzt9WyecNzV1XPgCDfGAZzQKNxLXgynz5QDuWWPSAZBZSHptvWRL3BjCvzUXRdKvHL2b7yGrRQcWyaqsaBCncVG7BFggS8w9snUts67BSh3EqKpXLUm5UMHfD7ZBe9GhARjbNQMLJ1QD3Spr6oMTBU6EhdB4RD8CP2xUxr2u3d6fos36PD98XS6oX8TQjLpsMwncs5DAMiD4nNnR8NBfyghGCWvCVifVwvA8B8TJxE1aiyiv2L429BCWfyzAme5sZW8rDb14NeCQHhZbtNqfXhcp2tAnaAT"
    ]
  }
'
```

Result:
```json
{
  "jsonrpc": "2.0",
  "result": {
    "context": {
      "slot": 218
    },
    "value": {
      "err": null,
      "accounts": null,
      "logs": [
        "BPF program 83astBRguLMdt2h5U1Tpdq5tjFoJ6noeGwaY3mDLVcri success"
      ]
    }
  },
  "id": 1
}
```

## Subscription Websocket

After connecting to the RPC PubSub websocket at `ws://<ADDRESS>/`:

- Submit subscription requests to the websocket using the methods below
- Multiple subscriptions may be active at once
- Many subscriptions take the optional [`commitment` parameter](jsonrpc-api.md#configuring-state-commitment), defining how finalized a change should be to trigger a notification. For subscriptions, if commitment is unspecified, the default value is `"finalized"`.

### accountSubscribe

Subscribe to an account to receive notifications when the lamports or data for a given account public key changes

#### Parameters:

- `<string>` - account Pubkey, as base-58 encoded string
- `<object>` - (optional) Configuration object containing the following optional fields:
  - `<object>` - (optional) [Commitment](jsonrpc-api.md#configuring-state-commitment)
  - `encoding: <string>` - encoding for Account data, either "base58" (*slow*), "base64", "base64+zstd" or "jsonParsed".
    "jsonParsed" encoding attempts to use program-specific state parsers to return more human-readable and explicit account state data. If "jsonParsed" is requested but a parser cannot be found, the field falls back to binary encoding, detectable when the `data` field is type `<string>`.

#### Results:

- `<number>` - Subscription id \(needed to unsubscribe\)

#### Example:

Request:
```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "method": "accountSubscribe",
  "params": [
    "CM78CPUeXjn8o3yroDHxUtKsZZgoy4GPkPPXfouKNH12",
    {
      "encoding": "base64",
      "commitment": "finalized"
    }
  ]
}
{
  "jsonrpc": "2.0",
  "id": 1,
  "method": "accountSubscribe",
  "params": [
    "CM78CPUeXjn8o3yroDHxUtKsZZgoy4GPkPPXfouKNH12",
    {
      "encoding": "jsonParsed"
    }
  ]
}
```

Result:
```json
{"jsonrpc": "2.0","result": 23784,"id": 1}
```

#### Notification Format:

The notification format is the same as seen in the [getAccountInfo](jsonrpc-api.md#getAccountInfo) RPC HTTP method.

Base58 encoding:
```json
{
  "jsonrpc": "2.0",
  "method": "accountNotification",
  "params": {
    "result": {
      "context": {
        "slot": 5199307
      },
      "value": {
        "data": ["11116bv5nS2h3y12kD1yUKeMZvGcKLSjQgX6BeV7u1FrjeJcKfsHPXHRDEHrBesJhZyqnnq9qJeUuF7WHxiuLuL5twc38w2TXNLxnDbjmuR", "base58"],
        "executable": false,
        "lamports": 33594,
        "owner": "11111111111111111111111111111111",
        "rentEpoch": 635
      }
    },
    "subscription": 23784
  }
}
```

Parsed-JSON encoding:
```json
{
  "jsonrpc": "2.0",
  "method": "accountNotification",
  "params": {
    "result": {
      "context": {
        "slot": 5199307
      },
      "value": {
        "data": {
           "program": "nonce",
           "parsed": {
              "type": "initialized",
              "info": {
                 "authority": "Bbqg1M4YVVfbhEzwA9SpC9FhsaG83YMTYoR4a8oTDLX",
                 "blockhash": "LUaQTmM7WbMRiATdMMHaRGakPtCkc2GHtH57STKXs6k",
                 "feeCalculator": {
                    "lamportsPerSignature": 5000
                 }
              }
           }
        },
        "executable": false,
        "lamports": 33594,
        "owner": "11111111111111111111111111111111",
        "rentEpoch": 635
      }
    },
    "subscription": 23784
  }
}
```

### accountUnsubscribe

Unsubscribe from account change notifications

#### Parameters:

- `<number>` - id of account Subscription to cancel

#### Results:

- `<bool>` - unsubscribe success message

#### Example:

Request:
```json
{"jsonrpc":"2.0", "id":1, "method":"accountUnsubscribe", "params":[0]}

```

Result:
```json
{"jsonrpc": "2.0","result": true,"id": 1}
```

### logsSubscribe

Subscribe to transaction logging

#### Parameters:

- `filter: <string>|<object>` - filter criteria for the logs to receive results by account type; currently supported:
  - "all" - subscribe to all transactions except for simple vote transactions
  - "allWithVotes" - subscribe to all transactions including simple vote transactions
  - `{ "mentions": [ <string> ] }` - subscribe to all transactions that mention the provided Pubkey (as base-58 encoded string)
- `<object>` - (optional) Configuration object containing the following optional fields:
  - (optional) [Commitment](jsonrpc-api.md#configuring-state-commitment)

#### Results:

- `<integer>` - Subscription id \(needed to unsubscribe\)

#### Example:

Request:
```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "method": "logsSubscribe",
  "params": [
    {
      "mentions": [ "11111111111111111111111111111111" ]
    },
    {
      "commitment": "finalized"
    }
  ]
}
{
  "jsonrpc": "2.0",
  "id": 1,
  "method": "logsSubscribe",
  "params": [ "all" ]
}
```

Result:
```json
{"jsonrpc": "2.0","result": 24040,"id": 1}
```

#### Notification Format:

The notification will be an RpcResponse JSON object with value equal to:

- `signature: <string>` - The transaction signature base58 encoded.
- `err: <object | null>` - Error if transaction failed, null if transaction succeeded. [TransactionError definitions](https://github.com/solana-labs/solana/blob/master/sdk/src/transaction.rs#L24)
- `logs: <array | null>` - Array of log messages the transaction instructions output during execution, null if simulation failed before the transaction was able to execute (for example due to an invalid blockhash or signature verification failure)

Example:

```json
{
  "jsonrpc": "2.0",
  "method": "logsNotification",
  "params": {
    "result": {
      "context": {
        "slot": 5208469
      },
      "value": {
        "signature": "5h6xBEauJ3PK6SWCZ1PGjBvj8vDdWG3KpwATGy1ARAXFSDwt8GFXM7W5Ncn16wmqokgpiKRLuS83KUxyZyv2sUYv",
        "err": null,
        "logs": [
          "BPF program 83astBRguLMdt2h5U1Tpdq5tjFoJ6noeGwaY3mDLVcri success"
        ]
      }
    },
    "subscription": 24040
  }
}
```

### logsUnsubscribe

Unsubscribe from transaction logging

#### Parameters:

- `<integer>` - id of subscription to cancel

#### Results:

- `<bool>` - unsubscribe success message

#### Example:

Request:
```json
{"jsonrpc":"2.0", "id":1, "method":"logsUnsubscribe", "params":[0]}
```

Result:
```json
{"jsonrpc": "2.0","result": true,"id": 1}
```

### programSubscribe

Subscribe to a program to receive notifications when the lamports or data for a given account owned by the program changes

#### Parameters:

- `<string>` - program_id Pubkey, as base-58 encoded string
- `<object>` - (optional) Configuration object containing the following optional fields:
  - (optional) [Commitment](jsonrpc-api.md#configuring-state-commitment)
  - `encoding: <string>` - encoding for Account data, either "base58" (*slow*), "base64", "base64+zstd" or "jsonParsed".
    "jsonParsed" encoding attempts to use program-specific state parsers to return more human-readable and explicit account state data. If "jsonParsed" is requested but a parser cannot be found, the field falls back to base64 encoding, detectable when the `data` field is type `<string>`.
  - (optional) `filters: <array>` - filter results using various [filter objects](jsonrpc-api.md#filters); account must meet all filter criteria to be included in results

#### Results:

- `<integer>` - Subscription id \(needed to unsubscribe\)

#### Example:

Request:
```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "method": "programSubscribe",
  "params": [
    "11111111111111111111111111111111",
    {
      "encoding": "base64",
      "commitment": "finalized"
    }
  ]
}
{
  "jsonrpc": "2.0",
  "id": 1,
  "method": "programSubscribe",
  "params": [
    "11111111111111111111111111111111",
    {
      "encoding": "jsonParsed"
    }
  ]
}
{
  "jsonrpc": "2.0",
  "id": 1,
  "method": "programSubscribe",
  "params": [
    "11111111111111111111111111111111",
    {
      "encoding": "base64",
      "filters": [
        {
          "dataSize": 80
        }
      ]
    }
  ]
}
```

Result:
```json
{"jsonrpc": "2.0","result": 24040,"id": 1}
```

#### Notification Format:

The notification format is a <b>single</b> program account object as seen in the [getProgramAccounts](jsonrpc-api.md#getProgramAccounts) RPC HTTP method.

Base58 encoding:
```json
{
  "jsonrpc": "2.0",
  "method": "programNotification",
  "params": {
    "result": {
      "context": {
        "slot": 5208469
      },
      "value": {
        "pubkey": "H4vnBqifaSACnKa7acsxstsY1iV1bvJNxsCY7enrd1hq",
        "account": {
          "data": ["11116bv5nS2h3y12kD1yUKeMZvGcKLSjQgX6BeV7u1FrjeJcKfsHPXHRDEHrBesJhZyqnnq9qJeUuF7WHxiuLuL5twc38w2TXNLxnDbjmuR", "base58"],
          "executable": false,
          "lamports": 33594,
          "owner": "11111111111111111111111111111111",
          "rentEpoch": 636
        },
      }
    },
    "subscription": 24040
  }
}
```

Parsed-JSON encoding:
```json
{
  "jsonrpc": "2.0",
  "method": "programNotification",
  "params": {
    "result": {
      "context": {
        "slot": 5208469
      },
      "value": {
        "pubkey": "H4vnBqifaSACnKa7acsxstsY1iV1bvJNxsCY7enrd1hq",
        "account": {
          "data": {
             "program": "nonce",
             "parsed": {
                "type": "initialized",
                "info": {
                   "authority": "Bbqg1M4YVVfbhEzwA9SpC9FhsaG83YMTYoR4a8oTDLX",
                   "blockhash": "LUaQTmM7WbMRiATdMMHaRGakPtCkc2GHtH57STKXs6k",
                   "feeCalculator": {
                      "lamportsPerSignature": 5000
                   }
                }
             }
          },
          "executable": false,
          "lamports": 33594,
          "owner": "11111111111111111111111111111111",
          "rentEpoch": 636
        },
      }
    },
    "subscription": 24040
  }
}
```

### programUnsubscribe

Unsubscribe from program-owned account change notifications

#### Parameters:

- `<integer>` - id of account Subscription to cancel

#### Results:

- `<bool>` - unsubscribe success message

#### Example:

Request:
```json
{"jsonrpc":"2.0", "id":1, "method":"programUnsubscribe", "params":[0]}

```

Result:
```json
{"jsonrpc": "2.0","result": true,"id": 1}
```

### signatureSubscribe

Subscribe to a transaction signature to receive notification when the transaction is confirmed On `signatureNotification`, the subscription is automatically cancelled

#### Parameters:

- `<string>` - Transaction Signature, as base-58 encoded string
- `<object>` - (optional) [Commitment](jsonrpc-api.md#configuring-state-commitment)

#### Results:

- `integer` - subscription id \(needed to unsubscribe\)

#### Example:

Request:
```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "method": "signatureSubscribe",
  "params": [
    "2EBVM6cB8vAAD93Ktr6Vd8p67XPbQzCJX47MpReuiCXJAtcjaxpvWpcg9Ege1Nr5Tk3a2GFrByT7WPBjdsTycY9b"
  ]
}

{
  "jsonrpc": "2.0",
  "id": 1,
  "method": "signatureSubscribe",
  "params": [
    "2EBVM6cB8vAAD93Ktr6Vd8p67XPbQzCJX47MpReuiCXJAtcjaxpvWpcg9Ege1Nr5Tk3a2GFrByT7WPBjdsTycY9b",
    {
      "commitment": "finalized"
    }
  ]
}
```

Result:
```json
{"jsonrpc": "2.0","result": 0,"id": 1}
```

#### Notification Format:

The notification will be an RpcResponse JSON object with value containing an object with:
- `err: <object | null>` - Error if transaction failed, null if transaction succeeded. [TransactionError definitions](https://github.com/solana-labs/solana/blob/master/sdk/src/transaction.rs#L24)

Example:
```json
{
  "jsonrpc": "2.0",
  "method": "signatureNotification",
  "params": {
    "result": {
      "context": {
        "slot": 5207624
      },
      "value": {
        "err": null
      }
    },
    "subscription": 24006
  }
}
```

### signatureUnsubscribe

Unsubscribe from signature confirmation notification

#### Parameters:

- `<integer>` - subscription id to cancel

#### Results:

- `<bool>` - unsubscribe success message

#### Example:

Request:
```json
{"jsonrpc":"2.0", "id":1, "method":"signatureUnsubscribe", "params":[0]}

```

Result:
```json
{"jsonrpc": "2.0","result": true,"id": 1}
```

### slotSubscribe

Subscribe to receive notification anytime a slot is processed by the validator

#### Parameters:

None

#### Results:

- `integer` - subscription id \(needed to unsubscribe\)

#### Example:

Request:
```json
{"jsonrpc":"2.0", "id":1, "method":"slotSubscribe"}

```

Result:
```json
{"jsonrpc": "2.0","result": 0,"id": 1}
```

#### Notification Format:

The notification will be an object with the following fields:

- `parent: <u64>` - The parent slot
- `root: <u64>` - The current root slot
- `slot: <u64>` - The newly set slot value

Example:
```json
{
  "jsonrpc": "2.0",
  "method": "slotNotification",
  "params": {
    "result": {
      "parent": 75,
      "root": 44,
      "slot": 76
    },
    "subscription": 0
  }
}
```

### slotUnsubscribe

Unsubscribe from slot notifications

#### Parameters:

- `<integer>` - subscription id to cancel

#### Results:

- `<bool>` - unsubscribe success message

#### Example:

Request:
```json
{"jsonrpc":"2.0", "id":1, "method":"slotUnsubscribe", "params":[0]}

```

Result:
```json
{"jsonrpc": "2.0","result": true,"id": 1}
```

### slotsUpdatesSubscribe - Unstable

**This subscription is unstable; the format of this subscription may change in
the future and it may not always be supported**

Subscribe to receive a notification from the validator on a variety of updates
on every slot

#### Parameters:

None

#### Results:

- `integer` - subscription id \(needed to unsubscribe\)

#### Example:

Request:
```json
{"jsonrpc":"2.0", "id":1, "method":"slotsUpdatesSubscribe"}

```

Result:
```json
{"jsonrpc": "2.0","result": 0,"id": 1}
```

#### Notification Format:

The notification will be an object with the following fields:

- `parent: <u64>` - The parent slot
- `slot: <u64>` - The newly updated slot
- `timestamp: <i64>` - The Unix timestamp of the update
- `type: <string>` - The update type, one of:
  - "firstShredReceived"
  - "completed"
  - "createdBank"
  - "frozen"
  - "dead"
  - "optimisticConfirmation"
  - "root"

```bash
{
  "jsonrpc": "2.0",
  "method": "slotsUpdatesNotification",
  "params": {
    "result": {
      "parent": 75,
      "slot": 76,
      "timestamp": 1625081266243,
      "type": "optimisticConfirmation"
    },
    "subscription": 0
  }
}
```

### slotsUpdatesUnsubscribe

Unsubscribe from slot-update notifications

#### Parameters:

- `<integer>` - subscription id to cancel

#### Results:

- `<bool>` - unsubscribe success message

#### Example:

Request:
```json
{"jsonrpc":"2.0", "id":1, "method":"slotsUpdatesUnsubscribe", "params":[0]}

```

Result:
```json
{"jsonrpc": "2.0","result": true,"id": 1}
```

### rootSubscribe

Subscribe to receive notification anytime a new root is set by the validator.

#### Parameters:

None

#### Results:

- `integer` - subscription id \(needed to unsubscribe\)

#### Example:

Request:
```json
{"jsonrpc":"2.0", "id":1, "method":"rootSubscribe"}

```

Result:
```json
{"jsonrpc": "2.0","result": 0,"id": 1}
```

#### Notification Format:

The result is the latest root slot number.

```json
{
  "jsonrpc": "2.0",
  "method": "rootNotification",
  "params": {
    "result": 42,
    "subscription": 0
  }
}
```

### rootUnsubscribe

Unsubscribe from root notifications

#### Parameters:

- `<integer>` - subscription id to cancel

#### Results:

- `<bool>` - unsubscribe success message

#### Example:

Request:
```json
{"jsonrpc":"2.0", "id":1, "method":"rootUnsubscribe", "params":[0]}

```

Result:
```json
{"jsonrpc": "2.0","result": true,"id": 1}
```

### voteSubscribe - Unstable, disabled by default

**This subscription is unstable and only available if the validator was started
with the `--rpc-pubsub-enable-vote-subscription` flag.  The format of this
subscription may change in the future**

Subscribe to receive notification anytime a new vote is observed in gossip.
These votes are pre-consensus therefore there is no guarantee these votes will
enter the ledger.

#### Parameters:

None

#### Results:

- `integer` - subscription id \(needed to unsubscribe\)

#### Example:

Request:
```json
{"jsonrpc":"2.0", "id":1, "method":"voteSubscribe"}

```

Result:
```json
{"jsonrpc": "2.0","result": 0,"id": 1}
```

#### Notification Format:

The notification will be an object with the following fields:
- `hash: <string>` - The vote hash
- `slots: <array>` - The slots covered by the vote, as an array of u64 integers
- `timestamp: <i64 | null>` - The timestamp of the vote

```json
{
  "jsonrpc": "2.0",
  "method": "voteNotification",
  "params": {
    "result": {
      "hash": "8Rshv2oMkPu5E4opXTRyuyBeZBqQ4S477VG26wUTFxUM",
      "slots": [1, 2],
      "timestamp": null
    },
    "subscription": 0
  }
}
```

### voteUnsubscribe

Unsubscribe from vote notifications

#### Parameters:

- `<integer>` - subscription id to cancel

#### Results:

- `<bool>` - unsubscribe success message

#### Example:

Request:
```json
{"jsonrpc":"2.0", "id":1, "method":"voteUnsubscribe", "params":[0]}
```

Response:
```json
{"jsonrpc": "2.0","result": true,"id": 1}
```

## JSON RPC API Deprecated Methods

### getConfirmedBlock

**DEPRECATED: Please use [getBlock](jsonrpc-api.md#getblock) instead**
This method is expected to be removed in solana-core v1.8

Returns identity and transaction information about a confirmed block in the ledger

#### Parameters:

- `<u64>` - slot, as u64 integer
- `<object>` - (optional) Configuration object containing the following optional fields:
  - (optional) `encoding: <string>` - encoding for each returned Transaction, either "json", "jsonParsed", "base58" (*slow*), "base64". If parameter not provided, the default encoding is "json".
  "jsonParsed" encoding attempts to use program-specific instruction parsers to return more human-readable and explicit data in the `transaction.message.instructions` list. If "jsonParsed" is requested but a parser cannot be found, the instruction falls back to regular JSON encoding (`accounts`, `data`, and `programIdIndex` fields).
  - (optional) `transactionDetails: <string>` - level of transaction detail to return, either "full", "signatures", or "none". If parameter not provided, the default detail level is "full".
  - (optional) `rewards: bool` - whether to populate the `rewards` array. If parameter not provided, the default includes rewards.
  - (optional) [Commitment](jsonrpc-api.md#configuring-state-commitment); "processed" is not supported. If parameter not provided, the default is "finalized".

#### Results:

The result field will be an object with the following fields:

- `<null>` - if specified block is not confirmed
- `<object>` - if block is confirmed, an object with the following fields:
  - `blockhash: <string>` - the blockhash of this block, as base-58 encoded string
  - `previousBlockhash: <string>` - the blockhash of this block's parent, as base-58 encoded string; if the parent block is not available due to ledger cleanup, this field will return "11111111111111111111111111111111"
  - `parentSlot: <u64>` - the slot index of this block's parent
  - `transactions: <array>` - present if "full" transaction details are requested; an array of JSON objects containing:
    - `transaction: <object|[string,encoding]>` - [Transaction](#transaction-structure) object, either in JSON format or encoded binary data, depending on encoding parameter
    - `meta: <object>` - transaction status metadata object, containing `null` or:
      - `err: <object | null>` - Error if transaction failed, null if transaction succeeded. [TransactionError definitions](https://github.com/solana-labs/solana/blob/master/sdk/src/transaction.rs#L24)
      - `fee: <u64>` - fee this transaction was charged, as u64 integer
      - `preBalances: <array>` - array of u64 account balances from before the transaction was processed
      - `postBalances: <array>` - array of u64 account balances after the transaction was processed
      - `innerInstructions: <array|undefined>` - List of [inner instructions](#inner-instructions-structure) or omitted if inner instruction recording was not yet enabled during this transaction
      - `preTokenBalances: <array|undefined>` - List of [token balances](#token-balances-structure) from before the transaction was processed or omitted if token balance recording was not yet enabled during this transaction
      - `postTokenBalances: <array|undefined>` - List of [token balances](#token-balances-structure) from after the transaction was processed or omitted if token balance recording was not yet enabled during this transaction
      - `logMessages: <array>` - array of string log messages or omitted if log message recording was not yet enabled during this transaction
      - DEPRECATED: `status: <object>` - Transaction status
        - `"Ok": <null>` - Transaction was successful
        - `"Err": <ERR>` - Transaction failed with TransactionError
  - `signatures: <array>` - present if "signatures" are requested for transaction details; an array of signatures strings, corresponding to the transaction order in the block
  - `rewards: <array>` - present if rewards are requested; an array of JSON objects containing:
    - `pubkey: <string>` - The public key, as base-58 encoded string, of the account that received the reward
    - `lamports: <i64>`- number of reward lamports credited or debited by the account, as a i64
    - `postBalance: <u64>` - account balance in lamports after the reward was applied
    - `rewardType: <string|undefined>` - type of reward: "fee", "rent", "voting", "staking"
    - `commission: <u8|undefined>` - vote account commission when the reward was credited, only present for voting and staking rewards
  - `blockTime: <i64 | null>` - estimated production time, as Unix timestamp (seconds since the Unix epoch). null if not available

#### Example:

Request:
```bash
curl http://localhost:8899 -X POST -H "Content-Type: application/json" -d '
  {"jsonrpc": "2.0","id":1,"method":"getConfirmedBlock","params":[430, {"encoding": "json","transactionDetails":"full","rewards":false}]}
'
```

Result:
```json
{
  "jsonrpc": "2.0",
  "result": {
    "blockTime": null,
    "blockhash": "3Eq21vXNB5s86c62bVuUfTeaMif1N2kUqRPBmGRJhyTA",
    "parentSlot": 429,
    "previousBlockhash": "mfcyqEXB3DnHXki6KjjmZck6YjmZLvpAByy2fj4nh6B",
    "transactions": [
      {
        "meta": {
          "err": null,
          "fee": 5000,
          "innerInstructions": [],
          "logMessages": [],
          "postBalances": [
            499998932500,
            26858640,
            1,
            1,
            1
          ],
          "postTokenBalances": [],
          "preBalances": [
            499998937500,
            26858640,
            1,
            1,
            1
          ],
          "preTokenBalances": [],
          "status": {
            "Ok": null
          }
        },
        "transaction": {
          "message": {
            "accountKeys": [
              "3UVYmECPPMZSCqWKfENfuoTv51fTDTWicX9xmBD2euKe",
              "AjozzgE83A3x1sHNUR64hfH7zaEBWeMaFuAN9kQgujrc",
              "SysvarS1otHashes111111111111111111111111111",
              "SysvarC1ock11111111111111111111111111111111",
              "Vote111111111111111111111111111111111111111"
            ],
            "header": {
              "numReadonlySignedAccounts": 0,
              "numReadonlyUnsignedAccounts": 3,
              "numRequiredSignatures": 1
            },
            "instructions": [
              {
                "accounts": [
                  1,
                  2,
                  3,
                  0
                ],
                "data": "37u9WtQpcm6ULa3WRQHmj49EPs4if7o9f1jSRVZpm2dvihR9C8jY4NqEwXUbLwx15HBSNcP1",
                "programIdIndex": 4
              }
            ],
            "recentBlockhash": "mfcyqEXB3DnHXki6KjjmZck6YjmZLvpAByy2fj4nh6B"
          },
          "signatures": [
            "2nBhEBYYvfaAe16UMNqRHre4YNSskvuYgx3M6E4JP1oDYvZEJHvoPzyUidNgNX5r9sTyN1J9UxtbCXy2rqYcuyuv"
          ]
        }
      }
    ]
  },
  "id": 1
}
```

#### Example:
Request:
```bash
curl http://localhost:8899 -X POST -H "Content-Type: application/json" -d '
  {"jsonrpc": "2.0","id":1,"method":"getConfirmedBlock","params":[430, "base64"]}
'
```

Result:
```json
{
  "jsonrpc": "2.0",
  "result": {
    "blockTime": null,
    "blockhash": "3Eq21vXNB5s86c62bVuUfTeaMif1N2kUqRPBmGRJhyTA",
    "parentSlot": 429,
    "previousBlockhash": "mfcyqEXB3DnHXki6KjjmZck6YjmZLvpAByy2fj4nh6B",
    "rewards": [],
    "transactions": [
      {
        "meta": {
          "err": null,
          "fee": 5000,
          "innerInstructions": [],
          "logMessages": [],
          "postBalances": [
            499998932500,
            26858640,
            1,
            1,
            1
          ],
          "postTokenBalances": [],
          "preBalances": [
            499998937500,
            26858640,
            1,
            1,
            1
          ],
          "preTokenBalances": [],
          "status": {
            "Ok": null
          }
        },
        "transaction": [
          "AVj7dxHlQ9IrvdYVIjuiRFs1jLaDMHixgrv+qtHBwz51L4/ImLZhszwiyEJDIp7xeBSpm/TX5B7mYzxa+fPOMw0BAAMFJMJVqLw+hJYheizSoYlLm53KzgT82cDVmazarqQKG2GQsLgiqktA+a+FDR4/7xnDX7rsusMwryYVUdixfz1B1Qan1RcZLwqvxvJl4/t3zHragsUp0L47E24tAFUgAAAABqfVFxjHdMkoVmOYaR1etoteuKObS21cc1VbIQAAAAAHYUgdNXR0u3xNdiTr072z2DVec9EQQ/wNo1OAAAAAAAtxOUhPBp2WSjUNJEgfvy70BbxI00fZyEPvFHNfxrtEAQQEAQIDADUCAAAAAQAAAAAAAACtAQAAAAAAAAdUE18R96XTJCe+YfRfUp6WP+YKCy/72ucOL8AoBFSpAA==",
          "base64"
        ]
      }
    ]
  },
  "id": 1
}
```

For more details on returned data:
[Transaction Structure](jsonrpc-api.md#transaction-structure)
[Inner Instructions Structure](jsonrpc-api.md#inner-instructions-structure)
[Token Balances Structure](jsonrpc-api.md#token-balances-structure)

### getConfirmedBlocks

**DEPRECATED: Please use [getBlocks](jsonrpc-api.md#getblocks) instead**
This method is expected to be removed in solana-core v1.8

Returns a list of confirmed blocks between two slots

#### Parameters:

- `<u64>` - start_slot, as u64 integer
- `<u64>` - (optional) end_slot, as u64 integer
- (optional) [Commitment](jsonrpc-api.md#configuring-state-commitment); "processed" is not supported. If parameter not provided, the default is "finalized".

#### Results:

The result field will be an array of u64 integers listing confirmed blocks
between `start_slot` and either `end_slot`, if provided, or latest confirmed block,
inclusive.  Max range allowed is 500,000 slots.


#### Example:

Request:
```bash
curl http://localhost:8899 -X POST -H "Content-Type: application/json" -d '
  {"jsonrpc": "2.0","id":1,"method":"getConfirmedBlocks","params":[5, 10]}
'
```

Result:
```json
{"jsonrpc":"2.0","result":[5,6,7,8,9,10],"id":1}
```

### getConfirmedBlocksWithLimit

**DEPRECATED: Please use [getBlocksWithLimit](jsonrpc-api.md#getblockswithlimit) instead**
This method is expected to be removed in solana-core v1.8

Returns a list of confirmed blocks starting at the given slot

#### Parameters:

- `<u64>` - start_slot, as u64 integer
- `<u64>` - limit, as u64 integer
- (optional) [Commitment](jsonrpc-api.md#configuring-state-commitment); "processed" is not supported. If parameter not provided, the default is "finalized".

#### Results:

The result field will be an array of u64 integers listing confirmed blocks
starting at `start_slot` for up to `limit` blocks, inclusive.

#### Example:

Request:
```bash
curl http://localhost:8899 -X POST -H "Content-Type: application/json" -d '
  {"jsonrpc": "2.0","id":1,"method":"getConfirmedBlocksWithLimit","params":[5, 3]}
'
```

Result:
```json
{"jsonrpc":"2.0","result":[5,6,7],"id":1}
```

### getConfirmedSignaturesForAddress2

**DEPRECATED: Please use [getSignaturesForAddress](jsonrpc-api.md#getsignaturesforaddress) instead**
This method is expected to be removed in solana-core v1.8

Returns confirmed signatures for transactions involving an
address backwards in time from the provided signature or most recent confirmed block

#### Parameters:
* `<string>` - account address as base-58 encoded string
* `<object>` - (optional) Configuration object containing the following fields:
  * `limit: <number>` - (optional) maximum transaction signatures to return (between 1 and 1,000, default: 1,000).
  * `before: <string>` - (optional) start searching backwards from this transaction signature.
                         If not provided the search starts from the top of the highest max confirmed block.
  * `until: <string>` - (optional) search until this transaction signature, if found before limit reached.
  * (optional) [Commitment](jsonrpc-api.md#configuring-state-commitment); "processed" is not supported. If parameter not provided, the default is "finalized".

#### Results:
The result field will be an array of transaction signature information, ordered
from newest to oldest transaction:
* `<object>`
  * `signature: <string>` - transaction signature as base-58 encoded string
  * `slot: <u64>` - The slot that contains the block with the transaction
  * `err: <object | null>` - Error if transaction failed, null if transaction succeeded. [TransactionError definitions](https://github.com/solana-labs/solana/blob/master/sdk/src/transaction.rs#L24)
  * `memo: <string |null>` - Memo associated with the transaction, null if no memo is present
  * `blockTime: <i64 | null>` - estimated production time, as Unix timestamp (seconds since the Unix epoch) of when transaction was processed. null if not available.

#### Example:
Request:
```bash
curl http://localhost:8899 -X POST -H "Content-Type: application/json" -d '
  {
    "jsonrpc": "2.0",
    "id": 1,
    "method": "getConfirmedSignaturesForAddress2",
    "params": [
      "Vote111111111111111111111111111111111111111",
      {
        "limit": 1
      }
    ]
  }
'
```

Result:
```json
{
  "jsonrpc": "2.0",
  "result": [
    {
      "err": null,
      "memo": null,
      "signature": "5h6xBEauJ3PK6SWCZ1PGjBvj8vDdWG3KpwATGy1ARAXFSDwt8GFXM7W5Ncn16wmqokgpiKRLuS83KUxyZyv2sUYv",
      "slot": 114,
      "blockTime": null
    }
  ],
  "id": 1
}
```

### getConfirmedTransaction

**DEPRECATED: Please use [getTransaction](jsonrpc-api.md#gettransaction) instead**
This method is expected to be removed in solana-core v1.8

Returns transaction details for a confirmed transaction

#### Parameters:

- `<string>` - transaction signature as base-58 encoded string
- `<object>` - (optional) Configuration object containing the following optional fields:
  - (optional) `encoding: <string>` - encoding for each returned Transaction, either "json", "jsonParsed", "base58" (*slow*), "base64". If parameter not provided, the default encoding is "json".
  "jsonParsed" encoding attempts to use program-specific instruction parsers to return more human-readable and explicit data in the `transaction.message.instructions` list. If "jsonParsed" is requested but a parser cannot be found, the instruction falls back to regular JSON encoding (`accounts`, `data`, and `programIdIndex` fields).
  - (optional) [Commitment](jsonrpc-api.md#configuring-state-commitment); "processed" is not supported. If parameter not provided, the default is "finalized".

#### Results:

- `<null>` - if transaction is not found or not confirmed
- `<object>` - if transaction is confirmed, an object with the following fields:
  - `slot: <u64>` - the slot this transaction was processed in
  - `transaction: <object|[string,encoding]>` - [Transaction](#transaction-structure) object, either in JSON format or encoded binary data, depending on encoding parameter
  - `blockTime: <i64 | null>` - estimated production time, as Unix timestamp (seconds since the Unix epoch) of when the transaction was processed. null if not available
  - `meta: <object | null>` - transaction status metadata object:
    - `err: <object | null>` - Error if transaction failed, null if transaction succeeded. [TransactionError definitions](https://github.com/solana-labs/solana/blob/master/sdk/src/transaction.rs#L24)
    - `fee: <u64>` - fee this transaction was charged, as u64 integer
    - `preBalances: <array>` - array of u64 account balances from before the transaction was processed
    - `postBalances: <array>` - array of u64 account balances after the transaction was processed
    - `innerInstructions: <array|undefined>` - List of [inner instructions](#inner-instructions-structure) or omitted if inner instruction recording was not yet enabled during this transaction
    - `preTokenBalances: <array|undefined>` - List of  [token balances](#token-balances-structure) from before the transaction was processed or omitted if token balance recording was not yet enabled during this transaction
    - `postTokenBalances: <array|undefined>` - List of [token balances](#token-balances-structure) from after the transaction was processed or omitted if token balance recording was not yet enabled during this transaction
    - `logMessages: <array>` - array of string log messages or omitted if log message recording was not yet enabled during this transaction
    - DEPRECATED: `status: <object>` - Transaction status
      - `"Ok": <null>` - Transaction was successful
      - `"Err": <ERR>` - Transaction failed with TransactionError

#### Example:
Request:
```bash
curl http://localhost:8899 -X POST -H "Content-Type: application/json" -d '
  {
    "jsonrpc": "2.0",
    "id": 1,
    "method": "getConfirmedTransaction",
    "params": [
      "2nBhEBYYvfaAe16UMNqRHre4YNSskvuYgx3M6E4JP1oDYvZEJHvoPzyUidNgNX5r9sTyN1J9UxtbCXy2rqYcuyuv",
      "json"
    ]
  }
'
```

Result:
```json
{
  "jsonrpc": "2.0",
  "result": {
    "meta": {
      "err": null,
      "fee": 5000,
      "innerInstructions": [],
      "postBalances": [
        499998932500,
        26858640,
        1,
        1,
        1
      ],
      "postTokenBalances": [],
      "preBalances": [
        499998937500,
        26858640,
        1,
        1,
        1
      ],
      "preTokenBalances": [],
      "status": {
        "Ok": null
      }
    },
    "slot": 430,
    "transaction": {
      "message": {
        "accountKeys": [
          "3UVYmECPPMZSCqWKfENfuoTv51fTDTWicX9xmBD2euKe",
          "AjozzgE83A3x1sHNUR64hfH7zaEBWeMaFuAN9kQgujrc",
          "SysvarS1otHashes111111111111111111111111111",
          "SysvarC1ock11111111111111111111111111111111",
          "Vote111111111111111111111111111111111111111"
        ],
        "header": {
          "numReadonlySignedAccounts": 0,
          "numReadonlyUnsignedAccounts": 3,
          "numRequiredSignatures": 1
        },
        "instructions": [
          {
            "accounts": [
              1,
              2,
              3,
              0
            ],
            "data": "37u9WtQpcm6ULa3WRQHmj49EPs4if7o9f1jSRVZpm2dvihR9C8jY4NqEwXUbLwx15HBSNcP1",
            "programIdIndex": 4
          }
        ],
        "recentBlockhash": "mfcyqEXB3DnHXki6KjjmZck6YjmZLvpAByy2fj4nh6B"
      },
      "signatures": [
        "2nBhEBYYvfaAe16UMNqRHre4YNSskvuYgx3M6E4JP1oDYvZEJHvoPzyUidNgNX5r9sTyN1J9UxtbCXy2rqYcuyuv"
      ]
    }
  },
  "blockTime": null,
  "id": 1
}
```

#### Example:
Request:
```bash
curl http://localhost:8899 -X POST -H "Content-Type: application/json" -d '
  {
    "jsonrpc": "2.0",
    "id": 1,
    "method": "getConfirmedTransaction",
    "params": [
      "2nBhEBYYvfaAe16UMNqRHre4YNSskvuYgx3M6E4JP1oDYvZEJHvoPzyUidNgNX5r9sTyN1J9UxtbCXy2rqYcuyuv",
      "base64"
    ]
  }
'
```

Result:
```json
{
  "jsonrpc": "2.0",
  "result": {
    "meta": {
      "err": null,
      "fee": 5000,
      "innerInstructions": [],
      "postBalances": [
        499998932500,
        26858640,
        1,
        1,
        1
      ],
      "postTokenBalances": [],
      "preBalances": [
        499998937500,
        26858640,
        1,
        1,
        1
      ],
      "preTokenBalances": [],
      "status": {
        "Ok": null
      }
    },
    "slot": 430,
    "transaction": [
      "AVj7dxHlQ9IrvdYVIjuiRFs1jLaDMHixgrv+qtHBwz51L4/ImLZhszwiyEJDIp7xeBSpm/TX5B7mYzxa+fPOMw0BAAMFJMJVqLw+hJYheizSoYlLm53KzgT82cDVmazarqQKG2GQsLgiqktA+a+FDR4/7xnDX7rsusMwryYVUdixfz1B1Qan1RcZLwqvxvJl4/t3zHragsUp0L47E24tAFUgAAAABqfVFxjHdMkoVmOYaR1etoteuKObS21cc1VbIQAAAAAHYUgdNXR0u3xNdiTr072z2DVec9EQQ/wNo1OAAAAAAAtxOUhPBp2WSjUNJEgfvy70BbxI00fZyEPvFHNfxrtEAQQEAQIDADUCAAAAAQAAAAAAAACtAQAAAAAAAAdUE18R96XTJCe+YfRfUp6WP+YKCy/72ucOL8AoBFSpAA==",
      "base64"
    ]
  },
  "id": 1
}
```
