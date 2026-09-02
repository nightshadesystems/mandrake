//! `network` commands: one API call each.

use mandrake_core::{
    Id,
    network::{Address, Link, Route},
};
use serde::Deserialize;
use serde_json::{Map, Value, json};

use crate::{
    cli::{
        AddressesCmd, AggrsCmd, EtherstubsCmd, LinksCmd, NetworkCmd, RoutesCmd, VlansCmd, VnicsCmd,
    },
    client::{Client, Error},
    cmd::done,
    output,
    storage::metadata_value,
};

/// `{ "items": [...] }`.
#[derive(Debug, Deserialize)]
struct Items<T> {
    items: Vec<T>,
}

pub async fn run(client: &Client, cmd: NetworkCmd, json: bool) -> Result<(), Error> {
    match cmd {
        NetworkCmd::Links(cmd) => links(client, cmd, json).await,
        NetworkCmd::Aggrs(cmd) => aggrs(client, cmd, json).await,
        NetworkCmd::Vlans(cmd) => vlans(client, cmd, json).await,
        NetworkCmd::Etherstubs(cmd) => etherstubs(client, cmd, json).await,
        NetworkCmd::Vnics(cmd) => vnics(client, cmd, json).await,
        NetworkCmd::Addresses(cmd) => addresses(client, cmd, json).await,
        NetworkCmd::Routes(cmd) => routes(client, cmd, json).await,
    }
}

// ------------------------------------------------------------ links

const LINK_HEADERS: [&str; 8] = ["ID", "NAME", "KIND", "STATE", "OVER", "MAC", "VID", "MTU"];

fn link_row(l: &Link) -> Vec<String> {
    vec![
        l.id.to_string(),
        if l.protected {
            format!("{} (protected)", l.name)
        } else {
            l.name.clone()
        },
        l.kind.to_string(),
        format!("{:?}", l.state).to_lowercase(),
        if l.over.is_empty() {
            "-".to_owned()
        } else {
            l.over.join(",")
        },
        output::opt(l.mac.as_deref()),
        l.vid.map_or_else(|| "-".to_owned(), |v| v.to_string()),
        l.mtu.map_or_else(|| "-".to_owned(), |m| m.to_string()),
    ]
}

fn print_link(l: &Link, json: bool) -> Result<(), Error> {
    if json {
        output::json(&serde_json::to_value(l)?);
        return Ok(());
    }
    let mut pairs = vec![
        ("id", l.id.to_string()),
        ("name", l.name.clone()),
        ("kind", l.kind.to_string()),
        ("state", format!("{:?}", l.state).to_lowercase()),
        (
            "over",
            if l.over.is_empty() {
                "-".to_owned()
            } else {
                l.over.join(", ")
            },
        ),
        (
            "mtu",
            l.mtu.map_or_else(|| "-".to_owned(), |m| m.to_string()),
        ),
        ("mac", output::opt(l.mac.as_deref())),
        ("protected", l.protected.to_string()),
    ];
    if let Some(m) = l.mac_mode {
        pairs.push(("mac mode", format!("{m:?}").to_lowercase()));
    }
    if let Some(v) = l.vid {
        pairs.push(("vid", v.to_string()));
    }
    if let Some(s) = l.speed_mbps {
        pairs.push(("speed", format!("{s} Mb/s")));
    }
    if let Some(d) = l.duplex {
        pairs.push(("duplex", format!("{d:?}").to_lowercase()));
    }
    if let Some(d) = &l.device {
        pairs.push(("device", d.clone()));
    }
    if let Some(m) = &l.media {
        pairs.push(("media", m.clone()));
    }
    if let Some(a) = &l.aggr {
        pairs.push(("policy", a.policy.clone()));
        pairs.push((
            "lacp",
            format!("{} {}", a.lacp_mode.as_str(), a.lacp_timer.as_str()),
        ));
        pairs.push((
            "ports",
            a.ports
                .iter()
                .map(|p| format!("{} ({})", p.name, p.state))
                .collect::<Vec<_>>()
                .join(", "),
        ));
    }
    if let Some(z) = &l.zone {
        pairs.push(("zone", z.clone()));
    }
    if let Some(m) = &l.metadata {
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

async fn links(client: &Client, cmd: LinksCmd, json: bool) -> Result<(), Error> {
    match cmd {
        LinksCmd::List => {
            let list: Items<Link> = client.json("GET", "/network/links", &[], None).await?;
            if json {
                output::json(&json!({ "items": list.items }));
            } else {
                let rows: Vec<_> = list.items.iter().map(link_row).collect();
                output::table(&LINK_HEADERS, &rows);
            }
        }
        LinksCmd::Get { id } => {
            let l: Link = client
                .json("GET", &format!("/network/links/{id}"), &[], None)
                .await?;
            print_link(&l, json)?;
        }
        LinksCmd::Update { id, mtu, metadata } => {
            let mut body = Map::new();
            if let Some(m) = mtu {
                body.insert("mtu".to_owned(), json!(m));
            }
            if let Some(m) = metadata_value(&metadata) {
                body.insert("metadata".to_owned(), m);
            }
            if body.is_empty() {
                return Err(Error::Config("nothing to update".to_owned()));
            }
            let l: Link = client
                .json(
                    "PATCH",
                    &format!("/network/links/{id}"),
                    &[],
                    Some(&Value::Object(body)),
                )
                .await?;
            print_link(&l, json)?;
        }
    }
    Ok(())
}

async fn create_link(client: &Client, path: &str, body: &Value, json: bool) -> Result<(), Error> {
    let l: Link = client.json("POST", path, &[], Some(body)).await?;
    print_link(&l, json)
}

async fn delete_link(client: &Client, path: &str, id: Id, json: bool) -> Result<(), Error> {
    client
        .empty("DELETE", &format!("{path}/{id}"), None)
        .await?;
    done(json, "deleted", id);
    Ok(())
}

async fn aggrs(client: &Client, cmd: AggrsCmd, json: bool) -> Result<(), Error> {
    match cmd {
        AggrsCmd::Create {
            name,
            ports,
            policy,
            lacp,
            timer,
            metadata,
        } => {
            let body = json!({
                "name": name,
                "ports": ports,
                "policy": policy,
                "lacp_mode": lacp,
                "lacp_timer": timer,
                "metadata": metadata_value(&metadata),
            });
            create_link(client, "/network/aggrs", &body, json).await
        }
        AggrsCmd::Delete { id } => delete_link(client, "/network/aggrs", id, json).await,
    }
}

async fn vlans(client: &Client, cmd: VlansCmd, json: bool) -> Result<(), Error> {
    match cmd {
        VlansCmd::Create {
            name,
            vid,
            over,
            metadata,
        } => {
            let body = json!({
                "name": name,
                "vid": vid,
                "over": over,
                "metadata": metadata_value(&metadata),
            });
            create_link(client, "/network/vlans", &body, json).await
        }
        VlansCmd::Delete { id } => delete_link(client, "/network/vlans", id, json).await,
    }
}

async fn etherstubs(client: &Client, cmd: EtherstubsCmd, json: bool) -> Result<(), Error> {
    match cmd {
        EtherstubsCmd::Create { name, metadata } => {
            let body = json!({ "name": name, "metadata": metadata_value(&metadata) });
            create_link(client, "/network/etherstubs", &body, json).await
        }
        EtherstubsCmd::Delete { id } => delete_link(client, "/network/etherstubs", id, json).await,
    }
}

async fn vnics(client: &Client, cmd: VnicsCmd, json: bool) -> Result<(), Error> {
    match cmd {
        VnicsCmd::Create {
            name,
            over,
            mac,
            vid,
            mtu,
            metadata,
        } => {
            let body = json!({
                "name": name,
                "over": over,
                "mac": mac,
                "vid": vid,
                "mtu": mtu,
                "metadata": metadata_value(&metadata),
            });
            create_link(client, "/network/vnics", &body, json).await
        }
        VnicsCmd::Delete { id } => delete_link(client, "/network/vnics", id, json).await,
    }
}

// ------------------------------------------------------------ addresses

const ADDRESS_HEADERS: [&str; 7] = [
    "ID",
    "NAME",
    "ADDRESS",
    "KIND",
    "FAMILY",
    "STATE",
    "PERSISTENT",
];

fn address_row(a: &Address) -> Vec<String> {
    vec![
        a.id.to_string(),
        if a.protected {
            format!("{} (management)", a.name)
        } else {
            a.name.clone()
        },
        output::opt(a.address.as_deref()),
        a.kind.to_string(),
        a.family.to_string(),
        a.state.clone(),
        a.persistent.to_string(),
    ]
}

fn print_address(a: &Address, json: bool) -> Result<(), Error> {
    if json {
        output::json(&serde_json::to_value(a)?);
    } else {
        output::table(&ADDRESS_HEADERS, &[address_row(a)]);
    }
    Ok(())
}

async fn addresses(client: &Client, cmd: AddressesCmd, json: bool) -> Result<(), Error> {
    match cmd {
        AddressesCmd::List => {
            let list: Items<Address> = client.json("GET", "/network/addresses", &[], None).await?;
            if json {
                output::json(&json!({ "items": list.items }));
            } else {
                let rows: Vec<_> = list.items.iter().map(address_row).collect();
                output::table(&ADDRESS_HEADERS, &rows);
            }
        }
        AddressesCmd::Get { id } => {
            let a: Address = client
                .json("GET", &format!("/network/addresses/{id}"), &[], None)
                .await?;
            print_address(&a, json)?;
        }
        AddressesCmd::Create {
            interface,
            kind,
            address,
            alias,
            temporary,
            metadata,
        } => {
            let body = json!({
                "interface": interface,
                "kind": kind,
                "address": address,
                "alias": alias,
                "temporary": temporary,
                "metadata": metadata_value(&metadata),
            });
            let a: Address = client
                .json("POST", "/network/addresses", &[], Some(&body))
                .await?;
            print_address(&a, json)?;
        }
        AddressesCmd::Delete { id } => {
            client
                .empty("DELETE", &format!("/network/addresses/{id}"), None)
                .await?;
            done(json, "deleted", id);
        }
    }
    Ok(())
}

// ------------------------------------------------------------ routes

const ROUTE_HEADERS: [&str; 7] = [
    "ID",
    "DESTINATION",
    "GATEWAY",
    "FAMILY",
    "INTERFACE",
    "KIND",
    "PERSISTENT",
];

fn route_row(r: &Route) -> Vec<String> {
    vec![
        r.id.to_string(),
        r.destination.clone(),
        output::opt(r.gateway.as_deref()),
        r.family.to_string(),
        output::opt(r.interface.as_deref()),
        format!("{:?}", r.kind).to_lowercase(),
        r.persistent.to_string(),
    ]
}

async fn routes(client: &Client, cmd: RoutesCmd, json: bool) -> Result<(), Error> {
    match cmd {
        RoutesCmd::List => {
            let list: Items<Route> = client.json("GET", "/network/routes", &[], None).await?;
            if json {
                output::json(&json!({ "items": list.items }));
            } else {
                let rows: Vec<_> = list.items.iter().map(route_row).collect();
                output::table(&ROUTE_HEADERS, &rows);
            }
        }
        RoutesCmd::Create {
            destination,
            gateway,
        } => {
            let body = json!({ "destination": destination, "gateway": gateway });
            let r: Route = client
                .json("POST", "/network/routes", &[], Some(&body))
                .await?;
            if json {
                output::json(&serde_json::to_value(&r)?);
            } else {
                output::table(&ROUTE_HEADERS, &[route_row(&r)]);
            }
        }
        RoutesCmd::Delete { id } => {
            client
                .empty("DELETE", &format!("/network/routes/{id}"), None)
                .await?;
            done(json, "deleted", id);
        }
    }
    Ok(())
}
