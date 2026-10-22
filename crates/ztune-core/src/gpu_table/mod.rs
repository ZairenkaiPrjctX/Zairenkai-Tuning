// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2021-2025 LibXZR and the KonaBess contributors
// Copyright 2026 ZairenkaiPrjctX contributors
//! Adreno GPU power-level tables in a decompiled device tree: parse, inspect, and the one
//! edit Zairenkai supports, capping the maximum frequency by removing the top levels.
//!
//! Adapted from KonaBess `app/src/main/java/xzr/konabess/GpuTableEditor.java` (`decode`,
//! `decode_bin`, `decode_level`, `genTable`, `genBack`, `offset_initial_level`,
//! `offset_initial_level_old`, `offset_ca_target_level`) at commit
//! ff387dc7f630d7460c7a6561f83f71846217ee27 (https://github.com/libxzr/KonaBess, GPL-3.0).
//! Changes (2026-10-22): Rust port over text with typed errors; the Android UI is not
//! ported; the only edit is removing levels above a frequency cap (no new levels, no
//! frequency or voltage edits, so nothing can exceed the stock table); every index property
//! (`initial`, `ca-target`, `throttle`) is shifted by the number of removed levels and
//! clamped at 0, where upstream shifted `initial` by one per removal and `ca-target` only on
//! lito/lagoon; `throttle-pwrlevel` is shifted, never forced to 0 as upstream's save did;
//! the single-bin index fix-up only touches direct properties of the `qcom,kgsl-3d0` node;
//! only an exact `reg` property is dropped from a level (upstream dropped any line
//! containing "reg"). Table layout, bin parsing and regeneration order are upstream's.
//!
//! Nothing here reads or writes a partition. See `docs/GPU_TABLE.md`.

pub mod chip;
pub mod dts;

use serde::Serialize;
use sha2::{Digest, Sha256};

pub use chip::Chip;

use crate::contract::{Error, ErrorCode};

/// Index properties that count levels from the top (0 = highest frequency).
pub const INDEX_PROPERTIES: &[&str] = &[
    "qcom,initial-pwrlevel",
    "qcom,ca-target-pwrlevel",
    "qcom,throttle-pwrlevel",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Level {
    pub lines: Vec<String>,
}

impl Level {
    fn int(&self, name: &str) -> Option<u64> {
        self.lines
            .iter()
            .filter_map(|l| dts::decode_int_line(l))
            .find(|p| p.name == name)
            .map(|p| p.value)
    }

    pub fn freq_hz(&self) -> Option<u64> {
        self.int("qcom,gpu-freq")
    }

    pub fn vote(&self) -> Option<u64> {
        self.int("qcom,level").or_else(|| self.int("qcom,cx-level"))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Bin {
    pub id: u32,
    pub header: Vec<String>,
    pub levels: Vec<Level>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Table {
    pub chip: Chip,
    pub bins: Vec<Bin>,
    rest: Vec<String>,
    insert_at: usize,
}

fn bad(msg: impl Into<String>) -> Error {
    Error::new(ErrorCode::InvalidValue, msg)
}

fn bin_id(header: &str, fallback: usize) -> u32 {
    let name = header.trim().trim_end_matches('{').trim();
    let digits: String = name
        .chars()
        .rev()
        .take_while(|c| c.is_ascii_digit())
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    digits
        .parse()
        .unwrap_or_else(|_| u32::try_from(fallback).unwrap_or(u32::MAX))
}

fn decode_bin(block: &[String], index: usize) -> Result<Bin, Error> {
    let mut bin = Bin {
        id: bin_id(&block[0], index),
        header: Vec::new(),
        levels: Vec::new(),
    };
    let mut depth = 0i32;
    let mut start = 0usize;
    for (k, raw) in block.iter().enumerate().skip(1) {
        let line = raw.trim();
        if line.is_empty() {
            continue;
        }
        if line.contains('{') {
            if depth != 0 {
                return Err(bad(format!("nested node inside a power level: {line:?}")));
            }
            start = k;
            depth += 1;
            continue;
        }
        if line.contains('}') {
            depth -= 1;
            if depth < 0 {
                break;
            }
            bin.levels.push(decode_level(&block[start..=k]));
            continue;
        }
        if depth == 0 {
            bin.header.push(line.to_string());
        }
    }
    Ok(bin)
}

fn decode_level(lines: &[String]) -> Level {
    Level {
        lines: lines
            .iter()
            .map(|l| l.trim())
            .filter(|l| !l.contains('{') && !l.contains('}'))
            .filter(|l| dts::property_name(l) != Some("reg"))
            .filter(|l| !l.is_empty())
            .map(str::to_string)
            .collect(),
    }
}

/// Parses the power-level tables of `chip` out of a decompiled device tree.
pub fn parse(text: &str, chip: Chip) -> Result<Table, Error> {
    let single = chip.single_bin();
    let mut rest: Vec<String> = text.lines().map(str::to_string).collect();
    let mut bins = Vec::new();
    let mut insert_at = None;
    let mut start: Option<usize> = None;
    let mut depth = 0i32;
    let mut i = 0;
    while i < rest.len() {
        let line = rest[i].trim().to_string();
        let opens_single = single && line == "qcom,gpu-pwrlevels {";
        let opens_bin =
            !single && line.contains("qcom,gpu-pwrlevels-") && !line.contains("compatible = ");
        if opens_single || opens_bin {
            if depth != 0 {
                return Err(bad(format!("power-level table nested at line {}", i + 1)));
            }
            start = Some(i);
            insert_at.get_or_insert(i);
            depth += 1;
            i += 1;
            continue;
        }
        if start.is_some() {
            if line.contains('{') {
                depth += 1;
            }
            if line.contains('}') {
                depth -= 1;
            }
        }
        if let (0, Some(s)) = (depth, start) {
            let block: Vec<String> = rest.drain(s..=i).collect();
            bins.push(decode_bin(&block, bins.len())?);
            start = None;
            if single {
                break;
            }
            i = s;
            continue;
        }
        i += 1;
    }
    if start.is_some() {
        return Err(bad("unterminated power-level table"));
    }
    let insert_at = insert_at.ok_or_else(|| {
        Error::new(
            ErrorCode::Unsupported,
            "no qcom,gpu-pwrlevels table for this chip in the device tree",
        )
    })?;
    for b in &bins {
        if b.levels.is_empty() {
            return Err(bad(format!("bin {} has no power levels", b.id)));
        }
        if let Some(n) = b.levels.iter().position(|l| l.freq_hz().is_none()) {
            return Err(bad(format!("bin {} level {n} has no qcom,gpu-freq", b.id)));
        }
    }
    Ok(Table {
        chip,
        bins,
        rest,
        insert_at,
    })
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LevelSummary {
    pub index: usize,
    pub freq_hz: u64,
    pub vote: Option<u64>,
    pub vote_name: Option<&'static str>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BinSummary {
    pub id: u32,
    pub levels: Vec<LevelSummary>,
    pub index_properties: Vec<(String, u64)>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct IndexChange {
    pub property: String,
    pub location: String,
    pub before: u64,
    pub after: u64,
    pub clamped: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CapPlan {
    pub bin: u32,
    pub max_freq_hz: u64,
    pub removed_hz: Vec<u64>,
    pub remaining_hz: Vec<u64>,
    pub index_changes: Vec<IndexChange>,
}

fn shift_index(line: &mut String, by: u64, location: &str) -> Option<IndexChange> {
    let p = dts::decode_int_line(line)?;
    if !INDEX_PROPERTIES.contains(&p.name.as_str()) {
        return None;
    }
    let after = p.value.saturating_sub(by);
    *line = dts::encode_int_line(&p.name, after);
    Some(IndexChange {
        property: p.name,
        location: location.into(),
        before: p.value,
        after,
        clamped: p.value < by,
    })
}

impl Table {
    pub fn summary(&self) -> Vec<BinSummary> {
        self.bins
            .iter()
            .map(|b| BinSummary {
                id: b.id,
                levels: b
                    .levels
                    .iter()
                    .enumerate()
                    .map(|(index, l)| LevelSummary {
                        index,
                        freq_hz: l.freq_hz().unwrap_or(0),
                        vote: l.vote(),
                        vote_name: l.vote().and_then(|v| self.chip.level_name(v)),
                    })
                    .collect(),
                index_properties: b
                    .header
                    .iter()
                    .filter_map(|l| dts::decode_int_line(l))
                    .filter(|p| INDEX_PROPERTIES.contains(&p.name.as_str()))
                    .map(|p| (p.name, p.value))
                    .collect(),
            })
            .collect()
    }

    /// Removes every level above `max_hz` from bin `bin_id` and shifts index properties.
    pub fn cap_max_frequency(&mut self, bin_id: u32, max_hz: u64) -> Result<CapPlan, Error> {
        let single = self.chip.single_bin();
        let bin = self
            .bins
            .iter_mut()
            .find(|b| b.id == bin_id)
            .ok_or_else(|| bad(format!("no bin {bin_id} in the table")))?;
        let freqs: Vec<u64> = bin.levels.iter().filter_map(Level::freq_hz).collect();
        let nonzero: Vec<u64> = freqs.iter().copied().take_while(|f| *f > 0).collect();
        if freqs[nonzero.len()..].iter().any(|f| *f > 0) || nonzero.windows(2).any(|w| w[0] < w[1])
        {
            return Err(bad(format!(
                "bin {bin_id} is not ordered from the highest frequency down; refusing to edit"
            )));
        }
        let removed = nonzero.iter().take_while(|f| **f > max_hz).count();
        if removed == nonzero.len() {
            return Err(bad(format!(
                "{max_hz} Hz is below the lowest level ({} Hz); at least one level must remain",
                nonzero.last().copied().unwrap_or(0)
            )));
        }
        bin.levels.drain(..removed);
        let by = removed as u64;
        let mut index_changes = Vec::new();
        if by > 0 {
            for line in bin.header.iter_mut() {
                index_changes.extend(shift_index(line, by, &format!("bin {bin_id}")));
            }
            if single {
                index_changes.extend(shift_kgsl_node(&mut self.rest, by));
            }
        }
        Ok(CapPlan {
            bin: bin_id,
            max_freq_hz: max_hz,
            removed_hz: nonzero[..removed].to_vec(),
            remaining_hz: freqs[removed..].to_vec(),
            index_changes,
        })
    }

    /// Regenerates the device tree with the (possibly edited) tables.
    pub fn render(&self) -> String {
        let mut table = Vec::new();
        let level_lines = |out: &mut Vec<String>, b: &Bin| {
            for (n, l) in b.levels.iter().enumerate() {
                out.push(format!("qcom,gpu-pwrlevel@{n} {{"));
                out.push(format!("reg = <{n}>;"));
                out.extend(l.lines.iter().cloned());
                out.push("};".into());
            }
        };
        if self.chip.single_bin() {
            table.push("qcom,gpu-pwrlevels {".to_string());
            table.extend(self.bins[0].header.iter().cloned());
            level_lines(&mut table, &self.bins[0]);
            table.push("};".into());
        } else {
            for b in &self.bins {
                table.push(format!("qcom,gpu-pwrlevels-{} {{", b.id));
                table.extend(b.header.iter().cloned());
                level_lines(&mut table, b);
                table.push("};".into());
            }
        }
        let mut out = self.rest.clone();
        out.splice(self.insert_at..self.insert_at, table);
        let mut s = out.join("\n");
        s.push('\n');
        s
    }
}

/// Single-bin chips keep `qcom,initial-pwrlevel` on the `qcom,kgsl-3d0` node.
fn shift_kgsl_node(lines: &mut [String], by: u64) -> Vec<IndexChange> {
    let mut changes = Vec::new();
    let Some(start) = lines
        .iter()
        .position(|l| l.contains("qcom,kgsl-3d0") && l.contains('{'))
    else {
        return changes;
    };
    let mut depth = 1i32;
    for line in lines.iter_mut().skip(start + 1) {
        if line.contains('{') {
            depth += 1;
        }
        if line.contains('}') {
            depth -= 1;
            if depth == 0 {
                break;
            }
        }
        if depth == 1 {
            changes.extend(shift_index(line, by, "qcom,kgsl-3d0"));
        }
    }
    changes
}

/// The token a caller must echo back to stage a plan: it binds the exact input bytes, the
/// edit and the device the plan was made on.
pub fn confirmation_token(input: &[u8], bin: u32, max_hz: u64, device_model: &str) -> String {
    let mut h = Sha256::new();
    h.update(input);
    h.update(format!("\0{bin}\0{max_hz}\0{device_model}").as_bytes());
    h.finalize()[..8]
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

pub fn sha256_hex(data: &[u8]) -> String {
    Sha256::digest(data)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const MULTI: &str = "/dts-v1/;\n/ {\n\tmodel = \"Qualcomm Technologies, Inc. kona v2.1 SoC\";\n\
\tqcom,kgsl-3d0@3d00000 {\n\t\tqcom,gpu-pwrlevel-bins {\n\t\t\tcompatible = \"qcom,gpu-pwrlevels-bins\";\n\
\t\t\tqcom,gpu-pwrlevels-0 {\n\t\t\t\tqcom,speed-bin = <0x0>;\n\t\t\t\tqcom,initial-pwrlevel = <0x2>;\n\
\t\t\t\tqcom,ca-target-pwrlevel = <0x1>;\n\
\t\t\t\tqcom,gpu-pwrlevel@0 {\n\t\t\t\t\treg = <0x0>;\n\t\t\t\t\tqcom,gpu-freq = <0x2b5523c0>;\n\t\t\t\t\tqcom,level = <0x180>;\n\t\t\t\t};\n\
\t\t\t\tqcom,gpu-pwrlevel@1 {\n\t\t\t\t\treg = <0x1>;\n\t\t\t\t\tqcom,gpu-freq = <0x23c34600>;\n\t\t\t\t\tqcom,level = <0x100>;\n\t\t\t\t};\n\
\t\t\t\tqcom,gpu-pwrlevel@2 {\n\t\t\t\t\treg = <0x2>;\n\t\t\t\t\tqcom,gpu-freq = <0x0bebc200>;\n\t\t\t\t\tqcom,level = <0x40>;\n\t\t\t\t};\n\
\t\t\t};\n\t\t\tqcom,gpu-pwrlevels-1 {\n\t\t\t\tqcom,speed-bin = <0x1>;\n\t\t\t\tqcom,initial-pwrlevel = <0x1>;\n\
\t\t\t\tqcom,gpu-pwrlevel@0 {\n\t\t\t\t\treg = <0x0>;\n\t\t\t\t\tqcom,gpu-freq = <0x23c34600>;\n\t\t\t\t\tqcom,level = <0x100>;\n\t\t\t\t};\n\
\t\t\t\tqcom,gpu-pwrlevel@1 {\n\t\t\t\t\treg = <0x1>;\n\t\t\t\t\tqcom,gpu-freq = <0x0bebc200>;\n\t\t\t\t\tqcom,level = <0x40>;\n\t\t\t\t};\n\
\t\t\t};\n\t\t};\n\t};\n};\n";

    #[test]
    fn parse_multi_bin() {
        let chip = chip::identify(MULTI).unwrap();
        assert_eq!(chip, Chip::Kona);
        let t = parse(MULTI, chip).unwrap();
        assert_eq!(t.bins.len(), 2);
        let s = t.summary();
        assert_eq!(s[0].levels[0].freq_hz, 727_000_000);
        assert_eq!(s[0].levels[1].vote_name, Some("NOM"));
        assert_eq!(
            s[0].index_properties[0],
            ("qcom,initial-pwrlevel".into(), 2)
        );
        assert_eq!(s[1].id, 1);
    }

    #[test]
    fn render_round_trips_through_parse() {
        let t = parse(MULTI, Chip::Kona).unwrap();
        let again = parse(&t.render(), Chip::Kona).unwrap();
        assert_eq!(t.summary(), again.summary());
        // Lines outside the table are kept verbatim.
        assert!(t
            .render()
            .contains("compatible = \"qcom,gpu-pwrlevels-bins\";"));
    }

    #[test]
    fn cap_removes_top_levels_and_shifts_indices() {
        let mut t = parse(MULTI, Chip::Kona).unwrap();
        let plan = t.cap_max_frequency(0, 600_000_000).unwrap();
        assert_eq!(plan.removed_hz, vec![727_000_000]);
        assert_eq!(plan.remaining_hz, vec![600_000_000, 200_000_000]);
        assert_eq!(plan.index_changes.len(), 2);
        assert_eq!(
            (plan.index_changes[0].before, plan.index_changes[0].after),
            (2, 1)
        );
        assert_eq!(
            (plan.index_changes[1].before, plan.index_changes[1].after),
            (1, 0)
        );
        let out = parse(&t.render(), Chip::Kona).unwrap();
        let s = out.summary();
        assert_eq!(s[0].levels.len(), 2);
        assert_eq!(s[0].levels[0].freq_hz, 600_000_000);
        assert_eq!(s[0].index_properties[0].1, 1);
        // The other bin is untouched.
        assert_eq!(s[1].levels.len(), 2);
    }

    #[test]
    fn cap_guards() {
        let mut t = parse(MULTI, Chip::Kona).unwrap();
        assert_eq!(
            t.cap_max_frequency(0, 100).unwrap_err().code,
            ErrorCode::InvalidValue
        );
        assert!(t.cap_max_frequency(9, 600_000_000).is_err());
        let p = t.cap_max_frequency(0, 800_000_000).unwrap();
        assert!(p.removed_hz.is_empty());
    }

    #[test]
    fn single_bin_shifts_kgsl_node() {
        let dts = "/ {\nmodel = \"Qualcomm Technologies, Inc. Waipio v2 SoC\";\n\
qcom,kgsl-3d0@3d00000 {\nqcom,initial-pwrlevel = <1>;\nqcom,gpu-pwrlevels {\n\
qcom,gpu-pwrlevel@0 {\nreg = <0>;\nqcom,gpu-freq = <818000000>;\n};\n\
qcom,gpu-pwrlevel@1 {\nreg = <1>;\nqcom,gpu-freq = <600000000>;\n};\n\
qcom,gpu-pwrlevel@2 {\nreg = <2>;\nqcom,gpu-freq = <0>;\n};\n};\n};\n};\n";
        let chip = chip::identify(dts).unwrap();
        assert_eq!(chip, Chip::WaipioSingleBin);
        let mut t = parse(dts, chip).unwrap();
        let plan = t.cap_max_frequency(0, 700_000_000).unwrap();
        assert_eq!(plan.index_changes[0].location, "qcom,kgsl-3d0");
        assert_eq!(plan.index_changes[0].after, 0);
        assert!(t.render().contains("qcom,initial-pwrlevel = <0>;"));
        assert_eq!(plan.remaining_hz, vec![600_000_000, 0]);
    }

    #[test]
    fn unordered_or_missing_tables_refused() {
        assert_eq!(
            parse("model = \"kona v2.1\";\n", Chip::Kona)
                .unwrap_err()
                .code,
            ErrorCode::Unsupported
        );
        let unordered = MULTI.replacen("<0x2b5523c0>", "<0x0bebc100>", 1);
        let mut t = parse(&unordered, Chip::Kona).unwrap();
        assert!(t.cap_max_frequency(0, 150_000_000).is_err());
    }

    #[test]
    fn token_binds_inputs() {
        let a = confirmation_token(b"x", 0, 1, "m");
        assert_eq!(a.len(), 16);
        assert_ne!(a, confirmation_token(b"x", 0, 2, "m"));
        assert_ne!(a, confirmation_token(b"y", 0, 1, "m"));
        assert_ne!(a, confirmation_token(b"x", 0, 1, "n"));
    }
}
