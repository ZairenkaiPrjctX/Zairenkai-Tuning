// SPDX-License-Identifier: GPL-3.0-only
// Copyright 2026 ZairenkaiPrjctX contributors
//! End-to-end tests of the `ztune` binary against a fixture root: the contract documents
//! other components consume.

mod common;

use std::process::Command;

use common::{device, get, kona_dts, put};
use serde_json::Value;

fn profiles_dir() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/profiles")
}

/// Runs ztune on the fixture with the synthetic device_validated test profile.
fn ztune(f: &common::Fixture, args: &[&str]) -> (i32, Value) {
    let dir = profiles_dir();
    let mut all = vec!["--profiles-dir", dir.to_str().unwrap()];
    all.extend_from_slice(args);
    ztune_bare(f, &all)
}

/// Runs ztune on the fixture with only the embedded profiles (none today).
fn ztune_bare(f: &common::Fixture, args: &[&str]) -> (i32, Value) {
    let out = Command::new(env!("CARGO_BIN_EXE_ztune"))
        .arg("--sysroot")
        .arg(&f.root)
        .arg("--state-dir")
        .arg(&f.state)
        .args(args)
        .output()
        .unwrap();
    let doc: Value = serde_json::from_slice(&out.stdout)
        .unwrap_or_else(|e| panic!("{e}: {}", String::from_utf8_lossy(&out.stdout)));
    assert_eq!(doc["schema"], "zairenkai.tuning");
    assert_eq!(doc["schema_version"], 1);
    assert_eq!(doc["component"]["name"], "zairenkai-tuning");
    (out.status.code().unwrap(), doc)
}

#[test]
fn identity_names_licence_and_upstreams() {
    let f = device("identity");
    let (code, d) = ztune(&f, &["identity"]);
    assert_eq!(code, 0);
    assert_eq!(d["kind"], "identity");
    assert_eq!(d["license"], "GPL-3.0-only");
    assert_eq!(d["upstreams"].as_array().unwrap().len(), 3);
    assert_eq!(
        d["operations"],
        serde_json::json!(["io.scheduler", "gpu.governor"])
    );
}

#[test]
fn capabilities_report_advertised_values_and_states() {
    let f = device("caps");
    let (code, d) = ztune(&f, &["capabilities"]);
    assert_eq!(code, 0);
    let caps = d["capabilities"].as_array().unwrap();
    let cap = |id: &str| caps.iter().find(|c| c["id"] == id).unwrap().clone();
    assert_eq!(cap("telemetry.cpu")["state"], "available");
    assert_eq!(cap("telemetry.thermal.gpu")["state"], "available");
    let io = cap("io.scheduler");
    assert_eq!(io["state"], "not_verified");
    assert_eq!(
        io["allowed_values"],
        serde_json::json!(["mq-deadline", "kyber", "none"])
    );
    assert_eq!(io["current"], "mq-deadline");
    let gpu = cap("gpu.governor");
    // userspace is advertised by the node but refused by Zairenkai.
    assert_eq!(
        gpu["allowed_values"],
        serde_json::json!(["msm-adreno-tz", "performance", "powersave"])
    );
    // After a verified write on this boot, the capability is verified.
    assert_eq!(ztune(&f, &["apply", "gpu.governor", "powersave"]).0, 0);
    let (_, d) = ztune(&f, &["capabilities"]);
    let gpu = d["capabilities"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["id"] == "gpu.governor")
        .unwrap()
        .clone();
    assert_eq!(gpu["state"], "verified");
    assert_eq!(gpu["authorization"]["write_authorized"], true);
    assert_eq!(
        gpu["authorization"]["granted_values"],
        serde_json::json!(["msm-adreno-tz", "powersave"])
    );
}

#[test]
fn telemetry_has_units_and_never_invents_values() {
    let f = device("tele");
    let (code, d) = ztune(&f, &["telemetry", "--interval-ms", "0"]);
    assert_eq!(code, 0);
    assert_eq!(d["platform"]["vendor"], "qualcomm");
    let c0 = &d["cpu"]["cores"][0];
    assert_eq!(c0["cur_freq"]["value"], 1_804_800);
    assert_eq!(c0["cur_freq"]["unit"], "kHz");
    assert_eq!(c0["governor"]["value"], "schedutil");
    assert_eq!(c0["load"]["status"], "not_sampled");
    assert!(c0["load"]["value"].is_null());
    assert_eq!(d["thermal"]["cpu"]["value"], 45.1);
    assert_eq!(d["thermal"]["cpu"]["unit"], "celsius");
    assert_eq!(d["thermal"]["gpu"]["value"], 51.3);
    assert_eq!(d["memory"]["total"]["value"], 8_000_000u64 * 1024);
    assert_eq!(d["memory"]["used"]["unit"], "bytes");
    assert_eq!(d["gpu"]["cur_freq"]["value"], 257_000_000);
    assert_eq!(d["gpu"]["cur_freq"]["unit"], "Hz");
    assert_eq!(d["storage"].as_array().unwrap().len(), 2);
    // Remove a sensor: the reading says so instead of reporting 0.
    std::fs::remove_dir_all(f.root.join("sys/class/thermal/thermal_zone1")).unwrap();
    let (_, d) = ztune(&f, &["telemetry", "--interval-ms", "0"]);
    assert_eq!(d["thermal"]["gpu"]["status"], "unsupported");
    assert!(d["thermal"]["gpu"]["value"].is_null());
}

#[test]
fn apply_separates_request_from_result_and_restores() {
    let f = device("apply");
    let (code, d) = ztune(&f, &["apply", "gpu.governor", "powersave"]);
    assert_eq!(code, 0);
    assert_eq!(d["kind"], "operation");
    assert_eq!(d["request"]["status"], "accepted");
    assert_eq!(d["operation"]["status"], "applied_verified");
    assert_eq!(d["operation"]["targets"][0]["before"], "msm-adreno-tz");
    assert_eq!(d["operation"]["targets"][0]["actual"], "powersave");
    assert_eq!(
        get(&f.root, "/sys/class/kgsl/kgsl-3d0/devfreq/governor"),
        "powersave"
    );
    let (code, d) = ztune(&f, &["restore"]);
    assert_eq!(code, 0);
    assert_eq!(d["kind"], "restore");
    assert_eq!(d["targets"][0]["outcome"], "restored");
    assert_eq!(
        get(&f.root, "/sys/class/kgsl/kgsl-3d0/devfreq/governor"),
        "msm-adreno-tz"
    );
}

#[test]
fn rejected_requests_exit_4_and_write_nothing() {
    let f = device("reject");
    let (code, d) = ztune(&f, &["apply", "gpu.governor", "userspace"]);
    assert_eq!(code, 4);
    assert_eq!(d["request"]["status"], "rejected");
    assert_eq!(d["operation"]["status"], "not_started");
    assert_eq!(d["error"]["code"], "refused");
    let (code, d) = ztune(&f, &["apply", "io.scheduler", "../../x"]);
    assert_eq!(code, 4);
    assert_eq!(d["error"]["code"], "invalid_value");
    std::fs::remove_dir_all(f.root.join("sys/class/kgsl")).unwrap();
    let (code, d) = ztune(&f, &["apply", "gpu.governor", "powersave"]);
    assert_eq!(code, 5);
    assert_eq!(d["error"]["code"], "unsupported");
    let (code, d) = ztune(&f, &["bogus"]);
    assert_eq!(code, 2);
    assert_eq!(d["error"]["code"], "usage");
}

#[test]
fn failures_also_name_the_error_on_stderr() {
    let f = device("stderr");
    let out = Command::new(env!("CARGO_BIN_EXE_ztune"))
        .arg("--sysroot")
        .arg(&f.root)
        .args(["apply", "gpu.governor", "userspace"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(4));
    assert!(String::from_utf8_lossy(&out.stderr).starts_with("ztune: refused: "));
}

#[test]
fn gpu_table_plan_needs_matching_device_and_token() {
    let f = device("gputable");
    let dts = f.state.with_extension("dts");
    std::fs::create_dir_all(dts.parent().unwrap()).unwrap();
    std::fs::write(&dts, kona_dts()).unwrap();
    let dts_s = dts.to_str().unwrap();

    let (code, d) = ztune(&f, &["gpu-table", "inspect", "--dts", dts_s]);
    assert_eq!(code, 0, "{d}");
    let bin = &d["inspection"]["bins"][0];
    assert_eq!(d["inspection"]["chip"], "kona");
    assert_eq!(bin["levels"][0]["freq_hz"], 727_000_000);
    assert_eq!(bin["levels"][2]["vote_name"], "NOM");

    let plan_args = [
        "gpu-table",
        "plan",
        "--dts",
        dts_s,
        "--bin",
        "0",
        "--max-freq-hz",
        "600000000",
    ];
    let (code, d) = ztune(&f, &plan_args);
    assert_eq!(code, 0, "{d}");
    assert_eq!(
        d["plan"]["edit"]["removed_hz"],
        serde_json::json!([727_000_000])
    );
    assert_eq!(d["plan"]["edit"]["index_changes"][0]["after"], 2);
    assert!(d["plan"]["flash"]
        .as_str()
        .unwrap()
        .starts_with("not_implemented"));
    let token = d["plan"]["confirmation_token"]
        .as_str()
        .unwrap()
        .to_string();

    let out = f.state.with_extension("staged");
    let out_s = out.to_str().unwrap();
    let stage = |confirm: Option<&str>| {
        let mut a: Vec<&str> = vec![
            "gpu-table",
            "stage",
            "--dts",
            dts_s,
            "--bin",
            "0",
            "--max-freq-hz",
            "600000000",
            "--out",
            out_s,
        ];
        if let Some(c) = confirm {
            a.extend(["--confirm", c]);
        }
        ztune(&f, &a)
    };
    let (code, d) = stage(None);
    assert_eq!(code, 4);
    assert_eq!(d["error"]["code"], "confirmation_required");
    let (code, d) = stage(Some("0000000000000000"));
    assert_eq!(code, 4);
    assert!(!out.exists());
    let (code, d2) = stage(Some(&token));
    assert_eq!(code, 0, "{d} {d2}");
    assert_eq!(d2["staged"]["verified"], true);
    assert_eq!(
        std::fs::read_to_string(out.join("original.dts")).unwrap(),
        kona_dts()
    );
    let modified = std::fs::read_to_string(out.join("modified.dts")).unwrap();
    assert!(!modified.contains("0x2b5523c0"));
    assert!(modified.contains("qcom,initial-pwrlevel = <2>;"));
    // Staging never overwrites.
    let (code, d) = stage(Some(&token));
    assert_eq!((code, d["error"]["code"].as_str()), (4, Some("refused")));
    std::fs::remove_dir_all(&out).unwrap();

    // Another device: refused before anything is written.
    put(
        &f.root,
        "/sys/firmware/devicetree/base/model",
        "Qualcomm Technologies, Inc. Lahaina v2.1 SoC\0",
    );
    let (code, d) = ztune(&f, &plan_args);
    assert_eq!(code, 4);
    assert_eq!(d["error"]["code"], "device_mismatch");
    std::fs::remove_file(f.root.join("sys/firmware/devicetree/base/model")).unwrap();
    let (code, d) = ztune(&f, &plan_args);
    assert_eq!(
        (code, d["error"]["code"].as_str()),
        (4, Some("device_mismatch"))
    );
    std::fs::remove_file(&dts).unwrap();
}

#[test]
fn gpu_table_rejects_other_input() {
    let f = device("gpubad");
    let p = f.state.with_extension("bad.dts");
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(&p, "/ { model = \"Exynos\"; compatible = \"qcom,x\"; };\n").unwrap();
    let (code, d) = ztune(&f, &["gpu-table", "inspect", "--dts", p.to_str().unwrap()]);
    assert_eq!(
        (code, d["error"]["code"].as_str()),
        (5, Some("unsupported"))
    );
    std::fs::write(&p, [0xffu8, 0xfe, 0x00]).unwrap();
    let (code, d) = ztune(&f, &["gpu-table", "inspect", "--dts", p.to_str().unwrap()]);
    assert_eq!(
        (code, d["error"]["code"].as_str()),
        (4, Some("invalid_value"))
    );
    std::fs::remove_file(&p).unwrap();
}

#[test]
fn device_reports_identity_family_and_tier() {
    let f = device("device");
    let (code, d) = ztune_bare(&f, &["device"]);
    assert_eq!(code, 0);
    assert_eq!(d["kind"], "device");
    assert_eq!(d["device"]["codename"], "synthkona");
    assert_eq!(d["device"]["soc"]["family"], "qualcomm");
    assert_eq!(d["device"]["soc"]["platform_id"], "sm8250");
    assert_eq!(d["device"]["android_api"], 35);
    assert_eq!(d["resolution"]["tier"], "S");
    assert!(d["resolution"]["write_grants"]
        .as_array()
        .unwrap()
        .is_empty());
    let (_, d) = ztune(&f, &["device"]);
    assert_eq!(d["resolution"]["tier"], "D");
    assert_eq!(d["resolution"]["profile_id"], "synthkona-test-only");
}

#[test]
fn writes_need_an_exact_device_validated_profile() {
    let f = device("authz");
    let (code, d) = ztune_bare(&f, &["apply", "gpu.governor", "powersave"]);
    assert_eq!(code, 4);
    assert_eq!(d["request"]["status"], "rejected");
    assert_eq!(d["error"]["code"], "not_authorized");
    assert_eq!(
        get(&f.root, "/sys/class/kgsl/kgsl-3d0/devfreq/governor"),
        "msm-adreno-tz"
    );
    let (_, d) = ztune_bare(&f, &["capabilities"]);
    let gpu = d["capabilities"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["id"] == "gpu.governor")
        .unwrap()
        .clone();
    assert_eq!(gpu["authorization"]["write_authorized"], false);
    assert_eq!(gpu["authorization"]["tier"], "S");
    // A different build is a different device: the profile no longer matches.
    put(
        &f.root,
        "/vendor/build.prop",
        &std::fs::read_to_string(f.root.join("vendor/build.prop"))
            .unwrap()
            .replace("SYNTH.0001", "SYNTH.0002"),
    );
    let (code, d) = ztune(&f, &["apply", "gpu.governor", "powersave"]);
    assert_eq!(
        (code, d["error"]["code"].as_str()),
        (4, Some("not_authorized"))
    );
}

#[test]
fn synthetic_profiles_never_count_on_a_real_root() {
    let out = Command::new(env!("CARGO_BIN_EXE_ztune"))
        .args(["--profiles-dir", profiles_dir().to_str().unwrap(), "device"])
        .output()
        .unwrap();
    let d: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert!(d["profiles"]["loaded"].as_array().unwrap().is_empty());
    assert_eq!(d["profiles"]["rejected"][0]["error"]["code"], "refused");
    assert!(d["resolution"]["write_grants"]
        .as_array()
        .unwrap()
        .is_empty());
}
