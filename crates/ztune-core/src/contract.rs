// SPDX-License-Identifier: GPL-3.0-only
// Copyright 2026 ZairenkaiPrjctX contributors
//! The integration contract, `zairenkai.tuning` schema 1 (`docs/CONTRACT.md`).
//!
//! Every document `ztune` prints is one JSON object with `schema`, `schema_version`, `kind`
//! and `component`. Readings carry a status and a unit; a failed read is never replaced by a
//! number. Operation results keep the request decision (`request.status`) apart from what
//! happened on the device (`operation.status`), and report the value read back.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

pub const SCHEMA: &str = "zairenkai.tuning";
pub const SCHEMA_VERSION: u32 = 1;
pub const COMPONENT: &str = "zairenkai-tuning";
pub const LICENSE: &str = "GPL-3.0-only";
pub const SOURCE: &str = "https://github.com/ZairenkaiPrjctX/Zairenkai-Tuning";

/// Why a reading has (or has no) value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DataStatus {
    Ok,
    /// The node does not exist on this device.
    Unsupported,
    PermissionDenied,
    /// The node exists but gave no value now.
    Unavailable,
    /// The node gave a value that is not in the expected format.
    ParseError,
    /// A value needs two samples and only one was taken.
    NotSampled,
}

/// Units used by readings. Values are never pre-scaled to friendlier units.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Unit {
    #[serde(rename = "kHz")]
    KiloHertz,
    #[serde(rename = "Hz")]
    Hertz,
    #[serde(rename = "percent")]
    Percent,
    #[serde(rename = "celsius")]
    Celsius,
    #[serde(rename = "bytes")]
    Bytes,
    #[serde(rename = "text")]
    Text,
    #[serde(rename = "count")]
    Count,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Reading<T> {
    pub status: DataStatus,
    pub value: Option<T>,
    pub unit: Unit,
    /// Where the value came from (a path, or a derivation).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

impl<T> Reading<T> {
    pub fn ok(value: T, unit: Unit, source: impl Into<String>) -> Self {
        Self {
            status: DataStatus::Ok,
            value: Some(value),
            unit,
            source: Some(source.into()),
            detail: None,
        }
    }

    pub fn missing(status: DataStatus, unit: Unit, detail: impl Into<String>) -> Self {
        debug_assert!(status != DataStatus::Ok);
        Self {
            status,
            value: None,
            unit,
            source: None,
            detail: Some(detail.into()),
        }
    }

    pub fn with_source(mut self, source: impl Into<String>) -> Self {
        self.source = Some(source.into());
        self
    }

    pub fn map<U>(self, f: impl FnOnce(T) -> U) -> Reading<U> {
        Reading {
            status: self.status,
            value: self.value.map(f),
            unit: self.unit,
            source: self.source,
            detail: self.detail,
        }
    }
}

/// Capability state. Nothing is `verified` without a read-back on this boot, and nothing is
/// `available` without a successful read.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CapabilityState {
    /// Read capability: read successfully now.
    Available,
    /// Write capability: the node and its advertised values exist, but no write has been
    /// read back on this boot.
    NotVerified,
    /// Write capability: a write was read back on this boot.
    Verified,
    /// The node exists but cannot be used now (permission, read error).
    Unavailable,
    /// The node does not exist, or is ambiguous, on this device.
    Unsupported,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Capability {
    pub id: String,
    pub state: CapabilityState,
    /// The operation this capability allows, if it is a write capability.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub operation: Option<String>,
    /// Values the device advertises for the operation (never a built-in list).
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub allowed_values: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current: Option<String>,
    pub evidence: Vec<String>,
}

/// Machine-readable error codes. The set only grows within schema 1.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    Usage,
    /// The value is malformed or not one the device advertises.
    InvalidValue,
    /// The capability does not exist on this device.
    Unsupported,
    /// Refused by a compiled-in safety rule.
    Refused,
    /// The device does not match what the operation was planned for.
    DeviceMismatch,
    /// A confirmation token is missing or does not match the plan.
    ConfirmationRequired,
    PermissionDenied,
    /// Another `ztune` holds the lock.
    Busy,
    Io,
    /// The value read back differs from the value written.
    VerificationFailed,
    /// The feature exists in the contract but is intentionally not implemented.
    NotImplemented,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Error {
    pub code: ErrorCode,
    pub message: String,
}

impl Error {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}: {}", self.code, self.message)
    }
}

/// Whether the request was accepted for execution. Says nothing about the device.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RequestStatus {
    Accepted,
    Rejected,
}

/// What actually happened on the device.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OperationStatus {
    /// Nothing ran (rejected request, or a dry run).
    NotStarted,
    /// Every target already had the value; nothing was written.
    NoChange,
    /// Every target was written and read back with the requested value.
    AppliedVerified,
    /// A write or read-back failed and every written target was restored and read back.
    RolledBack,
    /// A write or read-back failed and at least one target could not be restored.
    Failed,
}

/// One node an operation touched, with what was read before and after.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TargetResult {
    pub target: String,
    pub path: String,
    pub before: Option<String>,
    pub requested: String,
    /// The value read back after the operation (after rollback, if one ran).
    pub actual: Option<String>,
    pub outcome: TargetOutcome,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TargetOutcome {
    Unchanged,
    Written,
    WouldWrite,
    Restored,
    RestoreFailed,
    WriteFailed,
    VerificationFailed,
    NotAttempted,
    /// Restore skipped: the value is no longer the one ztune wrote (changed externally).
    ChangedExternally,
}

/// The JSON document envelope.
pub fn document(kind: &str, version: &str, body: Map<String, Value>) -> Value {
    let mut m = Map::new();
    m.insert("schema".into(), Value::from(SCHEMA));
    m.insert("schema_version".into(), Value::from(SCHEMA_VERSION));
    m.insert("kind".into(), Value::from(kind));
    m.insert(
        "component".into(),
        serde_json::json!({ "name": COMPONENT, "version": version }),
    );
    m.extend(body);
    Value::Object(m)
}

/// Serializes a contract value. The contract's own types cannot fail to serialize (no maps
/// with non-string keys).
pub fn val<T: Serialize>(v: &T) -> Value {
    serde_json::to_value(v).expect("contract types serialize")
}

/// Builds the body of a document from named values.
pub fn body(pairs: Vec<(&str, Value)>) -> Map<String, Value> {
    pairs.into_iter().map(|(k, v)| (k.to_string(), v)).collect()
}

pub fn error_document(version: &str, error: &Error) -> Value {
    document("error", version, body(vec![("error", val(error))]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn envelope_and_units() {
        let r = Reading::ok(1_804_800u64, Unit::KiloHertz, "/sys/x");
        let d = document("telemetry", "0.1.0", body(vec![("freq", val(&r))]));
        assert_eq!(d["schema"], "zairenkai.tuning");
        assert_eq!(d["schema_version"], 1);
        assert_eq!(d["component"]["name"], "zairenkai-tuning");
        assert_eq!(d["freq"]["unit"], "kHz");
        assert_eq!(d["freq"]["status"], "ok");
        let m: Reading<u64> = Reading::missing(DataStatus::Unsupported, Unit::Celsius, "no zone");
        let v = serde_json::to_value(&m).unwrap();
        assert!(v["value"].is_null());
        assert_eq!(v["status"], "unsupported");
    }

    #[test]
    fn error_document_shape() {
        let d = error_document("0.1.0", &Error::new(ErrorCode::Busy, "locked"));
        assert_eq!(d["kind"], "error");
        assert_eq!(d["error"]["code"], "busy");
    }
}
