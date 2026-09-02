//! `/network/*`: links, addresses, routes (ADR-0011).
//!
//! Illumos is the source of truth; every read goes through the driver
//! (cached briefly). Links, addresses, and routes have no property store,
//! so their ids are derived from the host id and their names.
//!
//! The management path is protected: the address a request reached the
//! daemon on (the `Host` header, or the configured listener address when
//! it is a specific one), the IP interface carrying it, and every link
//! beneath it down to the physical port refuse delete.

use std::{
    collections::{HashMap, HashSet},
    net::IpAddr,
};

use axum::{
    Json, Router,
    extract::{Path, State},
    http::{HeaderMap, StatusCode, header},
    routing::{delete, get, post},
};
use mandrake_core::{
    Id, Role,
    api::{Metadata, ObjectRef},
    network::{
        Address, AddressCreate, AddressFamily, AddressKind, AggrCreate, EtherstubCreate, LacpMode,
        LacpTimer, Link, LinkKind, LinkUpdate, Route, RouteCreate, RouteKind, VlanCreate,
        VnicCreate,
    },
};
use mandrake_net::{
    AddressInfo, AddressSpec, AggrSpec, LinkInfo, RouteInfo, RouteSpec, VlanSpec, VnicSpec, parse,
};
use serde::Serialize;
use serde_json::json;

use super::Ctx;
use crate::{
    app::AppState,
    audit::{Context, Record},
    auth::Auth,
    error::{ApiError, ApiResult},
    metadata,
};

/// `{ "items": [...] }`, the unpaged list shape.
#[derive(Debug, Serialize)]
pub struct Items<T> {
    /// The items.
    pub items: Vec<T>,
}

/// The network routes, mounted under `/api/v1`.
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/network/links", get(list_links))
        .route("/network/links/{id}", get(get_link).patch(update_link))
        .route("/network/aggrs", post(create_aggr))
        .route("/network/aggrs/{id}", delete(delete_aggr))
        .route("/network/vlans", post(create_vlan))
        .route("/network/vlans/{id}", delete(delete_vlan))
        .route("/network/etherstubs", post(create_etherstub))
        .route("/network/etherstubs/{id}", delete(delete_etherstub))
        .route("/network/vnics", post(create_vnic))
        .route("/network/vnics/{id}", delete(delete_vnic))
        .route(
            "/network/addresses",
            get(list_addresses).post(create_address),
        )
        .route(
            "/network/addresses/{id}",
            get(get_address).delete(delete_address),
        )
        .route("/network/routes", get(list_routes).post(create_route))
        .route("/network/routes/{id}", delete(delete_route))
}

// ------------------------------------------------------------ identity

fn link_id(state: &AppState, name: &str) -> Id {
    Id::derived(state.host_id, "link", name)
}

fn address_id(state: &AppState, name: &str) -> Id {
    Id::derived(state.host_id, "address", name)
}

fn route_id(state: &AppState, r: &RouteInfo) -> Id {
    Id::derived(
        state.host_id,
        "route",
        &format!(
            "{}:{}:{}",
            r.family,
            r.destination,
            r.gateway.as_deref().unwrap_or("")
        ),
    )
}

async fn metadata_for(state: &AppState, ids: &[Id]) -> ApiResult<HashMap<Id, Metadata>> {
    let ids = ids.to_vec();
    state
        .db
        .call(move |conn| metadata::get_many(conn, &ids))
        .await
}

// ------------------------------------------------------------ listings

async fn all_links(state: &AppState) -> ApiResult<Vec<LinkInfo>> {
    let net = state.net.clone();
    Ok(state
        .links_cache
        .get_or(|| async move { net.list_links().await })
        .await?)
}

async fn all_addresses(state: &AppState) -> ApiResult<Vec<AddressInfo>> {
    let net = state.net.clone();
    Ok(state
        .addresses_cache
        .get_or(|| async move { net.list_addresses().await })
        .await?)
}

async fn all_routes(state: &AppState) -> ApiResult<Vec<RouteInfo>> {
    let net = state.net.clone();
    Ok(state
        .routes_cache
        .get_or(|| async move { net.list_routes().await })
        .await?)
}

fn invalidate(state: &AppState) {
    state.links_cache.clear();
    state.addresses_cache.clear();
    state.routes_cache.clear();
}

// ------------------------------------------------------------ protection

/// Names on the management path.
#[derive(Debug, Default)]
struct Protection {
    addresses: HashSet<String>,
    links: HashSet<String>,
}

/// The IP a request was addressed to, from its `Host` header.
fn host_ip(headers: &HeaderMap) -> Option<IpAddr> {
    let host = headers.get(header::HOST)?.to_str().ok()?.trim();
    if let Ok(ip) = host.parse() {
        return Some(ip);
    }
    let ip = if let Some(rest) = host.strip_prefix('[') {
        rest.split(']').next()?
    } else {
        host.rsplit_once(':').map_or(host, |(h, _)| h)
    };
    ip.parse().ok()
}

fn management_ips(state: &AppState, headers: &HeaderMap) -> Vec<IpAddr> {
    let mut ips = Vec::new();
    ips.extend(state.listen);
    ips.extend(host_ip(headers));
    ips
}

fn address_ip(a: &AddressInfo) -> Option<IpAddr> {
    a.address.as_deref()?.split('/').next()?.parse().ok()
}

/// The management addresses, their interfaces, and every link beneath.
fn compute_protection(ips: &[IpAddr], links: &[LinkInfo], addresses: &[AddressInfo]) -> Protection {
    let mut prot = Protection::default();
    let mut stack: Vec<String> = Vec::new();
    for a in addresses {
        if address_ip(a).is_some_and(|ip| ips.contains(&ip)) {
            prot.addresses.insert(a.name.clone());
            stack.push(a.interface.clone());
        }
    }
    while let Some(name) = stack.pop() {
        if prot.links.insert(name.clone())
            && let Some(l) = links.iter().find(|l| l.name == name)
        {
            stack.extend(l.over.iter().cloned());
        }
    }
    prot
}

async fn protection(
    state: &AppState,
    headers: &HeaderMap,
    links: &[LinkInfo],
) -> ApiResult<Protection> {
    let addresses = all_addresses(state).await?;
    Ok(compute_protection(
        &management_ips(state, headers),
        links,
        &addresses,
    ))
}

fn protected_error(what: &str) -> ApiError {
    ApiError::typed(StatusCode::FORBIDDEN, "protected", "Forbidden").detail(format!(
        "{what} carries the management address and cannot be changed through the API"
    ))
}

// ------------------------------------------------------------ views

fn to_link(info: LinkInfo, id: Id, protected: bool, metadata: Option<Metadata>) -> Link {
    Link {
        id,
        name: info.name,
        kind: info.kind,
        state: info.state,
        over: info.over,
        mtu: info.mtu,
        mac: info.mac,
        mac_mode: info.mac_mode,
        vid: info.vid,
        speed_mbps: info.speed_mbps,
        duplex: info.duplex,
        device: info.device,
        media: info.media,
        aggr: info.aggr,
        zone: info.zone,
        protected,
        metadata,
    }
}

fn to_address(info: AddressInfo, id: Id, protected: bool, metadata: Option<Metadata>) -> Address {
    Address {
        id,
        name: info.name,
        interface: info.interface,
        kind: info.kind,
        family: info.family,
        address: info.address,
        state: info.state,
        persistent: info.persistent,
        protected,
        metadata,
    }
}

fn to_route(info: RouteInfo, id: Id) -> Route {
    Route {
        id,
        destination: info.destination,
        gateway: info.gateway,
        family: info.family,
        interface: info.interface,
        flags: info.flags,
        kind: info.kind,
        persistent: info.persistent,
    }
}

async fn links_view(state: &AppState, headers: &HeaderMap) -> ApiResult<Vec<Link>> {
    let infos = all_links(state).await?;
    let prot = protection(state, headers, &infos).await?;
    let ids: Vec<Id> = infos.iter().map(|l| link_id(state, &l.name)).collect();
    let mut meta = metadata_for(state, &ids).await?;
    Ok(infos
        .into_iter()
        .zip(ids)
        .map(|(info, id)| {
            let protected = prot.links.contains(&info.name);
            to_link(info, id, protected, meta.remove(&id))
        })
        .collect())
}

async fn link_view(state: &AppState, headers: &HeaderMap, name: &str) -> ApiResult<Link> {
    links_view(state, headers)
        .await?
        .into_iter()
        .find(|l| l.name == name)
        .ok_or_else(|| ApiError::not_found("link"))
}

async fn find_link(state: &AppState, id: Id) -> ApiResult<LinkInfo> {
    all_links(state)
        .await?
        .into_iter()
        .find(|l| link_id(state, &l.name) == id)
        .ok_or_else(|| ApiError::not_found("link"))
}

async fn addresses_view(state: &AppState, headers: &HeaderMap) -> ApiResult<Vec<Address>> {
    let infos = all_addresses(state).await?;
    let prot = compute_protection(&management_ips(state, headers), &[], &infos);
    let ids: Vec<Id> = infos.iter().map(|a| address_id(state, &a.name)).collect();
    let mut meta = metadata_for(state, &ids).await?;
    Ok(infos
        .into_iter()
        .zip(ids)
        .map(|(info, id)| {
            let protected = prot.addresses.contains(&info.name);
            to_address(info, id, protected, meta.remove(&id))
        })
        .collect())
}

async fn find_address(state: &AppState, id: Id) -> ApiResult<AddressInfo> {
    all_addresses(state)
        .await?
        .into_iter()
        .find(|a| address_id(state, &a.name) == id)
        .ok_or_else(|| ApiError::not_found("address"))
}

async fn address_view(state: &AppState, headers: &HeaderMap, name: &str) -> ApiResult<Address> {
    addresses_view(state, headers)
        .await?
        .into_iter()
        .find(|a| a.name == name)
        .ok_or_else(|| ApiError::not_found("address"))
}

async fn routes_view(state: &AppState) -> ApiResult<Vec<Route>> {
    Ok(all_routes(state)
        .await?
        .into_iter()
        .map(|r| {
            let id = route_id(state, &r);
            to_route(r, id)
        })
        .collect())
}

fn summary_link(l: &Link) -> serde_json::Value {
    json!({
        "name": l.name,
        "kind": l.kind,
        "over": l.over,
        "mtu": l.mtu,
        "vid": l.vid,
        "mac": l.mac,
        "metadata": l.metadata,
    })
}

// ------------------------------------------------------------ validation

/// illumos link names: a letter, then letters, digits, and underscores,
/// ending in a digit, at most 31 characters.
fn valid_link_name(name: &str) -> bool {
    let b = name.as_bytes();
    (2..=31).contains(&b.len())
        && b[0].is_ascii_alphabetic()
        && b.iter().all(|c| c.is_ascii_alphanumeric() || *c == b'_')
        && b[b.len() - 1].is_ascii_digit()
}

fn require_link_name(name: &str) -> ApiResult<()> {
    if valid_link_name(name) {
        Ok(())
    } else {
        Err(ApiError::unprocessable(
            "invalid link name; letters, digits, and underscores, ending in a digit",
        ))
    }
}

fn require_vid(vid: u16) -> ApiResult<()> {
    if (1..=4094).contains(&vid) {
        Ok(())
    } else {
        Err(ApiError::unprocessable("vid must be between 1 and 4094"))
    }
}

fn require_mtu(mtu: u32) -> ApiResult<()> {
    if (576..=9216).contains(&mtu) {
        Ok(())
    } else {
        Err(ApiError::unprocessable("mtu must be between 576 and 9216"))
    }
}

fn valid_policy(policy: &str) -> bool {
    !policy.is_empty() && policy.split(',').all(|p| matches!(p, "L2" | "L3" | "L4"))
}

/// `ip/prefix` with the prefix within the family's range.
fn parse_prefixed(s: &str) -> Option<(IpAddr, u8)> {
    let (ip, prefix) = s.split_once('/')?;
    let ip: IpAddr = ip.parse().ok()?;
    let prefix: u8 = prefix.parse().ok()?;
    let max = if ip.is_ipv4() { 32 } else { 128 };
    (prefix <= max).then_some((ip, prefix))
}

/// The link `name` from the current listing, or 422.
fn require_existing<'a>(links: &'a [LinkInfo], name: &str, role: &str) -> ApiResult<&'a LinkInfo> {
    links
        .iter()
        .find(|l| l.name == name)
        .ok_or_else(|| ApiError::unprocessable(&format!("{role} `{name}` is not a link")))
}

// ------------------------------------------------------------ links

/// `GET /network/links`.
pub async fn list_links(
    State(state): State<AppState>,
    auth: Auth,
    headers: HeaderMap,
) -> ApiResult<Json<Items<Link>>> {
    auth.require(Role::Viewer)?;
    Ok(Json(Items {
        items: links_view(&state, &headers).await?,
    }))
}

/// `GET /network/links/{id}`.
pub async fn get_link(
    State(state): State<AppState>,
    auth: Auth,
    headers: HeaderMap,
    Path(id): Path<Id>,
) -> ApiResult<Json<Link>> {
    auth.require(Role::Viewer)?;
    let info = find_link(&state, id).await?;
    Ok(Json(link_view(&state, &headers, &info.name).await?))
}

/// `PATCH /network/links/{id}`.
pub async fn update_link(
    State(state): State<AppState>,
    auth: Auth,
    Ctx(ctx): Ctx,
    headers: HeaderMap,
    Path(id): Path<Id>,
    Json(patch): Json<LinkUpdate>,
) -> ApiResult<Json<Link>> {
    auth.require(Role::Operator)?;
    let info = find_link(&state, id).await?;
    if let Some(mtu) = patch.mtu {
        require_mtu(mtu)?;
        state.net.set_mtu(&info.name, mtu).await?;
        invalidate(&state);
    }
    if let Some(m) = &patch.metadata
        && !m.is_empty()
    {
        let m = m.clone();
        state
            .db
            .call(move |conn| metadata::merge(conn, id, &m))
            .await?;
    }
    let link = link_view(&state, &headers, &info.name).await?;
    state
        .record(
            &auth.actor,
            &ctx,
            Record::ok("link.update", ObjectRef::new("link", id, &link.name))
                .after(summary_link(&link)),
        )
        .await?;
    Ok(Json(link))
}

/// Shared tail of every link creation: invalidate, store metadata, read
/// back, audit.
async fn finish_link_create(
    state: &AppState,
    auth: &Auth,
    ctx: &Context,
    headers: &HeaderMap,
    name: &str,
    metadata_patch: Option<&Metadata>,
    action: &str,
) -> ApiResult<(StatusCode, Json<Link>)> {
    invalidate(state);
    let id = link_id(state, name);
    if let Some(m) = metadata_patch
        && !m.is_empty()
    {
        let m = m.clone();
        state
            .db
            .call(move |conn| metadata::merge(conn, id, &m))
            .await?;
    }
    let link = link_view(state, headers, name).await?;
    state
        .record(
            &auth.actor,
            ctx,
            Record::ok(action, ObjectRef::new("link", id, name)).after(summary_link(&link)),
        )
        .await?;
    Ok((StatusCode::CREATED, Json(link)))
}

/// `POST /network/aggrs`.
pub async fn create_aggr(
    State(state): State<AppState>,
    auth: Auth,
    Ctx(ctx): Ctx,
    headers: HeaderMap,
    Json(body): Json<AggrCreate>,
) -> ApiResult<(StatusCode, Json<Link>)> {
    auth.require(Role::Operator)?;
    require_link_name(&body.name)?;
    if body.ports.is_empty() {
        return Err(ApiError::unprocessable(
            "an aggregation needs at least one port",
        ));
    }
    let policy = body.policy.clone().unwrap_or_else(|| "L4".to_owned());
    if !valid_policy(&policy) {
        return Err(ApiError::unprocessable(
            "policy must be L2, L3, L4, or a comma-separated combination",
        ));
    }
    let links = all_links(&state).await?;
    let prot = protection(&state, &headers, &links).await?;
    for port in &body.ports {
        let link = require_existing(&links, port, "port")?;
        if link.kind != LinkKind::Phys {
            return Err(ApiError::unprocessable(&format!(
                "port `{port}` is not a physical link"
            )));
        }
        if prot.links.contains(port) {
            return Err(protected_error(&format!("port `{port}`")));
        }
    }
    let spec = AggrSpec {
        name: body.name.clone(),
        ports: body.ports.clone(),
        policy,
        lacp_mode: body.lacp_mode.unwrap_or(LacpMode::Active),
        lacp_timer: body.lacp_timer.unwrap_or(LacpTimer::Short),
    };
    state.net.create_aggr(&spec).await?;
    finish_link_create(
        &state,
        &auth,
        &ctx,
        &headers,
        &body.name,
        body.metadata.as_ref(),
        "aggr.create",
    )
    .await
}

/// `POST /network/vlans`.
pub async fn create_vlan(
    State(state): State<AppState>,
    auth: Auth,
    Ctx(ctx): Ctx,
    headers: HeaderMap,
    Json(body): Json<VlanCreate>,
) -> ApiResult<(StatusCode, Json<Link>)> {
    auth.require(Role::Operator)?;
    require_link_name(&body.name)?;
    require_vid(body.vid)?;
    let links = all_links(&state).await?;
    let over = require_existing(&links, &body.over, "over")?;
    if !matches!(over.kind, LinkKind::Phys | LinkKind::Aggr) {
        return Err(ApiError::unprocessable(
            "VLANs sit on physical links or aggregations",
        ));
    }
    let spec = VlanSpec {
        name: body.name.clone(),
        vid: body.vid,
        over: body.over.clone(),
    };
    state.net.create_vlan(&spec).await?;
    finish_link_create(
        &state,
        &auth,
        &ctx,
        &headers,
        &body.name,
        body.metadata.as_ref(),
        "vlan.create",
    )
    .await
}

/// `POST /network/etherstubs`.
pub async fn create_etherstub(
    State(state): State<AppState>,
    auth: Auth,
    Ctx(ctx): Ctx,
    headers: HeaderMap,
    Json(body): Json<EtherstubCreate>,
) -> ApiResult<(StatusCode, Json<Link>)> {
    auth.require(Role::Operator)?;
    require_link_name(&body.name)?;
    state.net.create_etherstub(&body.name).await?;
    finish_link_create(
        &state,
        &auth,
        &ctx,
        &headers,
        &body.name,
        body.metadata.as_ref(),
        "etherstub.create",
    )
    .await
}

/// `POST /network/vnics`.
pub async fn create_vnic(
    State(state): State<AppState>,
    auth: Auth,
    Ctx(ctx): Ctx,
    headers: HeaderMap,
    Json(body): Json<VnicCreate>,
) -> ApiResult<(StatusCode, Json<Link>)> {
    auth.require(Role::Operator)?;
    require_link_name(&body.name)?;
    if let Some(vid) = body.vid {
        require_vid(vid)?;
    }
    if let Some(mtu) = body.mtu {
        require_mtu(mtu)?;
    }
    let mac =
        match &body.mac {
            Some(m) => Some(parse::normalize_mac(m).ok_or_else(|| {
                ApiError::unprocessable("mac must be six colon-separated hex bytes")
            })?),
            None => None,
        };
    let links = all_links(&state).await?;
    let over = require_existing(&links, &body.over, "over")?;
    if !matches!(
        over.kind,
        LinkKind::Phys | LinkKind::Aggr | LinkKind::Etherstub
    ) {
        return Err(ApiError::unprocessable(
            "VNICs sit on physical links, aggregations, or etherstubs",
        ));
    }
    let spec = VnicSpec {
        name: body.name.clone(),
        over: body.over.clone(),
        mac,
        vid: body.vid,
        mtu: body.mtu,
    };
    state.net.create_vnic(&spec).await?;
    finish_link_create(
        &state,
        &auth,
        &ctx,
        &headers,
        &body.name,
        body.metadata.as_ref(),
        "vnic.create",
    )
    .await
}

/// Delete a link of one kind: 404 when the id is another kind, 403 on the
/// management path, 409 while anything sits over it.
async fn remove_link(
    state: &AppState,
    auth: &Auth,
    ctx: &Context,
    headers: &HeaderMap,
    id: Id,
    kind: LinkKind,
) -> ApiResult<StatusCode> {
    auth.require(Role::Operator)?;
    let info = find_link(state, id).await?;
    if info.kind != kind {
        return Err(ApiError::not_found(kind.as_str()));
    }
    let links = all_links(state).await?;
    let prot = protection(state, headers, &links).await?;
    if prot.links.contains(&info.name) {
        return Err(protected_error(&format!("{kind} `{}`", info.name)));
    }
    let dependents: Vec<&str> = links
        .iter()
        .filter(|l| l.over.contains(&info.name))
        .map(|l| l.name.as_str())
        .collect();
    if !dependents.is_empty() {
        return Err(
            ApiError::typed(StatusCode::CONFLICT, "busy", "Conflict").detail(format!(
                "{} is in use by {}",
                info.name,
                dependents.join(", ")
            )),
        );
    }
    state.net.delete_link(&info.name, kind).await?;
    invalidate(state);
    let _ = state.db.call(move |conn| metadata::remove(conn, id)).await;
    state
        .record(
            &auth.actor,
            ctx,
            Record::ok(
                &format!("{kind}.delete"),
                ObjectRef::new("link", id, &info.name),
            )
            .before(json!({ "name": info.name, "kind": info.kind, "over": info.over })),
        )
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

/// `DELETE /network/aggrs/{id}`.
pub async fn delete_aggr(
    State(state): State<AppState>,
    auth: Auth,
    Ctx(ctx): Ctx,
    headers: HeaderMap,
    Path(id): Path<Id>,
) -> ApiResult<StatusCode> {
    remove_link(&state, &auth, &ctx, &headers, id, LinkKind::Aggr).await
}

/// `DELETE /network/vlans/{id}`.
pub async fn delete_vlan(
    State(state): State<AppState>,
    auth: Auth,
    Ctx(ctx): Ctx,
    headers: HeaderMap,
    Path(id): Path<Id>,
) -> ApiResult<StatusCode> {
    remove_link(&state, &auth, &ctx, &headers, id, LinkKind::Vlan).await
}

/// `DELETE /network/etherstubs/{id}`.
pub async fn delete_etherstub(
    State(state): State<AppState>,
    auth: Auth,
    Ctx(ctx): Ctx,
    headers: HeaderMap,
    Path(id): Path<Id>,
) -> ApiResult<StatusCode> {
    remove_link(&state, &auth, &ctx, &headers, id, LinkKind::Etherstub).await
}

/// `DELETE /network/vnics/{id}`.
pub async fn delete_vnic(
    State(state): State<AppState>,
    auth: Auth,
    Ctx(ctx): Ctx,
    headers: HeaderMap,
    Path(id): Path<Id>,
) -> ApiResult<StatusCode> {
    remove_link(&state, &auth, &ctx, &headers, id, LinkKind::Vnic).await
}

// ------------------------------------------------------------ addresses

/// `GET /network/addresses`.
pub async fn list_addresses(
    State(state): State<AppState>,
    auth: Auth,
    headers: HeaderMap,
) -> ApiResult<Json<Items<Address>>> {
    auth.require(Role::Viewer)?;
    Ok(Json(Items {
        items: addresses_view(&state, &headers).await?,
    }))
}

/// `GET /network/addresses/{id}`.
pub async fn get_address(
    State(state): State<AppState>,
    auth: Auth,
    headers: HeaderMap,
    Path(id): Path<Id>,
) -> ApiResult<Json<Address>> {
    auth.require(Role::Viewer)?;
    let info = find_address(&state, id).await?;
    Ok(Json(address_view(&state, &headers, &info.name).await?))
}

fn valid_alias(alias: &str) -> bool {
    (1..=16).contains(&alias.len())
        && alias
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_')
}

/// `POST /network/addresses`.
pub async fn create_address(
    State(state): State<AppState>,
    auth: Auth,
    Ctx(ctx): Ctx,
    headers: HeaderMap,
    Json(body): Json<AddressCreate>,
) -> ApiResult<(StatusCode, Json<Address>)> {
    auth.require(Role::Operator)?;
    if body.interface != "lo0" {
        let links = all_links(&state).await?;
        require_existing(&links, &body.interface, "interface")?;
    }
    let family = match body.kind {
        AddressKind::Static => {
            let Some(address) = &body.address else {
                return Err(ApiError::unprocessable(
                    "a static address needs an address with prefix length",
                ));
            };
            let Some((ip, _)) = parse_prefixed(address) else {
                return Err(ApiError::unprocessable(
                    "address must be `a.b.c.d/prefix` or `xx::/prefix`",
                ));
            };
            AddressFamily::of(&ip.to_string())
        }
        AddressKind::Dhcp | AddressKind::Addrconf => {
            if body.address.is_some() {
                return Err(ApiError::unprocessable(
                    "address is only for static addresses",
                ));
            }
            if body.kind == AddressKind::Dhcp {
                AddressFamily::Inet
            } else {
                AddressFamily::Inet6
            }
        }
    };
    let alias = body.alias.clone().unwrap_or_else(|| {
        match family {
            AddressFamily::Inet => "v4",
            AddressFamily::Inet6 => "v6",
        }
        .to_owned()
    });
    if !valid_alias(&alias) {
        return Err(ApiError::unprocessable(
            "alias must be 1 to 16 letters, digits, or underscores",
        ));
    }
    let addrobj = format!("{}/{alias}", body.interface);
    let spec = AddressSpec {
        addrobj: addrobj.clone(),
        kind: body.kind,
        address: body.address.clone(),
        temporary: body.temporary,
    };
    state.net.create_address(&spec).await?;
    invalidate(&state);
    let id = address_id(&state, &addrobj);
    if let Some(m) = &body.metadata
        && !m.is_empty()
    {
        let m = m.clone();
        state
            .db
            .call(move |conn| metadata::merge(conn, id, &m))
            .await?;
    }
    let address = address_view(&state, &headers, &addrobj).await?;
    state
        .record(
            &auth.actor,
            &ctx,
            Record::ok(
                "address.create",
                ObjectRef::new("address", id, &address.name),
            )
            .after(json!({
                "interface": address.interface,
                "kind": address.kind,
                "address": address.address,
                "persistent": address.persistent,
                "metadata": address.metadata,
            })),
        )
        .await?;
    Ok((StatusCode::CREATED, Json(address)))
}

/// `DELETE /network/addresses/{id}`.
pub async fn delete_address(
    State(state): State<AppState>,
    auth: Auth,
    Ctx(ctx): Ctx,
    headers: HeaderMap,
    Path(id): Path<Id>,
) -> ApiResult<StatusCode> {
    auth.require(Role::Operator)?;
    let info = find_address(&state, id).await?;
    let addresses = all_addresses(&state).await?;
    let prot = compute_protection(&management_ips(&state, &headers), &[], &addresses);
    if prot.addresses.contains(&info.name) {
        return Err(protected_error(&format!("address `{}`", info.name)));
    }
    state.net.delete_address(&info.name).await?;
    invalidate(&state);
    let _ = state.db.call(move |conn| metadata::remove(conn, id)).await;
    state
        .record(
            &auth.actor,
            &ctx,
            Record::ok("address.delete", ObjectRef::new("address", id, &info.name)).before(
                json!({ "interface": info.interface, "kind": info.kind, "address": info.address }),
            ),
        )
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

// ------------------------------------------------------------ routes

/// `GET /network/routes`.
pub async fn list_routes(
    State(state): State<AppState>,
    auth: Auth,
) -> ApiResult<Json<Items<Route>>> {
    auth.require(Role::Viewer)?;
    Ok(Json(Items {
        items: routes_view(&state).await?,
    }))
}

/// `POST /network/routes`.
pub async fn create_route(
    State(state): State<AppState>,
    auth: Auth,
    Ctx(ctx): Ctx,
    Json(body): Json<RouteCreate>,
) -> ApiResult<(StatusCode, Json<Route>)> {
    auth.require(Role::Operator)?;
    let Ok(gateway) = body.gateway.parse::<IpAddr>() else {
        return Err(ApiError::unprocessable("gateway must be an IP address"));
    };
    let family = if gateway.is_ipv4() {
        AddressFamily::Inet
    } else {
        AddressFamily::Inet6
    };
    if body.destination != "default" {
        let Some((dest, _)) = parse_prefixed(&body.destination) else {
            return Err(ApiError::unprocessable(
                "destination must be `default` or a network with prefix length",
            ));
        };
        if dest.is_ipv4() != gateway.is_ipv4() {
            return Err(ApiError::unprocessable(
                "destination and gateway must be the same family",
            ));
        }
    }
    let spec = RouteSpec {
        destination: body.destination.clone(),
        gateway: gateway.to_string(),
        family,
    };
    state.net.add_route(&spec).await?;
    invalidate(&state);
    let added = RouteInfo {
        destination: spec.destination.clone(),
        gateway: Some(spec.gateway.clone()),
        family,
        interface: None,
        flags: Some("UG".to_owned()),
        kind: RouteKind::Static,
        persistent: true,
    };
    let id = route_id(&state, &added);
    let route = routes_view(&state)
        .await?
        .into_iter()
        .find(|r| r.id == id)
        .unwrap_or_else(|| to_route(added, id));
    state
        .record(
            &auth.actor,
            &ctx,
            Record::ok(
                "route.create",
                ObjectRef::new("route", id, &route.destination),
            )
            .after(json!({ "destination": route.destination, "gateway": route.gateway, "family": route.family })),
        )
        .await?;
    Ok((StatusCode::CREATED, Json(route)))
}

/// `DELETE /network/routes/{id}`.
pub async fn delete_route(
    State(state): State<AppState>,
    auth: Auth,
    Ctx(ctx): Ctx,
    Path(id): Path<Id>,
) -> ApiResult<StatusCode> {
    auth.require(Role::Operator)?;
    let info = all_routes(&state)
        .await?
        .into_iter()
        .find(|r| route_id(&state, r) == id)
        .ok_or_else(|| ApiError::not_found("route"))?;
    if info.kind != RouteKind::Static {
        return Err(ApiError::forbidden(
            "only static routes can be removed; interface and dynamic routes are managed by the system",
        ));
    }
    let Some(gateway) = info.gateway.clone() else {
        return Err(ApiError::forbidden("this route has no gateway to remove"));
    };
    let spec = RouteSpec {
        destination: info.destination.clone(),
        gateway,
        family: info.family,
    };
    state.net.delete_route(&spec).await?;
    invalidate(&state);
    state
        .record(
            &auth.actor,
            &ctx,
            Record::ok(
                "route.delete",
                ObjectRef::new("route", id, &info.destination),
            )
            .before(json!({ "destination": info.destination, "gateway": info.gateway, "family": info.family })),
        )
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use axum::http::HeaderValue;
    use mandrake_core::network::LinkState;

    use super::*;

    fn link(name: &str, kind: LinkKind, over: &[&str]) -> LinkInfo {
        let mut l = LinkInfo::new(name, kind);
        l.state = LinkState::Up;
        l.over = over.iter().map(|s| (*s).to_owned()).collect();
        l
    }

    fn address(name: &str, ip: &str) -> AddressInfo {
        AddressInfo {
            name: name.to_owned(),
            interface: name.split('/').next().unwrap().to_owned(),
            kind: AddressKind::Static,
            family: AddressFamily::of(ip),
            address: Some(format!("{ip}/24")),
            state: "ok".to_owned(),
            persistent: true,
        }
    }

    #[test]
    fn protection_follows_the_path_to_the_port() {
        let links = vec![
            link("e1000g0", LinkKind::Phys, &[]),
            link("e1000g1", LinkKind::Phys, &[]),
            link("e1000g2", LinkKind::Phys, &[]),
            link("aggr0", LinkKind::Aggr, &["e1000g1", "e1000g2"]),
            link("vlan10", LinkKind::Vlan, &["aggr0"]),
            link("stub0", LinkKind::Etherstub, &[]),
        ];
        let addresses = vec![
            address("vlan10/v4", "10.10.0.5"),
            address("e1000g0/v4", "192.168.1.10"),
        ];
        let ips = ["10.10.0.5".parse().unwrap()];
        let p = compute_protection(&ips, &links, &addresses);
        assert!(p.addresses.contains("vlan10/v4"));
        assert!(!p.addresses.contains("e1000g0/v4"));
        for name in ["vlan10", "aggr0", "e1000g1", "e1000g2"] {
            assert!(p.links.contains(name), "{name}");
        }
        assert!(!p.links.contains("e1000g0"));
        assert!(!p.links.contains("stub0"));
    }

    #[test]
    fn host_header_forms() {
        let mut h = HeaderMap::new();
        h.insert(header::HOST, HeaderValue::from_static("192.168.1.10:8443"));
        assert_eq!(host_ip(&h), Some("192.168.1.10".parse().unwrap()));
        h.insert(header::HOST, HeaderValue::from_static("[fe80::1]:8443"));
        assert_eq!(host_ip(&h), Some("fe80::1".parse().unwrap()));
        h.insert(header::HOST, HeaderValue::from_static("mandrake.example"));
        assert_eq!(host_ip(&h), None);
    }

    #[test]
    fn validators() {
        assert!(valid_link_name("vnic0"));
        assert!(valid_link_name("mgmt_1"));
        assert!(!valid_link_name("vnic"));
        assert!(!valid_link_name("0vnic0"));
        assert!(!valid_link_name("bad-name0"));
        assert!(valid_policy("L2,L3"));
        assert!(!valid_policy("L5"));
        assert_eq!(parse_prefixed("10.0.0.0/8").map(|(_, p)| p), Some(8));
        assert!(parse_prefixed("10.0.0.0/33").is_none());
        assert!(parse_prefixed("10.0.0.0").is_none());
        assert_eq!(parse_prefixed("fe80::/10").map(|(_, p)| p), Some(10));
    }
}
