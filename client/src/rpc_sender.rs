//! A transport for RPC calls.

use crate::{client_error::Result, rpc_request::RpcRequest};

/// A transport for RPC calls.
///
/// `RpcSender` implements the underlying transport of requests to, and
/// responses from, a Solana node, and is used primarily by [`RpcClient`].
///
/// It is typically implemented by [`HttpSender`] in production, and
/// [`MockSender`] in unit tests.
///
/// [`RpcClient`]: crate::rpc_client::RpcClient
/// [`HttpSender`]: crate::http_sender::HttpSender
/// [`MockSender`]: crate::mock_sender::MockSender
pub trait RpcSender {
    fn send(&self, request: RpcRequest, params: serde_json::Value) -> Result<serde_json::Value>;
}
