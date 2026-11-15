use std::sync::Arc;

use lemonfiber_fixtures::http::{Answer, Fake};
use lemonfiber_ports::http::Method;
use lemonfiber_ports::service::{Failure, KnownSolver, Protocol, SolverAt, Solvers};

use super::Prowlarr;

const TAGS: &str = r#"[{"id":1,"label":"movies"},{"id":4,"label":"lemonfiber-solver"}]"#;

const PROXIES: &str = r#"[
  {"id":3,"name":"FlareSolverr","implementation":"FlareSolverr","tags":[4],
   "fields":[{"name":"host","value":"http://flaresolverr:8191/"},{"name":"requestTimeout","value":60}]},
  {"id":5,"name":"Squid","implementation":"Http","tags":[1],
   "fields":[{"name":"host","value":"squid"},{"name":"port","value":3128}]},
  {"id":6,"name":"Unaddressed","implementation":"FlareSolverr","tags":[],"fields":[]}
]"#;

fn prowlarr(fake: &Arc<Fake>) -> Prowlarr {
    Prowlarr::new(
        fake.clone(),
        "http://127.0.0.1:9696",
        Arc::new("prowlarr-key".to_owned()),
    )
}

fn solver(protocol: &str) -> SolverAt {
    SolverAt {
        name: "Byparr".to_owned(),
        url: "http://byparr:8191".to_owned(),
        protocol: Protocol(protocol.to_owned()),
        tag: "lemonfiber-solver".to_owned(),
    }
}

fn posted(fake: &Fake, fragment: &str) -> String {
    fake.requests()
        .into_iter()
        .filter(|request| request.method == Method::Post && request.url.ends_with(fragment))
        .find_map(|request| request.body)
        .unwrap_or_default()
}

#[tokio::test]
async fn only_the_challenge_solvers_it_fetches_through_are_listed_with_their_tags() {
    let fake = Fake::by_path(vec![
        ("/indexerproxy", Answer::reply(200, PROXIES)),
        ("/tag", Answer::reply(200, TAGS)),
    ]);

    let known = prowlarr(&fake).proxies().await;

    assert_eq!(
        known.ok(),
        Some(vec![KnownSolver {
            id: "3".to_owned(),
            name: "FlareSolverr".to_owned(),
            url: "http://flaresolverr:8191".to_owned(),
            protocol: Protocol("flaresolverr".to_owned()),
            tags: vec!["lemonfiber-solver".to_owned()],
        }])
    );
}

#[tokio::test]
async fn a_solver_is_added_under_the_tag_it_already_holds() {
    let fake = Fake::by_route(vec![
        (Method::Get, "/tag", Answer::reply(200, TAGS)),
        (Method::Post, "/indexerproxy", Answer::reply(201, "{}")),
    ]);

    assert!(prowlarr(&fake)
        .add_proxy(&solver("flaresolverr"))
        .await
        .is_ok());

    assert!(posted(&fake, "/api/v1/tag").is_empty());
    let body = posted(&fake, "/api/v1/indexerproxy");
    for expected in [
        r#""name":"Byparr""#,
        r#""implementation":"FlareSolverr""#,
        r#""configContract":"FlareSolverrSettings""#,
        r#"{"name":"host","value":"http://byparr:8191"}"#,
        r#"{"name":"requestTimeout","value":60}"#,
        r#""tags":[4]"#,
    ] {
        assert!(body.contains(expected), "missing {expected}: {body}");
    }
}

#[tokio::test]
async fn a_tag_it_does_not_hold_is_made_before_the_solver_is_added_under_it() {
    let fake = Fake::by_route(vec![
        (
            Method::Get,
            "/tag",
            Answer::reply(200, r#"[{"id":1,"label":"movies"}]"#),
        ),
        (
            Method::Post,
            "/tag",
            Answer::reply(201, r#"{"id":9,"label":"lemonfiber-solver"}"#),
        ),
        (Method::Post, "/indexerproxy", Answer::reply(201, "{}")),
    ]);

    assert!(prowlarr(&fake)
        .add_proxy(&solver("flaresolverr"))
        .await
        .is_ok());

    assert_eq!(
        posted(&fake, "/api/v1/tag"),
        r#"{"label":"lemonfiber-solver"}"#
    );
    let body = posted(&fake, "/api/v1/indexerproxy");
    assert!(body.contains(r#""tags":[9]"#), "{body}");
}

#[tokio::test]
async fn a_tag_it_refuses_to_make_refuses_the_solver() {
    let fake = Fake::by_route(vec![
        (Method::Get, "/tag", Answer::reply(200, "[]")),
        (Method::Post, "/tag", Answer::reply(400, "label taken")),
    ]);

    assert!(prowlarr(&fake)
        .add_proxy(&solver("flaresolverr"))
        .await
        .is_err());
    assert!(!fake.asked_for("/indexerproxy"));
}

#[tokio::test]
async fn a_made_tag_it_answers_unreadably_refuses_the_solver() {
    let fake = Fake::by_route(vec![
        (Method::Get, "/tag", Answer::reply(200, "[]")),
        (Method::Post, "/tag", Answer::reply(201, "not a tag")),
    ]);

    let refused = prowlarr(&fake).add_proxy(&solver("flaresolverr")).await;

    assert!(
        matches!(&refused, Err(failure) if failure.to_string().contains("the tag it made could not be read")),
        "{refused:?}"
    );
    assert!(!fake.asked_for("/indexerproxy"));
}

#[tokio::test]
async fn a_solver_failing_its_test_is_refused() {
    let fake = Fake::by_route(vec![
        (Method::Get, "/tag", Answer::reply(200, TAGS)),
        (
            Method::Post,
            "/indexerproxy",
            Answer::reply(400, r#"[{"errorMessage":"Unable to connect"}]"#),
        ),
    ]);

    assert!(prowlarr(&fake)
        .add_proxy(&solver("flaresolverr"))
        .await
        .is_err());
}

#[tokio::test]
async fn a_solver_in_another_api_is_unsupported_and_nothing_is_asked() {
    let fake = Fake::silent();

    let refused = prowlarr(&fake).add_proxy(&solver("socks5")).await;

    assert!(
        matches!(refused, Err(Failure::Unsupported { .. })),
        "{refused:?}"
    );
    assert!(fake.requests().is_empty());
}
