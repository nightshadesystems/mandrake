//! `zones` commands: one API call each.

use mandrake_core::{
    api::{Job, Page},
    zone::{Zone, ZoneNic},
};
use serde_json::{Map, Value, json};

use crate::{
    cli::{ZoneCreateArgs, ZoneUpdateArgs, ZonesCmd},
    client::{Client, Error},
    cmd::pages,
    images::print_job,
    output,
    storage::{metadata_value, parse_size},
};

/// `NAME,OVER[,vid=N][,address=A/P][,gateway=G][,mac=M]` to a NIC.
pub(crate) fn parse_nic(text: &str) -> Result<ZoneNic, Error> {
    let mut parts = text.split(',').map(str::trim);
    let name = parts
        .next()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| Error::Config(format!("nic needs NAME,OVER: {text}")))?;
    let over = parts
        .next()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| Error::Config(format!("nic needs NAME,OVER: {text}")))?;
    let mut nic = ZoneNic {
        name: name.to_owned(),
        over: over.to_owned(),
        mac: None,
        vid: None,
        address: None,
        gateway: None,
    };
    for extra in parts {
        let (k, v) = extra
            .split_once('=')
            .ok_or_else(|| Error::Config(format!("nic option must be key=value: {extra}")))?;
        match k {
            "vid" => {
                nic.vid = Some(
                    v.parse()
                        .map_err(|_| Error::Config(format!("bad vid: {v}")))?,
                );
            }
            "address" => nic.address = Some(v.to_owned()),
            "gateway" => nic.gateway = Some(v.to_owned()),
            "mac" => nic.mac = Some(v.to_owned()),
            _ => return Err(Error::Config(format!("unknown nic option: {k}"))),
        }
    }
    Ok(nic)
}

fn nics_value(specs: &[String]) -> Result<Value, Error> {
    let nics = specs
        .iter()
        .map(|s| parse_nic(s))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(serde_json::to_value(nics)?)
}

const ZONE_HEADERS: [&str; 8] = [
    "ID", "NAME", "BRAND", "STATE", "NICS", "CPU", "MEMORY", "AUTOBOOT",
];

fn zone_row(z: &Zone) -> Vec<String> {
    vec![
        z.id.to_string(),
        z.name.clone(),
        z.brand.to_string(),
        z.state.to_string(),
        if z.nics.is_empty() {
            "-".to_owned()
        } else {
            z.nics
                .iter()
                .map(|n| format!("{}@{}", n.name, n.over))
                .collect::<Vec<_>>()
                .join(",")
        },
        z.cpu_cap.map_or_else(|| "-".to_owned(), |c| c.to_string()),
        z.memory_cap_bytes
            .map_or_else(|| "-".to_owned(), output::size),
        z.autoboot.to_string(),
    ]
}

fn print_zone(z: &Zone, json: bool) -> Result<(), Error> {
    if json {
        output::json(&serde_json::to_value(z)?);
        return Ok(());
    }
    let mut pairs = vec![
        ("id", z.id.to_string()),
        ("name", z.name.clone()),
        ("brand", z.brand.to_string()),
        ("state", z.state.to_string()),
        (
            "image",
            z.image_id.map_or_else(|| "-".to_owned(), |i| i.to_string()),
        ),
        ("zonepath", z.zonepath.clone()),
        ("dataset", output::opt(z.dataset.as_deref())),
        (
            "cpu cap",
            z.cpu_cap.map_or_else(|| "-".to_owned(), |c| c.to_string()),
        ),
        (
            "memory cap",
            z.memory_cap_bytes
                .map_or_else(|| "-".to_owned(), output::size),
        ),
        ("autoboot", z.autoboot.to_string()),
        ("hostname", output::opt(z.hostname.as_deref())),
        (
            "resolvers",
            if z.resolvers.is_empty() {
                "-".to_owned()
            } else {
                z.resolvers.join(", ")
            },
        ),
    ];
    if let Some(m) = &z.metadata {
        if let Some(n) = &m.display_name {
            pairs.push(("display name", n.clone()));
        }
        if let Some(n) = &m.description {
            pairs.push(("description", n.clone()));
        }
    }
    output::kv(&pairs);
    if !z.nics.is_empty() {
        println!();
        let rows: Vec<Vec<String>> = z
            .nics
            .iter()
            .map(|n| {
                vec![
                    n.name.clone(),
                    n.over.clone(),
                    n.vid.map_or_else(|| "-".to_owned(), |v| v.to_string()),
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

fn create_body(a: &ZoneCreateArgs) -> Result<Value, Error> {
    let mut body = Map::new();
    body.insert("name".to_owned(), Value::String(a.name.clone()));
    body.insert("brand".to_owned(), Value::String(a.brand.clone()));
    if let Some(i) = a.image {
        body.insert("image_id".to_owned(), Value::String(i.to_string()));
    }
    if let Some(p) = &a.pool {
        body.insert("pool".to_owned(), Value::String(p.clone()));
    }
    body.insert("nics".to_owned(), nics_value(&a.nics)?);
    if let Some(c) = a.cpu_cap {
        body.insert("cpu_cap".to_owned(), json!(c));
    }
    if let Some(m) = &a.memory {
        body.insert("memory_cap_bytes".to_owned(), json!(parse_size(m)?));
    }
    body.insert("autoboot".to_owned(), Value::Bool(!a.no_autoboot));
    body.insert("start".to_owned(), Value::Bool(!a.no_start));
    if let Some(h) = &a.hostname {
        body.insert("hostname".to_owned(), Value::String(h.clone()));
    }
    body.insert("resolvers".to_owned(), json!(a.resolvers));
    if let Some(m) = metadata_value(&a.metadata) {
        body.insert("metadata".to_owned(), m);
    }
    Ok(Value::Object(body))
}

fn update_body(a: &ZoneUpdateArgs) -> Result<Value, Error> {
    let mut body = Map::new();
    if a.clear_nics {
        body.insert("nics".to_owned(), json!([]));
    } else if !a.nics.is_empty() {
        body.insert("nics".to_owned(), nics_value(&a.nics)?);
    }
    if let Some(c) = &a.cpu_cap {
        if c.eq_ignore_ascii_case("none") {
            body.insert("cpu_cap".to_owned(), Value::Null);
        } else {
            let n: f64 = c
                .parse()
                .map_err(|_| Error::Config(format!("bad cpu cap: {c}")))?;
            body.insert("cpu_cap".to_owned(), json!(n));
        }
    }
    if let Some(m) = &a.memory {
        if m.eq_ignore_ascii_case("none") {
            body.insert("memory_cap_bytes".to_owned(), Value::Null);
        } else {
            body.insert("memory_cap_bytes".to_owned(), json!(parse_size(m)?));
        }
    }
    if let Some(b) = a.autoboot {
        body.insert("autoboot".to_owned(), Value::Bool(b));
    }
    if let Some(h) = &a.hostname {
        body.insert("hostname".to_owned(), Value::String(h.clone()));
    }
    if !a.resolvers.is_empty() {
        let list: Vec<&String> = a
            .resolvers
            .iter()
            .filter(|r| !r.eq_ignore_ascii_case("none"))
            .collect();
        body.insert("resolvers".to_owned(), json!(list));
    }
    if let Some(m) = metadata_value(&a.metadata) {
        body.insert("metadata".to_owned(), m);
    }
    if body.is_empty() {
        return Err(Error::Config("nothing to update".to_owned()));
    }
    Ok(Value::Object(body))
}

pub async fn run(client: &Client, cmd: ZonesCmd, json: bool) -> Result<(), Error> {
    match cmd {
        ZonesCmd::List {
            brand,
            state,
            paging,
        } => {
            let mut query = Vec::new();
            if let Some(b) = brand {
                query.push(("brand", b));
            }
            if let Some(s) = state {
                query.push(("state", s));
            }
            let page: Page<Zone> = pages(client, "/zones", &query, &paging).await?;
            if json {
                output::json(&serde_json::to_value(&page)?);
            } else {
                let rows: Vec<_> = page.items.iter().map(zone_row).collect();
                output::table(&ZONE_HEADERS, &rows);
                if let Some(c) = page.next_cursor {
                    eprintln!("more: --cursor {c}");
                }
            }
        }
        ZonesCmd::Get { id } => {
            let z: Zone = client
                .json("GET", &format!("/zones/{id}"), &[], None)
                .await?;
            print_zone(&z, json)?;
        }
        ZonesCmd::Create(args) => {
            let body = create_body(&args)?;
            let j: Job = client.json("POST", "/zones", &[], Some(&body)).await?;
            print_job(&j, json)?;
        }
        ZonesCmd::Update(args) => {
            let id = args.id;
            let body = update_body(&args)?;
            let z: Zone = client
                .json("PATCH", &format!("/zones/{id}"), &[], Some(&body))
                .await?;
            print_zone(&z, json)?;
        }
        ZonesCmd::Delete { id, purge } => {
            let path = if purge {
                format!("/zones/{id}?purge=true")
            } else {
                format!("/zones/{id}")
            };
            let j: Job = client.json("DELETE", &path, &[], None).await?;
            print_job(&j, json)?;
        }
        ZonesCmd::Start { id } => {
            let j: Job = client
                .json("POST", &format!("/zones/{id}/start"), &[], None)
                .await?;
            print_job(&j, json)?;
        }
        ZonesCmd::Stop { id, force } => {
            let j: Job = client
                .json(
                    "POST",
                    &format!("/zones/{id}/stop"),
                    &[],
                    Some(&json!({ "force": force })),
                )
                .await?;
            print_job(&j, json)?;
        }
        ZonesCmd::Restart { id } => {
            let j: Job = client
                .json("POST", &format!("/zones/{id}/restart"), &[], None)
                .await?;
            print_job(&j, json)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    #[test]
    fn nics() {
        let n = parse_nic("net0,stub0,vid=20,address=10.0.0.5/24,gateway=10.0.0.1").unwrap();
        assert_eq!(n.name, "net0");
        assert_eq!(n.over, "stub0");
        assert_eq!(n.vid, Some(20));
        assert_eq!(n.address.as_deref(), Some("10.0.0.5/24"));
        assert_eq!(n.gateway.as_deref(), Some("10.0.0.1"));
        assert!(parse_nic("net0").is_err());
        assert!(parse_nic("net0,stub0,bogus=1").is_err());
        assert!(parse_nic("net0,stub0,vid=x").is_err());
    }
}
