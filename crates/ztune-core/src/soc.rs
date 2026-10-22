// SPDX-License-Identifier: GPL-3.0-only
// Copyright 2026 ZairenkaiPrjctX contributors
//! SoC family identification from observed platform identifiers.
//!
//! The registry (`data/soc-registry.json`) is data with a source for every rule and label.
//! Detection is open-ended: any identifier that matches a family rule is reported with its raw
//! value, whether or not mainline Linux lists it, so a new Snapdragon, Dimensity, Exynos or
//! Tensor generation is detected without a code change. Nothing here decides support, a
//! profile or a write; a family match is Tier S at most. The GPU is never inferred from the
//! SoC.

use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

const REGISTRY_JSON: &str = include_str!("../data/soc-registry.json");

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Family {
    Qualcomm,
    Mediatek,
    SamsungExynos,
    GoogleTensor,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Signal {
    DtCompatible,
    Soc0Family,
    Soc0Machine,
    BoardPlatform,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Source {
    pub id: String,
    pub url: String,
    pub commit: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Rule {
    pub family: Family,
    pub signal: Signal,
    #[serde(default)]
    pub vendor: Option<String>,
    #[serde(default)]
    pub model_prefixes: Vec<String>,
    #[serde(default)]
    pub exact: Vec<String>,
    pub source: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Annotation {
    pub family: Family,
    pub key_kind: String,
    pub key: String,
    pub label: String,
    pub source: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Deserialize)]
pub struct KnownPlatforms {
    #[serde(default)]
    pub soc0_machine: Vec<String>,
    #[serde(default)]
    pub dt_compatible: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Registry {
    pub schema_version: u32,
    pub sources: Vec<Source>,
    pub family_rules: Vec<Rule>,
    pub annotations: Vec<Annotation>,
    pub known_platforms: std::collections::BTreeMap<Family, KnownPlatforms>,
}

/// The embedded registry. It is validated by tests, so parsing cannot fail at run time.
pub fn registry() -> &'static Registry {
    static R: OnceLock<Registry> = OnceLock::new();
    R.get_or_init(|| serde_json::from_str(REGISTRY_JSON).expect("embedded SoC registry parses"))
}

/// Identifiers observed on the device, raw. Every field is optional: absence is normal.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Observed {
    /// `/sys/firmware/devicetree/base/compatible`, split on NUL.
    pub dt_compatible: Vec<String>,
    /// `/sys/firmware/devicetree/base/model`.
    pub dt_model: Option<String>,
    /// `/sys/devices/soc0/family`, `machine`, `soc_id`.
    pub soc0_family: Option<String>,
    pub soc0_machine: Option<String>,
    pub soc0_soc_id: Option<String>,
    /// `ro.board.platform`, `ro.soc.manufacturer`, `ro.soc.model` from build.prop files.
    pub board_platform: Option<String>,
    pub soc_manufacturer: Option<String>,
    pub soc_model: Option<String>,
}

/// One identifier that matched a family rule.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Match {
    pub family: Family,
    pub signal: Signal,
    pub raw: String,
    /// Lower-case platform id (`sm8650`, `mt6789`, `exynos990`, `gs201`, `parrot`).
    pub platform_id: String,
    /// Whether mainline Linux lists this identifier (`known_platforms`).
    pub listed_in_mainline: bool,
    pub rule_source: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Label {
    pub label: String,
    pub source: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Identity {
    pub family: Family,
    /// The most specific platform id: `soc0_machine`, then a SoC compatible, then the board
    /// platform. `None` when nothing matched.
    pub platform_id: Option<String>,
    pub matches: Vec<Match>,
    /// Two signals naming different families. Identification is then `unknown`.
    pub conflicts: Vec<String>,
    pub labels: Vec<Label>,
    pub observed: Observed,
}

fn model_matches(model: &str, prefixes: &[String]) -> bool {
    prefixes.iter().any(|p| {
        model.strip_prefix(p.as_str()).is_some_and(|rest| {
            rest.starts_with(|c: char| c.is_ascii_digit())
                && rest
                    .chars()
                    .all(|c| c.is_ascii_digit() || c.is_ascii_lowercase())
        })
    })
}

fn listed(reg: &Registry, family: Family, signal: Signal, id: &str) -> bool {
    reg.known_platforms
        .get(&family)
        .is_some_and(|k| match signal {
            Signal::Soc0Machine => k.soc0_machine.iter().any(|m| m == id),
            Signal::DtCompatible => k.dt_compatible.iter().any(|m| m == id),
            _ => false,
        })
}

/// Identifies the SoC family from what was observed.
pub fn identify(observed: &Observed) -> Identity {
    let reg = registry();
    let mut matches = Vec::new();
    for rule in &reg.family_rules {
        match rule.signal {
            Signal::DtCompatible => {
                let vendor = rule.vendor.as_deref().unwrap_or("");
                for c in &observed.dt_compatible {
                    let Some((v, model)) = c.split_once(',') else {
                        continue;
                    };
                    if v == vendor && model_matches(model, &rule.model_prefixes) {
                        matches.push(Match {
                            family: rule.family,
                            signal: Signal::DtCompatible,
                            raw: c.clone(),
                            platform_id: model.to_string(),
                            listed_in_mainline: listed(
                                reg,
                                rule.family,
                                Signal::DtCompatible,
                                model,
                            ),
                            rule_source: rule.source.clone(),
                        });
                    }
                }
            }
            Signal::Soc0Family => {
                if let Some(f) = &observed.soc0_family {
                    if rule.exact.iter().any(|e| e == f.trim()) {
                        // The family attribute names the vendor; the machine names the SoC.
                        if let Some(m) = observed.soc0_machine.as_deref().map(str::trim) {
                            let id = m.to_ascii_lowercase();
                            matches.push(Match {
                                family: rule.family,
                                signal: Signal::Soc0Machine,
                                raw: m.to_string(),
                                listed_in_mainline: listed(
                                    reg,
                                    rule.family,
                                    Signal::Soc0Machine,
                                    &id,
                                ),
                                platform_id: id,
                                rule_source: rule.source.clone(),
                            });
                        }
                    }
                }
            }
            Signal::BoardPlatform => {
                if let Some(b) = observed.board_platform.as_deref().map(str::trim) {
                    if rule.exact.iter().any(|e| e == b) {
                        matches.push(Match {
                            family: rule.family,
                            signal: Signal::BoardPlatform,
                            raw: b.to_string(),
                            platform_id: b.to_ascii_lowercase(),
                            listed_in_mainline: false,
                            rule_source: rule.source.clone(),
                        });
                    }
                }
            }
            Signal::Soc0Machine => {}
        }
    }
    let mut families: Vec<Family> = matches.iter().map(|m| m.family).collect();
    families.sort_by_key(|f| *f as u8);
    families.dedup();
    let conflicts = if families.len() > 1 {
        vec![format!(
            "signals name different families: {}",
            matches
                .iter()
                .map(|m| format!("{:?} {} -> {:?}", m.signal, m.raw, m.family))
                .collect::<Vec<_>>()
                .join("; ")
        )]
    } else {
        Vec::new()
    };
    let family = match (families.as_slice(), conflicts.is_empty()) {
        ([f], true) => *f,
        _ => Family::Unknown,
    };
    let rank = |s: Signal| match s {
        Signal::Soc0Machine => 0,
        Signal::DtCompatible => 1,
        Signal::BoardPlatform => 2,
        Signal::Soc0Family => 3,
    };
    let platform_id = if family == Family::Unknown {
        None
    } else {
        matches
            .iter()
            .min_by_key(|m| rank(m.signal))
            .map(|m| m.platform_id.clone())
    };
    let labels = observed
        .dt_model
        .as_deref()
        .map(|model| {
            // The most specific pattern wins ("Lito v2" before "Lito").
            let mut hits: Vec<&Annotation> = reg
                .annotations
                .iter()
                .filter(|a| a.key_kind == "dt_model_contains" && model.contains(&a.key))
                .filter(|a| family == Family::Unknown || a.family == family)
                .collect();
            hits.sort_by_key(|a| std::cmp::Reverse(a.key.len()));
            hits.into_iter()
                .take(1)
                .map(|a| Label {
                    label: a.label.clone(),
                    source: a.source.clone(),
                })
                .collect()
        })
        .unwrap_or_default();
    Identity {
        family,
        platform_id,
        matches,
        conflicts,
        labels,
        observed: observed.clone(),
    }
}

/// Splits a NUL-separated device tree string list.
pub fn split_dt_strings(raw: &[u8]) -> Vec<String> {
    raw.split(|b| *b == 0)
        .filter(|s| !s.is_empty())
        .map(|s| String::from_utf8_lossy(s).trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dt(c: &[&str]) -> Observed {
        Observed {
            dt_compatible: c.iter().map(|s| s.to_string()).collect(),
            ..Default::default()
        }
    }

    #[test]
    fn registry_is_valid_and_sourced() {
        let r = registry();
        assert_eq!(r.schema_version, 1);
        let ids: Vec<&str> = r.sources.iter().map(|s| s.id.as_str()).collect();
        for rule in &r.family_rules {
            for s in rule.source.split_whitespace() {
                assert!(ids.contains(&s), "rule source {s} is not declared");
            }
            assert!(rule.exact.is_empty() != rule.model_prefixes.is_empty());
        }
        for a in &r.annotations {
            assert!(ids.contains(&a.source.as_str()));
        }
        for s in &r.sources {
            assert_eq!(s.commit.len(), 40, "{} must pin a full commit", s.id);
        }
        assert!(r.known_platforms[&Family::Qualcomm]
            .soc0_machine
            .contains(&"sm8650".to_string()));
    }

    #[test]
    fn each_family_from_its_own_signal() {
        let q = identify(&dt(&["qcom,sm8250-mtp", "qcom,sm8250"]));
        assert_eq!(
            (q.family, q.platform_id.as_deref()),
            (Family::Qualcomm, Some("sm8250"))
        );
        assert!(q.matches[0].listed_in_mainline);
        let m = identify(&dt(&["mediatek,mt6789"]));
        assert_eq!(
            (m.family, m.platform_id.as_deref()),
            (Family::Mediatek, Some("mt6789"))
        );
        // Not in mainline, still detected and labelled as unlisted.
        assert!(!m.matches[0].listed_in_mainline);
        let e = identify(&dt(&["samsung,exynos2100"]));
        assert_eq!(e.family, Family::SamsungExynos);
        let t = identify(&dt(&["google,gs101-oriole", "google,gs101"]));
        assert_eq!(
            (t.family, t.platform_id.as_deref()),
            (Family::GoogleTensor, Some("gs101"))
        );
        let z = identify(&Observed {
            board_platform: Some("zuma".into()),
            ..Default::default()
        });
        assert_eq!(z.family, Family::GoogleTensor);
    }

    #[test]
    fn soc0_machine_is_the_most_specific_id() {
        let o = Observed {
            dt_compatible: vec!["qcom,parrot".into()],
            soc0_family: Some("Snapdragon".into()),
            soc0_machine: Some("SM7635".into()),
            board_platform: Some("parrot".into()),
            ..Default::default()
        };
        let i = identify(&o);
        assert_eq!(i.family, Family::Qualcomm);
        assert_eq!(i.platform_id.as_deref(), Some("sm7635"));
        assert!(i.matches.iter().any(|m| m.signal == Signal::BoardPlatform));
    }

    #[test]
    fn unknown_and_conflicting_are_not_guessed() {
        let u = identify(&dt(&["rockchip,rk3588"]));
        assert_eq!((u.family, u.platform_id.clone()), (Family::Unknown, None));
        let c = identify(&Observed {
            dt_compatible: vec!["mediatek,mt6877".into()],
            board_platform: Some("gs201".into()),
            ..Default::default()
        });
        assert_eq!(c.family, Family::Unknown);
        assert_eq!(c.conflicts.len(), 1);
        assert_eq!(c.platform_id, None);
    }

    #[test]
    fn labels_are_sourced_and_most_specific() {
        let o = Observed {
            dt_compatible: vec!["qcom,lito".into()],
            dt_model: Some("Qualcomm Technologies, Inc. Lito v2 SoC".into()),
            ..Default::default()
        };
        let i = identify(&o);
        assert_eq!(i.labels[0].label, "Snapdragon 765 v2 series");
        assert_eq!(i.labels[0].source, "konabess");
    }

    #[test]
    fn dt_strings() {
        assert_eq!(
            split_dt_strings(b"qcom,kona-mtp\0qcom,kona\0"),
            vec!["qcom,kona-mtp", "qcom,kona"]
        );
    }
}
