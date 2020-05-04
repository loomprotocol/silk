use crate::request_response::RequestResponse;
use solana_sdk::{clock::Slot, timing::timestamp};
use std::{
    collections::HashMap,
    net::{IpAddr, SocketAddr},
};

pub const DEFAULT_REQUEST_EXPIRATION_MS: u64 = 10_000;
pub const NONCE_BYTES: usize = 4;

pub type Nonce = u32;

pub struct NodeOutstandingRequests<T, S>
where
    T: RequestResponse<Response = S>,
{
    requests: HashMap<Nonce, RequestStatus<T>>,
    nonce: Nonce,
}

impl<T, S> NodeOutstandingRequests<T, S>
where
    T: RequestResponse<Response = S>,
{
    // Returns boolean indicating whether sufficient time has passed for a request with
    // the given timestamp to be made
    fn add_request(&mut self, request: T) -> Nonce {
        let num_expected_responses = request.num_expected_responses();
        let nonce = self.nonce;
        self.requests.insert(
            nonce,
            RequestStatus {
                expire_timestamp: timestamp() + DEFAULT_REQUEST_EXPIRATION_MS,
                num_expected_responses,
                request,
            },
        );
        self.nonce = (self.nonce + 1) % std::u32::MAX;
        nonce
    }

    fn register_response(&mut self, nonce: u32, response: &S, now: u64) -> bool {
        let (is_valid, should_delete) = self
            .requests
            .get_mut(&nonce)
            .map(|status| {
                if status.num_expected_responses > 0
                    && now < status.expire_timestamp
                    && status.request.verify_response(response)
                {
                    status.num_expected_responses -= 1;
                    (true, status.num_expected_responses == 0)
                } else {
                    (false, true)
                }
            })
            .unwrap_or((false, false));

        if should_delete {
            self.requests
                .remove(&nonce)
                .expect("Delete must delete existing object");
        }

        is_valid
    }
}

impl<T, S> Default for NodeOutstandingRequests<T, S>
where
    T: RequestResponse<Response = S>,
{
    fn default() -> Self {
        Self {
            requests: HashMap::new(),
            nonce: 0,
        }
    }
}

pub struct RequestStatus<T> {
    expire_timestamp: u64,
    num_expected_responses: u32,
    request: T,
}

pub struct OutstandingRequests<T, S>
where
    T: RequestResponse<Response = S>,
{
    requests: HashMap<IpAddr, NodeOutstandingRequests<T, S>>,
}

impl<T, S> OutstandingRequests<T, S>
where
    T: RequestResponse<Response = S>,
{
    pub fn add_request(&mut self, socket_addr: &SocketAddr, request: T) -> Nonce {
        let node_outstanding_requests = self.requests.entry(socket_addr.ip()).or_default();
        node_outstanding_requests.add_request(request)
    }

    pub fn register_response(
        &mut self,
        socket_addr: &SocketAddr,
        nonce: u32,
        response: &S,
    ) -> bool {
        self.requests
            .get_mut(&socket_addr.ip())
            .map(|node_outstanding_requests| {
                let now = timestamp();
                node_outstanding_requests.register_response(nonce, response, now)
            })
            .unwrap_or(false)
    }
}

impl<T, S> Default for OutstandingRequests<T, S>
where
    T: RequestResponse<Response = S>,
{
    fn default() -> Self {
        Self {
            requests: HashMap::new(),
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::serve_repair::RepairType;
    use solana_ledger::shred::Shred;

    #[test]
    fn test_add_request() {
        let repair_type = RepairType::Orphan(9);
        let mut node_outstanding_requests = NodeOutstandingRequests::default();
        let nonce = node_outstanding_requests.nonce;
        node_outstanding_requests.add_request(repair_type);
        let request_status = node_outstanding_requests.requests.get(&nonce).unwrap();
        assert_eq!(request_status.request, repair_type);
        assert_eq!(
            request_status.num_expected_responses,
            repair_type.num_expected_responses()
        );
        assert_eq!(node_outstanding_requests.nonce, nonce + 1);
    }

    #[test]
    fn test_register_response() {
        let repair_type = RepairType::Orphan(9);
        let mut node_outstanding_requests = NodeOutstandingRequests::default();
        let mut nonce = node_outstanding_requests.nonce;
        node_outstanding_requests.add_request(repair_type);

        let shred = Shred::new_empty_data_shred();
        let mut expire_timestamp = node_outstanding_requests
            .requests
            .get(&nonce)
            .unwrap()
            .expire_timestamp;
        let mut num_expected_responses = node_outstanding_requests
            .requests
            .get(&nonce)
            .unwrap()
            .num_expected_responses;
        assert!(num_expected_responses > 1);

        // Response that passes all checks should decrease num_expected_responses
        assert!(node_outstanding_requests.register_response(nonce, &shred, expire_timestamp - 1));
        num_expected_responses -= 1;
        assert_eq!(
            node_outstanding_requests
                .requests
                .get(&nonce)
                .unwrap()
                .num_expected_responses,
            num_expected_responses
        );

        // Response with incorrect nonce is ignored
        assert!(!node_outstanding_requests.register_response(
            nonce + 1,
            &shred,
            expire_timestamp - 1
        ));
        assert!(!node_outstanding_requests.register_response(nonce + 1, &shred, expire_timestamp));
        assert_eq!(
            node_outstanding_requests
                .requests
                .get(&nonce)
                .unwrap()
                .num_expected_responses,
            num_expected_responses
        );

        // Response with timestamp over limit should remove status, preventing late
        // responses from being accepted
        assert!(!node_outstanding_requests.register_response(nonce, &shred, expire_timestamp));
        assert!(node_outstanding_requests.requests.get(&nonce).is_none());

        // If number of outstanding requests hits zero, should also remove the entry
        node_outstanding_requests.add_request(repair_type);
        nonce += 1;
        expire_timestamp = node_outstanding_requests
            .requests
            .get(&nonce)
            .unwrap()
            .expire_timestamp;
        num_expected_responses = node_outstanding_requests
            .requests
            .get(&nonce)
            .unwrap()
            .num_expected_responses;
        assert!(num_expected_responses > 1);
        for _ in 0..num_expected_responses {
            assert!(node_outstanding_requests.requests.get(&nonce).is_some());
            assert!(node_outstanding_requests.register_response(
                nonce,
                &shred,
                expire_timestamp - 1
            ));
        }
        assert!(node_outstanding_requests.requests.get(&nonce).is_none());
    }
}
