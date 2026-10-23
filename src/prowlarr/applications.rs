use async_trait::async_trait;
use lemonfiber_ports::http::Method;
use lemonfiber_ports::service::{
    AppSync, Application, ApplicationKind, Failure, RegisteredApplication,
};
use serde::Deserialize;
use serde_json::{Map, Value};

use super::Prowlarr;

const APPLICATIONS: &str = "/applications";

const TEST: &str = "/applications/test";

const THE_APPLICATIONS: &str = "the applications";

const BASE_URL: &str = "baseUrl";

const API_KEY: &str = "apiKey";

#[derive(Deserialize)]
struct ApplicationResource {
    id: i64,
    #[serde(default)]
    fields: Vec<ApplicationField>,
}

#[derive(Deserialize)]
struct ApplicationField {
    name: String,
    #[serde(default)]
    value: Value,
}

impl ApplicationResource {
    fn registered(self) -> Option<RegisteredApplication> {
        let base_url = self
            .fields
            .iter()
            .find(|field| field.name == BASE_URL)?
            .value
            .as_str()?
            .to_owned();
        Some(RegisteredApplication {
            id: self.id.to_string(),
            base_url,
        })
    }
}

#[async_trait]
impl AppSync for Prowlarr {
    async fn register_application(&self, application: &Application) -> Result<(), Failure> {
        self.servarr
            .done(
                Method::Post,
                APPLICATIONS,
                Some(application_body(application)),
            )
            .await
    }

    async fn applications(&self) -> Result<Vec<RegisteredApplication>, Failure> {
        let applications: Vec<ApplicationResource> =
            self.servarr.read(APPLICATIONS, THE_APPLICATIONS).await?;
        Ok(applications
            .into_iter()
            .filter_map(ApplicationResource::registered)
            .collect())
    }

    async fn test_application(&self, held: &RegisteredApplication) -> Result<(), Failure> {
        let resource = self.held_resource(held).await?;
        self.servarr
            .done(
                Method::Post,
                TEST,
                Some(Value::Object(resource).to_string()),
            )
            .await
    }

    async fn rekey_application(
        &self,
        held: &RegisteredApplication,
        key: &str,
    ) -> Result<(), Failure> {
        let mut resource = self.held_resource(held).await?;
        let id = resource
            .get("id")
            .and_then(Value::as_i64)
            .unwrap_or_default();
        for field in resource
            .get_mut("fields")
            .and_then(Value::as_array_mut)
            .into_iter()
            .flatten()
            .filter_map(Value::as_object_mut)
            .filter(|field| field.get("name").and_then(Value::as_str) == Some(API_KEY))
        {
            field.insert("value".to_owned(), Value::from(key));
        }
        self.servarr
            .done(
                Method::Put,
                &format!("{APPLICATIONS}/{id}"),
                Some(Value::Object(resource).to_string()),
            )
            .await
    }
}

impl Prowlarr {
    async fn held_resource(
        &self,
        held: &RegisteredApplication,
    ) -> Result<Map<String, Value>, Failure> {
        let listed: Vec<Map<String, Value>> =
            self.servarr.read(APPLICATIONS, THE_APPLICATIONS).await?;
        listed
            .into_iter()
            .find(|one| {
                one.get("id")
                    .and_then(Value::as_i64)
                    .is_some_and(|id| id.to_string() == held.id)
            })
            .ok_or_else(|| {
                self.servarr.endpoint().refused(&format!(
                    "Prowlarr no longer holds the application it listed as {}",
                    held.id
                ))
            })
    }
}

fn application_body(application: &Application) -> String {
    let (implementation, config_contract, categories) = schema(application.kind);
    serde_json::json!({
        "syncLevel": "fullSync",
        "name": application.name,
        "implementation": implementation,
        "configContract": config_contract,
        "fields": [
            { "name": "prowlarrUrl", "value": application.indexer_url },
            { "name": BASE_URL, "value": application.base_url },
            { "name": API_KEY, "value": application.api_key },
            { "name": "syncCategories", "value": categories },
        ],
    })
    .to_string()
}

fn schema(kind: ApplicationKind) -> (&'static str, &'static str, &'static [u32]) {
    match kind {
        ApplicationKind::Tv => (
            "Sonarr",
            "SonarrSettings",
            &[5000, 5010, 5020, 5030, 5040, 5045, 5050],
        ),
        ApplicationKind::Movies => (
            "Radarr",
            "RadarrSettings",
            &[2000, 2010, 2020, 2030, 2040, 2045, 2050, 2060, 2070, 2080],
        ),
        ApplicationKind::Music => (
            "Lidarr",
            "LidarrSettings",
            &[3000, 3010, 3020, 3030, 3040, 3050, 3060],
        ),
    }
}
