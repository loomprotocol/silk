use bincode::serialize;
use jsonrpc_core::Result as JsonResult;
use serde_json::{json, Value};
use solana_sdk::{
    clock::{Epoch, Slot},
    hash::Hash,
    message::MessageHeader,
    transaction::{Result, Transaction},
};
use std::{collections::HashMap, error, fmt, io, net::SocketAddr};

pub type RpcResponseIn<T> = JsonResult<Response<T>>;
pub type RpcResponse<T> = io::Result<Response<T>>;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RpcResponseContext {
    pub slot: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Response<T> {
    pub context: RpcResponseContext,
    pub value: T,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RpcConfirmedBlock {
    pub previous_blockhash: RpcEncodedHash,
    pub blockhash: RpcEncodedHash,
    pub parent_slot: Slot,
    pub transactions: Vec<(RpcEncodedTransaction, Option<RpcTransactionStatus>)>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum RpcTransactionEncoding {
    Binary,
    Json,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase", untagged)]
pub enum RpcEncodedHash {
    Binary(Hash),
    Json(String),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", untagged)]
pub enum RpcEncodedTransaction {
    Binary(Vec<u8>),
    Json(RpcTransaction),
}

impl RpcEncodedTransaction {
    pub fn encode(transaction: Transaction, encoding: RpcTransactionEncoding) -> Self {
        if encoding == RpcTransactionEncoding::Json {
            RpcEncodedTransaction::Json(RpcTransaction {
                signatures: transaction
                    .signatures
                    .iter()
                    .map(|sig| sig.to_string())
                    .collect(),
                message: RpcMessage {
                    header: transaction.message.header,
                    account_keys: transaction
                        .message
                        .account_keys
                        .iter()
                        .map(|pubkey| pubkey.to_string())
                        .collect(),
                    recent_blockhash: transaction.message.recent_blockhash.to_string(),
                    instructions: transaction
                        .message
                        .instructions
                        .iter()
                        .map(|instruction| RpcCompiledInstruction {
                            program_id_index: instruction.program_id_index,
                            accounts: instruction.accounts.clone(),
                            data: instruction.data.clone(),
                        })
                        .collect(),
                },
            })
        } else {
            RpcEncodedTransaction::Binary(serialize(&transaction).unwrap())
        }
    }
}

/// A duplicate representation of a Transaction for pretty JSON serialization
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RpcTransaction {
    pub signatures: Vec<String>,
    pub message: RpcMessage,
}

/// A duplicate representation of a Message for pretty JSON serialization
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RpcMessage {
    pub header: MessageHeader,
    pub account_keys: Vec<String>,
    pub recent_blockhash: String,
    pub instructions: Vec<RpcCompiledInstruction>,
}

/// A duplicate representation of a Message for pretty JSON serialization
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RpcCompiledInstruction {
    pub program_id_index: u8,
    pub accounts: Vec<u8>,
    pub data: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RpcTransactionStatus {
    pub status: Result<()>,
    pub fee: u64,
    pub pre_balances: Vec<u64>,
    pub post_balances: Vec<u64>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct RpcContactInfo {
    /// Pubkey of the node as a base-58 string
    pub pubkey: String,
    /// Gossip port
    pub gossip: Option<SocketAddr>,
    /// Tpu port
    pub tpu: Option<SocketAddr>,
    /// JSON RPC port
    pub rpc: Option<SocketAddr>,
}

/// Map of leader base58 identity pubkeys to the slot indices relative to the first epoch slot
pub type RpcLeaderSchedule = HashMap<String, Vec<usize>>;

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct RpcEpochInfo {
    /// The current epoch
    pub epoch: Epoch,

    /// The current slot, relative to the start of the current epoch
    pub slot_index: u64,

    /// The number of slots in this epoch
    pub slots_in_epoch: u64,

    /// The absolute current slot
    pub absolute_slot: Slot,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "kebab-case")]
pub struct RpcVersionInfo {
    /// The current version of solana-core
    pub solana_core: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct RpcVoteAccountStatus {
    pub current: Vec<RpcVoteAccountInfo>,
    pub delinquent: Vec<RpcVoteAccountInfo>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct RpcVoteAccountInfo {
    /// Vote account pubkey as base-58 encoded string
    pub vote_pubkey: String,

    /// The pubkey of the node that votes using this account
    pub node_pubkey: String,

    /// The current stake, in lamports, delegated to this vote account
    pub activated_stake: u64,

    /// An 8-bit integer used as a fraction (commission/MAX_U8) for rewards payout
    pub commission: u8,

    /// Whether this account is staked for the current epoch
    pub epoch_vote_account: bool,

    /// History of how many credits earned by the end of each epoch
    ///   each tuple is (Epoch, credits, prev_credits)
    pub epoch_credits: Vec<(Epoch, u64, u64)>,

    /// Most recent slot voted on by this vote account (0 if no votes exist)
    pub last_vote: u64,

    /// Current root slot for this vote account (0 if not root slot exists)
    pub root_slot: Slot,
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub enum RpcRequest {
    ConfirmTransaction,
    DeregisterNode,
    ValidatorExit,
    GetAccountInfo,
    GetBalance,
    GetBlockTime,
    GetClusterNodes,
    GetConfirmedBlock,
    GetConfirmedBlocks,
    GetEpochInfo,
    GetEpochSchedule,
    GetGenesisHash,
    GetInflation,
    GetLeaderSchedule,
    GetNumBlocksSinceSignatureConfirmation,
    GetProgramAccounts,
    GetRecentBlockhash,
    GetSignatureStatus,
    GetSlot,
    GetSlotLeader,
    GetStorageTurn,
    GetStorageTurnRate,
    GetSlotsPerSegment,
    GetStoragePubkeysForSlot,
    GetTransactionCount,
    GetVersion,
    GetVoteAccounts,
    RegisterNode,
    RequestAirdrop,
    SendTransaction,
    SignVote,
    GetMinimumBalanceForRentExemption,
}

impl RpcRequest {
    pub(crate) fn build_request_json(&self, id: u64, params: Value) -> Value {
        let jsonrpc = "2.0";
        let method = match self {
            RpcRequest::ConfirmTransaction => "confirmTransaction",
            RpcRequest::DeregisterNode => "deregisterNode",
            RpcRequest::ValidatorExit => "validatorExit",
            RpcRequest::GetAccountInfo => "getAccountInfo",
            RpcRequest::GetBalance => "getBalance",
            RpcRequest::GetBlockTime => "getBlockTime",
            RpcRequest::GetClusterNodes => "getClusterNodes",
            RpcRequest::GetConfirmedBlock => "getConfirmedBlock",
            RpcRequest::GetConfirmedBlocks => "getConfirmedBlocks",
            RpcRequest::GetEpochInfo => "getEpochInfo",
            RpcRequest::GetEpochSchedule => "getEpochSchedule",
            RpcRequest::GetGenesisHash => "getGenesisHash",
            RpcRequest::GetInflation => "getInflation",
            RpcRequest::GetLeaderSchedule => "getLeaderSchedule",
            RpcRequest::GetNumBlocksSinceSignatureConfirmation => {
                "getNumBlocksSinceSignatureConfirmation"
            }
            RpcRequest::GetProgramAccounts => "getProgramAccounts",
            RpcRequest::GetRecentBlockhash => "getRecentBlockhash",
            RpcRequest::GetSignatureStatus => "getSignatureStatus",
            RpcRequest::GetSlot => "getSlot",
            RpcRequest::GetSlotLeader => "getSlotLeader",
            RpcRequest::GetStorageTurn => "getStorageTurn",
            RpcRequest::GetStorageTurnRate => "getStorageTurnRate",
            RpcRequest::GetSlotsPerSegment => "getSlotsPerSegment",
            RpcRequest::GetStoragePubkeysForSlot => "getStoragePubkeysForSlot",
            RpcRequest::GetTransactionCount => "getTransactionCount",
            RpcRequest::GetVersion => "getVersion",
            RpcRequest::GetVoteAccounts => "getVoteAccounts",
            RpcRequest::RegisterNode => "registerNode",
            RpcRequest::RequestAirdrop => "requestAirdrop",
            RpcRequest::SendTransaction => "sendTransaction",
            RpcRequest::SignVote => "signVote",
            RpcRequest::GetMinimumBalanceForRentExemption => "getMinimumBalanceForRentExemption",
        };
        json!({
           "jsonrpc": jsonrpc,
           "id": id,
           "method": method,
           "params": params,
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum RpcError {
    RpcRequestError(String),
}

impl fmt::Display for RpcError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "invalid")
    }
}

impl error::Error for RpcError {
    fn description(&self) -> &str {
        "invalid"
    }

    fn cause(&self) -> Option<&dyn error::Error> {
        // Generic error, underlying cause isn't tracked.
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use solana_sdk::commitment_config::{CommitmentConfig, CommitmentLevel};

    #[test]
    fn test_build_request_json() {
        let test_request = RpcRequest::GetAccountInfo;
        let addr = json!("deadbeefXjn8o3yroDHxUtKsZZgoy4GPkPPXfouKNHhx");
        let request = test_request.build_request_json(1, json!([addr.clone()]));
        assert_eq!(request["method"], "getAccountInfo");
        assert_eq!(request["params"], json!([addr]));

        let test_request = RpcRequest::GetBalance;
        let request = test_request.build_request_json(1, json!([addr]));
        assert_eq!(request["method"], "getBalance");

        let test_request = RpcRequest::GetEpochInfo;
        let request = test_request.build_request_json(1, Value::Null);
        assert_eq!(request["method"], "getEpochInfo");

        let test_request = RpcRequest::GetInflation;
        let request = test_request.build_request_json(1, Value::Null);
        assert_eq!(request["method"], "getInflation");

        let test_request = RpcRequest::GetRecentBlockhash;
        let request = test_request.build_request_json(1, Value::Null);
        assert_eq!(request["method"], "getRecentBlockhash");

        let test_request = RpcRequest::GetSlot;
        let request = test_request.build_request_json(1, Value::Null);
        assert_eq!(request["method"], "getSlot");

        let test_request = RpcRequest::GetTransactionCount;
        let request = test_request.build_request_json(1, Value::Null);
        assert_eq!(request["method"], "getTransactionCount");

        let test_request = RpcRequest::RequestAirdrop;
        let request = test_request.build_request_json(1, Value::Null);
        assert_eq!(request["method"], "requestAirdrop");

        let test_request = RpcRequest::SendTransaction;
        let request = test_request.build_request_json(1, Value::Null);
        assert_eq!(request["method"], "sendTransaction");
    }

    #[test]
    fn test_build_request_json_config_options() {
        let commitment_config = CommitmentConfig {
            commitment: CommitmentLevel::Max,
        };
        let addr = json!("deadbeefXjn8o3yroDHxUtKsZZgoy4GPkPPXfouKNHhx");

        // Test request with CommitmentConfig and no params
        let test_request = RpcRequest::GetRecentBlockhash;
        let request = test_request.build_request_json(1, json!([commitment_config.clone()]));
        assert_eq!(request["params"], json!([commitment_config.clone()]));

        // Test request with CommitmentConfig and params
        let test_request = RpcRequest::GetBalance;
        let request =
            test_request.build_request_json(1, json!([addr.clone(), commitment_config.clone()]));
        assert_eq!(request["params"], json!([addr, commitment_config]));
    }
}
