//! Parsers for `zoneadm` and `zonecfg` output, and the renderers that turn
//! a [`ZoneSpec`] into zonecfg subcommands. Pure functions; tested against
//! `testdata/`.

use mandrake_core::zone::{ZoneNic, ZoneState};

use crate::types::{ZoneConfig, ZoneError, ZoneSpec, ZoneSummary};

/// Attributes Mandrake owns: set on create, rewritten on update, removed
/// when absent from an update.
pub const MANAGED_ATTRS: [&str; 2] = [crate::HOSTNAME_ATTR, crate::RESOLVERS_ATTR];

fn parse_err(command: &str, detail: impl Into<String>) -> ZoneError {
    ZoneError::Parse {
        command: command.to_owned(),
        detail: detail.into(),
    }
}

/// Parse `zoneadm list -pc`: `zoneid:name:state:zonepath:uuid:brand:ip-type`.
/// The global zone is left out.
pub fn zoneadm_list(out: &str) -> Result<Vec<ZoneSummary>, ZoneError> {
    let mut zones = Vec::new();
    for line in out.lines().filter(|l| !l.trim().is_empty()) {
        let f: Vec<&str> = line.split(':').collect();
        if f.len() < 7 {
            return Err(parse_err(
                "zoneadm list",
                format!("expected 7 fields: {line}"),
            ));
        }
        if f[1] == "global" {
            continue;
        }
        zones.push(ZoneSummary {
            name: f[1].to_owned(),
            state: ZoneState::from_zoneadm(f[2]),
            zonepath: f[3].to_owned(),
            uuid: (!f[4].is_empty()).then(|| f[4].to_owned()),
            brand: f[5].to_owned(),
            exclusive_ip: f[6] == "excl",
        });
    }
    Ok(zones)
}

/// `2G`, `512m`, `1073741824` to bytes.
pub fn size(s: &str) -> Option<u64> {
    let s = s.trim();
    let digits_end = s
        .find(|c: char| !(c.is_ascii_digit() || c == '.'))
        .unwrap_or(s.len());
    let (number, unit) = s.split_at(digits_end);
    let value: f64 = number.parse().ok()?;
    let mult: f64 = match unit.trim().to_ascii_lowercase().as_str() {
        "" => 1.0,
        "k" => 1024.0,
        "m" => 1024.0 * 1024.0,
        "g" => 1024.0 * 1024.0 * 1024.0,
        "t" => 1024.0 * 1024.0 * 1024.0 * 1024.0,
        _ => return None,
    };
    let bytes = value * mult;
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // finite, non-negative
    (bytes.is_finite() && bytes >= 0.0).then(|| bytes.round() as u64)
}

fn unquote(v: &str) -> String {
    let v = v.trim();
    v.strip_prefix('"')
        .and_then(|s| s.strip_suffix('"'))
        .unwrap_or(v)
        .to_owned()
}

/// A `set key=value` line.
fn setting(line: &str) -> Option<(&str, String)> {
    let rest = line.trim().strip_prefix("set ")?;
    let (k, v) = rest.split_once('=')?;
    Some((k.trim(), unquote(v)))
}

/// One `add <kind> ... end` block while it is being read.
#[derive(Default)]
struct Resource {
    kind: String,
    props: Vec<(String, String)>,
    lines: Vec<String>,
}

/// Parse `zonecfg -z <name> export`.
pub fn zonecfg_export(name: &str, out: &str) -> Result<ZoneConfig, ZoneError> {
    let mut cfg = ZoneConfig {
        name: name.to_owned(),
        ..ZoneConfig::default()
    };
    let mut resource: Option<Resource> = None;
    for raw in out.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some(res) = resource.as_mut() {
            if line == "end" {
                let res = resource.take().unwrap_or_default();
                finish_resource(&mut cfg, &res.kind, &res.props, &res.lines);
            } else {
                res.lines.push(line.to_owned());
                if let Some((k, v)) = setting(line) {
                    res.props.push((k.to_owned(), v));
                } else if !line.starts_with("add ") {
                    return Err(parse_err(
                        "zonecfg export",
                        format!("unexpected line in {}: {line}", res.kind),
                    ));
                }
            }
            continue;
        }
        if let Some(kind) = line.strip_prefix("add ") {
            resource = Some(Resource {
                kind: kind.trim().to_owned(),
                ..Resource::default()
            });
        } else if let Some((k, v)) = setting(line) {
            match k {
                "brand" => cfg.brand = v,
                "zonepath" => cfg.zonepath = v,
                "autoboot" => cfg.autoboot = v == "true",
                "ip-type" => cfg.ip_type = v,
                _ => cfg.other.push(line.to_owned()),
            }
        } else if line.starts_with("create") {
            // `create -b` opens the config.
        } else {
            cfg.other.push(line.to_owned());
        }
    }
    if cfg.brand.is_empty() {
        return Err(parse_err("zonecfg export", "no brand"));
    }
    Ok(cfg)
}

fn finish_resource(cfg: &mut ZoneConfig, kind: &str, props: &[(String, String)], lines: &[String]) {
    let get = |k: &str| props.iter().find(|(pk, _)| pk == k).map(|(_, v)| v.clone());
    match kind {
        "anet" => cfg.nics.push(ZoneNic {
            name: get("linkname").unwrap_or_default(),
            over: get("lower-link").unwrap_or_default(),
            mac: get("mac-address").filter(|m| m != "auto"),
            vid: get("vlan-id").and_then(|v| v.parse().ok()),
            address: get("allowed-address"),
            gateway: get("defrouter"),
        }),
        "capped-cpu" => cfg.cpu_cap = get("ncpus").and_then(|n| n.parse().ok()),
        "capped-memory" => cfg.memory_cap = get("physical").and_then(|p| size(&p)),
        "attr" => {
            if let (Some(name), Some(value)) = (get("name"), get("value")) {
                cfg.attrs.insert(name, value);
            }
        }
        "dataset" => {
            if let Some(name) = get("name") {
                cfg.datasets.push(name);
            }
        }
        _ => cfg
            .other
            .push(format!("add {kind}; {}; end", lines.join("; "))),
    }
}

fn nic_commands(nic: &ZoneNic, out: &mut Vec<String>) {
    out.push("add anet".to_owned());
    out.push(format!("set linkname={}", nic.name));
    out.push(format!("set lower-link={}", nic.over));
    if let Some(mac) = &nic.mac {
        out.push(format!("set mac-address={mac}"));
    }
    if let Some(vid) = nic.vid {
        out.push(format!("set vlan-id={vid}"));
    }
    if let Some(a) = &nic.address {
        out.push(format!("set allowed-address={a}"));
    }
    if let Some(g) = &nic.gateway {
        out.push(format!("set defrouter={g}"));
    }
    out.push("end".to_owned());
}

fn cap_commands(spec: &ZoneSpec, out: &mut Vec<String>) {
    if let Some(c) = spec.cpu_cap {
        out.push("add capped-cpu".to_owned());
        out.push(format!("set ncpus={c}"));
        out.push("end".to_owned());
    }
    if let Some(m) = spec.memory_cap {
        out.push("add capped-memory".to_owned());
        out.push(format!("set physical={m}"));
        out.push("end".to_owned());
    }
}

fn attr_add(key: &str, value: &str, out: &mut Vec<String>) {
    out.push("add attr".to_owned());
    out.push(format!("set name={key}"));
    out.push("set type=string".to_owned());
    out.push(format!("set value=\"{value}\""));
    out.push("end".to_owned());
}

/// The zonecfg subcommands that create `spec` from nothing, ending in
/// `commit`. Joined with `; ` for `zonecfg -z <name> <commands>`.
pub fn render_create(spec: &ZoneSpec) -> Vec<String> {
    let mut out = vec![
        "create -b".to_owned(),
        format!("set brand={}", spec.brand),
        format!("set zonepath={}", spec.zonepath),
        format!("set autoboot={}", spec.autoboot),
        "set ip-type=exclusive".to_owned(),
    ];
    for nic in &spec.nics {
        nic_commands(nic, &mut out);
    }
    cap_commands(spec, &mut out);
    for (k, v) in &spec.attrs {
        attr_add(k, v, &mut out);
    }
    out.push("commit".to_owned());
    out
}

/// The zonecfg subcommands that bring `current` to `spec`: NICs and caps
/// are replaced wholesale, attributes set or added, managed attributes
/// absent from `spec` removed. Ends in `commit`.
pub fn render_update(current: &ZoneConfig, spec: &ZoneSpec) -> Vec<String> {
    let mut out = vec![format!("set autoboot={}", spec.autoboot)];
    if !current.nics.is_empty() {
        out.push("remove -F anet".to_owned());
    }
    for nic in &spec.nics {
        nic_commands(nic, &mut out);
    }
    if current.cpu_cap.is_some() {
        out.push("remove -F capped-cpu".to_owned());
    }
    if current.memory_cap.is_some() {
        out.push("remove -F capped-memory".to_owned());
    }
    cap_commands(spec, &mut out);
    for (k, v) in &spec.attrs {
        if current.attrs.contains_key(k) {
            out.push(format!("select attr name={k}"));
            out.push(format!("set value=\"{v}\""));
            out.push("end".to_owned());
        } else {
            attr_add(k, v, &mut out);
        }
    }
    for k in MANAGED_ATTRS {
        if current.attrs.contains_key(k) && !spec.attrs.contains_key(k) {
            out.push(format!("remove attr name={k}"));
        }
    }
    out.push("commit".to_owned());
    out
}

/// The subcommands that set one attribute, adding it when `exists` is false.
pub fn render_set_attr(key: &str, value: &str, exists: bool) -> Vec<String> {
    let mut out = Vec::new();
    if exists {
        out.push(format!("select attr name={key}"));
        out.push(format!("set value=\"{value}\""));
        out.push("end".to_owned());
    } else {
        attr_add(key, value, &mut out);
    }
    out.push("commit".to_owned());
    out
}

/// Zone names: letters, digits, `_ . -`, not starting with a dot or dash.
pub fn valid_zone_name(name: &str) -> bool {
    let b = name.as_bytes();
    (1..=63).contains(&b.len())
        && b[0].is_ascii_alphanumeric()
        && b.iter()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'_' | b'.' | b'-'))
        && name != "global"
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::panic,
        clippy::expect_used,
        clippy::unwrap_used,
        clippy::err_expect
    )]

    use super::*;

    const LIST: &str = include_str!("../testdata/zoneadm-list-pc.synthetic.txt");
    const LX: &str = include_str!("../testdata/zonecfg-export.lx.synthetic.txt");
    const IPKG: &str = include_str!("../testdata/zonecfg-export.ipkg.synthetic.txt");

    #[test]
    fn list_skips_global() {
        let zones = zoneadm_list(LIST).unwrap();
        assert_eq!(zones.len(), 3);
        assert_eq!(zones[0].name, "web");
        assert_eq!(zones[0].state, ZoneState::Running);
        assert_eq!(zones[0].brand, "lx");
        assert_eq!(zones[0].zonepath, "/tank/zones/web");
        assert!(zones[0].exclusive_ip);
        assert_eq!(zones[1].name, "build");
        assert_eq!(zones[1].state, ZoneState::Installed);
        assert_eq!(zones[1].brand, "ipkg");
        assert_eq!(zones[2].brand, "bhyve");
        assert_eq!(zones[2].state, ZoneState::Configured);
        assert!(zoneadm_list("garbage").is_err());
    }

    #[test]
    fn export_of_an_lx_zone() {
        let cfg = zonecfg_export("web", LX).unwrap();
        assert_eq!(cfg.brand, "lx");
        assert_eq!(cfg.zonepath, "/tank/zones/web");
        assert!(cfg.autoboot);
        assert_eq!(cfg.ip_type, "exclusive");
        assert_eq!(cfg.nics.len(), 1);
        let nic = &cfg.nics[0];
        assert_eq!(nic.name, "net0");
        assert_eq!(nic.over, "stub0");
        assert_eq!(nic.mac.as_deref(), Some("02:08:20:a1:b2:c3"));
        assert_eq!(nic.vid, Some(20));
        assert_eq!(nic.address.as_deref(), Some("10.0.0.5/24"));
        assert_eq!(nic.gateway.as_deref(), Some("10.0.0.1"));
        assert_eq!(cfg.cpu_cap, Some(1.5));
        assert_eq!(cfg.memory_cap, Some(2 * 1024 * 1024 * 1024));
        assert_eq!(
            cfg.attrs.get("mandrake-id").map(String::as_str),
            Some("0192a1b2-c3d4-7e5f-8a9b-0c1d2e3f4a60")
        );
        assert_eq!(cfg.attrs.get("hostname").map(String::as_str), Some("web"));
        assert_eq!(
            cfg.attrs.get("kernel-version").map(String::as_str),
            Some("5.10.0")
        );
        assert!(cfg.other.iter().any(|l| l.contains("add rctl")));
    }

    #[test]
    fn export_of_a_native_zone_without_managed_bits() {
        let cfg = zonecfg_export("build", IPKG).unwrap();
        assert_eq!(cfg.brand, "ipkg");
        assert!(cfg.nics.is_empty());
        assert_eq!(cfg.cpu_cap, None);
        assert_eq!(cfg.memory_cap, None);
        assert!(!cfg.attrs.contains_key("mandrake-id"));
        assert_eq!(cfg.datasets, vec!["tank/data/build".to_owned()]);
        assert!(!cfg.autoboot);
    }

    fn spec() -> ZoneSpec {
        ZoneSpec {
            name: "web".to_owned(),
            brand: "lx".to_owned(),
            zonepath: "/tank/zones/web".to_owned(),
            autoboot: true,
            nics: vec![ZoneNic {
                name: "net0".to_owned(),
                over: "stub0".to_owned(),
                mac: None,
                vid: Some(20),
                address: Some("10.0.0.5/24".to_owned()),
                gateway: Some("10.0.0.1".to_owned()),
            }],
            cpu_cap: Some(1.5),
            memory_cap: Some(2_147_483_648),
            attrs: [
                ("mandrake-id".to_owned(), "abc".to_owned()),
                ("hostname".to_owned(), "web".to_owned()),
            ]
            .into_iter()
            .collect(),
        }
    }

    #[test]
    fn create_script() {
        assert_eq!(
            render_create(&spec()).join("; "),
            "create -b; set brand=lx; set zonepath=/tank/zones/web; set autoboot=true; \
             set ip-type=exclusive; add anet; set linkname=net0; set lower-link=stub0; \
             set vlan-id=20; set allowed-address=10.0.0.5/24; set defrouter=10.0.0.1; end; \
             add capped-cpu; set ncpus=1.5; end; add capped-memory; set physical=2147483648; end; \
             add attr; set name=hostname; set type=string; set value=\"web\"; end; \
             add attr; set name=mandrake-id; set type=string; set value=\"abc\"; end; commit"
        );
    }

    #[test]
    fn update_script_replaces_and_removes() {
        let current = zonecfg_export("web", LX).unwrap();
        let mut s = spec();
        s.cpu_cap = None;
        s.attrs.remove("hostname");
        let script = render_update(&current, &s).join("; ");
        assert!(script.starts_with("set autoboot=true; remove -F anet; add anet;"));
        assert!(
            script.contains("remove -F capped-cpu; remove -F capped-memory; add capped-memory;")
        );
        assert!(!script.contains("add capped-cpu"));
        assert!(script.contains("select attr name=mandrake-id; set value=\"abc\"; end"));
        assert!(script.contains("remove attr name=hostname"));
        assert!(script.ends_with("; commit"));
        assert_eq!(
            render_set_attr("mandrake-image", "img", false).join("; "),
            "add attr; set name=mandrake-image; set type=string; set value=\"img\"; end; commit"
        );
    }

    #[test]
    fn sizes_and_names() {
        assert_eq!(size("2G"), Some(2_147_483_648));
        assert_eq!(size("512m"), Some(536_870_912));
        assert_eq!(size("1024"), Some(1024));
        assert_eq!(size("lots"), None);
        assert!(valid_zone_name("web-1"));
        assert!(valid_zone_name("db.prod"));
        assert!(!valid_zone_name("global"));
        assert!(!valid_zone_name("-x"));
        assert!(!valid_zone_name("a b"));
    }
}
