use std::sync::Arc;
use std::time::{Duration, SystemTime};

use lemonfiber_fixtures::http::{Answer, Fake};
use lemonfiber_ports::http::Method;
use lemonfiber_ports::service::{
    Aggregator, Aggregators, AppSync, Application, ApplicationKind, Failure, IndexerUse, Indexers,
    Limits, RegisteredApplication,
};

use super::Prowlarr;

fn now() -> SystemTime {
    SystemTime::UNIX_EPOCH + Duration::from_secs(1_786_968_000)
}

fn aggregator(fake: &Arc<Fake>) -> Prowlarr {
    Prowlarr::new(
        fake.clone(),
        "http://127.0.0.1:9696",
        Arc::new("prowlarr-key".to_owned()),
    )
}

fn application(name: &str, kind: ApplicationKind, base_url: &str) -> Application {
    Application {
        name: name.to_owned(),
        kind,
        indexer_url: "http://prowlarr:9696".to_owned(),
        base_url: base_url.to_owned(),
        api_key: "arr-key".to_owned(),
    }
}

#[tokio::test]
async fn an_application_is_posted_to_its_v1_endpoint_with_the_key() {
    let fake = Fake::always(Answer::reply(201, ""));
    let tv = application("Sonarr", ApplicationKind::Tv, "http://sonarr:8989");
    assert!(aggregator(&fake).register_application(&tv).await.is_ok());

    let sent = fake.request();
    assert!(sent
        .as_ref()
        .is_some_and(|request| request.url.ends_with("/api/v1/applications")));
    assert!(sent.as_ref().is_some_and(|request| request
        .headers
        .iter()
        .any(|(name, value)| name == "X-Api-Key" && value == "prowlarr-key")));
    assert!(sent.is_some_and(|request| request
        .headers
        .iter()
        .any(|(name, value)| name == "Content-Type" && value == "application/json")));
}

#[tokio::test]
async fn a_television_curator_application_carries_its_schema_and_television_categories() {
    let fake = Fake::always(Answer::reply(201, ""));
    let tv = application("Sonarr", ApplicationKind::Tv, "http://sonarr:8989");
    assert!(aggregator(&fake).register_application(&tv).await.is_ok());

    let body = fake
        .request()
        .and_then(|request| request.body)
        .unwrap_or_default();
    for expected in [
        r#""syncLevel":"fullSync""#,
        r#""implementation":"Sonarr""#,
        r#""configContract":"SonarrSettings""#,
        r#""name":"prowlarrUrl""#,
        "http://prowlarr:9696",
        r#""name":"baseUrl""#,
        "http://sonarr:8989",
        r#""name":"apiKey""#,
        "arr-key",
        r#""name":"syncCategories""#,
        "5000",
    ] {
        assert!(
            body.contains(expected),
            "Sonarr body missing {expected}: {body}"
        );
    }
    assert!(
        !body.contains("2000"),
        "no movie category in a TV sync: {body}"
    );
}

#[tokio::test]
async fn a_movie_curator_application_carries_its_schema_and_movie_categories() {
    let fake = Fake::always(Answer::reply(201, ""));
    let movies = application("Radarr", ApplicationKind::Movies, "http://radarr:7878");
    assert!(aggregator(&fake)
        .register_application(&movies)
        .await
        .is_ok());

    let body = fake
        .request()
        .and_then(|request| request.body)
        .unwrap_or_default();
    for expected in [
        r#""implementation":"Radarr""#,
        r#""configContract":"RadarrSettings""#,
        "2000",
    ] {
        assert!(
            body.contains(expected),
            "Radarr body missing {expected}: {body}"
        );
    }
}

#[tokio::test]
async fn a_music_curator_application_carries_its_schema_and_music_categories() {
    let fake = Fake::always(Answer::reply(201, ""));
    let music = application("Lidarr", ApplicationKind::Music, "http://lidarr:8686");
    assert!(aggregator(&fake).register_application(&music).await.is_ok());

    let body = fake
        .request()
        .and_then(|request| request.body)
        .unwrap_or_default();
    for expected in [
        r#""implementation":"Lidarr""#,
        r#""configContract":"LidarrSettings""#,
        "3000",
    ] {
        assert!(
            body.contains(expected),
            "Lidarr body missing {expected}: {body}"
        );
    }
}

#[tokio::test]
async fn a_rejected_application_registration_is_refused() {
    let fake = Fake::always(Answer::reply(400, "unknown implementation"));
    let tv = application("Sonarr", ApplicationKind::Tv, "http://sonarr:8989");
    assert!(matches!(
        aggregator(&fake).register_application(&tv).await,
        Err(Failure::Refused { .. })
    ));
}

#[tokio::test]
async fn a_registration_with_no_answer_is_unavailable() {
    let fake = Fake::always(Answer::Silent);
    let tv = application("Sonarr", ApplicationKind::Tv, "http://sonarr:8989");
    assert!(matches!(
        aggregator(&fake).register_application(&tv).await,
        Err(Failure::Unavailable { .. })
    ));
}

#[tokio::test]
async fn the_applications_are_read_back_by_their_base_url() {
    let fake = Fake::always(Answer::reply(
        200,
        r#"[{"id":3,"name":"Sonarr","fields":[{"name":"baseUrl","value":"http://sonarr:8989"},{"name":"apiKey","value":"x"}]}]"#,
    ));
    let applications = aggregator(&fake).applications().await;
    assert_eq!(
        applications.ok(),
        Some(vec![RegisteredApplication {
            id: "3".to_owned(),
            base_url: "http://sonarr:8989".to_owned(),
        }])
    );

    let sent = fake.request();
    assert!(sent.is_some_and(|request| request.url.ends_with("/api/v1/applications")));
}

#[tokio::test]
async fn an_application_that_names_no_base_url_is_left_out_rather_than_guessed() {
    let fake = Fake::always(Answer::reply(
        200,
        r#"[{"id":3,"fields":[{"name":"baseUrl","value":"http://sonarr:8989"}]},{"id":4,"fields":[{"name":"apiKey","value":"x"}]}]"#,
    ));
    let applications = aggregator(&fake)
        .applications()
        .await
        .ok()
        .unwrap_or_default();
    assert_eq!(
        applications.len(),
        1,
        "the entry with no address is left out"
    );
    assert!(applications
        .iter()
        .any(|app| app.id == "3" && app.base_url == "http://sonarr:8989"));
}

#[tokio::test]
async fn an_unreadable_application_list_is_refused() {
    let fake = Fake::always(Answer::reply(200, "not an array"));
    assert!(matches!(
        aggregator(&fake).applications().await,
        Err(Failure::Refused { .. })
    ));
}

#[tokio::test]
async fn an_application_listing_that_is_refused_is_unauthorised() {
    let fake = Fake::always(Answer::reply(401, ""));
    assert!(matches!(
        aggregator(&fake).applications().await,
        Err(Failure::Unauthorised { .. })
    ));
}

#[tokio::test]
async fn an_application_listing_with_no_answer_is_unavailable() {
    let fake = Fake::always(Answer::Silent);
    assert!(matches!(
        aggregator(&fake).applications().await,
        Err(Failure::Unavailable { .. })
    ));
}

const INDEXERS: &str = r#"[
    {"id":1,"name":"Fast Indexer","enable":true,"status":null},
    {"id":2,"name":"Old Indexer","enable":false,"status":null}
]"#;

const STANDINGS: &str = r#"[{"indexerId":2,"disabledTill":"2026-08-16T20:00:00Z"}]"#;

const COUNTS: &str = r#"{"indexers":[
    {"indexerId":1,"numberOfQueries":142,"numberOfGrabs":7,
     "numberOfFailedQueries":3,"numberOfFailedGrabs":1}
]}"#;

#[tokio::test]
async fn indexer_use_is_read_from_the_aggregator_rather_than_from_the_indexers() {
    let fake = Fake::in_turn(vec![
        Answer::reply(200, INDEXERS),
        Answer::reply(200, STANDINGS),
        Answer::reply(200, COUNTS),
    ]);
    let indexers = aggregator(&fake).indexers(now()).await.unwrap_or_default();

    assert_eq!(
        indexers,
        vec![
            IndexerUse {
                name: "Fast Indexer".to_owned(),
                enabled: true,
                queries: 142,
                failed_queries: 3,
                grabs: 7,
                failed_grabs: 1,
                rested_until: None,
                limits: None,
                searched_from: None,
                grabbed_from: None,
            },
            IndexerUse {
                name: "Old Indexer".to_owned(),
                enabled: false,
                queries: 0,
                failed_queries: 0,
                grabs: 0,
                failed_grabs: 0,
                rested_until: Some("2026-08-16T20:00:00Z".to_owned()),
                limits: None,
                searched_from: None,
                grabbed_from: None,
            },
        ]
    );

    assert!(fake.asked_for("/api/v1/indexerstatus"));
    assert!(fake.asked_for("/api/v1/indexerstats?startDate=2026-08-16T12:00:00"));
    assert!(!fake.asked_for("/api/v1/history/since"));
}

const CAPPED: &str = r#"[{"id":1,"name":"Fast Indexer","enable":true,"fields":[
    {"name":"baseSettings.queryLimit","value":100},
    {"name":"baseSettings.grabLimit","value":null},
    {"name":"baseSettings.limitsUnit","value":1}
]}]"#;

const HOURLY_COUNTS: &str = r#"{"indexers":[{"indexerId":1,"numberOfQueries":40,
    "numberOfRssQueries":60,"numberOfGrabs":2}]}"#;

const HISTORY: &str = r#"[
    {"indexerId":1,"date":"2026-08-17T11:20:00Z","eventType":"indexerQuery"},
    {"indexerId":1,"date":"2026-08-17T11:05:30Z","eventType":"indexerRss"},
    {"indexerId":1,"date":"2026-08-17T11:40:00Z","eventType":"releaseGrabbed"},
    {"indexerId":1,"date":"not a moment","eventType":"indexerQuery"},
    {"indexerId":1,"date":"2026-08-17T11:45:00Z","eventType":2},
    {"indexerId":1,"date":"2026-08-17T11:46:00Z","eventType":3},
    {"indexerId":1,"date":"2026-08-17T11:50:00Z","eventType":1},
    {"indexerId":1,"date":"2026-08-17T11:47:00Z","eventType":4},
    {"indexerId":1,"date":"2026-08-17T11:48:00Z","eventType":"indexerAuth"},
    {"indexerId":1,"date":"2026-08-17T11:55:00Z"},
    {"indexerId":1,"date":"2026-08-17T10:30:00Z","eventType":"indexerQuery"},
    {"indexerId":9,"date":"2026-08-17T11:10:00Z","eventType":"indexerQuery"}
]"#;

#[tokio::test]
async fn a_recorded_cap_is_read_with_the_window_and_the_calls_it_is_counted_over() {
    let fake = Fake::in_turn(vec![
        Answer::reply(200, CAPPED),
        Answer::reply(200, "[]"),
        Answer::reply(200, HOURLY_COUNTS),
        Answer::reply(200, HISTORY),
    ]);
    let indexers = aggregator(&fake).indexers(now()).await.unwrap_or_default();

    assert_eq!(
        indexers.first().map(|indexer| indexer.limits),
        Some(Some(Limits {
            queries: Some(100),
            grabs: None,
            window: Duration::from_secs(3600),
        }))
    );
    assert_eq!(indexers.first().map(|indexer| indexer.queries), Some(100));
    assert!(fake.asked_for("/api/v1/indexerstats?startDate=2026-08-17T11:00:00"));
    assert!(fake.asked_for("/api/v1/history/since?date=2026-08-17T11:00:00"));
    assert_eq!(
        indexers.first().and_then(|indexer| indexer.searched_from),
        Some(SystemTime::UNIX_EPOCH + Duration::from_secs(1_786_964_730))
    );
    assert_eq!(
        indexers.first().and_then(|indexer| indexer.grabbed_from),
        Some(SystemTime::UNIX_EPOCH + Duration::from_secs(1_786_966_800))
    );
}

#[tokio::test]
async fn indexers_that_cannot_be_read_are_a_failure_rather_than_an_empty_list() {
    for answers in [
        vec![Answer::reply(200, "not an array")],
        vec![Answer::reply(200, INDEXERS), Answer::reply(200, "{}")],
        vec![
            Answer::reply(200, INDEXERS),
            Answer::reply(200, STANDINGS),
            Answer::reply(200, "not an object"),
        ],
    ] {
        let fake = Fake::in_turn(answers);
        assert!(matches!(
            aggregator(&fake).indexers(now()).await,
            Err(Failure::Refused { .. })
        ));
    }
}

const HELD: &str = r#"[{"id":3,"name":"Sonarr","fields":[{"name":"baseUrl","value":"http://sonarr:8989"},{"name":"apiKey","value":"********"}]}]"#;

fn held() -> RegisteredApplication {
    RegisteredApplication {
        id: "3".to_owned(),
        base_url: "http://sonarr:8989".to_owned(),
    }
}

#[tokio::test]
async fn an_application_is_tested_as_the_aggregator_holds_it_with_its_key_masked() {
    let fake = Fake::by_rules(vec![
        (
            Some(Method::Post),
            "/applications/test",
            Answer::reply(200, ""),
        ),
        (None, "/applications", Answer::reply(200, HELD)),
    ]);

    assert!(aggregator(&fake).test_application(&held()).await.is_ok());
    let sent = fake.request();
    assert!(sent
        .as_ref()
        .is_some_and(|request| request.url.ends_with("/api/v1/applications/test")));
    assert!(sent
        .and_then(|request| request.body)
        .is_some_and(|body| body.contains("********")));
}

#[tokio::test]
async fn an_application_whose_stored_key_fails_its_test_is_refused() {
    let fake = Fake::by_rules(vec![
        (
            Some(Method::Post),
            "/applications/test",
            Answer::reply(400, "[]"),
        ),
        (None, "/applications", Answer::reply(200, HELD)),
    ]);

    assert!(aggregator(&fake).test_application(&held()).await.is_err());
}

#[tokio::test]
async fn an_application_is_rekeyed_in_place_with_only_its_key_changed() {
    let fake = Fake::by_rules(vec![
        (
            Some(Method::Put),
            "/applications/3",
            Answer::reply(202, "{}"),
        ),
        (None, "/applications", Answer::reply(200, HELD)),
    ]);

    assert!(aggregator(&fake)
        .rekey_application(&held(), "the-new-key")
        .await
        .is_ok());
    let sent = fake.request();
    assert!(sent
        .as_ref()
        .is_some_and(|request| request.url.ends_with("/api/v1/applications/3")));
    let body = sent.and_then(|request| request.body).unwrap_or_default();
    assert!(
        body.contains(r#"{"name":"apiKey","value":"the-new-key"}"#),
        "{body}"
    );
    assert!(body.contains(r#""value":"http://sonarr:8989""#), "{body}");
}

#[tokio::test]
async fn an_application_the_aggregator_no_longer_lists_is_refused_by_name() {
    let fake = Fake::always(Answer::reply(200, "[]"));

    let refused = aggregator(&fake)
        .rekey_application(&held(), "the-new-key")
        .await;
    assert!(
        matches!(&refused, Err(failure) if failure.to_string().contains("no longer holds the application it listed as 3")),
        "{refused:?}"
    );
}

#[tokio::test]
async fn prowlarr_reads_from_no_aggregator_and_nothing_is_asked() {
    let fake = Fake::silent();
    let prowlarr = aggregator(&fake);
    let aggregator = Aggregator {
        name: "NZBHydra2".to_owned(),
        url: "http://nzbhydra2:5076".to_owned(),
        key: "a-key".to_owned(),
    };
    assert!(matches!(
        prowlarr.aggregators().await,
        Err(Failure::Unsupported { .. })
    ));
    assert!(matches!(
        prowlarr.add_aggregator(&aggregator).await,
        Err(Failure::Unsupported { .. })
    ));
    assert!(fake.requests().is_empty());
}
