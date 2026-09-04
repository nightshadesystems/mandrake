//! Boot environments, updates, and reboot over the API with the fake
//! drivers (ADR-0015).

#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::needless_pass_by_value,
    clippy::too_many_lines,
    clippy::panic
)]

use std::{sync::Arc, time::Duration};

use axum::{
    Router,
    body::{Body, to_bytes},
    http::{Method, Request, StatusCode, header},
};
use mandrake_core::shell::ScriptedRunner;
use mandraked::{
    app::{self, AppState},
    auth::SocketPeer,
    db::Db,
    drivers::Options,
    pkg::FakePkg,
};
use serde_json::{Value, json};
use tower::ServiceExt;

const PLAN: &str = include_str!("../testdata/pkg-update-nv.synthetic.txt");

struct Harness {
    app: Router,
    runner: Arc<ScriptedRunner>,
    pkg: Arc<FakePkg>,
}

async fn harness(plan: &str) -> Harness {
    let runner = Arc::new(ScriptedRunner::new());
    runner.ok("shutdown", "");
    let pkg = Arc::new(FakePkg::new().with_dry_run(plan));
    let options = Options::fake()
        .with_runner(runner.clone())
        .with_pkg(pkg.clone());
    let db = Db::open_in_memory().expect("db");
    let state = AppState::with_options(db, options).await.expect("state");
    Harness {
        app: app::router(state),
        runner,
        pkg,
    }
}

struct Reply {
    status: StatusCode,
    json: Value,
}

async fn root(app: &Router, method: Method, path: &str, body: Option<Value>) -> Reply {
    let mut b = Request::builder()
        .method(method)
        .uri(format!("/api/v1{path}"));
    if body.is_some() {
        b = b.header(header::CONTENT_TYPE, "application/json");
    }
    let mut req = b
        .body(body.map_or_else(Body::empty, |v| Body::from(v.to_string())))
        .expect("request");
    req.extensions_mut().insert(SocketPeer(Some(0)));
    let response = app.clone().oneshot(req).await.expect("response");
    let status = response.status();
    let bytes = to_bytes(response.into_body(), 1 << 20).await.expect("body");
    let json = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).unwrap_or(Value::Null)
    };
    Reply { status, json }
}

async fn wait_job(app: &Router, id: &str) -> Value {
    for _ in 0..500 {
        let r = root(app, Method::GET, &format!("/jobs/{id}"), None).await;
        let state = r.json["state"].as_str().unwrap_or("");
        if state == "succeeded" || state == "failed" {
            return r.json;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("job {id} did not finish");
}

#[tokio::test]
async fn boot_environments_follow_beadm_rules() {
    let h = harness("No updates available for this image.").await;
    let r = root(&h.app, Method::GET, "/system/boot-environments", None).await;
    assert_eq!(r.status, StatusCode::OK);
    let items = r.json["items"].as_array().unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["name"], "mandrake-0.1.0");
    assert_eq!(items[0]["active"], true);
    assert_eq!(items[0]["booted"], true);

    // Create, then the booted one cannot be destroyed, the new one can once inactive.
    let r = root(
        &h.app,
        Method::POST,
        "/system/boot-environments",
        Some(json!({ "name": "bad name!" })),
    )
    .await;
    assert_eq!(r.status, StatusCode::UNPROCESSABLE_ENTITY);
    let r = root(
        &h.app,
        Method::POST,
        "/system/boot-environments",
        Some(json!({ "name": "before-change" })),
    )
    .await;
    assert_eq!(r.status, StatusCode::CREATED, "{}", r.json);
    assert_eq!(r.json["active"], false);
    let r = root(
        &h.app,
        Method::POST,
        "/system/boot-environments",
        Some(json!({ "name": "before-change" })),
    )
    .await;
    assert_eq!(r.status, StatusCode::CONFLICT);

    let r = root(
        &h.app,
        Method::DELETE,
        "/system/boot-environments/mandrake-0.1.0",
        None,
    )
    .await;
    assert_eq!(r.status, StatusCode::CONFLICT, "booted");

    let r = root(
        &h.app,
        Method::POST,
        "/system/boot-environments/before-change/activate",
        None,
    )
    .await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.json);
    assert_eq!(r.json["active"], true);
    let r = root(
        &h.app,
        Method::DELETE,
        "/system/boot-environments/before-change",
        None,
    )
    .await;
    assert_eq!(r.status, StatusCode::CONFLICT, "active");

    let r = root(
        &h.app,
        Method::POST,
        "/system/boot-environments/mandrake-0.1.0/activate",
        None,
    )
    .await;
    assert_eq!(r.status, StatusCode::OK);
    let r = root(
        &h.app,
        Method::DELETE,
        "/system/boot-environments/before-change",
        None,
    )
    .await;
    assert_eq!(r.status, StatusCode::NO_CONTENT);
    let r = root(
        &h.app,
        Method::GET,
        "/system/boot-environments/before-change",
        None,
    )
    .await;
    assert_eq!(r.status, StatusCode::NOT_FOUND);

    let r = root(&h.app, Method::GET, "/audit?limit=50", None).await;
    let actions: Vec<&str> = r.json["items"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|a| a["action"].as_str())
        .collect();
    for a in ["be.create", "be.activate", "be.delete"] {
        assert!(actions.contains(&a), "{a} in {actions:?}");
    }
}

#[tokio::test]
async fn update_check_apply_and_reboot() {
    let h = harness(PLAN).await;

    // Nothing planned yet: apply is refused.
    let r = root(&h.app, Method::GET, "/system/updates", None).await;
    assert_eq!(r.status, StatusCode::OK);
    assert!(r.json["plan"].is_null());
    let r = root(&h.app, Method::POST, "/system/updates/apply", None).await;
    assert_eq!(r.status, StatusCode::UNPROCESSABLE_ENTITY);

    // Check: refresh, dry run, plan stored with the BE name from the incorporation.
    let r = root(&h.app, Method::POST, "/system/updates/check", None).await;
    assert_eq!(r.status, StatusCode::ACCEPTED, "{}", r.json);
    let job = wait_job(&h.app, r.json["id"].as_str().unwrap()).await;
    assert_eq!(job["state"], "succeeded", "{job}");
    assert_eq!(h.pkg.refreshes(), 1);
    let r = root(&h.app, Method::GET, "/system/updates", None).await;
    let plan = &r.json["plan"];
    assert_eq!(plan["packages"].as_array().unwrap().len(), 5);
    assert_eq!(plan["boot_environment"], "mandrake-0.2.0");
    assert_eq!(plan["mandrake_version"], "0.2.0");
    assert_eq!(plan["reboot_required"], true);
    assert_eq!(r.json["checking"], false);

    // Apply: pkg updates into the planned BE; the previous BE is remembered.
    let r = root(&h.app, Method::POST, "/system/updates/apply", None).await;
    assert_eq!(r.status, StatusCode::ACCEPTED, "{}", r.json);
    let job = wait_job(&h.app, r.json["id"].as_str().unwrap()).await;
    assert_eq!(job["state"], "succeeded", "{job}");
    assert_eq!(h.pkg.updated(), vec!["mandrake-0.2.0".to_owned()]);
    let r = root(&h.app, Method::GET, "/system/updates", None).await;
    assert!(r.json["plan"].is_null(), "{}", r.json);
    assert_eq!(r.json["applied_boot_environment"], "mandrake-0.2.0");
    assert_eq!(r.json["previous_boot_environment"], "mandrake-0.1.0");
    assert_eq!(r.json["applying"], false);

    // A second check finds nothing.
    let r = root(&h.app, Method::POST, "/system/updates/check", None).await;
    let job = wait_job(&h.app, r.json["id"].as_str().unwrap()).await;
    assert_eq!(job["message"], "up to date");
    let r = root(&h.app, Method::POST, "/system/updates/apply", None).await;
    assert_eq!(r.status, StatusCode::UNPROCESSABLE_ENTITY);

    // Reboot: audited, then shutdown through pfexec after the grace.
    let r = root(&h.app, Method::POST, "/system/reboot", None).await;
    assert_eq!(r.status, StatusCode::ACCEPTED, "{}", r.json);
    let second = root(&h.app, Method::POST, "/system/reboot", None).await;
    assert_eq!(second.status, StatusCode::CONFLICT);
    let job = wait_job(&h.app, r.json["id"].as_str().unwrap()).await;
    assert_eq!(job["state"], "succeeded", "{job}");
    let lines = h.runner.lines();
    assert_eq!(lines, vec!["pfexec shutdown -y -g 0 -i 6".to_owned()]);
    let r = root(&h.app, Method::GET, "/audit?limit=50", None).await;
    let actions: Vec<&str> = r.json["items"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|a| a["action"].as_str())
        .collect();
    for a in [
        "system.update_check",
        "system.update_apply",
        "system.reboot",
    ] {
        assert!(actions.contains(&a), "{a} in {actions:?}");
    }
}
