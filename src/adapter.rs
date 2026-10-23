//! What this adapter says it is, and the kit that serves its contract.

use std::sync::Arc;

use lemonfiber_adapter_kit::{Kit, Readiness};
use lemonfiber_contract::adapter::{About, Release};
use lemonfiber_contract::capabilities::indexer;

use crate::prowlarr::Prowlarr;

/// The upstream this adapter fronts, as its project names itself.
pub const UPSTREAM: &str = "Prowlarr";

/// The Prowlarr release this adapter's recordings were taken from.
pub const RELEASE: &str = "2.6.5.5623-ls164";

/// The digest of the image that release's recordings were taken from.
pub const DIGEST: &str = "sha256:4e992f9b9229bc8c97c666b10b042a7360bce1d99b196e8aa29fd9805849cbf1";

/// What this adapter says it is: the contract it speaks and the release it supports.
#[must_use]
pub fn about() -> About {
    About {
        speaks: vec![lemonfiber_contract::spoken(
            indexer::search::CAPABILITY,
            indexer::search::MAJOR,
        )],
        upstream: UPSTREAM.to_owned(),
        releases: vec![Release {
            version: RELEASE.to_owned(),
            digest: DIGEST.to_owned(),
        }],
    }
}

/// The contract, served from `prowlarr` under `key`, ready while Prowlarr answers.
#[must_use]
pub fn kit(key: String, prowlarr: Arc<Prowlarr>) -> Kit {
    let asked = Arc::clone(&prowlarr);
    let ready: Readiness = Arc::new(move || {
        let asked = Arc::clone(&asked);
        Box::pin(async move { asked.answers().await })
    });
    Kit::new(key, about())
        .ready_when(ready)
        .serving(indexer::search::served(prowlarr))
}

#[cfg(test)]
mod tests;
