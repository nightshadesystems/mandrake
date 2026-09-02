//! Host facts for the `system` resources.
//!
//! Shell-outs to `kstat`, `beadm`, and friends per ADR-0003, best effort:
//! on a non-illumos development host the figures that need illumos come
//! back as zero or `unknown` rather than failing. Parsers are pure and
//! unit-tested; the boot-environment parser moves to `mandrake-zfs` in
//! Phase 7.

// The parsers take the map `parse_kstat` returns; no other hasher is used.
#![allow(clippy::implicit_hasher)]

use std::collections::HashMap;

use mandrake_core::{
    Timestamp,
    api::{Memory, SystemResources},
};
use tokio::process::Command;

/// What `GET /system` needs from the host.
#[derive(Debug, Clone)]
pub struct Facts {
    /// Hostname.
    pub hostname: String,
    /// OmniOS release, for example `r151054`.
    pub omnios_release: String,
    /// Active boot environment.
    pub boot_environment: String,
    /// Seconds since boot.
    pub uptime_seconds: u64,
    /// Timezone name.
    pub timezone: Option<String>,
}

async fn run(program: &str, args: &[&str]) -> Option<String> {
    let output = Command::new(program).args(args).output().await.ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).into_owned())
}

async fn read(path: &str) -> Option<String> {
    tokio::fs::read_to_string(path).await.ok()
}

async fn hostname() -> String {
    if let Some(name) = read("/etc/nodename").await.map(|s| s.trim().to_owned()) {
        if !name.is_empty() {
            return name;
        }
    }
    run("hostname", &[])
        .await
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty())
        .or_else(|| std::env::var("COMPUTERNAME").ok())
        .or_else(|| std::env::var("HOSTNAME").ok())
        .unwrap_or_else(|| "unknown".to_owned())
}

async fn uptime_seconds() -> u64 {
    let Some(out) = run("kstat", &["-p", "unix:0:system_misc:boot_time"]).await else {
        return 0;
    };
    parse_kstat(&out)
        .get("unix:0:system_misc:boot_time")
        .and_then(|v| v.parse::<i64>().ok())
        .and_then(Timestamp::from_unix)
        .map_or(0, |boot| {
            u64::try_from(Timestamp::now().seconds_since(boot)).unwrap_or(0)
        })
}

/// Collect host identity facts.
pub async fn facts() -> Facts {
    let hostname = hostname().await;
    let omnios_release = read("/etc/os-release")
        .await
        .and_then(|s| parse_os_release_version(&s))
        .unwrap_or_else(|| "unknown".to_owned());
    let boot_environment = run("beadm", &["list", "-H"])
        .await
        .and_then(|s| parse_beadm_active(&s))
        .unwrap_or_else(|| "unknown".to_owned());
    let uptime_seconds = uptime_seconds().await;
    let timezone = read("/etc/default/init")
        .await
        .and_then(|s| parse_tz(&s))
        .or_else(|| std::env::var("TZ").ok());
    Facts {
        hostname,
        omnios_release,
        boot_environment,
        uptime_seconds,
        timezone,
    }
}

/// Sample CPU, load, and memory.
pub async fn resources() -> SystemResources {
    let cpus =
        std::thread::available_parallelism().map_or(1, |n| u32::try_from(n.get()).unwrap_or(1));
    let mut load_avg = [0.0; 3];
    let mut memory = Memory {
        total_bytes: 0,
        free_bytes: 0,
    };

    let kstat = run(
        "kstat",
        &[
            "-p",
            "unix:0:system_misc:avenrun_1min",
            "unix:0:system_misc:avenrun_5min",
            "unix:0:system_misc:avenrun_15min",
            "unix:0:system_pages:physmem",
            "unix:0:system_pages:freemem",
        ],
    )
    .await;
    if let Some(out) = kstat {
        let stats = parse_kstat(&out);
        let page = run("pagesize", &[])
            .await
            .and_then(|s| s.trim().parse::<u64>().ok())
            .unwrap_or(4096);
        load_avg = load_from_kstat(&stats);
        memory = memory_from_kstat(&stats, page);
    } else if let (Some(loadavg), Some(meminfo)) =
        (read("/proc/loadavg").await, read("/proc/meminfo").await)
    {
        load_avg = parse_proc_loadavg(&loadavg).unwrap_or(load_avg);
        memory = parse_meminfo(&meminfo).unwrap_or(memory);
    }

    SystemResources {
        cpus,
        load_avg,
        memory,
        sampled_at: Some(Timestamp::now()),
    }
}

/// Parse `kstat -p` output into `module:instance:name:stat` to value.
pub fn parse_kstat(out: &str) -> HashMap<String, String> {
    out.lines()
        .filter_map(|line| {
            let (key, value) = line.split_once('\t')?;
            Some((key.trim().to_owned(), value.trim().to_owned()))
        })
        .collect()
}

/// Load averages from `avenrun_*` stats, which are fixed point scaled by 256.
pub fn load_from_kstat(stats: &HashMap<String, String>) -> [f64; 3] {
    let get = |name: &str| {
        stats
            .get(&format!("unix:0:system_misc:{name}"))
            .and_then(|v| v.parse::<f64>().ok())
            .map_or(0.0, |v| v / 256.0)
    };
    [
        get("avenrun_1min"),
        get("avenrun_5min"),
        get("avenrun_15min"),
    ]
}

/// Memory from `system_pages` stats and the page size.
pub fn memory_from_kstat(stats: &HashMap<String, String>, page_size: u64) -> Memory {
    let pages = |name: &str| {
        stats
            .get(&format!("unix:0:system_pages:{name}"))
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(0)
    };
    Memory {
        total_bytes: pages("physmem").saturating_mul(page_size),
        free_bytes: pages("freemem").saturating_mul(page_size),
    }
}

/// The active boot environment from `beadm list -H` (fields separated by
/// `;`: name, uuid, active flags, mountpoint, space, policy, created).
/// `N` means booted now, `R` means active on next reboot.
pub fn parse_beadm_active(out: &str) -> Option<String> {
    out.lines().find_map(|line| {
        let mut fields = line.split(';');
        let name = fields.next()?;
        let _uuid = fields.next()?;
        let flags = fields.next()?;
        flags.contains('N').then(|| name.to_owned())
    })
}

/// `VERSION=` from `/etc/os-release`.
pub fn parse_os_release_version(out: &str) -> Option<String> {
    out.lines().find_map(|line| {
        let value = line.strip_prefix("VERSION=")?;
        Some(value.trim().trim_matches('"').to_owned())
    })
}

/// `TZ=` from `/etc/default/init`.
pub fn parse_tz(out: &str) -> Option<String> {
    out.lines()
        .map(str::trim)
        .filter(|l| !l.starts_with('#'))
        .find_map(|line| {
            line.strip_prefix("TZ=")
                .map(|v| v.trim_matches('"').to_owned())
        })
}

/// The three load averages from `/proc/loadavg`.
pub fn parse_proc_loadavg(out: &str) -> Option<[f64; 3]> {
    let mut it = out.split_whitespace().map(|f| f.parse::<f64>().ok());
    Some([it.next()??, it.next()??, it.next()??])
}

/// Total and available memory from `/proc/meminfo`.
pub fn parse_meminfo(out: &str) -> Option<Memory> {
    let kib = |key: &str| {
        out.lines().find_map(|line| {
            let rest = line.strip_prefix(key)?.trim_start_matches(':').trim();
            rest.split_whitespace().next()?.parse::<u64>().ok()
        })
    };
    Some(Memory {
        total_bytes: kib("MemTotal")?.saturating_mul(1024),
        free_bytes: kib("MemAvailable")
            .or_else(|| kib("MemFree"))?
            .saturating_mul(1024),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[allow(clippy::float_cmp)]
    fn kstat_parses_and_scales() {
        let out = "unix:0:system_misc:avenrun_1min\t384\n\
                   unix:0:system_misc:avenrun_5min\t256\n\
                   unix:0:system_misc:avenrun_15min\t128\n\
                   unix:0:system_pages:physmem\t4000000\n\
                   unix:0:system_pages:freemem\t1000000\n";
        let stats = parse_kstat(out);
        assert_eq!(load_from_kstat(&stats), [1.5, 1.0, 0.5]);
        let mem = memory_from_kstat(&stats, 4096);
        assert_eq!(mem.total_bytes, 4_000_000 * 4096);
        assert_eq!(mem.free_bytes, 1_000_000 * 4096);
    }

    #[test]
    fn beadm_picks_the_booted_be() {
        let out = "omnios-r151054;3f9c;R;;1.2G;static;2026-08-01 10:00\n\
                   mandrake-0.1.0;7a1b;N;/;2.4G;static;2026-09-01 09:00\n";
        assert_eq!(parse_beadm_active(out).as_deref(), Some("mandrake-0.1.0"));
        assert_eq!(parse_beadm_active("only;one;-;;;;"), None);
    }

    #[test]
    fn os_release_and_tz() {
        assert_eq!(
            parse_os_release_version("NAME=\"OmniOS\"\nVERSION=r151054\nID=omnios\n").as_deref(),
            Some("r151054")
        );
        assert_eq!(
            parse_tz("# comment\nTZ=Europe/Amsterdam\nCMASK=022\n").as_deref(),
            Some("Europe/Amsterdam")
        );
    }

    #[test]
    fn proc_files() {
        assert_eq!(
            parse_proc_loadavg("0.52 0.58 0.59 1/1284 12345\n"),
            Some([0.52, 0.58, 0.59])
        );
        let mem = parse_meminfo(
            "MemTotal:       16000 kB\nMemFree:         1000 kB\nMemAvailable:    8000 kB\n",
        );
        assert_eq!(
            mem,
            Some(Memory {
                total_bytes: 16_000 * 1024,
                free_bytes: 8_000 * 1024
            })
        );
    }
}
