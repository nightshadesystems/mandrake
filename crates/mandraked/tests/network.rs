//! Network routes against the fake driver.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::missing_panics_doc,
    clippy::needless_pass_by_value,
    clippy::too_many_lines
)]

use std::sync::Arc;

use axum::{
    Router,
    body::{Body, to_bytes},
    http::{Method, Request, StatusCode, header},
};
use mandrake_net::FakeNet;
use mandraked::{
    app::{self, AppState},
    auth::SocketPeer,
    db::Db,
    drivers::Options,
};
use serde_json::{Value, json};
use tower::ServiceExt;

/// The address the fake host's daemon listens on.
const MANAGEMENT: &str = "192.168.1.10";

async fn harness() -> Router {
    let net = Arc::new(FakeNet::typical());
    let db = Db::open_in_memory().expect("db");
    let options = Options::fake()
        .with_net(net)
        .with_listen(MANAGEMENT.parse().unwrap());
    let state = AppState::with_options(db, options).await.expect("state");
    app::router(state)
}

struct Reply {
    status: StatusCode,
    json: Value,
}

async fn call(
    app: &Router,
    method: Method,
    path: &str,
    body: Option<Value>,
    host: Option<&str>,
) -> Reply {
    let mut b = Request::builder()
        .method(method)
        .uri(format!("/api/v1{path}"));
    if body.is_some() {
        b = b.header(header::CONTENT_TYPE, "application/json");
    }
    if let Some(h) = host {
        b = b.header(header::HOST, h);
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

async fn root(app: &Router, method: Method, path: &str, body: Option<Value>) -> Reply {
    call(app, method, path, body, None).await
}

fn items(v: &Value) -> &Vec<Value> {
    v["items"].as_array().expect("items")
}

fn find<'a>(v: &'a Value, key: &str, value: &str) -> &'a Value {
    items(v)
        .iter()
        .find(|i| i[key] == value)
        .unwrap_or_else(|| panic!("no item with {key} = {value}: {v}"))
}

async fn as_viewer(app: &Router, method: Method, path: &str, body: Option<Value>) -> Reply {
    root(
        app,
        Method::POST,
        "/users",
        Some(json!({ "username": "vera", "password": "viewer password!", "role": "viewer" })),
    )
    .await;
    let login = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/auth/login")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({ "username": "vera", "password": "viewer password!" }).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    let cookie = login
        .headers()
        .get(header::SET_COOKIE)
        .unwrap()
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_owned();
    let mut b = Request::builder()
        .method(method)
        .uri(format!("/api/v1{path}"))
        .header(header::COOKIE, cookie)
        .header("x-mandrake-request", "1");
    if body.is_some() {
        b = b.header(header::CONTENT_TYPE, "application/json");
    }
    let req = b
        .body(body.map_or_else(Body::empty, |v| Body::from(v.to_string())))
        .unwrap();
    let response = app.clone().oneshot(req).await.unwrap();
    let status = response.status();
    let bytes = to_bytes(response.into_body(), 1 << 20).await.unwrap();
    Reply {
        status,
        json: if bytes.is_empty() {
            Value::Null
        } else {
            serde_json::from_slice(&bytes).unwrap_or(Value::Null)
        },
    }
}

#[tokio::test]
async fn links_list_with_stable_ids_and_protection() {
    let app = harness().await;
    let r = root(&app, Method::GET, "/network/links", None).await;
    assert_eq!(r.status, StatusCode::OK);
    assert_eq!(items(&r.json).len(), 4);
    let mgmt = find(&r.json, "name", "e1000g0");
    assert_eq!(mgmt["kind"], "phys");
    assert_eq!(mgmt["state"], "up");
    assert_eq!(mgmt["protected"], true);
    assert_eq!(mgmt["mac"], "00:0c:29:ab:cd:ef");
    assert_eq!(mgmt["speed_mbps"], 1000);
    assert!(mgmt.get("over").is_none());
    assert_eq!(find(&r.json, "name", "e1000g1")["protected"], false);
    assert_eq!(find(&r.json, "name", "e1000g3")["state"], "down");

    let id = mgmt["id"].as_str().unwrap().to_owned();
    let again = root(&app, Method::GET, &format!("/network/links/{id}"), None).await;
    assert_eq!(again.status, StatusCode::OK);
    assert_eq!(again.json["id"], id);
    assert_eq!(again.json["name"], "e1000g0");

    let viewer = as_viewer(&app, Method::GET, "/network/links", None).await;
    assert_eq!(viewer.status, StatusCode::OK);
    let denied = as_viewer(
        &app,
        Method::POST,
        "/network/etherstubs",
        Some(json!({ "name": "stub0" })),
    )
    .await;
    assert_eq!(denied.status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn topology_lifecycle() {
    let app = harness().await;

    // A port on the management path cannot join an aggregation.
    let r = root(
        &app,
        Method::POST,
        "/network/aggrs",
        Some(json!({ "name": "aggr0", "ports": ["e1000g0", "e1000g1"] })),
    )
    .await;
    assert_eq!(r.status, StatusCode::FORBIDDEN, "{}", r.json);
    assert!(
        r.json["type"].as_str().unwrap().ends_with("/protected"),
        "{}",
        r.json
    );

    let r = root(
        &app,
        Method::POST,
        "/network/aggrs",
        Some(json!({ "name": "aggr0", "ports": ["e1000g1", "e1000g2"], "metadata": { "display_name": "Uplink" } })),
    )
    .await;
    assert_eq!(r.status, StatusCode::CREATED, "{}", r.json);
    let aggr = r.json.clone();
    assert_eq!(aggr["kind"], "aggr");
    assert_eq!(aggr["over"], json!(["e1000g1", "e1000g2"]));
    assert_eq!(aggr["aggr"]["policy"], "L4");
    assert_eq!(aggr["aggr"]["lacp_mode"], "active");
    assert_eq!(aggr["speed_mbps"], 2000);
    assert_eq!(aggr["metadata"]["display_name"], "Uplink");
    let aggr_id = aggr["id"].as_str().unwrap().to_owned();

    // Bad inputs are 422.
    for (path, body) in [
        (
            "/network/aggrs",
            json!({ "name": "bad-name0", "ports": ["e1000g3"] }),
        ),
        ("/network/aggrs", json!({ "name": "aggr1", "ports": [] })),
        (
            "/network/aggrs",
            json!({ "name": "aggr1", "ports": ["e1000g3"], "policy": "L5" }),
        ),
        (
            "/network/vlans",
            json!({ "name": "vlan1", "vid": 5000, "over": "aggr0" }),
        ),
        (
            "/network/vlans",
            json!({ "name": "vlan1", "vid": 5, "over": "nope0" }),
        ),
        (
            "/network/vnics",
            json!({ "name": "vnic9", "over": "aggr0", "mac": "zz" }),
        ),
        (
            "/network/vnics",
            json!({ "name": "vnic9", "over": "aggr0", "mtu": 100 }),
        ),
    ] {
        let r = root(&app, Method::POST, path, Some(body.clone())).await;
        assert_eq!(
            r.status,
            StatusCode::UNPROCESSABLE_ENTITY,
            "{path} {body}: {}",
            r.json
        );
    }

    let r = root(
        &app,
        Method::POST,
        "/network/vlans",
        Some(json!({ "name": "vlan10", "vid": 10, "over": "aggr0" })),
    )
    .await;
    assert_eq!(r.status, StatusCode::CREATED, "{}", r.json);
    assert_eq!(r.json["vid"], 10);
    assert_eq!(r.json["over"], json!(["aggr0"]));
    let vlan_id = r.json["id"].as_str().unwrap().to_owned();

    let r = root(
        &app,
        Method::POST,
        "/network/etherstubs",
        Some(json!({ "name": "stub0" })),
    )
    .await;
    assert_eq!(r.status, StatusCode::CREATED, "{}", r.json);
    let stub_id = r.json["id"].as_str().unwrap().to_owned();
    let dup = root(
        &app,
        Method::POST,
        "/network/etherstubs",
        Some(json!({ "name": "stub0" })),
    )
    .await;
    assert_eq!(dup.status, StatusCode::CONFLICT);

    let r = root(
        &app,
        Method::POST,
        "/network/vnics",
        Some(json!({ "name": "vnic0", "over": "stub0", "vid": 20, "mac": "2:8:20:a1:b2:c3" })),
    )
    .await;
    assert_eq!(r.status, StatusCode::CREATED, "{}", r.json);
    assert_eq!(r.json["mac"], "02:08:20:a1:b2:c3");
    assert_eq!(r.json["mac_mode"], "fixed");
    assert_eq!(r.json["vid"], 20);
    assert_eq!(r.json["mtu"], 9000);
    let vnic_id = r.json["id"].as_str().unwrap().to_owned();

    // MTU and metadata patch.
    let r = root(
        &app,
        Method::PATCH,
        &format!("/network/links/{vnic_id}"),
        Some(json!({ "mtu": 1400, "metadata": { "tags": ["lab"] } })),
    )
    .await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.json);
    assert_eq!(r.json["mtu"], 1400);
    assert_eq!(r.json["metadata"]["tags"], json!(["lab"]));
    let r = root(
        &app,
        Method::PATCH,
        &format!("/network/links/{vnic_id}"),
        Some(json!({ "mtu": 64 })),
    )
    .await;
    assert_eq!(r.status, StatusCode::UNPROCESSABLE_ENTITY);

    // Deletes: busy while something sits over, wrong kind is 404.
    let r = root(
        &app,
        Method::DELETE,
        &format!("/network/aggrs/{aggr_id}"),
        None,
    )
    .await;
    assert_eq!(r.status, StatusCode::CONFLICT, "{}", r.json);
    assert!(
        r.json["type"].as_str().unwrap().ends_with("/busy"),
        "{}",
        r.json
    );
    let r = root(
        &app,
        Method::DELETE,
        &format!("/network/vlans/{aggr_id}"),
        None,
    )
    .await;
    assert_eq!(r.status, StatusCode::NOT_FOUND);
    let r = root(
        &app,
        Method::DELETE,
        &format!("/network/etherstubs/{stub_id}"),
        None,
    )
    .await;
    assert_eq!(r.status, StatusCode::CONFLICT);

    for (path, id) in [
        ("vlans", &vlan_id),
        ("aggrs", &aggr_id),
        ("vnics", &vnic_id),
        ("etherstubs", &stub_id),
    ] {
        let r = root(&app, Method::DELETE, &format!("/network/{path}/{id}"), None).await;
        assert_eq!(r.status, StatusCode::NO_CONTENT, "{path}: {}", r.json);
    }
    let r = root(&app, Method::GET, "/network/links", None).await;
    assert_eq!(items(&r.json).len(), 4);
    let r = root(
        &app,
        Method::GET,
        &format!("/network/links/{vnic_id}"),
        None,
    )
    .await;
    assert_eq!(r.status, StatusCode::NOT_FOUND);

    // The physical port on the management path refuses delete outright.
    let mgmt_id = find(
        &root(&app, Method::GET, "/network/links", None).await.json,
        "name",
        "e1000g0",
    )["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let r = root(
        &app,
        Method::DELETE,
        &format!("/network/vnics/{mgmt_id}"),
        None,
    )
    .await;
    assert_eq!(r.status, StatusCode::NOT_FOUND);

    let audit = root(&app, Method::GET, "/audit?limit=50", None).await;
    let actions: Vec<&str> = audit.json["items"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|a| a["action"].as_str())
        .collect();
    for action in [
        "aggr.create",
        "vlan.create",
        "vnic.create",
        "link.update",
        "aggr.delete",
    ] {
        assert!(
            actions.contains(&action),
            "{action} missing from {actions:?}"
        );
    }
}

#[tokio::test]
async fn addresses() {
    let app = harness().await;
    let r = root(&app, Method::GET, "/network/addresses", None).await;
    assert_eq!(r.status, StatusCode::OK);
    assert_eq!(items(&r.json).len(), 3);
    let mgmt = find(&r.json, "name", "e1000g0/v4");
    assert_eq!(mgmt["interface"], "e1000g0");
    assert_eq!(mgmt["kind"], "static");
    assert_eq!(mgmt["family"], "inet");
    assert_eq!(mgmt["address"], "192.168.1.10/24");
    assert_eq!(mgmt["protected"], true);
    assert_eq!(find(&r.json, "name", "lo0/v4")["protected"], false);
    let mgmt_id = mgmt["id"].as_str().unwrap().to_owned();

    let r = root(
        &app,
        Method::DELETE,
        &format!("/network/addresses/{mgmt_id}"),
        None,
    )
    .await;
    assert_eq!(r.status, StatusCode::FORBIDDEN);
    assert!(
        r.json["type"].as_str().unwrap().ends_with("/protected"),
        "{}",
        r.json
    );

    // Bad inputs.
    for body in [
        json!({ "interface": "e1000g1", "kind": "static" }),
        json!({ "interface": "e1000g1", "kind": "static", "address": "10.0.0.5" }),
        json!({ "interface": "e1000g1", "kind": "dhcp", "address": "10.0.0.5/24" }),
        json!({ "interface": "nope0", "kind": "dhcp" }),
        json!({ "interface": "e1000g1", "kind": "dhcp", "alias": "this-is-bad" }),
    ] {
        let r = root(&app, Method::POST, "/network/addresses", Some(body.clone())).await;
        assert_eq!(
            r.status,
            StatusCode::UNPROCESSABLE_ENTITY,
            "{body}: {}",
            r.json
        );
    }

    let r = root(
        &app,
        Method::POST,
        "/network/addresses",
        Some(json!({ "interface": "e1000g1", "kind": "static", "address": "10.0.0.5/24", "metadata": { "display_name": "Storage" } })),
    )
    .await;
    assert_eq!(r.status, StatusCode::CREATED, "{}", r.json);
    assert_eq!(r.json["name"], "e1000g1/v4");
    assert_eq!(r.json["persistent"], true);
    assert_eq!(r.json["protected"], false);
    assert_eq!(r.json["metadata"]["display_name"], "Storage");
    let new_id = r.json["id"].as_str().unwrap().to_owned();

    let dup = root(
        &app,
        Method::POST,
        "/network/addresses",
        Some(json!({ "interface": "e1000g1", "kind": "dhcp" })),
    )
    .await;
    assert_eq!(dup.status, StatusCode::CONFLICT, "{}", dup.json);

    let r = root(
        &app,
        Method::POST,
        "/network/addresses",
        Some(
            json!({ "interface": "e1000g1", "kind": "dhcp", "alias": "dhcp0", "temporary": true }),
        ),
    )
    .await;
    assert_eq!(r.status, StatusCode::CREATED, "{}", r.json);
    assert_eq!(r.json["name"], "e1000g1/dhcp0");
    assert!(r.json.get("address").is_none());
    assert_eq!(r.json["persistent"], false);
    let dhcp_id = r.json["id"].as_str().unwrap().to_owned();

    // The address a request arrives on is protected too, and so is the
    // link beneath it.
    let via = call(
        &app,
        Method::GET,
        "/network/addresses",
        None,
        Some("10.0.0.5:8443"),
    )
    .await;
    assert_eq!(find(&via.json, "name", "e1000g1/v4")["protected"], true);
    assert_eq!(find(&via.json, "name", "e1000g0/v4")["protected"], true);
    let via_links = call(&app, Method::GET, "/network/links", None, Some("10.0.0.5")).await;
    assert_eq!(find(&via_links.json, "name", "e1000g1")["protected"], true);
    let r = call(
        &app,
        Method::DELETE,
        &format!("/network/addresses/{new_id}"),
        None,
        Some("10.0.0.5"),
    )
    .await;
    assert_eq!(r.status, StatusCode::FORBIDDEN);

    let r = root(
        &app,
        Method::GET,
        &format!("/network/addresses/{new_id}"),
        None,
    )
    .await;
    assert_eq!(r.status, StatusCode::OK);
    assert_eq!(r.json["protected"], false);
    for id in [&new_id, &dhcp_id] {
        let r = root(
            &app,
            Method::DELETE,
            &format!("/network/addresses/{id}"),
            None,
        )
        .await;
        assert_eq!(r.status, StatusCode::NO_CONTENT, "{}", r.json);
    }
    let r = root(
        &app,
        Method::GET,
        &format!("/network/addresses/{new_id}"),
        None,
    )
    .await;
    assert_eq!(r.status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn routes() {
    let app = harness().await;
    let r = root(&app, Method::GET, "/network/routes", None).await;
    assert_eq!(r.status, StatusCode::OK);
    assert_eq!(items(&r.json).len(), 4);
    let default = find(&r.json, "destination", "default");
    assert_eq!(default["gateway"], "192.168.1.1");
    assert_eq!(default["kind"], "static");
    assert_eq!(default["persistent"], true);
    assert_eq!(default["family"], "inet");
    let local = find(&r.json, "destination", "192.168.1.0/24");
    assert_eq!(local["kind"], "interface");
    let local_id = local["id"].as_str().unwrap().to_owned();

    for body in [
        json!({ "destination": "10.20.0.0/16", "gateway": "nowhere" }),
        json!({ "destination": "10.20.0.0", "gateway": "192.168.1.1" }),
        json!({ "destination": "10.20.0.0/16", "gateway": "fe80::1" }),
    ] {
        let r = root(&app, Method::POST, "/network/routes", Some(body.clone())).await;
        assert_eq!(
            r.status,
            StatusCode::UNPROCESSABLE_ENTITY,
            "{body}: {}",
            r.json
        );
    }

    let r = root(
        &app,
        Method::POST,
        "/network/routes",
        Some(json!({ "destination": "10.20.0.0/16", "gateway": "192.168.1.1" })),
    )
    .await;
    assert_eq!(r.status, StatusCode::CREATED, "{}", r.json);
    assert_eq!(r.json["kind"], "static");
    assert_eq!(r.json["persistent"], true);
    let id = r.json["id"].as_str().unwrap().to_owned();
    let dup = root(
        &app,
        Method::POST,
        "/network/routes",
        Some(json!({ "destination": "10.20.0.0/16", "gateway": "192.168.1.1" })),
    )
    .await;
    assert_eq!(dup.status, StatusCode::CONFLICT);

    let r = root(&app, Method::GET, "/network/routes", None).await;
    assert_eq!(items(&r.json).len(), 5);
    assert_eq!(find(&r.json, "destination", "10.20.0.0/16")["id"], id);

    let r = root(
        &app,
        Method::DELETE,
        &format!("/network/routes/{local_id}"),
        None,
    )
    .await;
    assert_eq!(r.status, StatusCode::FORBIDDEN);
    let r = root(&app, Method::DELETE, &format!("/network/routes/{id}"), None).await;
    assert_eq!(r.status, StatusCode::NO_CONTENT);
    let r = root(&app, Method::DELETE, &format!("/network/routes/{id}"), None).await;
    assert_eq!(r.status, StatusCode::NOT_FOUND);
    let r = root(&app, Method::GET, "/network/routes", None).await;
    assert_eq!(items(&r.json).len(), 4);
}
