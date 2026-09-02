//! Parsers for `zpool`, `zfs`, and `diskinfo` output. Pure functions;
//! tested against `testdata/`.

use mandrake_core::{
    Timestamp,
    storage::{
        Bytes, DatasetKind, PoolHealth, ScanFunction, ScanState, ScanStatus, Vdev, VdevType,
    },
};

use crate::types::{DatasetInfo, DeviceInfo, SnapshotInfo, ZfsError};

fn parse_err(command: &str, detail: impl Into<String>) -> ZfsError {
    ZfsError::Parse {
        command: command.to_owned(),
        detail: detail.into(),
    }
}

/// `-` means not applicable in `-H` output.
fn opt(field: &str) -> Option<&str> {
    match field {
        "-" | "" | "none" => None,
        s => Some(s),
    }
}

fn num(field: &str) -> Option<u64> {
    opt(field)?.parse().ok()
}

/// `1.50x` or `1.50` to a ratio.
fn ratio(field: &str) -> Option<f64> {
    opt(field)?.trim_end_matches('x').parse().ok()
}

fn yes_no(field: &str) -> Option<bool> {
    match field {
        "yes" | "on" => Some(true),
        "no" | "off" => Some(false),
        _ => None,
    }
}

/// The columns `list_pools` asks `zpool list -Hp` for, in order.
pub const ZPOOL_LIST_COLUMNS: &str =
    "name,size,allocated,free,fragmentation,capacity,dedupratio,health";

/// One `zpool list` row.
#[derive(Debug, Clone, PartialEq)]
pub struct PoolListRow {
    /// Name.
    pub name: String,
    /// Size.
    pub size: Bytes,
    /// Allocated.
    pub allocated: Bytes,
    /// Free.
    pub free: Bytes,
    /// Fragmentation.
    pub fragmentation: Option<u32>,
    /// Capacity.
    pub capacity: Option<u32>,
    /// Dedup.
    pub dedup_ratio: Option<f64>,
    /// Health.
    pub health: PoolHealth,
}

/// Parse `zpool list -Hp -o` with [`ZPOOL_LIST_COLUMNS`].
pub fn zpool_list(out: &str) -> Result<Vec<PoolListRow>, ZfsError> {
    out.lines()
        .filter(|l| !l.trim().is_empty())
        .map(|line| {
            let f: Vec<&str> = line.split('\t').collect();
            if f.len() < 8 {
                return Err(parse_err(
                    "zpool list",
                    format!("expected 8 fields: {line}"),
                ));
            }
            Ok(PoolListRow {
                name: f[0].to_owned(),
                size: num(f[1]).unwrap_or(0),
                allocated: num(f[2]).unwrap_or(0),
                free: num(f[3]).unwrap_or(0),
                fragmentation: num(f[4].trim_end_matches('%')).and_then(|n| u32::try_from(n).ok()),
                capacity: num(f[5].trim_end_matches('%')).and_then(|n| u32::try_from(n).ok()),
                dedup_ratio: ratio(f[6]),
                health: f[7]
                    .parse()
                    .map_err(|e| parse_err("zpool list", format!("{e}: {line}")))?,
            })
        })
        .collect()
}

/// What `zpool status` says about one pool.
#[derive(Debug, Clone, PartialEq)]
pub struct PoolStatus {
    /// Name.
    pub name: String,
    /// State.
    pub state: PoolHealth,
    /// `status:` and `action:` paragraphs, joined.
    pub status_text: Option<String>,
    /// Scan.
    pub scan: Option<ScanStatus>,
    /// Vdev tree rooted at the pool.
    pub vdevs: Vdev,
    /// The `errors:` line.
    pub errors: Option<String>,
}

fn vdev_type(name: &str, level: usize) -> VdevType {
    if level == 0 {
        return VdevType::Root;
    }
    let base = name.rsplit_once('-').map_or(name, |(b, suffix)| {
        if suffix.chars().all(|c| c.is_ascii_digit()) {
            b
        } else {
            name
        }
    });
    match base {
        "mirror" => VdevType::Mirror,
        "raidz1" | "raidz" => VdevType::Raidz1,
        "raidz2" => VdevType::Raidz2,
        "raidz3" => VdevType::Raidz3,
        "replacing" => VdevType::Replacing,
        "spare" => VdevType::SpareGroup,
        "logs" => VdevType::Log,
        "cache" => VdevType::Cache,
        "spares" => VdevType::Spare,
        _ if name.starts_with('/') => VdevType::File,
        _ => VdevType::Disk,
    }
}

/// Parse the human `zpool status <pool>` output. Dates are taken as UTC.
pub fn zpool_status(out: &str) -> Result<PoolStatus, ZfsError> {
    let mut name = None;
    let mut state = None;
    let mut status_lines: Vec<String> = Vec::new();
    let mut scan_lines: Vec<String> = Vec::new();
    let mut config_lines: Vec<&str> = Vec::new();
    let mut errors = None;
    let mut section = "";

    for raw in out.lines() {
        let line = raw.trim_end();
        let trimmed = line.trim_start();
        if let Some((key, rest)) = trimmed.split_once(':') {
            let key = key.trim();
            let rest = rest.trim();
            let is_header = matches!(
                key,
                "pool"
                    | "state"
                    | "status"
                    | "action"
                    | "see"
                    | "scan"
                    | "scrub"
                    | "config"
                    | "errors"
            ) && !line.starts_with('\t')
                && !line.starts_with("    ");
            if is_header {
                section = match key {
                    "pool" => {
                        name = Some(rest.to_owned());
                        ""
                    }
                    "state" => {
                        state = Some(rest.to_owned());
                        ""
                    }
                    "status" | "action" | "see" => {
                        status_lines.push(rest.to_owned());
                        "status"
                    }
                    "scan" | "scrub" => {
                        scan_lines.push(rest.to_owned());
                        "scan"
                    }
                    "config" => "config",
                    "errors" => {
                        errors = Some(rest.to_owned());
                        ""
                    }
                    _ => "",
                };
                continue;
            }
        }
        match section {
            "status" if !trimmed.is_empty() => status_lines.push(trimmed.to_owned()),
            "scan" if !trimmed.is_empty() => scan_lines.push(trimmed.to_owned()),
            "config" if !trimmed.is_empty() => config_lines.push(line),
            _ => {}
        }
    }

    let name = name.ok_or_else(|| parse_err("zpool status", "no `pool:` line"))?;
    let state: PoolHealth = state
        .ok_or_else(|| parse_err("zpool status", "no `state:` line"))?
        .parse()
        .map_err(|e| parse_err("zpool status", format!("{e}")))?;
    let vdevs = parse_config(&config_lines, &name, state)?;
    Ok(PoolStatus {
        name,
        state,
        status_text: (!status_lines.is_empty()).then(|| status_lines.join("\n")),
        scan: parse_scan(&scan_lines),
        vdevs,
        errors,
    })
}

fn parse_config(lines: &[&str], pool: &str, pool_state: PoolHealth) -> Result<Vdev, ZfsError> {
    let mut root: Option<Vdev> = None;
    // (level, path of child indexes) for the current open nodes
    let mut stack: Vec<(usize, Vec<usize>)> = Vec::new();
    // `logs`, `cache`, and `spares` print at the pool's indentation but are
    // children of it; everything under them shifts one level deeper.
    let mut offset = 0;

    for line in lines {
        let body = line.trim_start_matches('\t');
        let indent = body.len() - body.trim_start().len();
        let fields: Vec<&str> = body.split_whitespace().collect();
        let Some(&first) = fields.first() else {
            continue;
        };
        if first == "NAME" {
            continue;
        }
        let physical = indent / 2;
        if physical == 0 {
            offset = usize::from(matches!(first, "logs" | "cache" | "spares"));
        }
        let level = physical + offset;
        let type_ = vdev_type(first, level);
        let (state, counters): (PoolHealth, &[&str]) =
            match fields.get(1).and_then(|s| s.parse().ok()) {
                Some(s) => (s, &fields[2..]),
                None => (pool_state, &fields[1..]),
            };
        let counter = |i: usize| counters.get(i).and_then(|c| c.parse::<u64>().ok());
        let trailing: Vec<&str> = counters.iter().skip(3).copied().collect();
        let node = Vdev {
            name: first.to_owned(),
            type_,
            state,
            read_errors: counter(0),
            write_errors: counter(1),
            checksum_errors: counter(2),
            note: (!trailing.is_empty()).then(|| trailing.join(" ")),
            children: Vec::new(),
        };

        if level == 0 {
            if first != pool {
                return Err(parse_err(
                    "zpool status",
                    format!("config root `{first}` is not `{pool}`"),
                ));
            }
            root = Some(node);
            stack = vec![(0, Vec::new())];
            continue;
        }
        let Some(tree) = root.as_mut() else {
            return Err(parse_err("zpool status", "vdev before the pool line"));
        };
        while stack.last().is_some_and(|(l, _)| *l >= level) {
            stack.pop();
        }
        let parent_path = stack.last().map(|(_, p)| p.clone()).unwrap_or_default();
        let parent = walk_mut(tree, &parent_path);
        parent.children.push(node);
        let mut path = parent_path;
        path.push(parent.children.len() - 1);
        stack.push((level, path));
    }
    root.ok_or_else(|| parse_err("zpool status", "no config section"))
}

fn walk_mut<'a>(node: &'a mut Vdev, path: &[usize]) -> &'a mut Vdev {
    let mut cur = node;
    for &i in path {
        cur = &mut cur.children[i];
    }
    cur
}

/// `Mon Sep  1 10:00:00 2026` as UTC.
pub fn ctime(s: &str) -> Option<Timestamp> {
    let s = s.trim();
    let fmt = time::macros::format_description!(
        "[weekday repr:short] [month repr:short] [day padding:space] [hour]:[minute]:[second] [year]"
    );
    let parsed = time::PrimitiveDateTime::parse(s, fmt).ok()?;
    Some(Timestamp::from(parsed.assume_utc()))
}

/// `123M/s`, `1.5G/s`, `50K/s` to bytes per second.
pub fn rate(s: &str) -> Option<u64> {
    let s = s.trim().trim_end_matches("/s");
    size(s)
}

/// `1.23G`, `456M`, `789`, `2T` to bytes (ZFS human units, powers of 1024).
pub fn size(s: &str) -> Option<u64> {
    let s = s.trim();
    let (digits, unit) = s.split_at(
        s.find(|c: char| !c.is_ascii_digit() && c != '.')
            .unwrap_or(s.len()),
    );
    let value: f64 = digits.parse().ok()?;
    let mult: f64 = match unit.trim() {
        "" | "B" => 1.0,
        "K" | "KB" => 1024.0,
        "M" | "MB" => 1024.0_f64.powi(2),
        "G" | "GB" => 1024.0_f64.powi(3),
        "T" | "TB" => 1024.0_f64.powi(4),
        "P" | "PB" => 1024.0_f64.powi(5),
        _ => return None,
    };
    // Sizes fit comfortably; truncation is intended.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    Some((value * mult).round() as u64)
}

/// Parse the `scan:` lines (first line is the summary).
pub fn parse_scan(lines: &[String]) -> Option<ScanStatus> {
    let first = lines.first()?;
    let summary = lines.join("\n");
    if first.starts_with("none requested") {
        return None;
    }
    let function = if first.starts_with("resilver") {
        ScanFunction::Resilver
    } else {
        ScanFunction::Scrub
    };
    let after = |marker: &str| first.find(marker).map(|i| first[i + marker.len()..].trim());
    let (state, started_at, finished_at) = if let Some(d) = after("in progress since") {
        (ScanState::InProgress, ctime(d), None)
    } else if let Some(d) = after("paused since") {
        (ScanState::InProgress, ctime(d), None)
    } else if let Some(d) = after("canceled on") {
        (ScanState::Canceled, None, ctime(d))
    } else if let Some(d) = after(" on ") {
        (ScanState::Finished, None, ctime(d))
    } else {
        (ScanState::Finished, None, None)
    };
    let errors = first
        .split_whitespace()
        .collect::<Vec<_>>()
        .windows(2)
        .find(|w| w[1].starts_with("error"))
        .and_then(|w| w[0].parse().ok());
    let mut progress = None;
    let mut rate_bps = None;
    for line in lines {
        for token in line.split(|c: char| c.is_whitespace() || c == ',') {
            if let Some(p) = token.strip_suffix("%") {
                if let Ok(v) = p.parse::<f64>() {
                    progress = Some(v / 100.0);
                }
            }
            if token.ends_with("/s") {
                rate_bps = rate(token);
            }
        }
    }
    Some(ScanStatus {
        function,
        state,
        progress,
        started_at,
        finished_at,
        errors,
        rate_bytes_per_second: rate_bps,
        summary,
    })
}

/// The columns `list_datasets` asks `zfs list -Hp` for, in order.
pub const ZFS_LIST_COLUMNS: &str = "name,type,mountpoint,mounted,used,available,referenced,logicalused,quota,reservation,compression,compressratio,atime,recordsize,volsize,volblocksize,origin,creation,nightshade.systems:mandrake-id";

/// Parse `zfs list -Hp -t filesystem,volume -o` with [`ZFS_LIST_COLUMNS`].
pub fn zfs_list(out: &str) -> Result<Vec<DatasetInfo>, ZfsError> {
    out.lines()
        .filter(|l| !l.trim().is_empty())
        .map(|line| {
            let f: Vec<&str> = line.split('\t').collect();
            if f.len() < 19 {
                return Err(parse_err("zfs list", format!("expected 19 fields: {line}")));
            }
            let kind = match f[1] {
                "filesystem" => DatasetKind::Filesystem,
                "volume" => DatasetKind::Volume,
                other => return Err(parse_err("zfs list", format!("unknown type `{other}`"))),
            };
            Ok(DatasetInfo {
                name: f[0].to_owned(),
                kind,
                mountpoint: opt(f[2]).filter(|m| *m != "legacy").map(str::to_owned),
                mounted: yes_no(f[3]).unwrap_or(false),
                used: num(f[4]).unwrap_or(0),
                available: num(f[5]).unwrap_or(0),
                referenced: num(f[6]).unwrap_or(0),
                logical_used: num(f[7]),
                quota: num(f[8]).filter(|q| *q > 0),
                reservation: num(f[9]).filter(|r| *r > 0),
                compression: opt(f[10]).map(str::to_owned),
                compress_ratio: ratio(f[11]),
                atime: yes_no(f[12]),
                recordsize: num(f[13]),
                volsize: num(f[14]),
                volblocksize: num(f[15]),
                origin: opt(f[16]).map(str::to_owned),
                created_at: num(f[17])
                    .and_then(|s| i64::try_from(s).ok())
                    .and_then(Timestamp::from_unix)
                    .ok_or_else(|| parse_err("zfs list", format!("bad creation: {line}")))?,
                mandrake_id: opt(f[18]).map(str::to_owned),
            })
        })
        .collect()
}

/// The columns `list_snapshots` asks for, in order.
pub const ZFS_SNAPSHOT_COLUMNS: &str =
    "name,used,referenced,creation,clones,nightshade.systems:mandrake-id";

/// Parse `zfs list -Hp -t snapshot -o` with [`ZFS_SNAPSHOT_COLUMNS`].
pub fn zfs_snapshots(out: &str) -> Result<Vec<SnapshotInfo>, ZfsError> {
    out.lines()
        .filter(|l| !l.trim().is_empty())
        .map(|line| {
            let f: Vec<&str> = line.split('\t').collect();
            if f.len() < 6 {
                return Err(parse_err(
                    "zfs list -t snapshot",
                    format!("expected 6 fields: {line}"),
                ));
            }
            Ok(SnapshotInfo {
                name: f[0].to_owned(),
                used: num(f[1]).unwrap_or(0),
                referenced: num(f[2]).unwrap_or(0),
                created_at: num(f[3])
                    .and_then(|s| i64::try_from(s).ok())
                    .and_then(Timestamp::from_unix)
                    .ok_or_else(|| {
                        parse_err("zfs list -t snapshot", format!("bad creation: {line}"))
                    })?,
                clones: opt(f[4])
                    .map(|c| c.split(',').map(str::to_owned).collect())
                    .unwrap_or_default(),
                mandrake_id: opt(f[5]).map(str::to_owned),
            })
        })
        .collect()
}

/// Parse `diskinfo -Hp`: `TYPE DISK VID PID SIZE RMV SSD`, tab separated.
pub fn diskinfo(out: &str) -> Vec<DeviceInfo> {
    out.lines()
        .filter(|l| !l.trim().is_empty())
        .filter_map(|line| {
            let f: Vec<&str> = line.split('\t').map(str::trim).collect();
            if f.len() < 5 {
                return None;
            }
            Some(DeviceInfo {
                name: f[1].to_owned(),
                bus: opt(f[0]).map(str::to_owned),
                vendor: opt(f[2]).map(str::to_owned),
                product: opt(f[3]).map(str::to_owned),
                serial: None,
                size: f[4].parse().or_else(|_| size(f[4]).ok_or(())).unwrap_or(0),
                removable: f.get(5).is_some_and(|v| *v == "yes"),
                solid_state: f.get(6).and_then(|v| yes_no(v)),
            })
        })
        .collect()
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

    const ZPOOL_LIST: &str = include_str!("../testdata/zpool-list-Hp.synthetic.txt");
    const STATUS_MIRROR: &str = include_str!("../testdata/zpool-status.tank.synthetic.txt");
    const STATUS_SCRUB: &str =
        include_str!("../testdata/zpool-status.tank-scrubbing.synthetic.txt");
    const STATUS_RPOOL: &str = include_str!("../testdata/zpool-status.rpool.synthetic.txt");
    const ZFS_LIST: &str = include_str!("../testdata/zfs-list-Hp.synthetic.txt");
    const ZFS_SNAPS: &str = include_str!("../testdata/zfs-list-snapshots-Hp.synthetic.txt");
    const DISKINFO: &str = include_str!("../testdata/diskinfo-Hp.synthetic.txt");

    #[test]
    fn pool_list_rows() {
        let rows = zpool_list(ZPOOL_LIST).unwrap_or_default();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].name, "rpool");
        assert_eq!(rows[0].health, PoolHealth::Online);
        assert_eq!(rows[1].name, "tank");
        assert_eq!(rows[1].size, 3_985_729_650_688);
        assert_eq!(rows[1].capacity, Some(12));
        assert_eq!(rows[1].dedup_ratio, Some(1.0));
    }

    #[test]
    fn status_tree_with_mirror_log_cache_and_spare() {
        let s = zpool_status(STATUS_MIRROR).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(s.name, "tank");
        assert_eq!(s.state, PoolHealth::Online);
        assert_eq!(s.vdevs.type_, VdevType::Root);
        let kinds: Vec<_> = s
            .vdevs
            .children
            .iter()
            .map(|c| (c.name.as_str(), c.type_))
            .collect();
        assert_eq!(
            kinds,
            vec![
                ("mirror-0", VdevType::Mirror),
                ("mirror-1", VdevType::Mirror),
                ("logs", VdevType::Log),
                ("cache", VdevType::Cache),
                ("spares", VdevType::Spare),
            ]
        );
        assert_eq!(s.vdevs.children[0].children.len(), 2);
        assert_eq!(s.vdevs.children[0].children[1].name, "c1t2d0");
        assert_eq!(s.vdevs.children[0].children[1].checksum_errors, Some(0));
        assert_eq!(s.vdevs.children[4].children[0].state, PoolHealth::Avail);
        assert_eq!(s.vdevs.leaves().len(), 7);
        let scan = s.scan.expect("scan");
        assert_eq!(scan.function, ScanFunction::Scrub);
        assert_eq!(scan.state, ScanState::Finished);
        assert_eq!(scan.errors, Some(0));
        assert!(scan.finished_at.is_some());
        assert_eq!(s.errors.as_deref(), Some("No known data errors"));
    }

    #[test]
    fn status_degraded_with_scrub_in_progress() {
        let s = zpool_status(STATUS_SCRUB).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(s.state, PoolHealth::Degraded);
        assert!(
            s.status_text
                .as_deref()
                .is_some_and(|t| t.contains("One or more devices"))
        );
        let scan = s.scan.expect("scan");
        assert_eq!(scan.state, ScanState::InProgress);
        assert!(scan.started_at.is_some());
        assert_eq!(scan.progress.map(|p| (p * 100.0).round()), Some(27.0));
        assert_eq!(scan.rate_bytes_per_second, Some(100 * 1024 * 1024));
        let faulted = &s.vdevs.children[0].children[1];
        assert_eq!(faulted.state, PoolHealth::Faulted);
        assert_eq!(faulted.read_errors, Some(12));
        assert_eq!(faulted.note.as_deref(), Some("too many errors"));
    }

    #[test]
    fn status_rpool_single_disk() {
        let s = zpool_status(STATUS_RPOOL).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(s.vdevs.children.len(), 1);
        assert_eq!(s.vdevs.children[0].type_, VdevType::Disk);
        assert!(s.scan.is_none());
    }

    #[test]
    fn dataset_rows() {
        let ds = zfs_list(ZFS_LIST).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(ds.len(), 6);
        let root = &ds[0];
        assert_eq!(root.name, "rpool");
        assert_eq!(root.kind, DatasetKind::Filesystem);
        assert_eq!(root.mountpoint.as_deref(), Some("/rpool"));
        assert!(root.mounted);
        assert_eq!(root.compression.as_deref(), Some("lz4"));
        assert_eq!(root.quota, None);
        let vol = ds
            .iter()
            .find(|d| d.kind == DatasetKind::Volume)
            .expect("volume");
        assert_eq!(vol.name, "tank/vms/disk0");
        assert_eq!(vol.volsize, Some(21_474_836_480));
        assert_eq!(vol.volblocksize, Some(8192));
        assert_eq!(vol.mountpoint, None);
        assert_eq!(
            vol.mandrake_id.as_deref(),
            Some("0192a1b2-c3d4-7e5f-8a9b-0c1d2e3f4a5b")
        );
        let clone = ds.iter().find(|d| d.origin.is_some()).expect("clone");
        assert_eq!(
            clone.origin.as_deref(),
            Some("tank/images/lx-ubuntu@import")
        );
        assert_eq!(ds[2].pool(), "tank");
        assert_eq!(ds[1].created_at.to_rfc3339(), "2025-08-01T10:00:00Z");
    }

    #[test]
    fn snapshot_rows() {
        let snaps = zfs_snapshots(ZFS_SNAPS).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(snaps.len(), 3);
        assert_eq!(snaps[0].dataset(), "tank/images/lx-ubuntu");
        assert_eq!(snaps[0].short_name(), "import");
        assert_eq!(snaps[0].clones, vec!["tank/zones/web1".to_owned()]);
        assert!(snaps[1].clones.is_empty());
        assert_eq!(
            snaps[2].mandrake_id.as_deref(),
            Some("0192a1b2-c3d4-7e5f-8a9b-0c1d2e3f4a5c")
        );
    }

    #[test]
    fn devices() {
        let d = diskinfo(DISKINFO);
        assert_eq!(d.len(), 4);
        assert_eq!(d[0].name, "c1t0d0");
        assert_eq!(d[0].bus.as_deref(), Some("SATA"));
        assert_eq!(d[0].size, 1_000_204_886_016);
        assert!(!d[0].removable);
        assert_eq!(d[0].solid_state, Some(true));
        assert_eq!(d[3].bus.as_deref(), Some("USB"));
        assert!(d[3].removable);
    }

    #[test]
    fn sizes_rates_and_dates() {
        assert_eq!(size("1.5G"), Some(1_610_612_736));
        assert_eq!(size("512"), Some(512));
        assert_eq!(rate("100M/s"), Some(104_857_600));
        assert_eq!(size("junk"), None);
        assert_eq!(
            ctime("Tue Sep  1 10:00:00 2026").map(Timestamp::to_rfc3339),
            Some("2026-09-01T10:00:00Z".to_owned())
        );
        assert_eq!(
            ctime("Tue Sep 15 09:05:07 2026").map(Timestamp::to_rfc3339),
            Some("2026-09-15T09:05:07Z".to_owned())
        );
    }
}
