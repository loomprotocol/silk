---
title: AccountsDB Replication for RPC Services
---

## Problem

Validators fall behind the network when bogged down by heavy RPC load. This
seems to be due to a combination of CPU load and lock contention caused by
serving RPC requests. The most expensive RPC requests involve account scans.

## Solution Overview

AccountsDB `replicas` that run separately from the main validator can be used to
offload account-scan requests. Replicas would only: request and pull account
updates from the validator, serve client account-state RPC requests, and manage
AccountsDb and AccountsBackgroundService clean + shrink.

The replica communicates to the main validator via a new RPC mechanism to fetch
metadata information about the replication and the accounts update from the validator.
The main validator supports only one replica node. A replica node can relay the
information to 1 or more other replicas forming a replication tree.

At the initial start of the replica node, it downloads the latest snapshot
from a validator and constructs the bank and AccountsDb from it. After that, it queries
its main validator for new slots and requests the validator to send the updated
accounts for that slot and update to its own AccountsDb.

The same RPC replication mechanism can be used between a replica to another replica.
This requires the replica to serve both the client and server in the replication model.

On the client RPC serving side, `JsonRpcAccountsService` is responsible for serving
the client RPC calls for accounts related information similar to the existing
`JsonRpcService`.

The replica will also take snapshots periodically so that it can start quickly after
a restart if the snapshot is not too old.

## Detailed Solution
The following sections provide more details of the design.

### Consistency Model
The AccountsDb information is replicated asynchronously from the main validator to the replica.
When a query against the replica's AccountsDb is made, the replica may not have the latest
information of the latest slot. In this regard, it will eventually be consistent. However, for
a particular slot, the information provided is consistent with that of its peer validator
for commitment levels confirmed and finalized. For V1, we only support queries at these two
levels.

### Solana Replica Node
A new node named solana-replica-node will be introduced whose main responsibility is to maintain
the AccountsDb replica. The RPC node or replica node is used interchangeably in this document.
It will be a separate executable from the validator.

The replica consists of the following major components:

The `ReplicaUpdatedSlotsRequestor`: this service is responsible for periodically sending the
request `ReplicaUpdatedSlotsRequest` to its peer validator or replica for the latest slots.
It specifies the latest slot (last_replicated_slot) for which the replica has already
fetched the accounts information for. This maintains the ReplWorkingSlotSet and manages
the lifecycle of BankForks, BlockCommitmentCache (for the highest confirmed slot) and
the optimistically confirmed bank.

The `ReplicaUpdatedSlotsServer`: this service is responsible for serving the
`ReplicaUpdatedSlotsRequest` and sends the `ReplicaUpdatedSlotsResponse` back to the requestor.
The response consists of a vector of new slots the validator knows of which is later than the
specified last_replicated_slot. This service also runs in the main validator. This service
gets the slots for replication from the BankForks, BlockCommitmentCache and OptimiscallyConfirmBank.

The `ReplicaAccountsRequestor`: this service is responsible for sending the request
`ReplicaAccountsRequest` to its peer validator or replica for the `ReplicaAccountInfo` for a
slot for which it has not completed accounts db replication. The `ReplicaAccountInfo` contains
the `ReplicaAccountMeta`, Hash and the AccountData. The `ReplicaAccountMeta` contains info about
the existing `AccountMeta` in addition to the account data length in bytes.

The `ReplicaAccountsServer`: this service is reponsible for serving the `ReplicaAccountsRequest`
and sends `ReplicaAccountsResponse` to the requestor. The response contains the count of the
ReplAccountInfo and the vector of ReplAccountInfo. This service runs both in the validator
and the replica relaying replication information. The server can stream the account information
from its AccountCache or from the storage if already flushed. This is similar to how a snapshot
package is created from the AccountsDb with the difference that the storage does not need to be
flushed to the disk before streaming to the client. If the account data is in the cache, it can
be directly streamed. Care must be taken to avoid account data for a slot being cleaned while
serving the streaming. When attempting a replication of a slot, if the slot is already cleaned
up with accounts data cleaned as result of update in later rooted slots, the replica should
forsake this slot and try the later uncleaned root slot.

During replication we also need to replicate the information of accounts that have been cleaned
up due to zero lamports, i.e. we need to be able to tell the difference between an account in a
given slot which was not updated and hence has no storage entry in that slot, and one that
holds 0 lamports and has been cleaned up through the history. We may record this via some
"Tombstone" mechanism -- recording the dead accounts cleaned up fora slot. The tombstones
themselves can be removed after exceeding the retention period expressed as epochs. Any
attempt to replicate slots with tombstones removed will fail and the replica should skip
this slot and try later ones.

The `JsonRpcAccountsService`: this is the RPC service serving client requests for account
information. The existing JsonRpcService serves other client calls than AccountsDb ones.
The replica node only serves the AccountsDb calls.

The existing JsonRpcService requires `BankForks`, `OptimisticallyConfirmedBank` and
`BlockCommitmentCache` to load the Bank. The JsonRpcAccountsService will need to use
information obtained from ReplicaUpdatedSlotsResponse to construct the AccountsDb.

The `AccountsBackgroundService`: this service also runs in the replica which is responsible
for taking snapshots periodically and shrinking the AccountsDb and doing accounts cleaning.
The existing code also uses BankForks which we need to keep in the replica.

### Compatibility Consideration

For protocol compatibility considerations, all the requests have the replication version which
is initially set to 1. Alternatively, we can use the validator's version. The RPC server side
shall check the request version and fail if it is not supported.

### Replication Setup
To limit adverse effects on the validator and the replica due to replication, they can be
configured with a list of replica nodes which can form a replication pair with it. And the
replica node is configured with the validator which can serve its requests.


### Fault Tolerance
The main responsibility of making sure the replication is tolerant of faults lies with the
replica. In case of request failures, the replica shall retry the requests.


### Interface

Following are the client RPC APIs supported by the replica node in JsonRpcAccountsService.

- getAccountInfo
- getBlockCommitment
- getMultipleAccounts
- getProgramAccounts
- getMinimumBalanceForRentExemption
- getInflationGovenor
- getInflationRate
- getEpochSchedule
- getRecentBlockhash
- getFees
- getFeeCalculatorForBlockhash
- getFeeRateGovernor
- getLargestAccounts
- getSupply
- getStakeActivation
- getTokenAccountBalance
- getTokenSupply
- getTokenLargestAccounts
- getTokenAccountsByOwner
- getTokenAccountsByDelegate

Following APIs are not included:

- getInflationReward
- getClusterNodes
- getRecentPerformanceSamples
- getGenesisHash
- getSignatueStatuses
- getMaxRetransmitSlot
- getMaxShredInsertSlot
- sendTransaction
- simulateTransaction
- getSlotLeader
- getSlotLeaders
- minimumLedgerSlot
- getBlock
- getBlockTime
- getBlocks
- getBlocksWithLimit
- getTransaction
- getSignaturesForAddress
- getFirstAvailableBlock
- getBlockProduction


Action Items

1. Build the replica framework and executable
2. Integrate snapshot restore code for bootstrap the AccountsDb.
3. Develop the ReplicaUpdatedSlotsRequestor and ReplicaUpdatedSlotsServer interface code
4. Develop the ReplicaUpdatedSlotsRequestor and ReplicaUpdatedSlotsServer detailed implementations: managing the ReplEligibleSlotSet lifecycle: adding new roots and deleting root to it. And interfaces managing ReplWorkingSlotSet interface: adding and removing. Develop component synthesising information from BankForks, BlockCommitmentCache and OptimistcallyConfirmedBank on the server side and maintaining information on the client side.
5. Develop the interface code for ReplicaAccountsRequestor and ReplicaAccountsServer
6. Develop detailed implementation for ReplicaAccountsRequestor and ReplicaAccountsServer and develop the replication account storage serializer and deserializer.
7. Develop the interface code JsonRpcAccountsService
8. Detailed Implementation of JsonRpcAccountsService, refactor code to share with part of JsonRpcService.
9. Integrate with the AccountsBackgroundService in the replica for shrinking, cleaning, snapshotting.
10. Metrics and performance testing
