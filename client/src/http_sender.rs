//! The standard [`RpcSender`] over HTTP.

use {
    crate::{
        client_error::Result,
        rpc_custom_error,
        rpc_request::{RpcError, RpcRequest, RpcResponseErrorData},
        rpc_response::RpcSimulateTransactionResult,
        rpc_sender::RpcSender,
    },
    log::*,
    reqwest::{
        self,
        header::{CONTENT_TYPE, RETRY_AFTER},
        StatusCode,
    },
    std::{
        sync::{
            atomic::{AtomicU64, Ordering},
            Arc,
        },
        thread::sleep,
        time::Duration,
    },
};

pub struct HttpSender {
    client: Arc<reqwest::blocking::Client>,
    url: String,
    request_id: AtomicU64,
}

/// The standard [`RpcSender`] over HTTP.
impl HttpSender {
    /// Create an HTTP RPC sender.
    ///
    /// The URL is an HTTP URL, usually for port 8899, as in
    /// "http://localhost:8899". The sender has a default timeout of 30 seconds.
    pub fn new(url: String) -> Self {
        Self::new_with_timeout(url, Duration::from_secs(30))
    }

    /// Create an HTTP RPC sender.
    ///
    /// The URL is an HTTP URL, usually for port 8899.
    pub fn new_with_timeout(url: String, timeout: Duration) -> Self {
        // `reqwest::blocking::Client` panics if run in a tokio async context.  Shuttle the
        // request to a different tokio thread to avoid this
        let client = Arc::new(
            tokio::task::block_in_place(move || {
                reqwest::blocking::Client::builder()
                    .timeout(timeout)
                    .build()
            })
            .expect("build rpc client"),
        );

        Self {
            client,
            url,
            request_id: AtomicU64::new(0),
        }
    }
}

#[derive(Deserialize, Debug)]
struct RpcErrorObject {
    code: i64,
    message: String,
    data: serde_json::Value,
}

impl RpcSender for HttpSender {
    fn send(&self, request: RpcRequest, params: serde_json::Value) -> Result<serde_json::Value> {
        let request_id = self.request_id.fetch_add(1, Ordering::Relaxed);
        let request_json = request.build_request_json(request_id, params).to_string();

        let mut too_many_requests_retries = 5;
        loop {
            // `reqwest::blocking::Client` panics if run in a tokio async context.  Shuttle the
            // request to a different tokio thread to avoid this
            let response = {
                let client = self.client.clone();
                let request_json = request_json.clone();
                tokio::task::block_in_place(move || {
                    client
                        .post(&self.url)
                        .header(CONTENT_TYPE, "application/json")
                        .body(request_json)
                        .send()
                })
            };

            match response {
                Ok(response) => {
                    if !response.status().is_success() {
                        if response.status() == StatusCode::TOO_MANY_REQUESTS
                            && too_many_requests_retries > 0
                        {
                            let mut duration = Duration::from_millis(500);
                            if let Some(retry_after) = response.headers().get(RETRY_AFTER) {
                                if let Ok(retry_after) = retry_after.to_str() {
                                    if let Ok(retry_after) = retry_after.parse::<u64>() {
                                        if retry_after < 120 {
                                            duration = Duration::from_secs(retry_after);
                                        }
                                    }
                                }
                            }

                            too_many_requests_retries -= 1;
                            debug!(
                                "Too many requests: server responded with {:?}, {} retries left, pausing for {:?}",
                                response, too_many_requests_retries, duration
                            );

                            sleep(duration);
                            continue;
                        }
                        return Err(response.error_for_status().unwrap_err().into());
                    }

                    let response_text = tokio::task::block_in_place(move || response.text())?;

                    let json: serde_json::Value = serde_json::from_str(&response_text)?;
                    if json["error"].is_object() {
                        return match serde_json::from_value::<RpcErrorObject>(json["error"].clone())
                        {
                            Ok(rpc_error_object) => {
                                let data = match rpc_error_object.code {
                                    rpc_custom_error::JSON_RPC_SERVER_ERROR_SEND_TRANSACTION_PREFLIGHT_FAILURE => {
                                        match serde_json::from_value::<RpcSimulateTransactionResult>(json["error"]["data"].clone()) {
                                            Ok(data) => RpcResponseErrorData::SendTransactionPreflightFailure(data),
                                            Err(err) => {
                                                debug!("Failed to deserialize RpcSimulateTransactionResult: {:?}", err);
                                                RpcResponseErrorData::Empty
                                            }
                                        }
                                    },
                                    rpc_custom_error::JSON_RPC_SERVER_ERROR_NODE_UNHEALTHY => {
                                        match serde_json::from_value::<rpc_custom_error::NodeUnhealthyErrorData>(json["error"]["data"].clone()) {
                                            Ok(rpc_custom_error::NodeUnhealthyErrorData {num_slots_behind}) => RpcResponseErrorData::NodeUnhealthy {num_slots_behind},
                                            Err(_err) => {
                                                RpcResponseErrorData::Empty
                                            }
                                        }
                                    },
                                    _ => RpcResponseErrorData::Empty
                                };

                                Err(RpcError::RpcResponseError {
                                    code: rpc_error_object.code,
                                    message: rpc_error_object.message,
                                    data,
                                }
                                .into())
                            }
                            Err(err) => Err(RpcError::RpcRequestError(format!(
                                "Failed to deserialize RPC error response: {} [{}]",
                                serde_json::to_string(&json["error"]).unwrap(),
                                err
                            ))
                            .into()),
                        };
                    }
                    return Ok(json["result"].clone());
                }
                Err(err) => {
                    return Err(err.into());
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test(flavor = "multi_thread")]
    async fn http_sender_on_tokio_multi_thread() {
        let http_sender = HttpSender::new("http://localhost:1234".to_string());
        let _ = http_sender.send(RpcRequest::GetVersion, serde_json::Value::Null);
    }

    #[tokio::test(flavor = "current_thread")]
    #[should_panic(expected = "can call blocking only when running on the multi-threaded runtime")]
    async fn http_sender_ontokio_current_thread_should_panic() {
        // RpcClient::new() will panic in the tokio current-thread runtime due to `tokio::task::block_in_place()` usage, and there
        // doesn't seem to be a way to detect whether the tokio runtime is multi_thread or current_thread...
        let _ = HttpSender::new("http://localhost:1234".to_string());
    }
}
