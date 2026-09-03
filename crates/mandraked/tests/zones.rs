//! Zone routes against the fake drivers.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::missing_panics_doc,
    clippy::needless_pass_by_value,
    clippy::too_many_lines
)]

use std::{sync::Arc, time::Duration};

use axum::{
    Router,
    body::{Body, to_bytes},
    http::{Method, Request, StatusCode, header},
};
use mandrake_core::storage::DatasetKind;
use mandrake_images::{FakeStore, FakeTransport, Importer, hex};
use mandrake_zfs::{FakeZfs, Zfs};
use mandrake_zones::{FakeZones, Zones};
use mandraked::{
    app::{self, AppState},
    auth::SocketPeer,
    db::Db,
    drivers::Options,
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use tower::ServiceExt;

const PAYLOAD: &[u8] = b"pretend this is a zfs stream";
const PAYLOAD_URL: &str = "https://images.example/debian-12.zfs.gz";

struct Harness {
    app: Router,
    zfs: Arc<FakeZfs>,
    zones: Arc<FakeZones>,
}

async fn harness() -> Harness {
    let transport = FakeTransport::new();
    transport.add(PAYLOAD_URL, PAYLOAD.to_vec());
    let importer = Importer::new(Arc::new(transport), Arc::new(FakeStore::new()));
    let zfs = Arc::new(FakeZfs::typical());
    let zones = Arc::new(FakeZones::typical());
    let db = Db::open_in_memory().expect("db");
    let options = Options::fake()
        .with_importer(importer)
        .with_zfs(zfs.clone())
        .with_zones(zones.clone());
    let state = AppState::with_options(db, options).await.expect("state");
    Harness {
        app: app::router(state),
        zfs,
        zones,
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

fn items(v: &Value) -> &Vec<Value> {
    v["items"].as_array().expect("items")
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

/// Import the lx image and make its dataset and snapshot exist in the
/// fake ZFS, which the fake image store does not touch.
async fn ready_image(h: &Harness) -> String {
    let r = root(
        &h.app,
        Method::POST,
        "/images/import",
        Some(json!({
            "name": "debian-12", "version": "20260901", "type": "zone-lx",
            "url": PAYLOAD_URL, "sha256": hex(&Sha256::digest(PAYLOAD)), "pool": "rpool"
        })),
    )
    .await;
    assert_eq!(r.status, StatusCode::ACCEPTED, "{}", r.json);
    let job = wait_job(&h.app, r.json["id"].as_str().unwrap()).await;
    assert_eq!(job["state"], "succeeded", "{job}");
    let image_id = r.json["target"]["id"].as_str().unwrap().to_owned();
    h.zfs.add_dataset("rpool/images", DatasetKind::Filesystem);
    h.zfs
        .add_dataset(&format!("rpool/images/{image_id}"), DatasetKind::Filesystem);
    h.zfs
        .create_snapshot(&format!("rpool/images/{image_id}"), "image", false)
        .await
        .unwrap();
    image_id
}

async fn zone_state(app: &Router, id: &str) -> String {
    let r = root(app, Method::GET, &format!("/zones/{id}"), None).await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.json);
    r.json["state"].as_str().unwrap().to_owned()
}

#[tokio::test]
async fn existing_zone_gets_a_stable_id() {
    let h = harness().await;
    let r = root(&h.app, Method::GET, "/zones", None).await;
    assert_eq!(r.status, StatusCode::OK);
    assert_eq!(items(&r.json).len(), 1);
    let build = &items(&r.json)[0];
    assert_eq!(build["name"], "build");
    assert_eq!(build["brand"], "ipkg");
    assert_eq!(build["state"], "installed");
    assert_eq!(build["pool"], "rpool");
    assert_eq!(build["dataset"], "rpool/zones/build");
    assert_eq!(build["autoboot"], false);
    let id = build["id"].as_str().unwrap().to_owned();
    // The id was stored on the zone and comes back the same.
    let again = root(&h.app, Method::GET, "/zones", None).await;
    assert_eq!(items(&again.json)[0]["id"], id);
    let cfg = h.zones.config("build").await.unwrap();
    assert_eq!(
        cfg.attrs.get("mandrake-id").map(String::as_str),
        Some(id.as_str())
    );
    let r = root(&h.app, Method::GET, "/zones?brand=lx", None).await;
    assert_eq!(items(&r.json).len(), 0);
}

#[tokio::test]
async fn lx_zone_lifecycle() {
    let h = harness().await;
    let image_id = ready_image(&h).await;

    // Bad requests.
    for body in [
        json!({ "name": "web", "brand": "lx" }),
        json!({ "name": "global", "brand": "lx", "image_id": image_id }),
        json!({ "name": "web", "brand": "lx", "image_id": image_id,
                "nics": [{ "name": "net0", "over": "nope0" }] }),
        json!({ "name": "web", "brand": "lx", "image_id": image_id,
                "nics": [{ "name": "net0", "over": "e1000g1", "vid": 5000 }] }),
        json!({ "name": "web", "brand": "lx", "image_id": image_id,
                "nics": [{ "name": "net0", "over": "e1000g1", "address": "10.0.0.5" }] }),
        json!({ "name": "web", "brand": "lx", "image_id": image_id, "memory_cap_bytes": 1024 }),
        json!({ "name": "web", "brand": "lx", "image_id": image_id, "cpu_cap": 0 }),
        json!({ "name": "web", "brand": "lx", "image_id": image_id, "pool": "tank" }),
    ] {
        let r = root(&h.app, Method::POST, "/zones", Some(body.clone())).await;
        assert!(
            r.status == StatusCode::UNPROCESSABLE_ENTITY || r.status == StatusCode::NOT_FOUND,
            "{body}: {} {}",
            r.status,
            r.json
        );
    }

    let r = root(
        &h.app,
        Method::POST,
        "/zones",
        Some(json!({
            "name": "web", "brand": "lx", "image_id": image_id,
            "nics": [{ "name": "net0", "over": "e1000g1", "vid": 20,
                       "address": "10.0.0.5/24", "gateway": "10.0.0.1" }],
            "cpu_cap": 1.5, "memory_cap_bytes": 1_073_741_824,
            "resolvers": ["10.0.0.1"], "metadata": { "display_name": "Web" }
        })),
    )
    .await;
    assert_eq!(r.status, StatusCode::ACCEPTED, "{}", r.json);
    assert_eq!(r.json["kind"], "zone.install");
    let zone_id = r.json["target"]["id"].as_str().unwrap().to_owned();
    // Listed at once as configured.
    assert_eq!(zone_state(&h.app, &zone_id).await, "configured");
    let job = wait_job(&h.app, r.json["id"].as_str().unwrap()).await;
    assert_eq!(job["state"], "succeeded", "{job}");

    let r = root(&h.app, Method::GET, &format!("/zones/{zone_id}"), None).await;
    assert_eq!(r.json["state"], "running");
    assert_eq!(r.json["brand"], "lx");
    assert_eq!(r.json["image_id"], image_id);
    assert_eq!(r.json["zonepath"], "/rpool/zones/web");
    assert_eq!(r.json["dataset"], "rpool/zones/web");
    assert_eq!(r.json["nics"][0]["over"], "e1000g1");
    assert_eq!(r.json["nics"][0]["vid"], 20);
    assert_eq!(r.json["cpu_cap"], 1.5);
    assert_eq!(r.json["memory_cap_bytes"], 1_073_741_824);
    assert_eq!(r.json["hostname"], "web");
    assert_eq!(r.json["resolvers"], json!(["10.0.0.1"]));
    assert_eq!(r.json["autoboot"], true);
    assert_eq!(r.json["metadata"]["display_name"], "Web");
    // The clone exists in ZFS.
    let ds = root(&h.app, Method::GET, "/storage/datasets?pool=rpool", None).await;
    assert!(
        items(&ds.json)
            .iter()
            .any(|d| d["name"] == "rpool/zones/web")
    );
    // The image counts its clone.
    let img = root(&h.app, Method::GET, &format!("/images/{image_id}"), None).await;
    assert_eq!(img.json["in_use_by"], 1);
    let del = root(&h.app, Method::DELETE, &format!("/images/{image_id}"), None).await;
    assert_eq!(del.status, StatusCode::CONFLICT);

    // Duplicate name.
    let r = root(
        &h.app,
        Method::POST,
        "/zones",
        Some(json!({ "name": "web", "brand": "lx", "image_id": image_id })),
    )
    .await;
    assert_eq!(r.status, StatusCode::CONFLICT);

    // Update: caps off, nics replaced, autoboot off.
    let r = root(
        &h.app,
        Method::PATCH,
        &format!("/zones/{zone_id}"),
        Some(json!({ "cpu_cap": null, "autoboot": false, "nics": [],
                     "resolvers": [], "metadata": { "tags": ["lab"] } })),
    )
    .await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.json);
    assert!(r.json.get("cpu_cap").is_none());
    assert_eq!(r.json["memory_cap_bytes"], 1_073_741_824);
    assert_eq!(r.json["autoboot"], false);
    assert_eq!(r.json["nics"], json!([]));
    assert!(r.json.get("resolvers").is_none());
    assert_eq!(r.json["metadata"]["tags"], json!(["lab"]));
    assert_eq!(r.json["metadata"]["display_name"], "Web");

    // Lifecycle: start while running is busy; stop, start, restart.
    let r = root(
        &h.app,
        Method::POST,
        &format!("/zones/{zone_id}/start"),
        None,
    )
    .await;
    assert_eq!(r.status, StatusCode::CONFLICT);
    let r = root(
        &h.app,
        Method::POST,
        &format!("/zones/{zone_id}/stop"),
        None,
    )
    .await;
    assert_eq!(r.status, StatusCode::ACCEPTED, "{}", r.json);
    assert_eq!(
        wait_job(&h.app, r.json["id"].as_str().unwrap()).await["state"],
        "succeeded"
    );
    assert_eq!(zone_state(&h.app, &zone_id).await, "installed");
    let r = root(
        &h.app,
        Method::POST,
        &format!("/zones/{zone_id}/restart"),
        None,
    )
    .await;
    assert_eq!(r.status, StatusCode::CONFLICT);
    let r = root(
        &h.app,
        Method::POST,
        &format!("/zones/{zone_id}/start"),
        None,
    )
    .await;
    assert_eq!(r.status, StatusCode::ACCEPTED);
    assert_eq!(
        wait_job(&h.app, r.json["id"].as_str().unwrap()).await["state"],
        "succeeded"
    );
    assert_eq!(zone_state(&h.app, &zone_id).await, "running");
    let r = root(
        &h.app,
        Method::POST,
        &format!("/zones/{zone_id}/restart"),
        None,
    )
    .await;
    assert_eq!(r.status, StatusCode::ACCEPTED);
    assert_eq!(
        wait_job(&h.app, r.json["id"].as_str().unwrap()).await["state"],
        "succeeded"
    );
    assert_eq!(zone_state(&h.app, &zone_id).await, "running");
    let r = root(
        &h.app,
        Method::POST,
        &format!("/zones/{zone_id}/stop"),
        Some(json!({ "force": true })),
    )
    .await;
    assert_eq!(r.status, StatusCode::ACCEPTED);
    assert_eq!(
        wait_job(&h.app, r.json["id"].as_str().unwrap()).await["state"],
        "succeeded"
    );
    assert_eq!(zone_state(&h.app, &zone_id).await, "installed");

    // Delete without purge keeps the dataset.
    let r = root(&h.app, Method::DELETE, &format!("/zones/{zone_id}"), None).await;
    assert_eq!(r.status, StatusCode::ACCEPTED, "{}", r.json);
    let job = wait_job(&h.app, r.json["id"].as_str().unwrap()).await;
    assert_eq!(job["state"], "succeeded", "{job}");
    let r = root(&h.app, Method::GET, &format!("/zones/{zone_id}"), None).await;
    assert_eq!(r.status, StatusCode::NOT_FOUND);
    let ds = root(&h.app, Method::GET, "/storage/datasets?pool=rpool", None).await;
    assert!(
        items(&ds.json)
            .iter()
            .any(|d| d["name"] == "rpool/zones/web")
    );

    let audit = root(&h.app, Method::GET, "/audit?limit=50", None).await;
    let actions: Vec<&str> = audit.json["items"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|a| a["action"].as_str())
        .collect();
    for a in [
        "zone.create",
        "zone.update",
        "zone.stop",
        "zone.start",
        "zone.restart",
        "zone.delete",
    ] {
        assert!(actions.contains(&a), "{a} missing from {actions:?}");
    }
    let events = root(&h.app, Method::GET, "/audit?limit=1", None).await;
    assert_eq!(events.status, StatusCode::OK);
}

#[tokio::test]
async fn native_zone_from_packages_and_purge() {
    let h = harness().await;
    let r = root(
        &h.app,
        Method::POST,
        "/zones",
        Some(json!({ "name": "tools", "brand": "lipkg", "start": false, "autoboot": false })),
    )
    .await;
    assert_eq!(r.status, StatusCode::ACCEPTED, "{}", r.json);
    let zone_id = r.json["target"]["id"].as_str().unwrap().to_owned();
    let job = wait_job(&h.app, r.json["id"].as_str().unwrap()).await;
    assert_eq!(job["state"], "succeeded", "{job}");
    assert_eq!(job["message"], "installed");
    assert_eq!(zone_state(&h.app, &zone_id).await, "installed");
    let ds = root(&h.app, Method::GET, "/storage/datasets?pool=rpool", None).await;
    assert!(
        items(&ds.json)
            .iter()
            .any(|d| d["name"] == "rpool/zones/tools")
    );

    let r = root(
        &h.app,
        Method::DELETE,
        &format!("/zones/{zone_id}?purge=true"),
        None,
    )
    .await;
    assert_eq!(r.status, StatusCode::ACCEPTED);
    let job = wait_job(&h.app, r.json["id"].as_str().unwrap()).await;
    assert_eq!(job["state"], "succeeded", "{job}");
    let ds = root(&h.app, Method::GET, "/storage/datasets?pool=rpool", None).await;
    assert!(
        !items(&ds.json)
            .iter()
            .any(|d| d["name"] == "rpool/zones/tools")
    );
    assert_eq!(h.zones.state_of("tools"), None);
}
