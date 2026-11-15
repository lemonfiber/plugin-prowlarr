//! The challenge solvers Prowlarr fetches an indexer site through: its indexer proxies,
//! each sending a site through it where the two share a tag.

use async_trait::async_trait;
use lemonfiber_manifest::ApiKind;
use lemonfiber_ports::http::Method;
use lemonfiber_ports::service::{Failure, KnownSolver, Protocol, SolverAt, Solvers};
use serde::Deserialize;
use serde_json::Value;

use super::Prowlarr;

const PROXIES: &str = "/indexerproxy";

const TAGS: &str = "/tag";

const THE_PROXIES: &str = "the indexer proxies";

const THE_TAGS: &str = "the tags";

const A_TAG: &str = "the tag it made could not be read";

const HOST: &str = "host";

const REQUEST_TIMEOUT: &str = "requestTimeout";

const IMPLEMENTATION: &str = "FlareSolverr";

const CONFIG_CONTRACT: &str = "FlareSolverrSettings";

const TIMEOUT_SECONDS: u32 = 60;

const ASKS_FLARESOLVERR: &str =
    "Prowlarr fetches through a challenge solver in the FlareSolverr API alone";

#[derive(Deserialize)]
struct ProxyResource {
    id: i64,
    #[serde(default)]
    name: String,
    #[serde(default)]
    implementation: String,
    #[serde(default)]
    fields: Vec<ProxyField>,
    #[serde(default)]
    tags: Vec<i64>,
}

#[derive(Deserialize)]
struct ProxyField {
    name: String,
    #[serde(default)]
    value: Value,
}

#[derive(Deserialize)]
struct Tag {
    id: i64,
    label: String,
}

impl ProxyResource {
    fn known(self, tags: &[Tag]) -> Option<KnownSolver> {
        if self.implementation != IMPLEMENTATION {
            return None;
        }
        let host = self
            .fields
            .iter()
            .find(|field| field.name == HOST)?
            .value
            .as_str()?
            .trim_end_matches('/')
            .to_owned();
        Some(KnownSolver {
            id: self.id.to_string(),
            name: self.name,
            url: host,
            protocol: Protocol(ApiKind::Flaresolverr.name().to_owned()),
            tags: tags
                .iter()
                .filter(|tag| self.tags.contains(&tag.id))
                .map(|tag| tag.label.clone())
                .collect(),
        })
    }
}

#[async_trait]
impl Solvers for Prowlarr {
    async fn proxies(&self) -> Result<Vec<KnownSolver>, Failure> {
        let proxies: Vec<ProxyResource> = self.servarr.read(PROXIES, THE_PROXIES).await?;
        let tags: Vec<Tag> = self.servarr.read(TAGS, THE_TAGS).await?;
        Ok(proxies
            .into_iter()
            .filter_map(|proxy| proxy.known(&tags))
            .collect())
    }

    async fn add_proxy(&self, solver: &SolverAt) -> Result<(), Failure> {
        if solver.protocol.0 != ApiKind::Flaresolverr.name() {
            return Err(self.servarr.unsupported(ASKS_FLARESOLVERR));
        }
        let tag = self.tagged(&solver.tag).await?;
        let body = serde_json::json!({
            "name": solver.name,
            "implementation": IMPLEMENTATION,
            "configContract": CONFIG_CONTRACT,
            "fields": [
                { "name": HOST, "value": solver.url },
                { "name": REQUEST_TIMEOUT, "value": TIMEOUT_SECONDS },
            ],
            "tags": [tag],
        });
        self.servarr
            .done(Method::Post, PROXIES, Some(body.to_string()))
            .await
    }
}

impl Prowlarr {
    async fn tagged(&self, label: &str) -> Result<i64, Failure> {
        let tags: Vec<Tag> = self.servarr.read(TAGS, THE_TAGS).await?;
        if let Some(tag) = tags.into_iter().find(|tag| tag.label == label) {
            return Ok(tag.id);
        }
        let response = self
            .servarr
            .ask(
                Method::Post,
                TAGS,
                Some(serde_json::json!({ "label": label }).to_string()),
            )
            .await?;
        let endpoint = self.servarr.endpoint();
        endpoint.expect_success(&response)?;
        endpoint.decode::<Tag>(&response, A_TAG).map(|tag| tag.id)
    }
}

#[cfg(test)]
mod tests;
