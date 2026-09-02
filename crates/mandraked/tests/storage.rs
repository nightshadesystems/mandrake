//! Storage routes against the fake driver.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::missing_panics_doc,
    clippy::needless_pass_by_value,
    clippy::too_many_lines,
    clippy::unreadable_literal
)]

use std::{sync::Arc, time::Duration};

use axum::{
    Router,
    body::{Body, to_bytes},
    http::{Method, Request, StatusCode, header},
};
use mandrake_core::storage::DatasetKind;
use mandrake_zfs::FakeZfs;
use mandraked::{
    app::{self, AppState},
    auth::SocketPeer,
    db::Db,
    drivers::Options,
};
use serde_json::{Value, json};
use tower::ServiceExt;

struct Harness {
    app: Router,
    zfs: Arc<FakeZfs>,
}

async fn harness() -> Harness {
    let zfs = Arc::new(FakeZfs::typical());
    let db = Db::open_in_memory().expect("db");
    let state = AppState::with_options(db, Options::fake().with_zfs(zfs.clone()))
        .await
        .expect("state");
    Harness {
        app: app::router(state),
        zfs,
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

async fn as_viewer(app: &Router, method: Method, path: &str, body: Option<Value>) -> Reply {
    // Create a viewer and log in.
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

fn mirror_pool(name: &str) -> Value {
    json!({
        "name": name,
        "vdevs": [{ "type": "mirror", "devices": ["c1t1d0", "c1t2d0"] }],
        "metadata": { "display_name": "Data" }
    })
}

#[tokio::test]
async fn devices_and_pools_reflect_the_host() {
    let h = harness().await;
    let r = root(&h.app, Method::GET, "/storage/devices", None).await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.json);
    let items = r.json["items"].as_array().unwrap();
    assert_eq!(items.len(), 5);
    assert_eq!(items[0]["name"], "c1t0d0");
    assert_eq!(items[0]["pool"], "rpool");
    assert!(items[1]["pool"].is_null());

    let r = root(&h.app, Method::GET, "/storage/pools", None).await;
    assert_eq!(r.status, StatusCode::OK);
    let pools = r.json["items"].as_array().unwrap();
    assert_eq!(pools.len(), 1);
    assert_eq!(pools[0]["name"], "rpool");
    assert_eq!(pools[0]["protected"], true);
    assert_eq!(pools[0]["vdevs"]["type"], "root");
    let rpool_id = pools[0]["id"].as_str().unwrap().to_owned();

    // Ids are stable across reads.
    let again = root(&h.app, Method::GET, "/storage/pools", None).await;
    assert_eq!(again.json["items"][0]["id"], rpool_id);
    let one = root(
        &h.app,
        Method::GET,
        &format!("/storage/pools/{rpool_id}"),
        None,
    )
    .await;
    assert_eq!(one.status, StatusCode::OK);
    assert_eq!(one.json["name"], "rpool");

    // rpool cannot be destroyed, even with the right name.
    let r = root(
        &h.app,
        Method::DELETE,
        &format!("/storage/pools/{rpool_id}"),
        Some(json!({ "name": "rpool" })),
    )
    .await;
    assert_eq!(r.status, StatusCode::FORBIDDEN);
    assert!(r.json["type"].as_str().unwrap().ends_with("protected"));

    // Metadata on a protected pool is fine.
    let r = root(
        &h.app,
        Method::PATCH,
        &format!("/storage/pools/{rpool_id}"),
        Some(json!({ "description": "boot" })),
    )
    .await;
    assert_eq!(r.status, StatusCode::OK);
    assert_eq!(r.json["metadata"]["description"], "boot");
}

#[tokio::test]
async fn pool_lifecycle_and_scrub_job() {
    let h = harness().await;
    let r = root(
        &h.app,
        Method::POST,
        "/storage/pools",
        Some(mirror_pool("tank")),
    )
    .await;
    assert_eq!(r.status, StatusCode::CREATED, "{}", r.json);
    assert_eq!(r.json["name"], "tank");
    assert_eq!(r.json["vdevs"]["children"][0]["type"], "mirror");
    assert_eq!(r.json["metadata"]["display_name"], "Data");
    let id = r.json["id"].as_str().unwrap().to_owned();

    // Duplicate and bad names.
    assert_eq!(
        root(
            &h.app,
            Method::POST,
            "/storage/pools",
            Some(mirror_pool("tank"))
        )
        .await
        .status,
        StatusCode::CONFLICT
    );
    assert_eq!(
        root(
            &h.app,
            Method::POST,
            "/storage/pools",
            Some(mirror_pool("9bad"))
        )
        .await
        .status,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    // Reusing a disk without force is refused by the tool.
    let r = root(
        &h.app,
        Method::POST,
        "/storage/pools",
        Some(json!({ "name": "other", "vdevs": [{ "type": "stripe", "devices": ["c1t1d0"] }] })),
    )
    .await;
    assert_eq!(r.status, StatusCode::UNPROCESSABLE_ENTITY, "{}", r.json);

    // The device list now shows membership.
    let r = root(&h.app, Method::GET, "/storage/devices", None).await;
    assert_eq!(r.json["items"][1]["pool"], "tank");

    // Scrub as a job.
    let r = root(
        &h.app,
        Method::POST,
        &format!("/storage/pools/{id}/scrub"),
        None,
    )
    .await;
    assert_eq!(r.status, StatusCode::ACCEPTED, "{}", r.json);
    let job_id = r.json["id"].as_str().unwrap().to_owned();
    assert_eq!(r.json["kind"], "pool.scrub");
    assert_eq!(
        root(
            &h.app,
            Method::POST,
            &format!("/storage/pools/{id}/scrub"),
            None
        )
        .await
        .status,
        StatusCode::CONFLICT
    );
    tokio::time::sleep(Duration::from_millis(60)).await;
    let running = root(&h.app, Method::GET, &format!("/jobs/{job_id}"), None).await;
    assert_eq!(running.json["state"], "running");
    h.zfs.finish_scrub("tank");
    let mut done = Value::Null;
    for _ in 0..100 {
        tokio::time::sleep(Duration::from_millis(20)).await;
        done = root(&h.app, Method::GET, &format!("/jobs/{job_id}"), None)
            .await
            .json;
        if done["state"] == "succeeded" {
            break;
        }
    }
    assert_eq!(done["state"], "succeeded", "{done}");
    assert_eq!(done["progress"], 1.0);

    // Destroy needs the name echoed and the admin role; root is admin.
    let r = root(
        &h.app,
        Method::DELETE,
        &format!("/storage/pools/{id}"),
        Some(json!({ "name": "wrong" })),
    )
    .await;
    assert_eq!(r.status, StatusCode::UNPROCESSABLE_ENTITY);
    let r = root(
        &h.app,
        Method::DELETE,
        &format!("/storage/pools/{id}"),
        Some(json!({ "name": "tank" })),
    )
    .await;
    assert_eq!(r.status, StatusCode::NO_CONTENT);
    assert_eq!(
        root(&h.app, Method::GET, &format!("/storage/pools/{id}"), None)
            .await
            .status,
        StatusCode::NOT_FOUND
    );

    let audit = root(&h.app, Method::GET, "/audit?action=pool.destroy", None).await;
    assert_eq!(audit.json["items"][0]["object"]["name"], "tank");
}

#[tokio::test]
async fn datasets_volumes_and_protection() {
    let h = harness().await;
    root(
        &h.app,
        Method::POST,
        "/storage/pools",
        Some(mirror_pool("tank")),
    )
    .await;

    // A volume without a size is refused; with one it is created with parents.
    let r = root(
        &h.app,
        Method::POST,
        "/storage/datasets",
        Some(json!({ "name": "tank/vms/disk0", "kind": "volume" })),
    )
    .await;
    assert_eq!(r.status, StatusCode::UNPROCESSABLE_ENTITY);
    let r = root(
        &h.app,
        Method::POST,
        "/storage/datasets",
        Some(json!({ "name": "tank/vms/disk0", "kind": "volume", "volsize_bytes": 1073741824, "sparse": true, "create_parents": true, "compression": "zstd" })),
    )
    .await;
    assert_eq!(r.status, StatusCode::CREATED, "{}", r.json);
    assert_eq!(r.json["kind"], "volume");
    assert_eq!(r.json["volsize_bytes"], 1073741824);
    assert_eq!(r.json["compression"], "zstd");
    let vol_id = r.json["id"].as_str().unwrap().to_owned();

    // Filesystem with quota, then clear the quota with an explicit null.
    let r = root(
        &h.app,
        Method::POST,
        "/storage/datasets",
        Some(json!({ "name": "tank/data", "kind": "filesystem", "quota_bytes": 10737418240u64 })),
    )
    .await;
    assert_eq!(r.status, StatusCode::CREATED, "{}", r.json);
    assert_eq!(r.json["quota_bytes"], 10737418240u64);
    let fs_id = r.json["id"].as_str().unwrap().to_owned();
    let r = root(
        &h.app,
        Method::PATCH,
        &format!("/storage/datasets/{fs_id}"),
        Some(json!({ "quota_bytes": null, "metadata": { "tags": ["a"] } })),
    )
    .await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.json);
    assert!(r.json["quota_bytes"].is_null());
    assert_eq!(r.json["metadata"]["tags"][0], "a");

    // Volumes grow but never shrink.
    let r = root(
        &h.app,
        Method::PATCH,
        &format!("/storage/datasets/{vol_id}"),
        Some(json!({ "volsize_bytes": 1 })),
    )
    .await;
    assert_eq!(r.status, StatusCode::UNPROCESSABLE_ENTITY);
    let r = root(
        &h.app,
        Method::PATCH,
        &format!("/storage/datasets/{vol_id}"),
        Some(json!({ "volsize_bytes": 2147483648u64 })),
    )
    .await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.json);

    // Lists and filters.
    let r = root(&h.app, Method::GET, "/storage/volumes", None).await;
    assert_eq!(r.json["items"].as_array().unwrap().len(), 1);
    let r = root(&h.app, Method::GET, "/storage/datasets?parent=tank", None).await;
    let names: Vec<&str> = r.json["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|d| d["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, vec!["tank/data", "tank/vms"]);
    let r = root(
        &h.app,
        Method::GET,
        "/storage/datasets?pool=rpool&limit=2",
        None,
    )
    .await;
    assert_eq!(r.json["items"].as_array().unwrap().len(), 2);
    assert!(r.json["next_cursor"].is_string());

    // Protected datasets refuse changes and destroy, but accept metadata.
    let r = root(&h.app, Method::GET, "/storage/datasets?pool=rpool", None).await;
    let be = r.json["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|d| d["name"] == "rpool/ROOT/omnios")
        .unwrap()
        .clone();
    assert_eq!(be["protected"], true);
    let be_id = be["id"].as_str().unwrap();
    assert_eq!(
        root(
            &h.app,
            Method::DELETE,
            &format!("/storage/datasets/{be_id}"),
            None
        )
        .await
        .status,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        root(
            &h.app,
            Method::PATCH,
            &format!("/storage/datasets/{be_id}"),
            Some(json!({ "compression": "off" }))
        )
        .await
        .status,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        root(
            &h.app,
            Method::PATCH,
            &format!("/storage/datasets/{be_id}"),
            Some(json!({ "metadata": { "notes": "current" } }))
        )
        .await
        .status,
        StatusCode::OK
    );
    assert_eq!(
        root(
            &h.app,
            Method::POST,
            "/storage/datasets",
            Some(json!({ "name": "rpool/ROOT/evil", "kind": "filesystem" }))
        )
        .await
        .status,
        StatusCode::FORBIDDEN
    );

    // Destroy with children needs recursive.
    let vms_id = names_to_id(&h.app, "tank/vms").await;
    let r = root(
        &h.app,
        Method::DELETE,
        &format!("/storage/datasets/{vms_id}"),
        None,
    )
    .await;
    assert_eq!(r.status, StatusCode::CONFLICT);
    assert!(r.json["type"].as_str().unwrap().ends_with("has-children"));
    let r = root(
        &h.app,
        Method::DELETE,
        &format!("/storage/datasets/{vms_id}?recursive=true"),
        None,
    )
    .await;
    assert_eq!(r.status, StatusCode::NO_CONTENT);
    assert_eq!(
        root(
            &h.app,
            Method::GET,
            &format!("/storage/datasets/{vol_id}"),
            None
        )
        .await
        .status,
        StatusCode::NOT_FOUND
    );

    // Viewers read but cannot mutate.
    assert_eq!(
        as_viewer(&h.app, Method::GET, "/storage/datasets", None)
            .await
            .status,
        StatusCode::OK
    );
    assert_eq!(
        as_viewer(
            &h.app,
            Method::POST,
            "/storage/datasets",
            Some(json!({ "name": "tank/x", "kind": "filesystem" }))
        )
        .await
        .status,
        StatusCode::FORBIDDEN
    );
}

async fn names_to_id(app: &Router, name: &str) -> String {
    let r = root(app, Method::GET, "/storage/datasets", None).await;
    r.json["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|d| d["name"] == name)
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_owned()
}

#[tokio::test]
async fn snapshots_rollback_and_clone() {
    let h = harness().await;
    root(
        &h.app,
        Method::POST,
        "/storage/pools",
        Some(mirror_pool("tank")),
    )
    .await;
    root(
        &h.app,
        Method::POST,
        "/storage/datasets",
        Some(json!({ "name": "tank/images", "kind": "filesystem" })),
    )
    .await;
    h.zfs.add_dataset("tank/images/lx", DatasetKind::Filesystem);

    let r = root(
        &h.app,
        Method::POST,
        "/storage/snapshots",
        Some(json!({ "dataset": "tank/images/lx", "name": "import" })),
    )
    .await;
    assert_eq!(r.status, StatusCode::CREATED, "{}", r.json);
    assert_eq!(r.json["name"], "tank/images/lx@import");
    assert_eq!(r.json["short_name"], "import");
    let snap_id = r.json["id"].as_str().unwrap().to_owned();
    assert_eq!(
        root(
            &h.app,
            Method::POST,
            "/storage/snapshots",
            Some(json!({ "dataset": "tank/images/lx", "name": "import" }))
        )
        .await
        .status,
        StatusCode::CONFLICT
    );
    assert_eq!(
        root(
            &h.app,
            Method::POST,
            "/storage/snapshots",
            Some(json!({ "dataset": "tank/nope", "name": "x" }))
        )
        .await
        .status,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        root(
            &h.app,
            Method::POST,
            "/storage/snapshots",
            Some(json!({ "dataset": "tank/images/lx", "name": "bad name" }))
        )
        .await
        .status,
        StatusCode::UNPROCESSABLE_ENTITY
    );

    // Clone, which then blocks destroying the snapshot.
    let r = root(
        &h.app,
        Method::POST,
        &format!("/storage/snapshots/{snap_id}/clone"),
        Some(json!({ "name": "tank/zones/web1" })),
    )
    .await;
    assert_eq!(r.status, StatusCode::CREATED, "{}", r.json);
    assert_eq!(r.json["origin"], "tank/images/lx@import");
    let r = root(
        &h.app,
        Method::GET,
        &format!("/storage/snapshots/{snap_id}"),
        None,
    )
    .await;
    assert_eq!(r.json["clones"][0], "tank/zones/web1");
    assert_eq!(
        root(
            &h.app,
            Method::DELETE,
            &format!("/storage/snapshots/{snap_id}"),
            None
        )
        .await
        .status,
        StatusCode::CONFLICT
    );

    // A second snapshot makes rollback to the first need discard_newer.
    root(
        &h.app,
        Method::POST,
        "/storage/snapshots",
        Some(json!({ "dataset": "tank/images/lx", "name": "later" })),
    )
    .await;
    let r = root(
        &h.app,
        Method::POST,
        &format!("/storage/snapshots/{snap_id}/rollback"),
        None,
    )
    .await;
    assert_eq!(r.status, StatusCode::CONFLICT, "{}", r.json);
    let r = root(
        &h.app,
        Method::POST,
        &format!("/storage/snapshots/{snap_id}/rollback"),
        Some(json!({ "discard_newer": true })),
    )
    .await;
    assert_eq!(r.status, StatusCode::NO_CONTENT, "{}", r.json);
    let r = root(
        &h.app,
        Method::GET,
        "/storage/snapshots?dataset=tank/images/lx",
        None,
    )
    .await;
    assert_eq!(r.json["items"].as_array().unwrap().len(), 1);

    // Recursive listing and ids stable.
    let r = root(
        &h.app,
        Method::GET,
        "/storage/snapshots?dataset=tank&recursive=true",
        None,
    )
    .await;
    assert_eq!(r.json["items"][0]["id"], snap_id);
    let audit = root(&h.app, Method::GET, "/audit?action=snapshot.clone", None).await;
    assert_eq!(audit.json["items"][0]["after"]["clone"], "tank/zones/web1");
}
