//! `vms` commands: one API call each.

use mandrake_core::{
    Id,
    api::{Job, Page},
    vm::{Vm, VmSnapshot},
};
use serde::Deserialize;
use serde_json::{Map, Value, json};

use crate::{
    cli::{VmCdromCmd, VmCreateArgs, VmDiskCmd, VmSnapshotCmd, VmUpdateArgs, VmsCmd},
    client::{Client, Error},
    cmd::{done, pages},
    images::print_job,
    output,
    storage::{metadata_value, parse_size},
    zones::parse_nic,
};

/// `GET /vms/{id}/snapshots`.
#[derive(Debug, Deserialize)]
struct SnapshotList {
    items: Vec<VmSnapshot>,
}

fn nics_value(specs: &[String]) -> Result<Value, Error> {
    let nics = specs
        .iter()
        .map(|s| parse_nic(s))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(serde_json::to_value(nics)?)
}

const VM_HEADERS: [&str; 9] = [
    "ID", "NAME", "STATE", "VCPUS", "MEMORY", "DISKS", "NICS", "VNC", "AUTOBOOT",
];

fn vm_row(v: &Vm) -> Vec<String> {
    vec![
        v.id.to_string(),
        v.name.clone(),
        v.state.to_string(),
        v.vcpus.to_string(),
        output::size(v.memory_bytes),
        if v.disks.is_empty() {
            "-".to_owned()
        } else {
            v.disks
                .iter()
                .map(|d| output::size(d.size_bytes))
                .collect::<Vec<_>>()
                .join(",")
        },
        if v.nics.is_empty() {
            "-".to_owned()
        } else {
            v.nics
                .iter()
                .map(|n| format!("{}@{}", n.name, n.over))
                .collect::<Vec<_>>()
                .join(",")
        },
        if v.vnc { "on" } else { "off" }.to_owned(),
        v.autoboot.to_string(),
    ]
}

fn print_vm(v: &Vm, json: bool) -> Result<(), Error> {
    if json {
        output::json(&serde_json::to_value(v)?);
        return Ok(());
    }
    let mut pairs = vec![
        ("id", v.id.to_string()),
        ("name", v.name.clone()),
        ("state", v.state.to_string()),
        ("vcpus", v.vcpus.to_string()),
        ("memory", output::size(v.memory_bytes)),
        ("bootrom", v.bootrom.to_string()),
        ("acpi", v.acpi.to_string()),
        ("vnc", if v.vnc { "on" } else { "off" }.to_owned()),
        ("autoboot", v.autoboot.to_string()),
        (
            "image",
            v.image_id.map_or_else(|| "-".to_owned(), |i| i.to_string()),
        ),
        ("pool", output::opt(v.pool.as_deref())),
        ("dataset", output::opt(v.dataset.as_deref())),
        ("zonepath", v.zonepath.clone()),
    ];
    if let Some(m) = &v.metadata {
        if let Some(n) = &m.display_name {
            pairs.push(("display name", n.clone()));
        }
        if let Some(n) = &m.description {
            pairs.push(("description", n.clone()));
        }
    }
    output::kv(&pairs);
    if !v.disks.is_empty() {
        println!();
        let rows: Vec<Vec<String>> = v
            .disks
            .iter()
            .map(|d| {
                vec![
                    d.index.to_string(),
                    d.dataset.clone(),
                    output::size(d.size_bytes),
                    if d.boot { "yes" } else { "" }.to_owned(),
                    d.image_id.map_or_else(|| "-".to_owned(), |i| i.to_string()),
                ]
            })
            .collect();
        output::table(&["DISK", "VOLUME", "SIZE", "BOOT", "IMAGE"], &rows);
    }
    if !v.cdroms.is_empty() {
        println!();
        let rows: Vec<Vec<String>> = v
            .cdroms
            .iter()
            .map(|c| vec![c.index.to_string(), c.image_id.to_string(), c.path.clone()])
            .collect();
        output::table(&["CDROM", "IMAGE", "PATH"], &rows);
    }
    if !v.nics.is_empty() {
        println!();
        let rows: Vec<Vec<String>> = v
            .nics
            .iter()
            .map(|n| {
                vec![
                    n.name.clone(),
                    n.over.clone(),
                    n.vid.map_or_else(|| "-".to_owned(), |x| x.to_string()),
                    output::opt(n.mac.as_deref()),
                    output::opt(n.address.as_deref()),
                    output::opt(n.gateway.as_deref()),
                ]
            })
            .collect();
        output::table(&["NIC", "OVER", "VID", "MAC", "ADDRESS", "GATEWAY"], &rows);
    }
    Ok(())
}

fn print_snapshots(items: &[VmSnapshot], json: bool) -> Result<(), Error> {
    if json {
        output::json(&serde_json::to_value(items)?);
        return Ok(());
    }
    let rows: Vec<Vec<String>> = items
        .iter()
        .map(|s| {
            vec![
                s.name.clone(),
                output::ts(Some(s.created_at)),
                output::size(s.used_bytes),
                s.id.to_string(),
            ]
        })
        .collect();
    output::table(&["SNAPSHOT", "CREATED", "USED", "ID"], &rows);
    Ok(())
}

fn create_body(a: &VmCreateArgs) -> Result<Value, Error> {
    let mut disks = Vec::new();
    match (a.image, &a.boot_size) {
        (Some(image), _) => disks.push(json!({ "image_id": image, "boot": true })),
        (None, Some(size)) => disks.push(json!({ "size_bytes": parse_size(size)?, "boot": true })),
        (None, None) => {
            return Err(Error::Config(
                "the boot disk needs --image or --boot-size".to_owned(),
            ));
        }
    }
    for size in &a.disks {
        disks.push(json!({ "size_bytes": parse_size(size)?, "boot": false }));
    }
    let mut body = Map::new();
    body.insert("name".to_owned(), Value::String(a.name.clone()));
    body.insert("vcpus".to_owned(), json!(a.vcpus));
    body.insert("memory_bytes".to_owned(), json!(parse_size(&a.memory)?));
    if let Some(b) = &a.bootrom {
        body.insert("bootrom".to_owned(), Value::String(b.clone()));
    }
    body.insert("acpi".to_owned(), Value::Bool(!a.no_acpi));
    if let Some(p) = &a.pool {
        body.insert("pool".to_owned(), Value::String(p.clone()));
    }
    body.insert("disks".to_owned(), Value::Array(disks));
    body.insert("cdroms".to_owned(), json!(a.cdroms));
    body.insert("nics".to_owned(), nics_value(&a.nics)?);
    body.insert("vnc".to_owned(), Value::Bool(!a.no_vnc));
    body.insert("autoboot".to_owned(), Value::Bool(!a.no_autoboot));
    body.insert("start".to_owned(), Value::Bool(!a.no_start));
    if let Some(m) = metadata_value(&a.metadata) {
        body.insert("metadata".to_owned(), m);
    }
    Ok(Value::Object(body))
}

fn update_body(a: &VmUpdateArgs) -> Result<Value, Error> {
    let mut body = Map::new();
    if let Some(v) = a.vcpus {
        body.insert("vcpus".to_owned(), json!(v));
    }
    if let Some(m) = &a.memory {
        body.insert("memory_bytes".to_owned(), json!(parse_size(m)?));
    }
    if let Some(b) = &a.bootrom {
        body.insert("bootrom".to_owned(), Value::String(b.clone()));
    }
    if let Some(b) = a.acpi {
        body.insert("acpi".to_owned(), Value::Bool(b));
    }
    if let Some(b) = a.vnc {
        body.insert("vnc".to_owned(), Value::Bool(b));
    }
    if let Some(b) = a.autoboot {
        body.insert("autoboot".to_owned(), Value::Bool(b));
    }
    if a.clear_nics {
        body.insert("nics".to_owned(), json!([]));
    } else if !a.nics.is_empty() {
        body.insert("nics".to_owned(), nics_value(&a.nics)?);
    }
    if let Some(m) = metadata_value(&a.metadata) {
        body.insert("metadata".to_owned(), m);
    }
    if body.is_empty() {
        return Err(Error::Config("nothing to update".to_owned()));
    }
    Ok(Value::Object(body))
}

async fn lifecycle(
    client: &Client,
    id: Id,
    verb: &str,
    body: Option<&Value>,
    json: bool,
) -> Result<(), Error> {
    let j: Job = client
        .json("POST", &format!("/vms/{id}/{verb}"), &[], body)
        .await?;
    print_job(&j, json)
}

async fn run_disk(client: &Client, cmd: VmDiskCmd, json: bool) -> Result<(), Error> {
    match cmd {
        VmDiskCmd::Add { id, size, image } => {
            let body = match (image, size) {
                (Some(image), _) => json!({ "image_id": image }),
                (None, Some(size)) => json!({ "size_bytes": parse_size(&size)? }),
                (None, None) => return Err(Error::Config("--size or --image".to_owned())),
            };
            let v: Vm = client
                .json("POST", &format!("/vms/{id}/disks"), &[], Some(&body))
                .await?;
            print_vm(&v, json)?;
        }
        VmDiskCmd::Resize { id, index, size } => {
            let body = json!({ "size_bytes": parse_size(&size)? });
            let v: Vm = client
                .json(
                    "PATCH",
                    &format!("/vms/{id}/disks/{index}"),
                    &[],
                    Some(&body),
                )
                .await?;
            print_vm(&v, json)?;
        }
        VmDiskCmd::Remove { id, index, purge } => {
            let path = if purge {
                format!("/vms/{id}/disks/{index}?purge=true")
            } else {
                format!("/vms/{id}/disks/{index}")
            };
            let v: Vm = client.json("DELETE", &path, &[], None).await?;
            print_vm(&v, json)?;
        }
    }
    Ok(())
}

async fn run_cdrom(client: &Client, cmd: VmCdromCmd, json: bool) -> Result<(), Error> {
    match cmd {
        VmCdromCmd::Attach { id, image } => {
            let v: Vm = client
                .json(
                    "POST",
                    &format!("/vms/{id}/cdroms"),
                    &[],
                    Some(&json!({ "image_id": image })),
                )
                .await?;
            print_vm(&v, json)?;
        }
        VmCdromCmd::Detach { id, index } => {
            let v: Vm = client
                .json("DELETE", &format!("/vms/{id}/cdroms/{index}"), &[], None)
                .await?;
            print_vm(&v, json)?;
        }
    }
    Ok(())
}

async fn run_snapshot(client: &Client, cmd: VmSnapshotCmd, json: bool) -> Result<(), Error> {
    match cmd {
        VmSnapshotCmd::List { id } => {
            let list: SnapshotList = client
                .json("GET", &format!("/vms/{id}/snapshots"), &[], None)
                .await?;
            print_snapshots(&list.items, json)?;
        }
        VmSnapshotCmd::Create { id, name, metadata } => {
            let mut body = Map::new();
            body.insert("name".to_owned(), Value::String(name));
            if let Some(m) = metadata_value(&metadata) {
                body.insert("metadata".to_owned(), m);
            }
            let s: VmSnapshot = client
                .json(
                    "POST",
                    &format!("/vms/{id}/snapshots"),
                    &[],
                    Some(&Value::Object(body)),
                )
                .await?;
            print_snapshots(&[s], json)?;
        }
        VmSnapshotCmd::Delete { id, name } => {
            client
                .empty("DELETE", &format!("/vms/{id}/snapshots/{name}"), None)
                .await?;
            done(json, "deleted", id);
        }
        VmSnapshotCmd::Rollback { id, name } => {
            client
                .empty(
                    "POST",
                    &format!("/vms/{id}/snapshots/{name}/rollback"),
                    None,
                )
                .await?;
            done(json, "rolled back", id);
        }
    }
    Ok(())
}

pub async fn run(client: &Client, cmd: VmsCmd, json: bool) -> Result<(), Error> {
    match cmd {
        VmsCmd::List { state, paging } => {
            let mut query = Vec::new();
            if let Some(s) = state {
                query.push(("state", s));
            }
            let page: Page<Vm> = pages(client, "/vms", &query, &paging).await?;
            if json {
                output::json(&serde_json::to_value(&page)?);
            } else {
                let rows: Vec<_> = page.items.iter().map(vm_row).collect();
                output::table(&VM_HEADERS, &rows);
                if let Some(c) = page.next_cursor {
                    eprintln!("more: --cursor {c}");
                }
            }
        }
        VmsCmd::Get { id } => {
            let v: Vm = client.json("GET", &format!("/vms/{id}"), &[], None).await?;
            print_vm(&v, json)?;
        }
        VmsCmd::Create(args) => {
            let body = create_body(&args)?;
            let j: Job = client.json("POST", "/vms", &[], Some(&body)).await?;
            print_job(&j, json)?;
        }
        VmsCmd::Update(args) => {
            let id = args.id;
            let body = update_body(&args)?;
            let v: Vm = client
                .json("PATCH", &format!("/vms/{id}"), &[], Some(&body))
                .await?;
            print_vm(&v, json)?;
        }
        VmsCmd::Delete { id, purge } => {
            let path = if purge {
                format!("/vms/{id}?purge=true")
            } else {
                format!("/vms/{id}")
            };
            let j: Job = client.json("DELETE", &path, &[], None).await?;
            print_job(&j, json)?;
        }
        VmsCmd::Start { id } => lifecycle(client, id, "start", None, json).await?,
        VmsCmd::Stop { id, force } => {
            lifecycle(client, id, "stop", Some(&json!({ "force": force })), json).await?;
        }
        VmsCmd::Restart { id } => lifecycle(client, id, "restart", None, json).await?,
        VmsCmd::Reset { id } => lifecycle(client, id, "reset", None, json).await?,
        VmsCmd::Disk(cmd) => run_disk(client, cmd, json).await?,
        VmsCmd::Cdrom(cmd) => run_cdrom(client, cmd, json).await?,
        VmsCmd::Snapshot(cmd) => run_snapshot(client, cmd, json).await?,
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;
    use crate::cli::MetadataArgs;

    fn args(image: Option<Id>, boot_size: Option<&str>) -> VmCreateArgs {
        VmCreateArgs {
            name: "vm0".to_owned(),
            vcpus: 2,
            memory: "2G".to_owned(),
            bootrom: None,
            no_acpi: false,
            pool: None,
            image,
            boot_size: boot_size.map(str::to_owned),
            disks: vec!["50G".to_owned()],
            cdroms: Vec::new(),
            nics: vec!["net0,stub0".to_owned()],
            no_vnc: false,
            no_autoboot: true,
            no_start: false,
            metadata: MetadataArgs {
                display_name: None,
                description: None,
                tags: Vec::new(),
                notes: None,
            },
        }
    }

    #[test]
    fn create_body_shapes() {
        let body = create_body(&args(None, Some("20G"))).unwrap();
        assert_eq!(body["memory_bytes"], json!(2_147_483_648_u64));
        assert_eq!(body["disks"][0]["size_bytes"], json!(21_474_836_480_u64));
        assert_eq!(body["disks"][0]["boot"], json!(true));
        assert_eq!(body["disks"][1]["size_bytes"], json!(53_687_091_200_u64));
        assert_eq!(body["autoboot"], json!(false));
        assert_eq!(body["nics"][0]["name"], json!("net0"));
        let id = Id::new();
        let body = create_body(&args(Some(id), None)).unwrap();
        assert_eq!(body["disks"][0]["image_id"], json!(id));
        assert!(create_body(&args(None, None)).is_err());
    }
}
