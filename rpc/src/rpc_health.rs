use {
    solana_gossip::cluster_info::ClusterInfo,
    solana_sdk::{clock::Slot, pubkey::Pubkey},
    std::{
        collections::HashSet,
        sync::atomic::{AtomicBool, Ordering},
        sync::Arc,
    },
};

#[derive(PartialEq, Clone, Copy, Debug)]
pub enum RpcHealthStatus {
    Ok,
    Behind { num_slots: Slot }, // Validator is behind its trusted validators
    Unknown,
}

pub struct RpcHealth {
    cluster_info: Arc<ClusterInfo>,
    trusted_validators: Option<HashSet<Pubkey>>,
    health_check_slot_distance: u64,
    override_health_check: Arc<AtomicBool>,
    #[cfg(test)]
    stub_health_status: std::sync::RwLock<Option<RpcHealthStatus>>,
}

impl RpcHealth {
    pub fn new(
        cluster_info: Arc<ClusterInfo>,
        trusted_validators: Option<HashSet<Pubkey>>,
        health_check_slot_distance: u64,
        override_health_check: Arc<AtomicBool>,
    ) -> Self {
        Self {
            cluster_info,
            trusted_validators,
            health_check_slot_distance,
            override_health_check,
            #[cfg(test)]
            stub_health_status: std::sync::RwLock::new(None),
        }
    }

    pub fn check(&self) -> RpcHealthStatus {
        #[cfg(test)]
        {
            if let Some(stub_health_status) = *self.stub_health_status.read().unwrap() {
                return stub_health_status;
            }
        }

        if self.override_health_check.load(Ordering::Relaxed) {
            RpcHealthStatus::Ok
        } else if let Some(trusted_validators) = &self.trusted_validators {
            match (
                self.cluster_info
                    .get_accounts_hash_for_node(&self.cluster_info.id(), |hashes| {
                        hashes
                            .iter()
                            .max_by(|a, b| a.0.cmp(&b.0))
                            .map(|slot_hash| slot_hash.0)
                    })
                    .flatten(),
                trusted_validators
                    .iter()
                    .filter_map(|trusted_validator| {
                        self.cluster_info
                            .get_accounts_hash_for_node(trusted_validator, |hashes| {
                                hashes
                                    .iter()
                                    .max_by(|a, b| a.0.cmp(&b.0))
                                    .map(|slot_hash| slot_hash.0)
                            })
                            .flatten()
                    })
                    .max(),
            ) {
                (
                    Some(latest_account_hash_slot),
                    Some(latest_trusted_validator_account_hash_slot),
                ) => {
                    // The validator is considered healthy if its latest account hash slot is within
                    // `health_check_slot_distance` of the latest trusted validator's account hash slot
                    if latest_account_hash_slot
                        > latest_trusted_validator_account_hash_slot
                            .saturating_sub(self.health_check_slot_distance)
                    {
                        RpcHealthStatus::Ok
                    } else {
                        let num_slots = latest_trusted_validator_account_hash_slot
                            .saturating_sub(latest_account_hash_slot);
                        warn!(
                            "health check: behind by {} slots: me={}, latest trusted_validator={}",
                            num_slots,
                            latest_account_hash_slot,
                            latest_trusted_validator_account_hash_slot
                        );
                        RpcHealthStatus::Behind { num_slots }
                    }
                }
                (latest_account_hash_slot, latest_trusted_validator_account_hash_slot) => {
                    if latest_account_hash_slot.is_none() {
                        warn!("health check: latest_account_hash_slot not available");
                    }
                    if latest_trusted_validator_account_hash_slot.is_none() {
                        warn!("health check: latest_trusted_validator_account_hash_slot not available");
                    }
                    RpcHealthStatus::Unknown
                }
            }
        } else {
            // No trusted validator point of reference available, so this validator is healthy
            // because it's running
            RpcHealthStatus::Ok
        }
    }

    #[cfg(test)]
    pub(crate) fn stub() -> Arc<Self> {
        use {
            solana_gossip::contact_info::ContactInfo, solana_sdk::signer::keypair::Keypair,
            solana_streamer::socket::SocketAddrSpace,
        };
        Arc::new(Self::new(
            Arc::new(ClusterInfo::new(
                ContactInfo::default(),
                Arc::new(Keypair::new()),
                SocketAddrSpace::Unspecified,
            )),
            None,
            42,
            Arc::new(AtomicBool::new(false)),
        ))
    }

    #[cfg(test)]
    pub(crate) fn stub_set_health_status(&self, stub_health_status: Option<RpcHealthStatus>) {
        *self.stub_health_status.write().unwrap() = stub_health_status;
    }
}
