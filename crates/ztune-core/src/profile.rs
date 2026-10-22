// SPDX-License-Identifier: GPL-3.0-only
// Copyright 2026 ZairenkaiPrjctX contributors
//! Device profiles and the support-tier resolver (`docs/PROFILES.md`).
//!
//! A profile is untrusted data. It names typed controls from [`OperationId`] with allowed
//! values; it never carries a path, a command or a script. Resolution:
//!
//! 1. an exact-device profile in state `device_validated` whose every identity key matches
//!    this device (codename, SoC/platform, board platform, Android API range, vendor build,
//!    kernel release, ABI) → Tier D, and only then are its controls write-authorized;
//! 2. a family-level match → Tier S, read-only;
//! 3. generic Linux/Android interfaces readable → Tier G, read-only;
//! 4. nothing → Tier X.
//!
//! A family match never authorizes a write, and a profile requiring a GPU identity cannot be
//! satisfied by ztune (the GPU is identified by Zairenkai-Graphics-Platform).

use serde::{Deserialize, Serialize};

use crate::contract::{Error, ErrorCode};
use crate::device::DeviceIdentity;
use crate::ops::{self, OperationId};
use crate::soc::Family;

pub const PROFILE_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ValidationState {
    Candidate,
    SourceVerified,
    FixtureTested,
    DeviceValidated,
    Deprecated,
    Rejected,
    Unsupported,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ApiRange {
    pub min: u32,
    pub max: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProfileIdentity {
    pub soc_family: Family,
    pub soc_platform_id: String,
    pub board_platform: String,
    pub device_codenames: Vec<String>,
    pub android_api: ApiRange,
    /// Exact `ro.vendor.build.id` values validated.
    pub vendor_build_ids: Vec<String>,
    /// Exact kernel releases (`/proc/sys/kernel/osrelease`) validated.
    pub kernel_releases: Vec<String>,
    pub abis: Vec<String>,
    /// Required GPU identity, if any. ztune cannot observe it, so a profile that sets it is
    /// never write-authorized by ztune.
    #[serde(default)]
    pub gpu_renderer: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ControlGrant {
    pub control: OperationId,
    pub allowed_values: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Provenance {
    pub url: String,
    pub commit: String,
    pub paths: Vec<String>,
    pub license: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Validation {
    pub hardware_tested: bool,
    /// Device validation records (device, build, kernel, logs).
    pub evidence: Vec<String>,
    pub reviewed_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Profile {
    pub schema_version: u32,
    pub profile_id: String,
    pub profile_version: String,
    pub status: ValidationState,
    /// True for profiles that exist only for tests. They are rejected outside `--profiles-dir`.
    #[serde(default)]
    pub synthetic: bool,
    pub identity: ProfileIdentity,
    pub controls: Vec<ControlGrant>,
    pub provenance: Vec<Provenance>,
    pub validation: Validation,
}

fn invalid(id: &str, msg: impl std::fmt::Display) -> Error {
    Error::new(ErrorCode::InvalidValue, format!("profile {id}: {msg}"))
}

/// Parses and validates one profile. Structural and consistency errors reject it.
pub fn parse(text: &str) -> Result<Profile, Error> {
    let p: Profile = serde_json::from_str(text)
        .map_err(|e| Error::new(ErrorCode::InvalidValue, format!("profile: {e}")))?;
    validate(&p)?;
    Ok(p)
}

pub fn validate(p: &Profile) -> Result<(), Error> {
    let id = p.profile_id.as_str();
    if p.schema_version != PROFILE_SCHEMA_VERSION {
        return Err(invalid(
            id,
            format!("schema_version {} is not 1", p.schema_version),
        ));
    }
    let id_ok = (1..=64).contains(&id.len())
        && id
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b"._-".contains(&b));
    if !id_ok {
        return Err(invalid(id, "profile_id must be 1-64 of [a-z0-9._-]"));
    }
    let ident = &p.identity;
    if ident.soc_family == Family::Unknown {
        return Err(invalid(id, "soc_family must name a family"));
    }
    if ident.device_codenames.is_empty() || ident.abis.is_empty() {
        return Err(invalid(id, "device_codenames and abis must not be empty"));
    }
    if ident.android_api.min > ident.android_api.max {
        return Err(invalid(id, "android_api.min is above android_api.max"));
    }
    for g in &p.controls {
        if g.allowed_values.is_empty() {
            return Err(invalid(
                id,
                format!("{} has no allowed values", g.control.name()),
            ));
        }
        for v in &g.allowed_values {
            if !ops::well_formed(v) {
                return Err(invalid(id, format!("{v:?} is not a kernel name")));
            }
            if g.control == OperationId::GpuGovernor && crate::gpu::refusal(v).is_some() {
                return Err(invalid(
                    id,
                    format!("{v:?} is refused by a compiled-in rule"),
                ));
            }
        }
    }
    if p.status == ValidationState::DeviceValidated {
        let v = &p.validation;
        if !v.hardware_tested || v.evidence.is_empty() || v.reviewed_at.is_none() {
            return Err(invalid(
                id,
                "device_validated needs hardware_tested, evidence and reviewed_at",
            ));
        }
        if p.provenance.is_empty() || p.provenance.iter().any(|s| s.commit.len() != 40) {
            return Err(invalid(
                id,
                "device_validated needs provenance with full commits",
            ));
        }
        if ident.vendor_build_ids.is_empty() || ident.kernel_releases.is_empty() {
            return Err(invalid(
                id,
                "device_validated needs exact vendor builds and kernels",
            ));
        }
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Tier {
    /// Universal app-level behaviour only; nothing readable here (not produced by ztune,
    /// which is a root-side tool, but part of the shared vocabulary).
    U,
    /// Generic Linux/Android interfaces readable.
    G,
    /// SoC family identified; read-only diagnostics and unverified recommendations.
    S,
    /// Exact device/build validated profile; named controls may be written through Core.
    D,
    /// Unknown or conflicting platform.
    X,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Resolution {
    pub tier: Tier,
    pub profile_id: Option<String>,
    /// Controls this device may write, with the values the profile allows.
    pub write_grants: Vec<ControlGrant>,
    /// Why each considered profile did or did not apply.
    pub explanation: Vec<String>,
}

/// Every identity key that differs between `p` and the device.
pub fn mismatches(p: &Profile, d: &DeviceIdentity) -> Vec<String> {
    let i = &p.identity;
    let mut m = Vec::new();
    let eq = |a: &Option<String>, b: &str| a.as_deref() == Some(b);
    if d.soc.family != i.soc_family {
        m.push(format!(
            "soc_family {:?} != {:?}",
            d.soc.family, i.soc_family
        ));
    }
    if !eq(&d.soc.platform_id, &i.soc_platform_id) {
        m.push(format!(
            "soc_platform_id {:?} != {:?}",
            d.soc.platform_id, i.soc_platform_id
        ));
    }
    if !eq(&d.board_platform, &i.board_platform) {
        m.push(format!(
            "board_platform {:?} != {:?}",
            d.board_platform, i.board_platform
        ));
    }
    if !d
        .codename
        .as_ref()
        .is_some_and(|c| i.device_codenames.contains(c))
    {
        m.push(format!(
            "codename {:?} not in {:?}",
            d.codename, i.device_codenames
        ));
    }
    if !d
        .android_api
        .is_some_and(|a| (i.android_api.min..=i.android_api.max).contains(&a))
    {
        m.push(format!(
            "android_api {:?} outside {}-{}",
            d.android_api, i.android_api.min, i.android_api.max
        ));
    }
    if !d
        .vendor_build_id
        .as_ref()
        .is_some_and(|b| i.vendor_build_ids.contains(b))
    {
        m.push(format!(
            "vendor_build_id {:?} not validated",
            d.vendor_build_id
        ));
    }
    if !d
        .kernel_release
        .as_ref()
        .is_some_and(|k| i.kernel_releases.contains(k))
    {
        m.push(format!(
            "kernel_release {:?} not validated",
            d.kernel_release
        ));
    }
    if !d.abis.first().is_some_and(|a| i.abis.contains(a)) {
        m.push(format!(
            "primary ABI {:?} not in {:?}",
            d.abis.first(),
            i.abis
        ));
    }
    if i.gpu_renderer.is_some() {
        m.push("profile requires a GPU identity, which ztune cannot observe".into());
    }
    m
}

/// Resolves the support tier and write grants. `generic_readable` says whether standard
/// cpufreq/devfreq/thermal nodes could be read.
pub fn resolve(profiles: &[Profile], d: &DeviceIdentity, generic_readable: bool) -> Resolution {
    let mut explanation = Vec::new();
    for p in profiles {
        let mm = mismatches(p, d);
        if !mm.is_empty() {
            explanation.push(format!(
                "{}: not this device ({})",
                p.profile_id,
                mm.join("; ")
            ));
            continue;
        }
        if p.status != ValidationState::DeviceValidated {
            explanation.push(format!(
                "{}: matches, but its state is {:?}; recommendations only",
                p.profile_id, p.status
            ));
            continue;
        }
        explanation.push(format!(
            "{}: exact device/build match, device_validated",
            p.profile_id
        ));
        return Resolution {
            tier: Tier::D,
            profile_id: Some(p.profile_id.clone()),
            write_grants: p.controls.clone(),
            explanation,
        };
    }
    let tier = if d.soc.family != Family::Unknown {
        explanation.push(format!(
            "family {:?} identified; family matches never authorize writes",
            d.soc.family
        ));
        Tier::S
    } else if generic_readable {
        explanation.push("no SoC family identified; generic interfaces only".into());
        Tier::G
    } else {
        explanation.push("no SoC family and no readable generic interface".into());
        Tier::X
    };
    if profiles.is_empty() {
        explanation.push("no device profile is installed".into());
    }
    Resolution {
        tier,
        profile_id: None,
        write_grants: Vec::new(),
        explanation,
    }
}

impl Resolution {
    /// The values this resolution allows for `op`, if `op` is granted.
    pub fn grant(&self, op: OperationId) -> Option<&[String]> {
        self.write_grants
            .iter()
            .find(|g| g.control == op)
            .map(|g| g.allowed_values.as_slice())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::soc;

    pub(crate) fn device() -> DeviceIdentity {
        DeviceIdentity {
            codename: Some("synthkona".into()),
            manufacturer: Some("Synthetic".into()),
            board_platform: Some("kona".into()),
            android_api: Some(35),
            vendor_build_id: Some("SYNTH.0001".into()),
            vendor_build_incremental: None,
            kernel_release: Some("4.19.157-synthetic".into()),
            abis: vec!["arm64-v8a".into()],
            cpu_policies: vec![],
            soc: soc::identify(&soc::Observed {
                dt_compatible: vec!["qcom,sm8250".into()],
                ..Default::default()
            }),
        }
    }

    fn profile(status: ValidationState) -> Profile {
        Profile {
            schema_version: 1,
            profile_id: "synthkona-io".into(),
            profile_version: "0.1.0".into(),
            status,
            synthetic: true,
            identity: ProfileIdentity {
                soc_family: Family::Qualcomm,
                soc_platform_id: "sm8250".into(),
                board_platform: "kona".into(),
                device_codenames: vec!["synthkona".into()],
                android_api: ApiRange { min: 33, max: 35 },
                vendor_build_ids: vec!["SYNTH.0001".into()],
                kernel_releases: vec!["4.19.157-synthetic".into()],
                abis: vec!["arm64-v8a".into()],
                gpu_renderer: None,
            },
            controls: vec![ControlGrant {
                control: OperationId::IoScheduler,
                allowed_values: vec!["mq-deadline".into(), "kyber".into()],
            }],
            provenance: vec![Provenance {
                url: "https://example.invalid/synthetic".into(),
                commit: "0".repeat(40),
                paths: vec![],
                license: "test-only".into(),
            }],
            validation: Validation {
                hardware_tested: true,
                evidence: vec!["synthetic test record".into()],
                reviewed_at: Some("2026-10-22".into()),
            },
        }
    }

    #[test]
    fn only_exact_device_validated_profiles_grant_writes() {
        let d = device();
        let r = resolve(&[profile(ValidationState::DeviceValidated)], &d, true);
        assert_eq!(r.tier, Tier::D);
        assert_eq!(r.grant(OperationId::IoScheduler).unwrap().len(), 2);
        assert!(r.grant(OperationId::GpuGovernor).is_none());

        for s in [
            ValidationState::Candidate,
            ValidationState::FixtureTested,
            ValidationState::SourceVerified,
        ] {
            let r = resolve(&[profile(s)], &d, true);
            assert_eq!(r.tier, Tier::S, "{s:?}");
            assert!(r.write_grants.is_empty());
        }
    }

    #[test]
    fn any_identity_difference_blocks_writes() {
        let base = device();
        let p = profile(ValidationState::DeviceValidated);
        type Change = Box<dyn Fn(&mut DeviceIdentity)>;
        let variants: Vec<Change> = vec![
            Box::new(|d| d.codename = Some("other".into())),
            Box::new(|d| d.vendor_build_id = Some("SYNTH.0002".into())),
            Box::new(|d| d.kernel_release = Some("4.19.158".into())),
            Box::new(|d| d.android_api = Some(36)),
            Box::new(|d| d.abis = vec!["armeabi-v7a".into()]),
            Box::new(|d| d.board_platform = None),
            Box::new(|d| d.codename = None),
        ];
        for change in variants {
            let mut d = base.clone();
            change(&mut d);
            let r = resolve(std::slice::from_ref(&p), &d, true);
            assert_ne!(r.tier, Tier::D);
            assert!(r.write_grants.is_empty());
        }
        let mut gpu = p.clone();
        gpu.identity.gpu_renderer = Some("Adreno (TM) 650".into());
        assert!(resolve(&[gpu], &base, true).write_grants.is_empty());
    }

    #[test]
    fn tiers_without_profiles() {
        let mut d = device();
        assert_eq!(resolve(&[], &d, true).tier, Tier::S);
        d.soc = soc::identify(&soc::Observed::default());
        assert_eq!(resolve(&[], &d, true).tier, Tier::G);
        assert_eq!(resolve(&[], &d, false).tier, Tier::X);
    }

    #[test]
    fn validation_rejects_weak_device_validated_profiles() {
        let mut p = profile(ValidationState::DeviceValidated);
        p.validation.hardware_tested = false;
        assert!(validate(&p).is_err());
        let mut p = profile(ValidationState::DeviceValidated);
        p.identity.kernel_releases.clear();
        assert!(validate(&p).is_err());
        let mut p = profile(ValidationState::Candidate);
        p.controls[0].allowed_values = vec!["$(reboot)".into()];
        assert!(validate(&p).is_err());
        let mut p = profile(ValidationState::Candidate);
        p.controls = vec![ControlGrant {
            control: OperationId::GpuGovernor,
            allowed_values: vec!["userspace".into()],
        }];
        assert!(validate(&p).is_err());
        assert!(parse(r#"{"schema_version":1,"path":"/sys/x"}"#).is_err());
        let ok = serde_json::to_string(&profile(ValidationState::DeviceValidated)).unwrap();
        assert!(parse(&ok).is_ok());
        // Unknown fields (a smuggled path or command) are rejected.
        let smuggled = ok.replacen(
            "\"synthetic\":true",
            "\"synthetic\":true,\"command\":\"sh\"",
            1,
        );
        assert!(parse(&smuggled).is_err());
    }
}
