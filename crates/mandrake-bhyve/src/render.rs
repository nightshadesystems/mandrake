//! The mapping between a VM and the bhyve brand's zone configuration.

use std::collections::BTreeMap;

use mandrake_core::vm::Bootrom;
use mandrake_zones::{ZoneConfig, ZoneFs, ZoneSpec, parse};

use crate::{
    BRAND,
    types::{BhyveError, DiskSpec, Result, VmConfig, VmSpec},
};

/// Where the brand puts the VNC socket, relative to the zone root.
pub const VNC_SOCKET: &str = "/tmp/vm.vnc";

/// The VNC socket's path on the host for a zonepath.
pub fn vnc_socket_path(zonepath: &str) -> String {
    format!("{}/root{VNC_SOCKET}", zonepath.trim_end_matches('/'))
}

/// The raw device for a zvol, as the brand sees it.
pub fn zvol_device(zvol: &str) -> String {
    format!("/dev/zvol/rdsk/{zvol}")
}

fn bootrom_value(b: Bootrom) -> &'static str {
    match b {
        Bootrom::Uefi => "BHYVE_RELEASE",
        Bootrom::UefiCsm => "BHYVE_RELEASE_CSM",
    }
}

fn bootrom_parse(s: &str) -> Option<Bootrom> {
    match s.trim() {
        "BHYVE_RELEASE" | "BHYVE_DEBUG" | "uefi" => Some(Bootrom::Uefi),
        "BHYVE_RELEASE_CSM" | "BHYVE_DEBUG_CSM" | "uefi-csm" => Some(Bootrom::UefiCsm),
        _ => None,
    }
}

/// Memory as the brand's `ram` attribute: whole mebibytes.
fn ram_value(bytes: u64) -> String {
    const MIB: u64 = 1 << 20;
    format!("{}M", bytes.div_ceil(MIB).max(1))
}

fn on_off(v: bool) -> &'static str {
    if v { "on" } else { "off" }
}

fn is_on(s: &str) -> bool {
    matches!(s.trim(), "on" | "true" | "yes" | "1")
}

/// The attribute name for a disk slot.
fn disk_attr(index: usize, boot: bool) -> String {
    if boot {
        "bootdisk".to_owned()
    } else {
        format!("disk{index}")
    }
}

/// The attribute name for a cdrom slot.
fn cdrom_attr(index: usize) -> String {
    if index == 0 {
        "cdrom".to_owned()
    } else {
        format!("cdrom{index}")
    }
}

/// The zone configuration for a VM.
pub fn to_zone_spec(vm: &VmSpec) -> ZoneSpec {
    let mut attrs: BTreeMap<String, String> = vm.attrs.clone();
    attrs.insert("vcpus".to_owned(), vm.vcpus.to_string());
    attrs.insert("ram".to_owned(), ram_value(vm.memory_bytes));
    attrs.insert("bootrom".to_owned(), bootrom_value(vm.bootrom).to_owned());
    attrs.insert("acpi".to_owned(), on_off(vm.acpi).to_owned());
    attrs.insert("vnc".to_owned(), on_off(vm.vnc).to_owned());
    let mut devices = Vec::new();
    for (i, d) in vm.disks.iter().enumerate() {
        attrs.insert(disk_attr(i, d.boot), d.zvol.clone());
        devices.push(zvol_device(&d.zvol));
    }
    let mut fs = Vec::new();
    for (i, path) in vm.cdroms.iter().enumerate() {
        attrs.insert(cdrom_attr(i), path.clone());
        fs.push(ZoneFs {
            dir: path.clone(),
            special: path.clone(),
            type_: "lofs".to_owned(),
            options: vec!["ro".to_owned(), "nodevices".to_owned()],
        });
    }
    ZoneSpec {
        name: vm.name.clone(),
        brand: BRAND.to_owned(),
        zonepath: vm.zonepath.clone(),
        autoboot: vm.autoboot,
        nics: vm.nics.clone(),
        cpu_cap: None,
        memory_cap: None,
        devices,
        fs,
        attrs,
    }
}

fn attr_err(attr: &str, value: &str) -> BhyveError {
    BhyveError::Attr {
        attr: attr.to_owned(),
        value: value.to_owned(),
    }
}

/// A slot number from `disk`, `disk3`, `cdrom`, `cdrom2`, ...; the bare
/// name is slot 1 for disks (slot 0 is `bootdisk`) and slot 0 for cdroms.
fn slot(key: &str, prefix: &str, bare: u32) -> Option<u32> {
    let rest = key.strip_prefix(prefix)?;
    if rest.is_empty() {
        Some(bare)
    } else {
        rest.parse().ok()
    }
}

/// A VM from its zone configuration.
pub fn from_zone_config(cfg: &ZoneConfig) -> Result<VmConfig> {
    if cfg.brand != BRAND {
        return Err(BhyveError::NotBhyve(format!(
            "{} is a {} zone",
            cfg.name, cfg.brand
        )));
    }
    let get = |k: &str| cfg.attrs.get(k).map(String::as_str);
    let vcpus = match get("vcpus") {
        Some(v) => v.trim().parse().map_err(|_| attr_err("vcpus", v))?,
        None => 1,
    };
    let memory_bytes = match get("ram") {
        Some(v) => parse::size(v).ok_or_else(|| attr_err("ram", v))?,
        None => 0,
    };
    let bootrom = match get("bootrom") {
        Some(v) => bootrom_parse(v).ok_or_else(|| attr_err("bootrom", v))?,
        None => Bootrom::Uefi,
    };
    let mut disks: Vec<(u32, DiskSpec)> = Vec::new();
    let mut cdroms: Vec<(u32, String)> = Vec::new();
    let mut attrs = BTreeMap::new();
    for (k, v) in &cfg.attrs {
        if k == "bootdisk" {
            disks.push((
                0,
                DiskSpec {
                    zvol: v.clone(),
                    boot: true,
                },
            ));
        } else if let Some(s) = slot(k, "disk", 1) {
            disks.push((
                s,
                DiskSpec {
                    zvol: v.clone(),
                    boot: false,
                },
            ));
        } else if let Some(s) = slot(k, "cdrom", 0) {
            cdroms.push((s, v.clone()));
        } else if (!parse::is_managed_attr(k) || k.starts_with("mandrake-"))
            && !matches!(k.as_str(), "vcpus" | "ram" | "bootrom" | "acpi" | "vnc")
        {
            attrs.insert(k.clone(), v.clone());
        }
    }
    disks.sort_by_key(|(s, _)| *s);
    cdroms.sort_by_key(|(s, _)| *s);
    Ok(VmConfig {
        name: cfg.name.clone(),
        zonepath: cfg.zonepath.clone(),
        vcpus,
        memory_bytes,
        bootrom,
        acpi: get("acpi").is_none_or(is_on),
        vnc: get("vnc").is_some_and(is_on),
        autoboot: cfg.autoboot,
        disks,
        cdroms,
        nics: cfg.nics.clone(),
        attrs,
    })
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use mandrake_core::zone::ZoneNic;

    use super::*;

    const EXPORT: &str = include_str!("../testdata/zonecfg-export.bhyve.synthetic.txt");

    fn spec() -> VmSpec {
        VmSpec {
            name: "vm0".to_owned(),
            zonepath: "/tank/vms/vm0".to_owned(),
            vcpus: 2,
            memory_bytes: 2 * 1024 * 1024 * 1024,
            bootrom: Bootrom::Uefi,
            acpi: true,
            vnc: true,
            autoboot: true,
            disks: vec![
                DiskSpec {
                    zvol: "tank/vms/vm0/disk0".to_owned(),
                    boot: true,
                },
                DiskSpec {
                    zvol: "tank/vms/vm0/disk1".to_owned(),
                    boot: false,
                },
            ],
            cdroms: vec!["/tank/images/iso/x.iso".to_owned()],
            nics: vec![ZoneNic {
                name: "net0".to_owned(),
                over: "stub0".to_owned(),
                mac: None,
                vid: None,
                address: None,
                gateway: None,
            }],
            attrs: [("mandrake-id".to_owned(), "abc".to_owned())]
                .into_iter()
                .collect(),
        }
    }

    #[test]
    fn renders_the_brand_configuration() {
        let z = to_zone_spec(&spec());
        assert_eq!(z.brand, "bhyve");
        assert_eq!(z.attrs.get("vcpus").map(String::as_str), Some("2"));
        assert_eq!(z.attrs.get("ram").map(String::as_str), Some("2048M"));
        assert_eq!(
            z.attrs.get("bootrom").map(String::as_str),
            Some("BHYVE_RELEASE")
        );
        assert_eq!(z.attrs.get("acpi").map(String::as_str), Some("on"));
        assert_eq!(z.attrs.get("vnc").map(String::as_str), Some("on"));
        assert_eq!(
            z.attrs.get("bootdisk").map(String::as_str),
            Some("tank/vms/vm0/disk0")
        );
        assert_eq!(
            z.attrs.get("disk1").map(String::as_str),
            Some("tank/vms/vm0/disk1")
        );
        assert_eq!(
            z.attrs.get("cdrom").map(String::as_str),
            Some("/tank/images/iso/x.iso")
        );
        assert_eq!(z.attrs.get("mandrake-id").map(String::as_str), Some("abc"));
        assert_eq!(
            z.devices,
            vec![
                "/dev/zvol/rdsk/tank/vms/vm0/disk0",
                "/dev/zvol/rdsk/tank/vms/vm0/disk1"
            ]
        );
        assert_eq!(z.fs.len(), 1);
        assert_eq!(z.fs[0].type_, "lofs");
        assert_eq!(z.fs[0].options, vec!["ro", "nodevices"]);
        let script = parse::render_create(&z).join("; ");
        assert!(script.contains("add device; set match=/dev/zvol/rdsk/tank/vms/vm0/disk0; end"));
        assert!(script.contains(
            "add fs; set dir=/tank/images/iso/x.iso; set special=/tank/images/iso/x.iso; \
             set type=lofs; add options ro; add options nodevices; end"
        ));
        assert!(script.contains(
            "add attr; set name=bootdisk; set type=string; set value=\"tank/vms/vm0/disk0\"; end"
        ));
        assert_eq!(
            vnc_socket_path("/tank/vms/vm0"),
            "/tank/vms/vm0/root/tmp/vm.vnc"
        );
    }

    #[test]
    fn reads_a_vm_back() {
        let cfg = parse::zonecfg_export("vm0", EXPORT).unwrap();
        let vm = from_zone_config(&cfg).unwrap();
        assert_eq!(vm.vcpus, 2);
        assert_eq!(vm.memory_bytes, 2 * 1024 * 1024 * 1024);
        assert_eq!(vm.bootrom, Bootrom::Uefi);
        assert!(vm.acpi);
        assert!(vm.vnc);
        assert_eq!(vm.disks.len(), 2);
        assert_eq!(vm.disks[0].0, 0);
        assert!(vm.disks[0].1.boot);
        assert_eq!(vm.disks[1].0, 1);
        assert_eq!(vm.disks[1].1.zvol, "tank/vms/vm0/disk1");
        assert_eq!(
            vm.cdroms,
            vec![(
                0,
                "/tank/images/iso/6f1d2c3b-4a59-4e8f-9c7d-2b3c4d5e6f70.iso".to_owned()
            )]
        );
        assert_eq!(vm.nics.len(), 1);
        assert_eq!(
            vm.attrs.get("mandrake-id").map(String::as_str),
            Some("0192a1b2-c3d4-7e5f-8a9b-0c1d2e3f4a70")
        );
        assert!(!vm.attrs.contains_key("vcpus"));
        assert_eq!(cfg.devices.len(), 2);
        assert_eq!(cfg.fs.len(), 1);

        // Round trip through the spec keeps the brand attributes.
        let back = to_zone_spec(&VmSpec {
            name: vm.name.clone(),
            zonepath: vm.zonepath.clone(),
            vcpus: vm.vcpus,
            memory_bytes: vm.memory_bytes,
            bootrom: vm.bootrom,
            acpi: vm.acpi,
            vnc: vm.vnc,
            autoboot: vm.autoboot,
            disks: vm.disks.iter().map(|(_, d)| d.clone()).collect(),
            cdroms: vm.cdroms.iter().map(|(_, p)| p.clone()).collect(),
            nics: vm.nics.clone(),
            attrs: vm.attrs.clone(),
        });
        assert_eq!(back.attrs.get("ram").map(String::as_str), Some("2048M"));
        assert_eq!(back.devices, cfg.devices);

        let mut lx = cfg.clone();
        lx.brand = "lx".to_owned();
        assert!(matches!(
            from_zone_config(&lx),
            Err(BhyveError::NotBhyve(_))
        ));
    }
}
