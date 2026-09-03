//! Image routes against the fake transport and store.

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
use mandrake_images::{FakeStore, FakeTransport, Importer, hex, index};
use mandraked::{
    app::{self, AppState},
    auth::SocketPeer,
    db::Db,
    drivers::Options,
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use tower::ServiceExt;

const INDEX_URL: &str = "https://images.example/mandrake/index.json";

struct Harness {
    app: Router,
    transport: FakeTransport,
    store: FakeStore,
    public_key: String,
}

/// A verified source with two images and one payload served.
fn seed(transport: &FakeTransport, secret: &str) {
    let payload = b"pretend this is a zfs stream".to_vec();
    transport.add(
        "https://images.example/mandrake/debian-12.zfs.gz",
        payload.clone(),
    );
    let index = json!({
        "name": "example",
        "images": [
            {
                "name": "debian-12", "version": "20260901", "type": "zone-lx",
                "url": "debian-12.zfs.gz", "sha256": hex(&Sha256::digest(&payload)),
                "size": payload.len(), "os": "debian-12", "description": "Debian 12 lx"
            },
            {
                "name": "omnios", "version": "r151054", "type": "vm-iso",
                "url": "/isos/omnios.iso", "sha256": "0".repeat(64), "size": 1024
            }
        ]
    });
    let bytes = serde_json::to_vec(&index).unwrap();
    let sig = index::sign(&bytes, secret).unwrap();
    transport.add(INDEX_URL, bytes);
    transport.add(&format!("{INDEX_URL}.sig"), sig.into_bytes());
}

async fn harness() -> Harness {
    let (secret, public_key) = index::keypair();
    let transport = FakeTransport::new();
    seed(&transport, &secret);
    let store = FakeStore::new();
    let importer = Importer::new(Arc::new(transport.clone()), Arc::new(store.clone()));
    let db = Db::open_in_memory().expect("db");
    let state = AppState::with_options(db, Options::fake().with_importer(importer))
        .await
        .expect("state");
    Harness {
        app: app::router(state),
        transport,
        store,
        public_key,
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
    for _ in 0..300 {
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
async fn sources_lifecycle() {
    let h = harness().await;
    let r = root(&h.app, Method::GET, "/images/sources", None).await;
    assert_eq!(r.status, StatusCode::OK);
    let builtin = items(&r.json);
    assert_eq!(builtin.len(), 2);
    assert_eq!(builtin[0]["builtin"], true);
    assert_eq!(builtin[0]["verified"], false);
    let builtin_id = builtin[0]["id"].as_str().unwrap().to_owned();

    // Built-in sources refuse rename and delete, accept enable/disable.
    let r = root(
        &h.app,
        Method::PATCH,
        &format!("/images/sources/{builtin_id}"),
        Some(json!({ "name": "mine" })),
    )
    .await;
    assert_eq!(r.status, StatusCode::FORBIDDEN);
    let r = root(
        &h.app,
        Method::DELETE,
        &format!("/images/sources/{builtin_id}"),
        None,
    )
    .await;
    assert_eq!(r.status, StatusCode::FORBIDDEN);
    let r = root(
        &h.app,
        Method::PATCH,
        &format!("/images/sources/{builtin_id}"),
        Some(json!({ "enabled": false })),
    )
    .await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.json);
    assert_eq!(r.json["enabled"], false);
    // Refreshing a built-in source whose index is unreachable records the error.
    let r = root(
        &h.app,
        Method::POST,
        &format!("/images/sources/{builtin_id}/refresh"),
        None,
    )
    .await;
    assert_eq!(r.status, StatusCode::OK);
    assert!(r.json["last_error"].as_str().unwrap().contains("404"));

    // Bad inputs.
    for body in [
        json!({ "name": "", "url": INDEX_URL }),
        json!({ "name": "x", "url": "ftp://nope" }),
        json!({ "name": "x", "url": INDEX_URL, "public_key": "not-a-key" }),
    ] {
        let r = root(&h.app, Method::POST, "/images/sources", Some(body.clone())).await;
        assert_eq!(
            r.status,
            StatusCode::UNPROCESSABLE_ENTITY,
            "{body}: {}",
            r.json
        );
    }

    // A verified source: fetched and verified at creation.
    let r = root(
        &h.app,
        Method::POST,
        "/images/sources",
        Some(json!({ "name": "example", "url": INDEX_URL, "public_key": h.public_key })),
    )
    .await;
    assert_eq!(r.status, StatusCode::CREATED, "{}", r.json);
    assert_eq!(r.json["verified"], true);
    assert_eq!(r.json["image_count"], 2);
    assert!(r.json.get("last_error").is_none());
    let source_id = r.json["id"].as_str().unwrap().to_owned();
    let dup = root(
        &h.app,
        Method::POST,
        "/images/sources",
        Some(json!({ "name": "example", "url": INDEX_URL })),
    )
    .await;
    assert_eq!(dup.status, StatusCode::CONFLICT);

    let r = root(&h.app, Method::GET, "/images/available", None).await;
    assert_eq!(r.status, StatusCode::OK);
    assert_eq!(items(&r.json).len(), 2);
    let debian = items(&r.json)
        .iter()
        .find(|e| e["name"] == "debian-12")
        .unwrap();
    assert_eq!(
        debian["url"],
        "https://images.example/mandrake/debian-12.zfs.gz"
    );
    assert_eq!(debian["imported"], false);
    let iso = items(&r.json)
        .iter()
        .find(|e| e["name"] == "omnios")
        .unwrap();
    assert_eq!(iso["url"], "https://images.example/isos/omnios.iso");
    let r = root(&h.app, Method::GET, "/images/available?type=vm-iso", None).await;
    assert_eq!(items(&r.json).len(), 1);

    // Clearing the key makes it unverified; the catalogue stays.
    let r = root(
        &h.app,
        Method::PATCH,
        &format!("/images/sources/{source_id}"),
        Some(json!({ "public_key": null })),
    )
    .await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.json);
    assert_eq!(r.json["verified"], false);
    assert_eq!(r.json["image_count"], 2);

    // A tampered index fails verification and keeps the old catalogue.
    let r = root(
        &h.app,
        Method::PATCH,
        &format!("/images/sources/{source_id}"),
        Some(json!({ "public_key": h.public_key })),
    )
    .await;
    assert_eq!(r.json["verified"], true);
    h.transport
        .add(INDEX_URL, b"{\"name\":\"evil\",\"images\":[]}".to_vec());
    let r = root(
        &h.app,
        Method::POST,
        &format!("/images/sources/{source_id}/refresh"),
        None,
    )
    .await;
    assert_eq!(r.status, StatusCode::OK);
    assert_eq!(r.json["verified"], false);
    assert!(r.json["last_error"].as_str().unwrap().contains("signature"));
    assert_eq!(r.json["image_count"], 2);

    let r = root(
        &h.app,
        Method::DELETE,
        &format!("/images/sources/{source_id}"),
        None,
    )
    .await;
    assert_eq!(r.status, StatusCode::NO_CONTENT);
    let r = root(&h.app, Method::GET, "/images/available", None).await;
    assert_eq!(items(&r.json).len(), 0);
}

#[tokio::test]
async fn import_from_a_source_and_delete() {
    let h = harness().await;
    let r = root(
        &h.app,
        Method::POST,
        "/images/sources",
        Some(json!({ "name": "example", "url": INDEX_URL, "public_key": h.public_key })),
    )
    .await;
    let source_id = r.json["id"].as_str().unwrap().to_owned();

    // Unknown entry and missing fields are 404 and 422.
    let r = root(
        &h.app,
        Method::POST,
        "/images/import",
        Some(json!({ "source_id": source_id, "name": "nope", "version": "1" })),
    )
    .await;
    assert_eq!(r.status, StatusCode::NOT_FOUND);
    let r = root(
        &h.app,
        Method::POST,
        "/images/import",
        Some(json!({ "name": "x", "version": "1", "url": "https://x/y" })),
    )
    .await;
    assert_eq!(r.status, StatusCode::UNPROCESSABLE_ENTITY);

    let r = root(
        &h.app,
        Method::POST,
        "/images/import",
        Some(
            json!({ "source_id": source_id, "name": "debian-12", "version": "20260901",
                     "metadata": { "display_name": "Debian" } }),
        ),
    )
    .await;
    assert_eq!(r.status, StatusCode::ACCEPTED, "{}", r.json);
    assert_eq!(r.json["kind"], "image.import");
    let job_id = r.json["id"].as_str().unwrap().to_owned();
    let image_id = r.json["target"]["id"].as_str().unwrap().to_owned();
    let job = wait_job(&h.app, &job_id).await;
    assert_eq!(job["state"], "succeeded", "{job}");

    let r = root(&h.app, Method::GET, &format!("/images/{image_id}"), None).await;
    assert_eq!(r.status, StatusCode::OK);
    assert_eq!(r.json["state"], "ready");
    assert_eq!(r.json["type"], "zone-lx");
    assert_eq!(r.json["pool"], "rpool");
    assert_eq!(r.json["dataset"], format!("rpool/images/{image_id}"));
    assert_eq!(r.json["source_name"], "example");
    assert_eq!(r.json["metadata"]["display_name"], "Debian");
    assert!(r.json["imported_at"].is_string());
    assert_eq!(
        h.store.snapshots(),
        vec![format!("rpool/images/{image_id}@image")]
    );

    // The catalogue knows it is imported; a second import is refused.
    let r = root(&h.app, Method::GET, "/images/available", None).await;
    let debian = items(&r.json)
        .iter()
        .find(|e| e["name"] == "debian-12")
        .unwrap();
    assert_eq!(debian["imported"], true);
    assert_eq!(debian["image_id"], image_id);
    let r = root(
        &h.app,
        Method::POST,
        "/images/import",
        Some(json!({ "source_id": source_id, "name": "debian-12", "version": "20260901" })),
    )
    .await;
    assert_eq!(r.status, StatusCode::CONFLICT);

    let r = root(&h.app, Method::GET, "/images?type=zone-lx", None).await;
    assert_eq!(items(&r.json).len(), 1);
    let r = root(&h.app, Method::GET, "/images?state=failed", None).await;
    assert_eq!(items(&r.json).len(), 0);

    let r = root(
        &h.app,
        Method::PATCH,
        &format!("/images/{image_id}"),
        Some(json!({ "description": "the base image" })),
    )
    .await;
    assert_eq!(r.status, StatusCode::OK);
    assert_eq!(r.json["metadata"]["description"], "the base image");

    let r = root(&h.app, Method::DELETE, &format!("/images/{image_id}"), None).await;
    assert_eq!(r.status, StatusCode::NO_CONTENT, "{}", r.json);
    assert!(h.store.datasets().is_empty());
    let r = root(&h.app, Method::GET, &format!("/images/{image_id}"), None).await;
    assert_eq!(r.status, StatusCode::NOT_FOUND);

    let audit = root(&h.app, Method::GET, "/audit?limit=50", None).await;
    let actions: Vec<&str> = audit.json["items"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|a| a["action"].as_str())
        .collect();
    for a in [
        "image-source.create",
        "image.import",
        "image.update",
        "image.delete",
    ] {
        assert!(actions.contains(&a), "{a} missing from {actions:?}");
    }
}

#[tokio::test]
async fn unverified_source_and_direct_imports() {
    let h = harness().await;
    let r = root(
        &h.app,
        Method::POST,
        "/images/sources",
        Some(json!({ "name": "unsigned", "url": INDEX_URL })),
    )
    .await;
    assert_eq!(r.status, StatusCode::CREATED);
    assert_eq!(r.json["verified"], false);
    assert_eq!(r.json["image_count"], 2);
    let source_id = r.json["id"].as_str().unwrap().to_owned();
    let r = root(
        &h.app,
        Method::POST,
        "/images/import",
        Some(json!({ "source_id": source_id, "name": "debian-12", "version": "20260901" })),
    )
    .await;
    assert_eq!(r.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(
        r.json["type"]
            .as_str()
            .unwrap()
            .ends_with("/unverified-source")
    );

    // Direct import with a wrong hash fails the job and marks the image.
    let r = root(
        &h.app,
        Method::POST,
        "/images/import",
        Some(json!({
            "name": "debian-12", "version": "manual", "type": "zone-lx",
            "url": "https://images.example/mandrake/debian-12.zfs.gz",
            "sha256": "f".repeat(64)
        })),
    )
    .await;
    assert_eq!(r.status, StatusCode::ACCEPTED, "{}", r.json);
    let job = wait_job(&h.app, r.json["id"].as_str().unwrap()).await;
    assert_eq!(job["state"], "failed");
    let image_id = r.json["target"]["id"].as_str().unwrap().to_owned();
    let r = root(&h.app, Method::GET, &format!("/images/{image_id}"), None).await;
    assert_eq!(r.json["state"], "failed");
    assert!(r.json["error"].as_str().unwrap().contains("sha256"));
    assert!(h.store.datasets().is_empty());
    let r = root(&h.app, Method::DELETE, &format!("/images/{image_id}"), None).await;
    assert_eq!(r.status, StatusCode::NO_CONTENT);

    // Direct import with the right hash succeeds without any source.
    let payload = b"pretend this is a zfs stream";
    let r = root(
        &h.app,
        Method::POST,
        "/images/import",
        Some(json!({
            "name": "debian-12", "version": "manual", "type": "zone-lx",
            "url": "https://images.example/mandrake/debian-12.zfs.gz",
            "sha256": hex(&Sha256::digest(payload)),
            "pool": "rpool"
        })),
    )
    .await;
    assert_eq!(r.status, StatusCode::ACCEPTED, "{}", r.json);
    let job = wait_job(&h.app, r.json["id"].as_str().unwrap()).await;
    assert_eq!(job["state"], "succeeded", "{job}");
    let r = root(
        &h.app,
        Method::POST,
        "/images/import",
        Some(json!({
            "name": "x", "version": "1", "type": "vm-iso",
            "url": "https://images.example/nope.iso", "sha256": "a".repeat(64), "pool": "nope"
        })),
    )
    .await;
    assert_eq!(r.status, StatusCode::NOT_FOUND);
}
