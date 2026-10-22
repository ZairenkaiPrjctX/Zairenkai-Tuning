// SPDX-License-Identifier: GPL-3.0-only
// Copyright 2026 ZairenkaiPrjctX contributors
//! Command-line parsing and dispatch. Every command prints one contract document on stdout.

use std::path::{Path, PathBuf};

use serde_json::{json, Value};
use ztune_core::contract::{
    self, body, document, error_document, val, Error, ErrorCode, OperationStatus, RequestStatus,
    SCHEMA, SCHEMA_VERSION,
};
use ztune_core::ops::OperationId;

use crate::journal::{self, Journal};
use crate::sysroot::SysRoot;
use crate::{apply, collect, gputable, profiles};
use ztune_core::device::DeviceIdentity;
use ztune_core::profile::{self, Resolution};

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
/// Set by `scripts/build-android.sh` so the binary names the source it was built from.
pub const COMMIT: Option<&str> = option_env!("ZTUNE_COMMIT");
pub const DEFAULT_STATE_DIR: &str = "/data/adb/zairenkai/tuning";

pub const EXIT_OK: u8 = 0;
pub const EXIT_FAILED: u8 = 1;
pub const EXIT_USAGE: u8 = 2;
pub const EXIT_REFUSED: u8 = 4;
pub const EXIT_UNSUPPORTED: u8 = 5;

pub const USAGE: &str = "\
usage: ztune [--sysroot DIR] [--state-dir DIR] [--profiles-dir DIR] <command>

commands:
  identity                         component, version, licence, contract, upstreams
  device                           exact-device identity, SoC family, support tier, profiles
  capabilities                     what this device supports (read and write)
  telemetry [--interval-ms N]      CPU, thermal, memory, GPU, storage (default 200 ms)
  apply <operation> <value> [--dry-run]
                                   operations: io.scheduler, gpu.governor
  restore                          restore every node ztune changed this boot
  gpu-table inspect --dts FILE
  gpu-table plan --dts FILE --bin N --max-freq-hz HZ
  gpu-table stage --dts FILE --bin N --max-freq-hz HZ --confirm TOKEN --out DIR
  gpu-table flash                  not implemented (never flashes)

exit codes: 0 ok, 1 failed, 2 usage, 4 refused, 5 unsupported";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    Identity,
    Device,
    Capabilities,
    Telemetry {
        interval_ms: u64,
    },
    Apply {
        op: OperationId,
        value: String,
        dry_run: bool,
    },
    Restore,
    GpuInspect {
        dts: PathBuf,
    },
    GpuPlan {
        dts: PathBuf,
        bin: u32,
        max_hz: u64,
    },
    GpuStage {
        dts: PathBuf,
        bin: u32,
        max_hz: u64,
        confirm: Option<String>,
        out: PathBuf,
    },
    GpuFlash,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Args {
    pub sysroot: PathBuf,
    pub state_dir: PathBuf,
    /// Extra profiles (tests, maintainers). Core never passes it.
    pub profiles_dir: Option<PathBuf>,
    pub command: Command,
}

fn take(it: &mut std::slice::Iter<'_, String>, flag: &str) -> Result<String, String> {
    it.next()
        .cloned()
        .ok_or_else(|| format!("{flag} needs a value"))
}

fn num<T: std::str::FromStr>(s: &str, flag: &str) -> Result<T, String> {
    s.parse()
        .map_err(|_| format!("{flag}: {s:?} is not a number"))
}

pub fn parse(argv: &[String]) -> Result<Args, String> {
    let mut sysroot = PathBuf::from("/");
    let mut state_dir = PathBuf::from(DEFAULT_STATE_DIR);
    let mut profiles_dir = None;
    let mut rest = Vec::new();
    let mut it = argv.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--sysroot" => sysroot = take(&mut it, a)?.into(),
            "--state-dir" => state_dir = take(&mut it, a)?.into(),
            "--profiles-dir" => profiles_dir = Some(take(&mut it, a)?.into()),
            _ => rest.push(a.clone()),
        }
    }
    let mut it = rest.iter();
    let cmd = it.next().ok_or("no command")?;
    let mut flags: Vec<(String, Option<String>)> = Vec::new();
    let mut positional = Vec::new();
    let sub = if cmd == "gpu-table" {
        Some(it.next().ok_or("gpu-table needs a subcommand")?.clone())
    } else {
        None
    };
    while let Some(a) = it.next() {
        match a.as_str() {
            "--dry-run" => flags.push((a.clone(), None)),
            f if f.starts_with("--") => flags.push((a.clone(), Some(take(&mut it, f)?))),
            _ => positional.push(a.clone()),
        }
    }
    let flag = |name: &str| -> Option<String> {
        flags
            .iter()
            .find(|(f, _)| f == name)
            .and_then(|(_, v)| v.clone())
    };
    let need = |name: &str| flag(name).ok_or_else(|| format!("{name} is required"));
    let known = |allowed: &[&str]| -> Result<(), String> {
        match flags.iter().find(|(f, _)| !allowed.contains(&f.as_str())) {
            Some((f, _)) => Err(format!("unknown option {f}")),
            None => Ok(()),
        }
    };
    let no_positional = |p: &Vec<String>| match p.first() {
        Some(x) => Err(format!("unexpected argument {x:?}")),
        None => Ok(()),
    };
    let command = match (cmd.as_str(), sub.as_deref()) {
        ("identity", _) => {
            known(&[])?;
            no_positional(&positional)?;
            Command::Identity
        }
        ("device", _) => {
            known(&[])?;
            no_positional(&positional)?;
            Command::Device
        }
        ("capabilities", _) => {
            known(&[])?;
            no_positional(&positional)?;
            Command::Capabilities
        }
        ("telemetry", _) => {
            known(&["--interval-ms"])?;
            no_positional(&positional)?;
            let interval_ms = match flag("--interval-ms") {
                Some(v) => num(&v, "--interval-ms")?,
                None => 200,
            };
            if interval_ms > 5_000 {
                return Err("--interval-ms must be at most 5000".into());
            }
            Command::Telemetry { interval_ms }
        }
        ("apply", _) => {
            known(&["--dry-run"])?;
            let [op, value] = positional.as_slice() else {
                return Err("apply needs <operation> <value>".into());
            };
            Command::Apply {
                op: OperationId::parse(op).ok_or_else(|| format!("unknown operation {op:?}"))?,
                value: value.clone(),
                dry_run: flags.iter().any(|(f, _)| f == "--dry-run"),
            }
        }
        ("restore", _) => {
            known(&[])?;
            no_positional(&positional)?;
            Command::Restore
        }
        ("gpu-table", Some("inspect")) => {
            known(&["--dts"])?;
            no_positional(&positional)?;
            Command::GpuInspect {
                dts: need("--dts")?.into(),
            }
        }
        ("gpu-table", Some("plan")) => {
            known(&["--dts", "--bin", "--max-freq-hz"])?;
            no_positional(&positional)?;
            Command::GpuPlan {
                dts: need("--dts")?.into(),
                bin: num(&need("--bin")?, "--bin")?,
                max_hz: num(&need("--max-freq-hz")?, "--max-freq-hz")?,
            }
        }
        ("gpu-table", Some("stage")) => {
            known(&["--dts", "--bin", "--max-freq-hz", "--confirm", "--out"])?;
            no_positional(&positional)?;
            Command::GpuStage {
                dts: need("--dts")?.into(),
                bin: num(&need("--bin")?, "--bin")?,
                max_hz: num(&need("--max-freq-hz")?, "--max-freq-hz")?,
                confirm: flag("--confirm"),
                out: need("--out")?.into(),
            }
        }
        ("gpu-table", Some("flash")) => Command::GpuFlash,
        ("gpu-table", Some(s)) => return Err(format!("unknown gpu-table subcommand {s:?}")),
        (c, _) => return Err(format!("unknown command {c:?}")),
    };
    Ok(Args {
        sysroot,
        state_dir,
        profiles_dir,
        command,
    })
}

fn code_exit(code: ErrorCode) -> u8 {
    match code {
        ErrorCode::Usage => EXIT_USAGE,
        ErrorCode::Unsupported | ErrorCode::NotImplemented => EXIT_UNSUPPORTED,
        ErrorCode::InvalidValue
        | ErrorCode::NotAuthorized
        | ErrorCode::Refused
        | ErrorCode::DeviceMismatch
        | ErrorCode::ConfirmationRequired => EXIT_REFUSED,
        _ => EXIT_FAILED,
    }
}

fn err(e: &Error) -> (u8, Value) {
    (code_exit(e.code), error_document(VERSION, e))
}

pub fn identity() -> Value {
    document(
        "identity",
        VERSION,
        body(vec![
            ("commit", val(&COMMIT)),
            ("license", val(&contract::LICENSE)),
            ("source", val(&contract::SOURCE)),
            (
                "contract",
                json!({ "schema": SCHEMA, "schema_version": SCHEMA_VERSION }),
            ),
            (
                "operations",
                val(&OperationId::ALL
                    .iter()
                    .map(|o| o.name())
                    .collect::<Vec<_>>()),
            ),
            (
                "upstreams",
                json!([
                    {"name": "RvSystem-Monitor", "url": "https://github.com/Rve27/RvSystem-Monitor",
                     "commit": "b07fe27598555e49084bbcc40be558803286b9ef", "license": "GPL-3.0"},
                    {"name": "Lynx", "url": "https://github.com/Zy0x/Lynx",
                     "commit": "e56d6cd5005a543fa9798aa19b3627033593fbb0", "license": "GPL-3.0"},
                    {"name": "KonaBess", "url": "https://github.com/libxzr/KonaBess",
                     "commit": "ff387dc7f630d7460c7a6561f83f71846217ee27", "license": "GPL-3.0"}
                ]),
            ),
        ]),
    )
}

/// Device identity, loaded profiles and the resolved tier.
fn resolve(a: &Args, root: &SysRoot) -> (DeviceIdentity, profiles::Loaded, Resolution) {
    let device = collect::device_identity(root);
    // Synthetic (test-only) profiles count only against a fixture root.
    let loaded = profiles::load(a.profiles_dir.as_deref(), a.sysroot != Path::new("/"));
    let generic = root.exists("/sys/devices/system/cpu/present") || root.exists("/sys/block");
    let resolution = profile::resolve(&loaded.profiles, &device, generic);
    (device, loaded, resolution)
}

/// Runs a parsed command: (exit code, document).
pub fn run(a: &Args) -> (u8, Value) {
    let root = SysRoot::new(&a.sysroot);
    match &a.command {
        Command::Identity => (EXIT_OK, identity()),
        Command::Device => {
            let (device, loaded, resolution) = resolve(a, &root);
            (
                EXIT_OK,
                document(
                    "device",
                    VERSION,
                    body(vec![
                        ("device", val(&device)),
                        ("resolution", val(&resolution)),
                        (
                            "profiles",
                            serde_json::json!({
                                "loaded": loaded.profiles.iter().map(|p| &p.profile_id).collect::<Vec<_>>(),
                                "rejected": loaded.rejected,
                            }),
                        ),
                    ]),
                ),
            )
        }
        Command::Capabilities => {
            let tele = collect::telemetry(&root, 0);
            let jr = journal::boot_id(&root)
                .ok()
                .map(|b| Journal::load(&a.state_dir, &b));
            let (_, _, resolution) = resolve(a, &root);
            let caps = collect::capabilities(&root, jr.as_ref(), &tele, &resolution);
            (
                EXIT_OK,
                document(
                    "capabilities",
                    VERSION,
                    body(vec![("capabilities", val(&caps))]),
                ),
            )
        }
        Command::Telemetry { interval_ms } => {
            let t = collect::telemetry(&root, *interval_ms);
            let v = serde_json::to_value(&t).expect("telemetry serializes");
            let Value::Object(m) = v else { unreachable!() };
            (EXIT_OK, document("telemetry", VERSION, m))
        }
        Command::Apply { op, value, dry_run } => {
            let (_, _, resolution) = resolve(a, &root);
            let o = apply::apply(&root, &a.state_dir, &resolution, *op, value, *dry_run);
            let exit = match (o.request.status, o.operation.status, &o.error) {
                (RequestStatus::Rejected, _, Some(e)) => code_exit(e.code),
                (_, OperationStatus::AppliedVerified, _)
                | (_, OperationStatus::NoChange, _)
                | (_, OperationStatus::NotStarted, None) => EXIT_OK,
                _ => EXIT_FAILED,
            };
            let v = serde_json::to_value(&o).expect("outcome serializes");
            let Value::Object(m) = v else { unreachable!() };
            (exit, document("operation", VERSION, m))
        }
        Command::Restore => {
            let o = apply::restore(&root, &a.state_dir);
            let exit = match (o.status, &o.error) {
                (OperationStatus::Failed, _) => EXIT_FAILED,
                (_, Some(e)) => code_exit(e.code),
                _ => EXIT_OK,
            };
            let v = serde_json::to_value(&o).expect("restore serializes");
            let Value::Object(m) = v else { unreachable!() };
            (exit, document("restore", VERSION, m))
        }
        Command::GpuInspect { dts } => match gputable::inspect(dts) {
            Ok(i) => (
                EXIT_OK,
                document("gpu_table", VERSION, body(vec![("inspection", val(&i))])),
            ),
            Err(e) => err(&e),
        },
        Command::GpuPlan { dts, bin, max_hz } => match gputable::plan(&root, dts, *bin, *max_hz) {
            Ok(p) => (
                EXIT_OK,
                document("gpu_table", VERSION, body(vec![("plan", val(&p))])),
            ),
            Err(e) => err(&e),
        },
        Command::GpuStage {
            dts,
            bin,
            max_hz,
            confirm,
            out,
        } => match gputable::stage(&root, dts, *bin, *max_hz, confirm.as_deref(), out) {
            Ok(s) => (
                EXIT_OK,
                document("gpu_table", VERSION, body(vec![("staged", val(&s))])),
            ),
            Err(e) => err(&e),
        },
        Command::GpuFlash => err(&Error::new(
            ErrorCode::NotImplemented,
            gputable::FLASH_STATUS,
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(s: &str) -> Result<Args, String> {
        parse(&s.split_whitespace().map(String::from).collect::<Vec<_>>())
    }

    #[test]
    fn parses_commands() {
        assert_eq!(p("identity").unwrap().command, Command::Identity);
        let a = p("--sysroot /tmp/x telemetry --interval-ms 0").unwrap();
        assert_eq!(a.sysroot, PathBuf::from("/tmp/x"));
        assert_eq!(a.command, Command::Telemetry { interval_ms: 0 });
        assert_eq!(
            p("apply io.scheduler kyber --dry-run").unwrap().command,
            Command::Apply {
                op: OperationId::IoScheduler,
                value: "kyber".into(),
                dry_run: true
            }
        );
        assert!(p("apply cpu.max 1").is_err());
        assert!(p("apply io.scheduler").is_err());
        assert!(p("telemetry --interval-ms 99999").is_err());
        assert!(p("identity --bogus 1").is_err());
        assert!(p("gpu-table plan --dts f --bin 0").is_err());
        assert!(matches!(
            p("gpu-table stage --dts f --bin 0 --max-freq-hz 1 --out o")
                .unwrap()
                .command,
            Command::GpuStage { confirm: None, .. }
        ));
        assert!(p("").is_err());
    }

    #[test]
    fn flash_is_not_implemented() {
        let (code, doc) = run(&p("gpu-table flash").unwrap());
        assert_eq!(code, EXIT_UNSUPPORTED);
        assert_eq!(doc["error"]["code"], "not_implemented");
    }
}
