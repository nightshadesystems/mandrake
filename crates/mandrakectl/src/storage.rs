//! `storage` commands: one API call each, printed as a table, key/value
//! block, or JSON.

use mandrake_core::{
    api::{Job, Page},
    storage::{Dataset, Device, Pool, Snapshot, Vdev},
};
use serde_json::{Map, Value, json};

use crate::{
    cli::{
        DatasetCreateArgs, DatasetUpdateArgs, DatasetsCmd, MetadataArgs, PoolsCmd, SnapshotsCmd,
        StorageCmd,
    },
    client::{Client, Error},
    cmd::{done, pages},
    output,
};

/// The metadata flags as a request body, or `None` when nothing was given.
pub(crate) fn metadata_value(m: &MetadataArgs) -> Option<Value> {
    let mut body = Map::new();
    if let Some(d) = &m.display_name {
        body.insert("display_name".to_owned(), Value::String(d.clone()));
    }
    if let Some(d) = &m.description {
        body.insert("description".to_owned(), Value::String(d.clone()));
    }
    if !m.tags.is_empty() {
        body.insert("tags".to_owned(), json!(m.tags));
    }
    if let Some(n) = &m.notes {
        body.insert("notes".to_owned(), Value::String(n.clone()));
    }
    (!body.is_empty()).then_some(Value::Object(body))
}

/// `10G`, `512M`, `1.5T`, `4K`, or plain bytes.
pub(crate) fn parse_size(text: &str) -> Result<u64, Error> {
    let t = text.trim();
    let digits_end = t
        .find(|c: char| !(c.is_ascii_digit() || c == '.'))
        .unwrap_or(t.len());
    let (number, unit) = t.split_at(digits_end);
    let value: f64 = number
        .parse()
        .map_err(|_| Error::Config(format!("not a size: {text}")))?;
    let unit = unit.trim().to_ascii_lowercase();
    let unit = unit.trim_end_matches('b').trim_end_matches('i');
    let mult: f64 = match unit {
        "" => 1.0,
        "k" => 1024.0,
        "m" => 1024.0 * 1024.0,
        "g" => 1024.0 * 1024.0 * 1024.0,
        "t" => 1024.0 * 1024.0 * 1024.0 * 1024.0,
        "p" => 1024.0 * 1024.0 * 1024.0 * 1024.0 * 1024.0,
        _ => return Err(Error::Config(format!("not a size: {text}"))),
    };
    let bytes = value * mult;
    if !bytes.is_finite() || bytes < 0.0 {
        return Err(Error::Config(format!("not a size: {text}")));
    }
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // checked above
    Ok(bytes.round() as u64)
}

/// A size flag: `none` clears, anything else must parse.
fn size_patch(text: &str) -> Result<Value, Error> {
    if text.eq_ignore_ascii_case("none") {
        Ok(Value::Null)
    } else {
        Ok(json!(parse_size(text)?))
    }
}

/// `TYPE:DEV[,DEV...]` to a vdev spec.
fn parse_vdev(text: &str) -> Result<Value, Error> {
    let (type_, devices) = text
        .split_once(':')
        .ok_or_else(|| Error::Config(format!("vdev must be TYPE:DEV[,DEV...]: {text}")))?;
    let devices: Vec<&str> = devices
        .split(',')
        .map(str::trim)
        .filter(|d| !d.is_empty())
        .collect();
    if devices.is_empty() {
        return Err(Error::Config(format!("vdev has no devices: {text}")));
    }
    if !matches!(
        type_,
        "stripe" | "mirror" | "raidz1" | "raidz2" | "raidz3" | "log" | "cache" | "spare"
    ) {
        return Err(Error::Config(format!("unknown vdev type: {type_}")));
    }
    Ok(json!({ "type": type_, "devices": devices }))
}

fn more(page_cursor: Option<&str>) {
    if let Some(c) = page_cursor {
        eprintln!("more: --cursor {c}");
    }
}

fn pct(v: Option<u32>) -> String {
    v.map_or_else(|| "-".to_owned(), |p| format!("{p}%"))
}

pub async fn run(client: &Client, cmd: StorageCmd, json: bool) -> Result<(), Error> {
    match cmd {
        StorageCmd::Devices => devices(client, json).await,
        StorageCmd::Pools(cmd) => pools(client, cmd, json).await,
        StorageCmd::Datasets(cmd) => datasets(client, cmd, json).await,
        StorageCmd::Volumes { pool, paging } => {
            let mut query = Vec::new();
            if let Some(p) = pool {
                query.push(("pool", p));
            }
            let page: Page<Dataset> = pages(client, "/storage/volumes", &query, &paging).await?;
            print_datasets(&page, json)
        }
        StorageCmd::Snapshots(cmd) => snapshots(client, cmd, json).await,
    }
}

// ------------------------------------------------------------ devices

async fn devices(client: &Client, json: bool) -> Result<(), Error> {
    let list: Page<Device> = client.json("GET", "/storage/devices", &[], None).await?;
    if json {
        output::json(&serde_json::to_value(&list)?);
        return Ok(());
    }
    let rows: Vec<Vec<String>> = list
        .items
        .iter()
        .map(|d| {
            vec![
                d.name.clone(),
                output::size(d.size_bytes),
                [d.vendor.as_deref(), d.product.as_deref()]
                    .into_iter()
                    .flatten()
                    .collect::<Vec<_>>()
                    .join(" "),
                output::opt(d.serial.as_deref()),
                match d.solid_state {
                    Some(true) => "ssd",
                    Some(false) => "hdd",
                    None => "-",
                }
                .to_owned(),
                d.pool.clone().unwrap_or_else(|| "free".to_owned()),
            ]
        })
        .collect();
    output::table(
        &["DEVICE", "SIZE", "MODEL", "SERIAL", "KIND", "POOL"],
        &rows,
    );
    Ok(())
}

// ------------------------------------------------------------ pools

const POOL_HEADERS: [&str; 9] = [
    "ID", "NAME", "HEALTH", "SIZE", "ALLOC", "FREE", "CAP", "FRAG", "SCAN",
];

fn pool_row(p: &Pool) -> Vec<String> {
    vec![
        p.id.to_string(),
        if p.protected {
            format!("{} (protected)", p.name)
        } else {
            p.name.clone()
        },
        p.health.to_string(),
        output::size(p.size_bytes),
        output::size(p.allocated_bytes),
        output::size(p.free_bytes),
        pct(p.capacity_percent),
        pct(p.fragmentation_percent),
        p.scan.as_ref().map_or_else(
            || "-".to_owned(),
            |s| {
                let state = format!("{:?}", s.state).to_lowercase();
                match s.progress {
                    Some(pr) if state == "in_progress" => {
                        format!("{:?} {pr:.0}%", s.function).to_lowercase()
                    }
                    _ => format!("{:?} {state}", s.function).to_lowercase(),
                }
            },
        ),
    ]
}

fn vdev_lines(v: &Vdev, depth: usize, out: &mut Vec<Vec<String>>) {
    let errors = [v.read_errors, v.write_errors, v.checksum_errors]
        .iter()
        .map(|e| e.map_or_else(|| "-".to_owned(), |n| n.to_string()))
        .collect::<Vec<_>>()
        .join("/");
    out.push(vec![
        format!("{}{}", "  ".repeat(depth), v.name),
        format!("{:?}", v.type_).to_lowercase(),
        v.state.to_string(),
        errors,
        output::opt(v.note.as_deref()),
    ]);
    for c in &v.children {
        vdev_lines(c, depth + 1, out);
    }
}

fn print_pool(p: &Pool, json: bool) -> Result<(), Error> {
    if json {
        output::json(&serde_json::to_value(p)?);
        return Ok(());
    }
    let mut pairs = vec![
        ("id", p.id.to_string()),
        ("name", p.name.clone()),
        ("health", p.health.to_string()),
        ("size", output::size(p.size_bytes)),
        ("allocated", output::size(p.allocated_bytes)),
        ("free", output::size(p.free_bytes)),
        ("capacity", pct(p.capacity_percent)),
        ("fragmentation", pct(p.fragmentation_percent)),
        ("protected", p.protected.to_string()),
    ];
    if let Some(m) = &p.metadata {
        if let Some(d) = &m.display_name {
            pairs.push(("display name", d.clone()));
        }
        if let Some(d) = &m.description {
            pairs.push(("description", d.clone()));
        }
    }
    if let Some(s) = &p.scan {
        pairs.push(("scan", s.summary.clone()));
    }
    if let Some(t) = &p.status_text {
        pairs.push(("status", t.clone()));
    }
    output::kv(&pairs);
    println!();
    let mut rows = Vec::new();
    vdev_lines(&p.vdevs, 0, &mut rows);
    output::table(&["VDEV", "TYPE", "STATE", "R/W/C", "NOTE"], &rows);
    Ok(())
}

fn print_job(j: &Job, json: bool) -> Result<(), Error> {
    if json {
        output::json(&serde_json::to_value(j)?);
    } else {
        output::kv(&[
            ("job", j.id.to_string()),
            ("state", format!("{:?}", j.state).to_lowercase()),
            ("kind", j.kind.clone()),
            ("message", output::opt(j.message.as_deref())),
        ]);
    }
    Ok(())
}

async fn pools(client: &Client, cmd: PoolsCmd, json: bool) -> Result<(), Error> {
    match cmd {
        PoolsCmd::List { paging } => {
            let page: Page<Pool> = pages(client, "/storage/pools", &[], &paging).await?;
            if json {
                output::json(&serde_json::to_value(&page)?);
            } else {
                let rows: Vec<_> = page.items.iter().map(pool_row).collect();
                output::table(&POOL_HEADERS, &rows);
                more(page.next_cursor.as_deref());
            }
        }
        PoolsCmd::Get { id } => {
            let p: Pool = client
                .json("GET", &format!("/storage/pools/{id}"), &[], None)
                .await?;
            print_pool(&p, json)?;
        }
        PoolsCmd::Create {
            name,
            vdevs,
            ashift,
            compression,
            force,
            metadata,
        } => {
            let vdevs = vdevs
                .iter()
                .map(|v| parse_vdev(v))
                .collect::<Result<Vec<_>, _>>()?;
            let body = json!({
                "name": name,
                "vdevs": vdevs,
                "ashift": ashift,
                "compression": compression,
                "force": force,
                "metadata": metadata_value(&metadata),
            });
            let p: Pool = client
                .json("POST", "/storage/pools", &[], Some(&body))
                .await?;
            print_pool(&p, json)?;
        }
        PoolsCmd::Update { id, metadata } => {
            let body = metadata_value(&metadata)
                .ok_or_else(|| Error::Config("nothing to update".to_owned()))?;
            let p: Pool = client
                .json("PATCH", &format!("/storage/pools/{id}"), &[], Some(&body))
                .await?;
            print_pool(&p, json)?;
        }
        PoolsCmd::Destroy { id, name } => {
            client
                .empty(
                    "DELETE",
                    &format!("/storage/pools/{id}"),
                    Some(&json!({ "name": name })),
                )
                .await?;
            done(json, "destroyed", id);
        }
        PoolsCmd::Scrub { id, stop } => {
            if stop {
                client
                    .empty("DELETE", &format!("/storage/pools/{id}/scrub"), None)
                    .await?;
                done(json, "scrub stopped", id);
            } else {
                let j: Job = client
                    .json("POST", &format!("/storage/pools/{id}/scrub"), &[], None)
                    .await?;
                print_job(&j, json)?;
            }
        }
    }
    Ok(())
}

// ------------------------------------------------------------ datasets

const DATASET_HEADERS: [&str; 8] = [
    "ID",
    "NAME",
    "KIND",
    "USED",
    "AVAIL",
    "REFER",
    "QUOTA",
    "MOUNTPOINT",
];

fn dataset_row(d: &Dataset) -> Vec<String> {
    vec![
        d.id.to_string(),
        if d.protected {
            format!("{} (protected)", d.name)
        } else {
            d.name.clone()
        },
        format!("{:?}", d.kind).to_lowercase(),
        output::size(d.used_bytes),
        output::size(d.available_bytes),
        output::size(d.referenced_bytes),
        d.quota_bytes.map_or_else(|| "-".to_owned(), output::size),
        match d.kind {
            mandrake_core::storage::DatasetKind::Volume => d.volsize_bytes.map_or_else(
                || "-".to_owned(),
                |v| format!("volsize {}", output::size(v)),
            ),
            mandrake_core::storage::DatasetKind::Filesystem => output::opt(d.mountpoint.as_deref()),
        },
    ]
}

fn print_datasets(page: &Page<Dataset>, json: bool) -> Result<(), Error> {
    if json {
        output::json(&serde_json::to_value(page)?);
    } else {
        let rows: Vec<_> = page.items.iter().map(dataset_row).collect();
        output::table(&DATASET_HEADERS, &rows);
        more(page.next_cursor.as_deref());
    }
    Ok(())
}

fn print_dataset(d: &Dataset, json: bool) -> Result<(), Error> {
    if json {
        output::json(&serde_json::to_value(d)?);
        return Ok(());
    }
    let opt_size = |v: Option<u64>| v.map_or_else(|| "-".to_owned(), output::size);
    let mut pairs = vec![
        ("id", d.id.to_string()),
        ("name", d.name.clone()),
        ("pool", d.pool.clone()),
        ("kind", format!("{:?}", d.kind).to_lowercase()),
        ("used", output::size(d.used_bytes)),
        ("available", output::size(d.available_bytes)),
        ("referenced", output::size(d.referenced_bytes)),
        ("quota", opt_size(d.quota_bytes)),
        ("reservation", opt_size(d.reservation_bytes)),
        ("compression", output::opt(d.compression.as_deref())),
        (
            "compress ratio",
            d.compress_ratio
                .map_or_else(|| "-".to_owned(), |r| format!("{r:.2}x")),
        ),
        ("protected", d.protected.to_string()),
        ("created", d.created_at.to_rfc3339()),
    ];
    match d.kind {
        mandrake_core::storage::DatasetKind::Volume => {
            pairs.push(("volsize", opt_size(d.volsize_bytes)));
            pairs.push(("volblocksize", opt_size(d.volblocksize_bytes)));
        }
        mandrake_core::storage::DatasetKind::Filesystem => {
            pairs.push(("mountpoint", output::opt(d.mountpoint.as_deref())));
            pairs.push(("mounted", d.mounted.to_string()));
            pairs.push(("recordsize", opt_size(d.recordsize_bytes)));
            pairs.push((
                "atime",
                d.atime.map_or_else(|| "-".to_owned(), |a| a.to_string()),
            ));
        }
    }
    if let Some(o) = &d.origin {
        pairs.push(("origin", o.clone()));
    }
    if let Some(m) = &d.metadata {
        if let Some(n) = &m.display_name {
            pairs.push(("display name", n.clone()));
        }
        if let Some(n) = &m.description {
            pairs.push(("description", n.clone()));
        }
    }
    output::kv(&pairs);
    Ok(())
}

/// The `POST /storage/datasets` body from the create flags.
fn dataset_create_body(a: DatasetCreateArgs) -> Result<Value, Error> {
    let volume = a.size.is_some();
    let mut body = Map::new();
    body.insert("name".to_owned(), Value::String(a.name));
    body.insert(
        "kind".to_owned(),
        Value::String(if volume { "volume" } else { "filesystem" }.to_owned()),
    );
    if let Some(s) = a.size {
        body.insert("volsize_bytes".to_owned(), json!(parse_size(&s)?));
        body.insert("sparse".to_owned(), Value::Bool(a.sparse));
    }
    if let Some(v) = a.volblocksize {
        body.insert("volblocksize_bytes".to_owned(), json!(parse_size(&v)?));
    }
    if let Some(c) = a.compression {
        body.insert("compression".to_owned(), Value::String(c));
    }
    if let Some(q) = a.quota {
        body.insert("quota_bytes".to_owned(), json!(parse_size(&q)?));
    }
    if let Some(r) = a.reservation {
        body.insert("reservation_bytes".to_owned(), json!(parse_size(&r)?));
    }
    if let Some(m) = a.mountpoint {
        body.insert("mountpoint".to_owned(), Value::String(m));
    }
    if let Some(r) = a.recordsize {
        body.insert("recordsize_bytes".to_owned(), json!(parse_size(&r)?));
    }
    if a.no_atime {
        body.insert("atime".to_owned(), Value::Bool(false));
    }
    body.insert("create_parents".to_owned(), Value::Bool(a.parents));
    if let Some(m) = metadata_value(&a.metadata) {
        body.insert("metadata".to_owned(), m);
    }
    Ok(Value::Object(body))
}

/// The `PATCH /storage/datasets/{id}` body from the update flags; an
/// error when nothing was given.
fn dataset_update_body(a: DatasetUpdateArgs) -> Result<Value, Error> {
    let mut body = Map::new();
    if let Some(s) = a.size {
        body.insert("volsize_bytes".to_owned(), json!(parse_size(&s)?));
    }
    if let Some(c) = a.compression {
        body.insert("compression".to_owned(), Value::String(c));
    }
    if let Some(q) = a.quota {
        body.insert("quota_bytes".to_owned(), size_patch(&q)?);
    }
    if let Some(r) = a.reservation {
        body.insert("reservation_bytes".to_owned(), size_patch(&r)?);
    }
    if let Some(m) = a.mountpoint {
        body.insert("mountpoint".to_owned(), Value::String(m));
    }
    if let Some(at) = a.atime {
        body.insert("atime".to_owned(), Value::Bool(at));
    }
    if let Some(m) = metadata_value(&a.metadata) {
        body.insert("metadata".to_owned(), m);
    }
    if body.is_empty() {
        return Err(Error::Config("nothing to update".to_owned()));
    }
    Ok(Value::Object(body))
}

async fn datasets(client: &Client, cmd: DatasetsCmd, json: bool) -> Result<(), Error> {
    match cmd {
        DatasetsCmd::List {
            pool,
            parent,
            kind,
            paging,
        } => {
            let mut query = Vec::new();
            if let Some(p) = pool {
                query.push(("pool", p));
            }
            if let Some(p) = parent {
                query.push(("parent", p));
            }
            if let Some(k) = kind {
                query.push(("kind", k));
            }
            let page: Page<Dataset> = pages(client, "/storage/datasets", &query, &paging).await?;
            print_datasets(&page, json)?;
        }
        DatasetsCmd::Get { id } => {
            let d: Dataset = client
                .json("GET", &format!("/storage/datasets/{id}"), &[], None)
                .await?;
            print_dataset(&d, json)?;
        }
        DatasetsCmd::Create(args) => {
            let body = dataset_create_body(args)?;
            let d: Dataset = client
                .json("POST", "/storage/datasets", &[], Some(&body))
                .await?;
            print_dataset(&d, json)?;
        }
        DatasetsCmd::Update(args) => {
            let id = args.id;
            let body = dataset_update_body(args)?;
            let d: Dataset = client
                .json(
                    "PATCH",
                    &format!("/storage/datasets/{id}"),
                    &[],
                    Some(&body),
                )
                .await?;
            print_dataset(&d, json)?;
        }
        DatasetsCmd::Destroy { id, recursive } => {
            let path = if recursive {
                format!("/storage/datasets/{id}?recursive=true")
            } else {
                format!("/storage/datasets/{id}")
            };
            client.empty("DELETE", &path, None).await?;
            done(json, "destroyed", id);
        }
    }
    Ok(())
}

// ------------------------------------------------------------ snapshots

const SNAPSHOT_HEADERS: [&str; 6] = ["ID", "NAME", "USED", "REFER", "CREATED", "CLONES"];

fn snapshot_row(s: &Snapshot) -> Vec<String> {
    vec![
        s.id.to_string(),
        s.name.clone(),
        output::size(s.used_bytes),
        output::size(s.referenced_bytes),
        s.created_at.to_rfc3339(),
        if s.clones.is_empty() {
            "-".to_owned()
        } else {
            s.clones.join(",")
        },
    ]
}

fn print_snapshot(s: &Snapshot, json: bool) -> Result<(), Error> {
    if json {
        output::json(&serde_json::to_value(s)?);
    } else {
        output::table(&SNAPSHOT_HEADERS, &[snapshot_row(s)]);
    }
    Ok(())
}

async fn snapshots(client: &Client, cmd: SnapshotsCmd, json: bool) -> Result<(), Error> {
    match cmd {
        SnapshotsCmd::List {
            dataset,
            recursive,
            paging,
        } => {
            let mut query = Vec::new();
            if let Some(d) = dataset {
                query.push(("dataset", d));
            }
            if recursive {
                query.push(("recursive", "true".to_owned()));
            }
            let page: Page<Snapshot> = pages(client, "/storage/snapshots", &query, &paging).await?;
            if json {
                output::json(&serde_json::to_value(&page)?);
            } else {
                let rows: Vec<_> = page.items.iter().map(snapshot_row).collect();
                output::table(&SNAPSHOT_HEADERS, &rows);
                more(page.next_cursor.as_deref());
            }
        }
        SnapshotsCmd::Get { id } => {
            let s: Snapshot = client
                .json("GET", &format!("/storage/snapshots/{id}"), &[], None)
                .await?;
            print_snapshot(&s, json)?;
        }
        SnapshotsCmd::Create {
            dataset,
            name,
            recursive,
            metadata,
        } => {
            let body = json!({
                "dataset": dataset,
                "name": name,
                "recursive": recursive,
                "metadata": metadata_value(&metadata),
            });
            let s: Snapshot = client
                .json("POST", "/storage/snapshots", &[], Some(&body))
                .await?;
            print_snapshot(&s, json)?;
        }
        SnapshotsCmd::Destroy { id } => {
            client
                .empty("DELETE", &format!("/storage/snapshots/{id}"), None)
                .await?;
            done(json, "destroyed", id);
        }
        SnapshotsCmd::Rollback { id, discard_newer } => {
            client
                .empty(
                    "POST",
                    &format!("/storage/snapshots/{id}/rollback"),
                    Some(&json!({ "discard_newer": discard_newer })),
                )
                .await?;
            done(json, "rolled back", id);
        }
        SnapshotsCmd::Clone { id, target } => {
            let d: Dataset = client
                .json(
                    "POST",
                    &format!("/storage/snapshots/{id}/clone"),
                    &[],
                    Some(&json!({ "name": target })),
                )
                .await?;
            print_dataset(&d, json)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    #[test]
    fn sizes() {
        assert_eq!(parse_size("1024").unwrap(), 1024);
        assert_eq!(parse_size("4K").unwrap(), 4096);
        assert_eq!(parse_size("10G").unwrap(), 10 * 1024 * 1024 * 1024);
        assert_eq!(parse_size("1.5t").unwrap(), 1_649_267_441_664);
        assert_eq!(parse_size("512 MiB").unwrap(), 512 * 1024 * 1024);
        assert!(parse_size("ten").is_err());
        assert!(parse_size("10X").is_err());
        assert_eq!(size_patch("none").unwrap(), Value::Null);
    }

    #[test]
    fn vdevs() {
        assert_eq!(
            parse_vdev("mirror:c1t1d0,c1t2d0").unwrap(),
            json!({ "type": "mirror", "devices": ["c1t1d0", "c1t2d0"] })
        );
        assert!(parse_vdev("mirror").is_err());
        assert!(parse_vdev("raid5:c1t1d0").is_err());
        assert!(parse_vdev("log:").is_err());
    }

    #[test]
    fn metadata_flags() {
        assert!(metadata_value(&MetadataArgs::default()).is_none());
        let m = MetadataArgs {
            display_name: Some("Data".to_owned()),
            tags: vec!["lab".to_owned()],
            ..MetadataArgs::default()
        };
        assert_eq!(
            metadata_value(&m).unwrap(),
            json!({ "display_name": "Data", "tags": ["lab"] })
        );
    }
}
