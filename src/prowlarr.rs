//! Prowlarr, the indexer manager, asked through the Servarr client the Servarr adapters
//! share.

use std::sync::Arc;

use adapter_kit_servarr::client::Servarr;
use adapter_kit_servarr::credential::Holding;
use async_trait::async_trait;
use lemonfiber_ports::http::Http;
use lemonfiber_ports::service::{Aggregator, Aggregators, Failure, KnownAggregator};

mod applications;
mod indexers;

/// What every failure is blamed on.
pub const SERVICE: &str = "prowlarr";

/// The version of Prowlarr's API this adapter speaks.
pub const API_VERSION: u32 = 1;

const READS_NO_AGGREGATOR: &str =
    "it reads from no aggregator; it is the one the curators read from";

/// A client for one Prowlarr, presenting the key `held` supplies on every call.
pub struct Prowlarr {
    servarr: Servarr,
}

impl Prowlarr {
    /// The Prowlarr reached at `base` over `http`, asked with the key `held` supplies.
    pub fn new(http: Arc<dyn Http>, base: impl Into<String>, held: Arc<dyn Holding>) -> Self {
        Self {
            servarr: Servarr::new(http, base, held, SERVICE, API_VERSION),
        }
    }

    /// Whether Prowlarr answers what it answers without a key.
    pub async fn answers(&self) -> bool {
        self.servarr.answers().await
    }
}

#[async_trait]
impl Aggregators for Prowlarr {
    async fn aggregators(&self) -> Result<Vec<KnownAggregator>, Failure> {
        Err(self.servarr.endpoint().unsupported(READS_NO_AGGREGATOR))
    }

    async fn add_aggregator(&self, _aggregator: &Aggregator) -> Result<(), Failure> {
        Err(self.servarr.endpoint().unsupported(READS_NO_AGGREGATOR))
    }
}

#[cfg(test)]
mod tests;
