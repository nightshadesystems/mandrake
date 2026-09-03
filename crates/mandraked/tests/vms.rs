//! VM routes against the fake drivers.

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

const RAW: &[u8] = b"pretend this is a raw disk image";
const RAW_URL: &str = "https://images.example/alpine.raw.xz";
const ISO: &[u8] = b"pretend this is an iso";
const ISO_URL: &str = "https://images.example/installer.iso";

struct Harness {
    app: Router,
    zfs: Arc<FakeZfs>,
    zones: Arc<FakeZones>,
}

async fn harness() -> Harness {
    let transport = FakeTransport::new();
    transport
        .add(RAW_URL, RAW.to_vec())
        .add(ISO_URL, ISO.to_vec());
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

/// Import an image directly and, for raw images, make the zvol and its
/// `@image` snapshot exist in the fake ZFS.
async fn ready_image(h: &Harness, name: &str, type_: &str, url: &str, body: &[u8]) -> String {
    let r = root(
        &h.app,
        Method::POST,
        "/images/import",
        Some(json!({
            "name": name, "version": "1", "type": type_, "url": url,
            "sha256": hex(&Sha256::digest(body)), "pool": "rpool"
        })),
    )
    .await;
    assert_eq!(r.status, StatusCode::ACCEPTED, "{}", r.json);
    let job = wait_job(&h.app, r.json["id"].as_str().unwrap()).await;
    assert_eq!(job["state"], "succeeded", "{job}");
    let image_id = r.json["target"]["id"].as_str().unwrap().to_owned();
    if type_ == "vm-raw" {
        h.zfs.add_dataset("rpool/images", DatasetKind::Filesystem);
        let zvol = format!("rpool/images/{image_id}");
        h.zfs.add_dataset(&zvol, DatasetKind::Volume);
        h.zfs
            .set_properties(&zvol, &[("volsize".to_owned(), "2147483648".to_owned())])
            .await
            .unwrap();
        h.zfs.create_snapshot(&zvol, "image", false).await.unwrap();
    }
    image_id
}

async fn vm(app: &Router, id: &str) -> Value {
    let r = root(app, Method::GET, &format!("/vms/{id}"), None).await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.json);
    r.json
}

#[tokio::test]
async fn vm_lifecycle_with_disks_cdroms_and_snapshots() {
    let h = harness().await;
    let raw = ready_image(&h, "alpine", "vm-raw", RAW_URL, RAW).await;
    let iso = ready_image(&h, "installer", "vm-iso", ISO_URL, ISO).await;

    let r = root(&h.app, Method::GET, "/vms", None).await;
    assert_eq!(r.status, StatusCode::OK);
    assert_eq!(items(&r.json).len(), 0);

    // Bad requests.
    for body in [
        json!({ "name": "vm0", "vcpus": 0, "memory_bytes": 1073741824, "disks": [{ "size_bytes": 10737418240u64 }] }),
        json!({ "name": "vm0", "vcpus": 2, "memory_bytes": 1024, "disks": [{ "size_bytes": 10737418240u64 }] }),
        json!({ "name": "vm0", "vcpus": 2, "memory_bytes": 1073741824, "disks": [] }),
        json!({ "name": "vm0", "vcpus": 2, "memory_bytes": 1073741824, "disks": [{}] }),
        json!({ "name": "vm0", "vcpus": 2, "memory_bytes": 1073741824,
                "disks": [{ "image_id": iso }] }),
        json!({ "name": "vm0", "vcpus": 2, "memory_bytes": 1073741824,
                "disks": [{ "image_id": raw }], "cdroms": [raw] }),
        json!({ "name": "build", "vcpus": 2, "memory_bytes": 1073741824,
                "disks": [{ "image_id": raw }] }),
    ] {
        let r = root(&h.app, Method::POST, "/vms", Some(body.clone())).await;
        assert!(
            r.status == StatusCode::UNPROCESSABLE_ENTITY || r.status == StatusCode::CONFLICT,
            "{body}: {} {}",
            r.status,
            r.json
        );
    }

    let r = root(
        &h.app,
        Method::POST,
        "/vms",
        Some(json!({
            "name": "vm0", "vcpus": 2, "memory_bytes": 2147483648u64,
            "disks": [{ "image_id": raw }, { "size_bytes": 10737418240u64 }],
            "cdroms": [iso],
            "nics": [{ "name": "net0", "over": "e1000g1" }],
            "metadata": { "display_name": "Alpine" }
        })),
    )
    .await;
    assert_eq!(r.status, StatusCode::ACCEPTED, "{}", r.json);
    assert_eq!(r.json["kind"], "vm.create");
    let vm_id = r.json["target"]["id"].as_str().unwrap().to_owned();
    assert_eq!(vm(&h.app, &vm_id).await["state"], "configured");
    let job = wait_job(&h.app, r.json["id"].as_str().unwrap()).await;
    assert_eq!(job["state"], "succeeded", "{job}");

    let v = vm(&h.app, &vm_id).await;
    assert_eq!(v["state"], "running");
    assert_eq!(v["vcpus"], 2);
    assert_eq!(v["memory_bytes"], 2147483648u64);
    assert_eq!(v["bootrom"], "uefi");
    assert_eq!(v["acpi"], true);
    assert_eq!(v["vnc"], true);
    assert_eq!(v["zonepath"], "/rpool/vms/vm0");
    assert_eq!(v["dataset"], "rpool/vms/vm0");
    assert_eq!(v["image_id"], raw);
    let disks = v["disks"].as_array().unwrap();
    assert_eq!(disks.len(), 2);
    assert_eq!(disks[0]["index"], 0);
    assert_eq!(disks[0]["boot"], true);
    assert_eq!(disks[0]["dataset"], "rpool/vms/vm0/disk0");
    assert_eq!(disks[0]["image_id"], raw);
    assert_eq!(disks[0]["size_bytes"], 2147483648u64);
    assert_eq!(disks[1]["index"], 1);
    assert_eq!(disks[1]["boot"], false);
    assert_eq!(disks[1]["size_bytes"], 10737418240u64);
    let cdroms = v["cdroms"].as_array().unwrap();
    assert_eq!(cdroms.len(), 1);
    assert_eq!(cdroms[0]["image_id"], iso);
    assert_eq!(v["nics"][0]["over"], "e1000g1");
    assert_eq!(v["metadata"]["display_name"], "Alpine");
    // The brand attributes are in the zonecfg.
    let cfg = h.zones.config("vm0").await.unwrap();
    assert_eq!(cfg.brand, "bhyve");
    assert_eq!(
        cfg.attrs.get("bootdisk").map(String::as_str),
        Some("rpool/vms/vm0/disk0")
    );
    assert_eq!(
        cfg.attrs.get("disk1").map(String::as_str),
        Some("rpool/vms/vm0/disk1")
    );
    assert_eq!(cfg.attrs.get("ram").map(String::as_str), Some("2048M"));
    assert_eq!(cfg.devices.len(), 2);
    assert_eq!(cfg.fs.len(), 1);
    // The VM is not a zone.
    let zl = root(&h.app, Method::GET, "/zones", None).await;
    assert!(!items(&zl.json).iter().any(|z| z["name"] == "vm0"));
    let vl = root(&h.app, Method::GET, "/vms?state=running", None).await;
    assert_eq!(items(&vl.json).len(), 1);

    // Update while running is written and flagged for the next boot.
    let r = root(
        &h.app,
        Method::PATCH,
        &format!("/vms/{vm_id}"),
        Some(json!({ "vcpus": 4, "bootrom": "uefi-csm", "vnc": false, "nics": [] })),
    )
    .await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.json);
    assert_eq!(r.json["vcpus"], 4);
    assert_eq!(r.json["bootrom"], "uefi-csm");
    assert_eq!(r.json["vnc"], false);
    assert_eq!(r.json["nics"], json!([]));

    // Snapshots while running are crash-consistent; rollback needs a stop.
    let r = root(
        &h.app,
        Method::POST,
        &format!("/vms/{vm_id}/snapshots"),
        Some(json!({ "name": "before-upgrade", "metadata": { "notes": "pre" } })),
    )
    .await;
    assert_eq!(r.status, StatusCode::CREATED, "{}", r.json);
    assert_eq!(r.json["name"], "before-upgrade");
    assert_eq!(r.json["metadata"]["notes"], "pre");
    let snap_id = r.json["id"].as_str().unwrap().to_owned();
    let r = root(
        &h.app,
        Method::GET,
        &format!("/vms/{vm_id}/snapshots"),
        None,
    )
    .await;
    assert_eq!(items(&r.json).len(), 1);
    assert_eq!(items(&r.json)[0]["id"], snap_id);
    let storage = root(
        &h.app,
        Method::GET,
        "/storage/snapshots?dataset=rpool/vms/vm0&recursive=true",
        None,
    )
    .await;
    assert_eq!(items(&storage.json).len(), 3);
    let r = root(
        &h.app,
        Method::POST,
        &format!("/vms/{vm_id}/snapshots/{snap_id}/rollback"),
        None,
    )
    .await;
    assert_eq!(r.status, StatusCode::CONFLICT);

    // Reset and stop.
    let r = root(&h.app, Method::POST, &format!("/vms/{vm_id}/reset"), None).await;
    assert_eq!(r.status, StatusCode::ACCEPTED);
    assert_eq!(
        wait_job(&h.app, r.json["id"].as_str().unwrap()).await["state"],
        "succeeded"
    );
    assert_eq!(vm(&h.app, &vm_id).await["state"], "running");
    let r = root(&h.app, Method::POST, &format!("/vms/{vm_id}/stop"), None).await;
    assert_eq!(r.status, StatusCode::ACCEPTED);
    assert_eq!(
        wait_job(&h.app, r.json["id"].as_str().unwrap()).await["state"],
        "succeeded"
    );
    assert_eq!(vm(&h.app, &vm_id).await["state"], "installed");
    let r = root(&h.app, Method::POST, &format!("/vms/{vm_id}/reset"), None).await;
    assert_eq!(r.status, StatusCode::CONFLICT);

    // Rollback now works.
    let r = root(
        &h.app,
        Method::POST,
        &format!("/vms/{vm_id}/snapshots/{snap_id}/rollback"),
        None,
    )
    .await;
    assert_eq!(r.status, StatusCode::NO_CONTENT, "{}", r.json);

    // Disks: add, resize (only up), remove (not the boot disk), purge.
    let r = root(
        &h.app,
        Method::POST,
        &format!("/vms/{vm_id}/disks"),
        Some(json!({ "size_bytes": 5368709120u64 })),
    )
    .await;
    assert_eq!(r.status, StatusCode::CREATED, "{}", r.json);
    assert_eq!(r.json["disks"].as_array().unwrap().len(), 3);
    assert_eq!(r.json["disks"][2]["dataset"], "rpool/vms/vm0/disk2");
    let r = root(
        &h.app,
        Method::PATCH,
        &format!("/vms/{vm_id}/disks/2"),
        Some(json!({ "size_bytes": 1024 })),
    )
    .await;
    assert_eq!(r.status, StatusCode::UNPROCESSABLE_ENTITY);
    let r = root(
        &h.app,
        Method::PATCH,
        &format!("/vms/{vm_id}/disks/2"),
        Some(json!({ "size_bytes": 10737418240u64 })),
    )
    .await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.json);
    assert_eq!(r.json["disks"][2]["size_bytes"], 10737418240u64);
    let r = root(
        &h.app,
        Method::DELETE,
        &format!("/vms/{vm_id}/disks/0"),
        None,
    )
    .await;
    assert_eq!(r.status, StatusCode::CONFLICT);
    let r = root(
        &h.app,
        Method::DELETE,
        &format!("/vms/{vm_id}/disks/2?purge=true"),
        None,
    )
    .await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.json);
    assert_eq!(r.json["disks"].as_array().unwrap().len(), 2);
    let ds = root(&h.app, Method::GET, "/storage/volumes?pool=rpool", None).await;
    assert!(
        !items(&ds.json)
            .iter()
            .any(|d| d["name"] == "rpool/vms/vm0/disk2")
    );
    let r = root(
        &h.app,
        Method::DELETE,
        &format!("/vms/{vm_id}/disks/7"),
        None,
    )
    .await;
    assert_eq!(r.status, StatusCode::NOT_FOUND);

    // Cdroms: detach, re-attach, duplicate.
    let r = root(
        &h.app,
        Method::DELETE,
        &format!("/vms/{vm_id}/cdroms/0"),
        None,
    )
    .await;
    assert_eq!(r.status, StatusCode::OK);
    assert_eq!(r.json["cdroms"], json!([]));
    let r = root(
        &h.app,
        Method::POST,
        &format!("/vms/{vm_id}/cdroms"),
        Some(json!({ "image_id": iso })),
    )
    .await;
    assert_eq!(r.status, StatusCode::CREATED, "{}", r.json);
    assert_eq!(r.json["cdroms"][0]["image_id"], iso);
    let r = root(
        &h.app,
        Method::POST,
        &format!("/vms/{vm_id}/cdroms"),
        Some(json!({ "image_id": iso })),
    )
    .await;
    assert_eq!(r.status, StatusCode::CONFLICT);

    // Snapshot delete, then VM delete with purge.
    let r = root(
        &h.app,
        Method::DELETE,
        &format!("/vms/{vm_id}/snapshots/{snap_id}"),
        None,
    )
    .await;
    assert_eq!(r.status, StatusCode::NO_CONTENT, "{}", r.json);
    let r = root(
        &h.app,
        Method::GET,
        &format!("/vms/{vm_id}/snapshots"),
        None,
    )
    .await;
    assert_eq!(items(&r.json).len(), 0);
    let r = root(
        &h.app,
        Method::DELETE,
        &format!("/vms/{vm_id}?purge=true"),
        None,
    )
    .await;
    assert_eq!(r.status, StatusCode::ACCEPTED);
    assert_eq!(r.json["kind"], "vm.delete");
    let job = wait_job(&h.app, r.json["id"].as_str().unwrap()).await;
    assert_eq!(job["state"], "succeeded", "{job}");
    let r = root(&h.app, Method::GET, &format!("/vms/{vm_id}"), None).await;
    assert_eq!(r.status, StatusCode::NOT_FOUND);
    let ds = root(&h.app, Method::GET, "/storage/datasets?pool=rpool", None).await;
    assert!(!items(&ds.json).iter().any(|d| d["name"] == "rpool/vms/vm0"));

    let audit = root(&h.app, Method::GET, "/audit?limit=100", None).await;
    let actions: Vec<&str> = audit.json["items"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|a| a["action"].as_str())
        .collect();
    for a in [
        "vm.create",
        "vm.update",
        "vm.snapshot",
        "vm.reset",
        "vm.stop",
        "vm.snapshot.rollback",
        "vm.disk.add",
        "vm.disk.resize",
        "vm.disk.remove",
        "vm.cdrom.detach",
        "vm.cdrom.attach",
        "vm.snapshot.delete",
        "vm.delete",
    ] {
        assert!(actions.contains(&a), "{a} missing from {actions:?}");
    }
}
