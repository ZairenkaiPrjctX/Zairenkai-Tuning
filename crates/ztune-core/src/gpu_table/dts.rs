// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2021 LibXZR and the KonaBess contributors
// Copyright 2026 ZairenkaiPrjctX contributors
//! Device tree source property lines: `name = <value>;`.
//!
//! Adapted from KonaBess `app/src/main/java/xzr/konabess/utils/DtsHelper.java` at commit
//! ff387dc7f630d7460c7a6561f83f71846217ee27 (https://github.com/libxzr/KonaBess, GPL-3.0).
//! Changes (2026-10-22): Rust port returning `Option` instead of throwing; only
//! single-cell values are accepted (upstream concatenated multi-cell digits); the
//! `decode_int_line_hz` and hex-line variants are not ported. The handling of dtc's
//! string rendering of 4-byte cells (`decode_stringed_int`) is upstream's.

/// A decoded single-cell property.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IntLine {
    pub name: String,
    pub value: u64,
}

/// dtc prints a cell whose bytes look printable as a string (e.g. `"6,\0"`); upstream
/// decodes three characters plus the implicit NUL as a big-endian u32.
fn decode_stringed(value: &str) -> Option<u64> {
    let inner = value.trim().strip_prefix('"')?.strip_suffix('"')?;
    let mut bytes = Vec::new();
    let mut chars = inner.chars();
    while let Some(c) = chars.next() {
        let b = if c == '\\' {
            match chars.next()? {
                'a' => 7,
                'b' => 8,
                'f' => 12,
                'n' => 10,
                'r' => 13,
                't' => 9,
                'v' => 11,
                '\\' => b'\\',
                '\'' => b'\'',
                '"' => b'"',
                _ => return None,
            }
        } else {
            u8::try_from(u32::from(c)).ok()?
        };
        bytes.push(b);
    }
    if bytes.len() != 3 {
        return None;
    }
    Some(bytes.iter().fold(0u64, |acc, b| acc * 256 + u64::from(*b)) * 256)
}

/// Decodes `name = <0x2b5523c0>;`, `name = <587000000>;` or a dtc string cell.
pub fn decode_int_line(line: &str) -> Option<IntLine> {
    let (name, value) = line.trim().split_once('=')?;
    let name = name.trim();
    if name.is_empty() {
        return None;
    }
    let value = value.trim().strip_suffix(';')?.trim();
    if value.starts_with('"') {
        return Some(IntLine {
            name: name.into(),
            value: decode_stringed(value)?,
        });
    }
    let cell = value.strip_prefix('<')?.strip_suffix('>')?.trim();
    if cell.contains(char::is_whitespace) {
        return None;
    }
    let value = match cell.strip_prefix("0x") {
        Some(hex) => u64::from_str_radix(hex, 16).ok()?,
        None => cell.parse().ok()?,
    };
    Some(IntLine {
        name: name.into(),
        value,
    })
}

pub fn property_name(line: &str) -> Option<&str> {
    let (name, _) = line.trim().split_once('=')?;
    Some(name.trim())
}

pub fn encode_int_line(name: &str, value: u64) -> String {
    format!("{name} = <{value}>;")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decode() {
        assert_eq!(
            decode_int_line("\t\tqcom,gpu-freq = <0x2b5523c0>;").unwrap(),
            IntLine {
                name: "qcom,gpu-freq".into(),
                value: 727_000_000
            }
        );
        assert_eq!(decode_int_line("qcom,level = <256>;").unwrap().value, 256);
        assert!(decode_int_line("reg = <0x0 0x1>;").is_none());
        assert!(decode_int_line("qcom,gpu-freq = <0xzz>;").is_none());
        assert!(decode_int_line("no value").is_none());
    }

    #[test]
    fn dtc_string_cells() {
        // 0x36 0x2c 0x30 0x00 printed as "6,0"
        assert_eq!(
            decode_int_line("qcom,gpu-freq = \"6,0\";").unwrap().value,
            0x362c_3000
        );
        assert!(decode_int_line("x = \"toolong\";").is_none());
    }

    #[test]
    fn encode() {
        assert_eq!(
            encode_int_line("qcom,initial-pwrlevel", 3),
            "qcom,initial-pwrlevel = <3>;"
        );
    }
}
