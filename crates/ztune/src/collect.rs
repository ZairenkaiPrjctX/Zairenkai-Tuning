// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Rve27 (Radika), Pavelc4 and the RvSystem-Monitor contributors
// Copyright (C) 2026 Zy0x (Noir) and the Lynx contributors
// Copyright 2026 ZairenkaiPrjctX contributors
//! Telemetry collection, tuning targets and capabilities.
//!
//! The collectors follow RvSystem-Monitor `rust/src/kernel/{cpu,thermal}.rs` and
//! `rust/src/mm/memory.rs` (commit b07fe27598555e49084bbcc40be558803286b9ef) and the target
//! discovery follows Lynx `native/lynxd/src/hardware/capability.rs` (`resolve_storage`,
//! `resolve_gpu`) and `detector.rs` (commit e56d6cd5005a543fa9798aa19b3627033593fbb0), both
//! GPL-3.0. Changes (2026-10-22): every read goes through [`SysRoot`]; each value carries a
//! status and unit and a failed read is never replaced by 0 or a built-in list; load needs
//! two explicit samples; GPU and storage targets must be unambiguous. See
//! `docs/PROVENANCE.md`.

use serde::Serialize;
use ztune_core::contract::{
    Authorization, Capability, CapabilityState, DataStatus, Error, ErrorCode, Reading, Unit,
};
use ztune_core::cpu::{self, StatSample};
use ztune_core::device::{self, CpuPolicy, DeviceIdentity};
use ztune_core::ops::OperationId;
use ztune_core::profile::Resolution;
use ztune_core::soc;
use ztune_core::{blockdev, gpu, memory, platform, thermal};

use crate::journal::Journal;
use crate::sysroot::{ReadError, SysRoot};

fn missing<T>(e: ReadError, unit: Unit) -> Reading<T> {
    Reading::missing(e.status, unit, e.detail)
}

/// First path that reads and parses, as upstream's fallback chains.
fn first_u64(root: &SysRoot, paths: &[String], unit: Unit) -> Reading<u64> {
    let mut last = None;
    for p in paths {
        match root.read_attr(p) {
            Ok(t) => match cpu::parse_u64(&t) {
                Some(v) => return Reading::ok(v, unit, p.as_str()),
                None => {
                    last = Some(ReadError {
                        status: DataStatus::ParseError,
                        detail: format!("{p}: {t:?} is not an integer"),
                    })
                }
            },
            Err(e) => {
                if last
                    .as_ref()
                    .is_none_or(|l: &ReadError| l.status == DataStatus::Unsupported)
                {
                    last = Some(e)
                }
            }
        }
    }
    missing(
        last.unwrap_or(ReadError {
            status: DataStatus::Unsupported,
            detail: "no path".into(),
        }),
        unit,
    )
}

fn first_text(root: &SysRoot, paths: &[String]) -> Reading<String> {
    let mut last = None;
    for p in paths {
        match root.read_attr(p) {
            Ok(t) if !t.is_empty() => return Reading::ok(t, Unit::Text, p.as_str()),
            Ok(_) => {
                last = Some(ReadError {
                    status: DataStatus::Unavailable,
                    detail: format!("{p}: empty"),
                })
            }
            Err(e) => last = Some(e),
        }
    }
    missing(
        last.unwrap_or(ReadError {
            status: DataStatus::Unsupported,
            detail: "no path".into(),
        }),
        Unit::Text,
    )
}

#[derive(Debug, Clone, Serialize)]
pub struct Core {
    pub id: u32,
    pub cur_freq: Reading<u64>,
    pub min_freq: Reading<u64>,
    pub max_freq: Reading<u64>,
    pub governor: Reading<String>,
    pub load: Reading<f64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CpuTelemetry {
    pub present: Reading<Vec<u32>>,
    pub load: Reading<f64>,
    pub cores: Vec<Core>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ThermalTelemetry {
    pub cpu: Reading<f64>,
    pub gpu: Reading<f64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct MemoryTelemetry {
    pub total: Reading<u64>,
    pub available: Reading<u64>,
    pub used: Reading<u64>,
    pub swap_total: Reading<u64>,
    pub swap_used: Reading<u64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct GpuTelemetry {
    pub devfreq: Option<String>,
    pub cur_freq: Reading<u64>,
    pub governor: Reading<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct StorageDevice {
    pub device: String,
    pub scheduler: Reading<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Telemetry {
    pub interval_ms: u64,
    pub soc: soc::Identity,
    pub platform: platform::Detection,
    pub cpu: CpuTelemetry,
    pub thermal: ThermalTelemetry,
    pub memory: MemoryTelemetry,
    pub gpu: GpuTelemetry,
    pub storage: Vec<StorageDevice>,
}

pub fn detect_platform(root: &SysRoot) -> platform::Detection {
    let compatible = platform::DT_COMPATIBLE
        .iter()
        .find_map(|p| root.read_bounded(p, 4096).ok());
    let present: Vec<&str> = platform::NODE_SIGNALS
        .iter()
        .map(|(p, _, _)| *p)
        .filter(|p| root.exists(p))
        .collect();
    platform::detect(compatible.as_deref(), &present)
}

/// The exact-device identity tuple (`ztune_core::device`). Nothing here runs a command.
pub fn device_identity(root: &SysRoot) -> DeviceIdentity {
    let props: Vec<Vec<(String, String)>> = device::BUILD_PROPS
        .iter()
        .filter_map(|p| root.read_proc(p).ok())
        .map(|t| device::parse_props(&t))
        .collect();
    let dt = |p: &str| root.read_bounded(p, 4096).ok();
    let attr = |p: &str| root.read_attr(p).ok().filter(|v| !v.is_empty());
    let observed = soc::Observed {
        dt_compatible: dt("/sys/firmware/devicetree/base/compatible")
            .or_else(|| dt("/proc/device-tree/compatible"))
            .map(|b| soc::split_dt_strings(&b))
            .unwrap_or_default(),
        dt_model: dt("/sys/firmware/devicetree/base/model")
            .or_else(|| dt("/proc/device-tree/model"))
            .and_then(|b| soc::split_dt_strings(&b).into_iter().next()),
        soc0_family: attr("/sys/devices/soc0/family"),
        soc0_machine: attr("/sys/devices/soc0/machine"),
        soc0_soc_id: attr("/sys/devices/soc0/soc_id"),
        ..Default::default()
    };
    let policies = root
        .list("/sys/devices/system/cpu/cpufreq")
        .into_iter()
        .filter_map(|n| {
            let policy: u32 = n.strip_prefix("policy")?.parse().ok()?;
            let dir = format!("/sys/devices/system/cpu/cpufreq/{n}");
            let cpus = attr(&format!("{dir}/related_cpus"))
                .map(|v| {
                    v.split_whitespace()
                        .filter_map(|c| c.parse().ok())
                        .collect()
                })
                .unwrap_or_default();
            Some(CpuPolicy {
                policy,
                cpus,
                max_freq_khz: attr(&format!("{dir}/cpuinfo_max_freq")).and_then(|v| v.parse().ok()),
            })
        })
        .collect();
    DeviceIdentity::from_parts(
        &props,
        attr("/proc/sys/kernel/osrelease"),
        policies,
        observed,
    )
}

pub fn read_stat(root: &SysRoot) -> Result<StatSample, ReadError> {
    root.read_proc("/proc/stat").map(|t| cpu::parse_stat(&t))
}

fn load_reading(
    prev: &Option<StatSample>,
    curr: &Result<StatSample, ReadError>,
    core: Option<u32>,
) -> Reading<f64> {
    match (prev, curr) {
        (_, Err(e)) => missing(e.clone(), Unit::Percent),
        (None, Ok(_)) => Reading::missing(
            DataStatus::NotSampled,
            Unit::Percent,
            "load needs two samples (--interval-ms > 0)",
        ),
        (Some(p), Ok(c)) => match cpu::core_load(p, c, core) {
            Some(v) => Reading::ok(v, Unit::Percent, "/proc/stat (two samples)"),
            None => Reading::missing(
                DataStatus::Unavailable,
                Unit::Percent,
                "no jiffies elapsed or core absent from /proc/stat",
            ),
        },
    }
}

fn thermal_zones(root: &SysRoot) -> Vec<thermal::Zone> {
    root.list("/sys/class/thermal")
        .into_iter()
        .filter(|n| n.starts_with("thermal_zone"))
        .filter_map(|n| {
            let dir = format!("/sys/class/thermal/{n}");
            let kind = root.read_attr(&format!("{dir}/type")).ok()?.to_lowercase();
            let temp = root
                .read_attr(&format!("{dir}/temp"))
                .ok()
                .and_then(|t| thermal::normalize(&t));
            Some(thermal::Zone { dir, kind, temp })
        })
        .collect()
}

fn zone_reading(zones: &[thermal::Zone], priority: &[&str]) -> Reading<f64> {
    if zones.is_empty() {
        return Reading::missing(
            DataStatus::Unsupported,
            Unit::Celsius,
            "no readable /sys/class/thermal zone",
        );
    }
    match thermal::select(zones, priority) {
        Some(i) => {
            let z = &zones[i];
            let c = z.temp.expect("selected zones have a temperature");
            let mut r = Reading::ok(
                c.value,
                Unit::Celsius,
                format!("{}/temp ({})", z.dir, z.kind),
            );
            r.detail = Some(format!("raw value divided by {}", c.divisor));
            r
        }
        None => Reading::missing(
            DataStatus::Unsupported,
            Unit::Celsius,
            "no zone matches the priority list with a plausible value",
        ),
    }
}

fn bytes(v: Option<u64>, text: &Result<String, ReadError>, key: &str) -> Reading<u64> {
    match (v, text) {
        (_, Err(e)) => missing(e.clone(), Unit::Bytes),
        (Some(v), Ok(_)) => Reading::ok(v, Unit::Bytes, format!("/proc/meminfo {key}")),
        (None, Ok(_)) => Reading::missing(
            DataStatus::Unsupported,
            Unit::Bytes,
            format!("/proc/meminfo has no {key}"),
        ),
    }
}

/// Collects telemetry. With `interval_ms > 0`, `/proc/stat` is sampled twice.
pub fn telemetry(root: &SysRoot, interval_ms: u64) -> Telemetry {
    let first = if interval_ms > 0 {
        let s = read_stat(root).ok();
        std::thread::sleep(std::time::Duration::from_millis(interval_ms));
        s
    } else {
        None
    };
    let second = read_stat(root);

    let present = match root.read_attr("/sys/devices/system/cpu/present") {
        Ok(t) => match cpu::parse_present(&t) {
            Some(v) => Reading::ok(v, Unit::Count, "/sys/devices/system/cpu/present"),
            None => Reading::missing(DataStatus::ParseError, Unit::Count, format!("{t:?}")),
        },
        Err(e) => missing(e, Unit::Count),
    };
    let cores = present
        .value
        .clone()
        .unwrap_or_default()
        .into_iter()
        .map(|id| Core {
            id,
            cur_freq: first_u64(root, &cpu::freq_paths(id, cpu::CUR_FREQ), Unit::KiloHertz),
            min_freq: first_u64(root, &cpu::freq_paths(id, cpu::MIN_FREQ), Unit::KiloHertz),
            max_freq: first_u64(root, &cpu::freq_paths(id, cpu::MAX_FREQ), Unit::KiloHertz),
            governor: first_text(root, &cpu::freq_paths(id, cpu::GOVERNOR)),
            load: load_reading(&first, &second, Some(id)),
        })
        .collect();

    let zones = thermal_zones(root);
    let mem_text = root.read_proc("/proc/meminfo");
    let m = mem_text
        .as_ref()
        .map(|t| memory::parse_meminfo(t))
        .unwrap_or_default();

    let gpu = match gpu_node(root) {
        Ok(Some(dir)) => GpuTelemetry {
            cur_freq: first_u64(root, &[format!("{dir}/cur_freq")], Unit::Hertz),
            governor: first_text(root, &[format!("{dir}/governor")]),
            devfreq: Some(dir),
        },
        Ok(None) => GpuTelemetry {
            devfreq: None,
            cur_freq: Reading::missing(DataStatus::Unsupported, Unit::Hertz, "no GPU devfreq node"),
            governor: Reading::missing(DataStatus::Unsupported, Unit::Text, "no GPU devfreq node"),
        },
        Err(e) => GpuTelemetry {
            devfreq: None,
            cur_freq: Reading::missing(DataStatus::Unsupported, Unit::Hertz, e.message.clone()),
            governor: Reading::missing(DataStatus::Unsupported, Unit::Text, e.message),
        },
    };

    let storage = block_devices(root)
        .into_iter()
        .map(|(device, sched)| StorageDevice {
            scheduler: match sched {
                Ok(s) => Reading::ok(
                    s.current,
                    Unit::Text,
                    format!("/sys/block/{device}/queue/scheduler"),
                ),
                Err(e) => missing(e, Unit::Text),
            },
            device,
        })
        .collect();

    Telemetry {
        interval_ms,
        soc: device_identity(root).soc,
        platform: detect_platform(root),
        cpu: CpuTelemetry {
            load: load_reading(&first, &second, None),
            present,
            cores,
        },
        thermal: ThermalTelemetry {
            cpu: zone_reading(&zones, thermal::CPU_ZONES),
            gpu: zone_reading(&zones, thermal::GPU_ZONES),
        },
        memory: MemoryTelemetry {
            total: bytes(m.total, &mem_text, "MemTotal"),
            available: bytes(m.available, &mem_text, "MemAvailable"),
            used: bytes(m.used(), &mem_text, "MemTotal - MemAvailable"),
            swap_total: bytes(m.swap_total, &mem_text, "SwapTotal"),
            swap_used: bytes(m.swap_used(), &mem_text, "SwapTotal - SwapFree"),
        },
        gpu,
        storage,
    }
}

/// The GPU's devfreq directory: KGSL first, else the single GPU-named devfreq entry.
pub fn gpu_node(root: &SysRoot) -> Result<Option<String>, Error> {
    if root.exists(&format!("{}/governor", gpu::KGSL_DEVFREQ)) {
        return Ok(Some(gpu::KGSL_DEVFREQ.to_string()));
    }
    let names = root.list(gpu::DEVFREQ_CLASS);
    match gpu::pick_devfreq(&names) {
        Ok(Some(n)) => Ok(Some(format!("{}/{n}", gpu::DEVFREQ_CLASS))),
        Ok(None) => Ok(None),
        Err(hits) => Err(Error::new(
            ErrorCode::Unsupported,
            format!(
                "several GPU-like devfreq nodes ({}); refusing to guess",
                hits.iter()
                    .map(|s| s.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        )),
    }
}

/// Internal storage devices and their scheduler: non-virtual, non-removable.
pub fn block_devices(root: &SysRoot) -> Vec<(String, Result<blockdev::Scheduler, ReadError>)> {
    root.list("/sys/block")
        .into_iter()
        .filter(|n| blockdev::candidate_name(n))
        .filter(|n| {
            !root
                .link_target(&format!("/sys/block/{n}"))
                .is_some_and(|t| t.contains("/virtual/"))
        })
        .filter(|n| {
            root.read_attr(&format!("/sys/block/{n}/removable"))
                .as_deref()
                != Ok("1")
        })
        .filter(|n| root.exists(&format!("/sys/block/{n}/queue/scheduler")))
        .map(|n| {
            let p = format!("/sys/block/{n}/queue/scheduler");
            let s = root.read_attr(&p).and_then(|t| {
                blockdev::parse_scheduler(&t).ok_or(ReadError {
                    status: DataStatus::ParseError,
                    detail: format!("{p}: no selectable scheduler in {t:?}"),
                })
            });
            (n, s)
        })
        .collect()
}

/// A node an operation would write, with its current and advertised values.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Target {
    pub name: String,
    pub path: String,
    pub current: String,
    pub available: Vec<String>,
}

/// Resolves every target of `op`. An unreadable target fails the whole resolution, so an
/// operation never applies to part of the device silently.
pub fn targets(root: &SysRoot, op: OperationId) -> Result<Vec<Target>, Error> {
    match op {
        OperationId::IoScheduler => {
            let devs = block_devices(root);
            if devs.is_empty() {
                return Err(Error::new(
                    ErrorCode::Unsupported,
                    "no internal block device with a selectable I/O scheduler",
                ));
            }
            devs.into_iter()
                .map(|(name, s)| match s {
                    Ok(s) => Ok(Target {
                        path: format!("/sys/block/{name}/queue/scheduler"),
                        name,
                        current: s.current,
                        available: s.available,
                    }),
                    Err(e) => Err(read_failure(e)),
                })
                .collect()
        }
        OperationId::GpuGovernor => {
            let dir = gpu_node(root)?.ok_or_else(|| {
                Error::new(ErrorCode::Unsupported, "no GPU devfreq node on this device")
            })?;
            let path = format!("{dir}/governor");
            let current = root.read_attr(&path).map_err(read_failure)?;
            let available = gpu::parse_governors(
                &root
                    .read_attr(&format!("{dir}/available_governors"))
                    .map_err(read_failure)?,
            );
            if available.is_empty() {
                return Err(Error::new(
                    ErrorCode::Unsupported,
                    format!("{dir}/available_governors lists nothing"),
                ));
            }
            Ok(vec![Target {
                name: "gpu".into(),
                path,
                current,
                available,
            }])
        }
    }
}

fn read_failure(e: ReadError) -> Error {
    let code = match e.status {
        DataStatus::PermissionDenied => ErrorCode::PermissionDenied,
        DataStatus::Unsupported => ErrorCode::Unsupported,
        _ => ErrorCode::Io,
    };
    Error::new(code, e.detail)
}

/// Values every target advertises (an operation must fit all of them).
pub fn common_values(targets: &[Target]) -> Vec<String> {
    let Some(first) = targets.first() else {
        return Vec::new();
    };
    first
        .available
        .iter()
        .filter(|v| targets.iter().all(|t| t.available.contains(v)))
        .cloned()
        .collect()
}

pub fn capabilities(
    root: &SysRoot,
    journal: Option<&Journal>,
    tele: &Telemetry,
    resolution: &Resolution,
) -> Vec<Capability> {
    let mut caps = Vec::new();
    let read_cap = |id: &str, ok: bool, evidence: Vec<String>| Capability {
        id: id.into(),
        state: if ok {
            CapabilityState::Available
        } else {
            CapabilityState::Unsupported
        },
        operation: None,
        allowed_values: vec![],
        current: None,
        evidence,
        authorization: None,
    };
    let cpu_ok = tele
        .cpu
        .cores
        .iter()
        .any(|c| c.cur_freq.status == DataStatus::Ok);
    caps.push(read_cap(
        "telemetry.cpu",
        cpu_ok,
        tele.cpu
            .cores
            .iter()
            .filter_map(|c| c.cur_freq.source.clone())
            .take(1)
            .collect(),
    ));
    for (id, r) in [
        ("telemetry.thermal.cpu", &tele.thermal.cpu),
        ("telemetry.thermal.gpu", &tele.thermal.gpu),
    ] {
        caps.push(read_cap(
            id,
            r.status == DataStatus::Ok,
            r.source.clone().or(r.detail.clone()).into_iter().collect(),
        ));
    }
    caps.push(read_cap(
        "telemetry.memory",
        tele.memory.total.status == DataStatus::Ok,
        vec!["/proc/meminfo".into()],
    ));
    caps.push(read_cap(
        "telemetry.gpu",
        tele.gpu.cur_freq.status == DataStatus::Ok,
        tele.gpu.devfreq.clone().into_iter().collect(),
    ));
    for op in OperationId::ALL {
        let mut c = Capability {
            id: op.capability().into(),
            state: CapabilityState::Unsupported,
            operation: Some(op.name().into()),
            allowed_values: vec![],
            current: None,
            evidence: vec![],
            authorization: None,
        };
        match targets(root, op) {
            Ok(ts) => {
                c.allowed_values = common_values(&ts)
                    .into_iter()
                    .filter(|v| ztune_core::ops::validate(op, v, &ts[0].available).is_ok())
                    .collect();
                let currents: Vec<&str> = ts.iter().map(|t| t.current.as_str()).collect();
                c.current = if currents.windows(2).all(|w| w[0] == w[1]) {
                    currents.first().map(|s| s.to_string())
                } else {
                    Some("mixed".into())
                };
                c.evidence = ts.iter().map(|t| t.path.clone()).collect();
                let granted: Vec<String> = resolution
                    .grant(op)
                    .map(|g| {
                        c.allowed_values
                            .iter()
                            .filter(|v| g.contains(v))
                            .cloned()
                            .collect()
                    })
                    .unwrap_or_default();
                c.authorization = Some(Authorization {
                    write_authorized: !granted.is_empty(),
                    tier: resolution.tier,
                    profile_id: resolution.profile_id.clone(),
                    reason: if granted.is_empty() {
                        format!(
                            "no device_validated profile grants {} on this exact device/build: {}",
                            op.name(),
                            resolution.explanation.join("; ")
                        )
                    } else {
                        format!("granted by profile {:?}", resolution.profile_id)
                    },
                    granted_values: granted,
                });
                c.state = if c.allowed_values.is_empty() {
                    CapabilityState::Unsupported
                } else if journal.is_some_and(|j| j.verified_for(op)) {
                    CapabilityState::Verified
                } else {
                    CapabilityState::NotVerified
                };
            }
            Err(e) => {
                c.state = match e.code {
                    ErrorCode::PermissionDenied | ErrorCode::Io => CapabilityState::Unavailable,
                    _ => CapabilityState::Unsupported,
                };
                c.evidence = vec![e.message];
            }
        }
        caps.push(c);
    }
    caps
}
