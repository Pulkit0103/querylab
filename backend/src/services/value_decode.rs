//! Converts PostgreSQL result values (binary protocol) into JSON.
//!
//! Precision is preserved over convenience: `numeric`, out-of-range `int8`,
//! `money` and special floats are returned as strings so JavaScript clients
//! never silently round them.

use serde_json::{json, Value};
use sqlx::{
    postgres::{
        types::{PgInterval, PgTimeTz},
        PgRow, PgValueRef,
    },
    Decode, Postgres, Row, TypeInfo, ValueRef,
};

/// Largest integer JavaScript can represent exactly (2^53 - 1).
const MAX_SAFE_INTEGER: i64 = 9_007_199_254_740_991;
/// Byte values longer than this are truncated in the hex preview.
const MAX_BYTEA_PREVIEW: usize = 2048;

fn decode<'r, T: Decode<'r, Postgres>>(value: PgValueRef<'r>) -> Option<T> {
    T::decode(value).ok()
}

fn int_json(v: i64) -> Value {
    if v.abs() > MAX_SAFE_INTEGER {
        Value::String(v.to_string())
    } else {
        json!(v)
    }
}

fn float_json(v: f64) -> Value {
    if v.is_finite() {
        json!(v)
    } else if v.is_nan() {
        Value::String("NaN".into())
    } else if v > 0.0 {
        Value::String("Infinity".into())
    } else {
        Value::String("-Infinity".into())
    }
}

/// Decodes PostgreSQL's binary NUMERIC representation to a decimal string.
pub fn numeric_to_string(bytes: &[u8]) -> Option<String> {
    let read = |i: usize| -> Option<i16> {
        Some(i16::from_be_bytes([*bytes.get(i)?, *bytes.get(i + 1)?]))
    };
    let ndigits = read(0)? as usize;
    let weight = read(2)? as i32;
    let sign = read(4)? as u16;
    let dscale = read(6)? as usize;
    match sign {
        0xC000 => return Some("NaN".into()),
        0xD000 => return Some("Infinity".into()),
        0xF000 => return Some("-Infinity".into()),
        0x0000 | 0x4000 => {}
        _ => return None,
    }
    let digits: Vec<i16> = (0..ndigits)
        .map(|i| read(8 + i * 2))
        .collect::<Option<_>>()?;

    // Integer part: base-10000 groups 0..=weight.
    let mut int_part = String::new();
    if weight < 0 {
        int_part.push('0');
    } else {
        for g in 0..=weight {
            let d = digits.get(g as usize).copied().unwrap_or(0);
            if g == 0 {
                int_part.push_str(&d.to_string());
            } else {
                int_part.push_str(&format!("{d:04}"));
            }
        }
    }

    // Fractional part: groups after the weight, padded to dscale digits.
    let mut frac = String::new();
    if dscale > 0 {
        let mut g = weight + 1;
        while frac.len() < dscale {
            let d = if g < 0 {
                0
            } else {
                digits.get(g as usize).copied().unwrap_or(0)
            };
            frac.push_str(&format!("{d:04}"));
            g += 1;
        }
        frac.truncate(dscale);
    }

    let mut out = String::new();
    if sign == 0x4000 && (int_part != "0" || frac.chars().any(|c| c != '0')) {
        out.push('-');
    }
    out.push_str(&int_part);
    if !frac.is_empty() {
        out.push('.');
        out.push_str(&frac);
    }
    Some(out)
}

fn inet_to_string(bytes: &[u8]) -> Option<String> {
    // family, bits, is_cidr, nb, address bytes
    let (&family, rest) = bytes.split_first()?;
    let (&bits, rest) = rest.split_first()?;
    let (&is_cidr, rest) = rest.split_first()?;
    let (&nb, addr) = rest.split_first()?;
    let addr = addr.get(..nb as usize)?;
    let (ip, max_bits) = match family {
        2 if addr.len() == 4 => (std::net::IpAddr::from(<[u8; 4]>::try_from(addr).ok()?), 32),
        3 if addr.len() == 16 => (
            std::net::IpAddr::from(<[u8; 16]>::try_from(addr).ok()?),
            128,
        ),
        _ => return None,
    };
    Some(if is_cidr == 1 || bits != max_bits {
        format!("{ip}/{bits}")
    } else {
        ip.to_string()
    })
}

fn interval_to_string(iv: &PgInterval) -> String {
    let mut parts = Vec::new();
    let (years, months) = (iv.months / 12, iv.months % 12);
    let plural = |n: i64, unit: &str| format!("{n} {unit}{}", if n.abs() == 1 { "" } else { "s" });
    if years != 0 {
        parts.push(plural(years.into(), "year"));
    }
    if months != 0 {
        parts.push(plural(months.into(), "mon"));
    }
    if iv.days != 0 {
        parts.push(plural(iv.days.into(), "day"));
    }
    if iv.microseconds != 0 || parts.is_empty() {
        let neg = iv.microseconds < 0;
        let us = iv.microseconds.unsigned_abs();
        let (h, m, s, frac) = (
            us / 3_600_000_000,
            (us / 60_000_000) % 60,
            (us / 1_000_000) % 60,
            us % 1_000_000,
        );
        let mut t = format!("{}{h:02}:{m:02}:{s:02}", if neg { "-" } else { "" });
        if frac != 0 {
            t.push_str(format!(".{frac:06}").trim_end_matches('0'));
        }
        parts.push(t);
    }
    parts.join(" ")
}

fn pg_epoch() -> chrono::NaiveDateTime {
    chrono::NaiveDate::from_ymd_opt(2000, 1, 1)
        .expect("valid date")
        .and_hms_opt(0, 0, 0)
        .expect("valid time")
}

/// Binary DATE: i32 days since 2000-01-01.
pub fn date_from_binary(bytes: &[u8]) -> Option<String> {
    let days = i32::from_be_bytes(bytes.try_into().ok()?);
    Some(match days {
        i32::MAX => "infinity".into(),
        i32::MIN => "-infinity".into(),
        d => pg_epoch()
            .date()
            .checked_add_signed(chrono::Duration::try_days(d.into())?)
            .map(|v| v.to_string())
            .unwrap_or_else(|| format!("<date out of range: {d} days>")),
    })
}

/// Binary TIMESTAMP/TIMESTAMPTZ: i64 microseconds since 2000-01-01 (UTC for tz).
pub fn timestamp_from_binary(bytes: &[u8], with_tz: bool) -> Option<String> {
    let micros = i64::from_be_bytes(bytes.try_into().ok()?);
    Some(match micros {
        i64::MAX => "infinity".into(),
        i64::MIN => "-infinity".into(),
        us => match pg_epoch().checked_add_signed(chrono::Duration::microseconds(us)) {
            Some(ts) if with_tz => ts.and_utc().to_rfc3339(),
            Some(ts) => ts.format("%Y-%m-%d %H:%M:%S%.f").to_string(),
            None => format!("<timestamp out of range: {us} µs>"),
        },
    })
}

fn bytes_to_hex(bytes: &[u8]) -> String {
    let shown = &bytes[..bytes.len().min(MAX_BYTEA_PREVIEW)];
    let mut s = String::with_capacity(2 + shown.len() * 2);
    s.push_str("\\x");
    for b in shown {
        s.push_str(&format!("{b:02x}"));
    }
    if bytes.len() > MAX_BYTEA_PREVIEW {
        s.push_str(&format!("… ({} bytes)", bytes.len()));
    }
    s
}

fn array_json(type_name: &str, value: PgValueRef<'_>) -> Option<Value> {
    let elem = type_name.strip_suffix("[]")?;
    Some(match elem {
        "BOOL" => json!(decode::<Vec<Option<bool>>>(value)?),
        "INT2" => json!(decode::<Vec<Option<i16>>>(value)?),
        "INT4" => json!(decode::<Vec<Option<i32>>>(value)?),
        "INT8" => Value::Array(
            decode::<Vec<Option<i64>>>(value)?
                .into_iter()
                .map(|v| v.map(int_json).unwrap_or(Value::Null))
                .collect(),
        ),
        "FLOAT4" => Value::Array(
            decode::<Vec<Option<f32>>>(value)?
                .into_iter()
                .map(|v| v.map(|f| float_json(f.into())).unwrap_or(Value::Null))
                .collect(),
        ),
        "FLOAT8" => Value::Array(
            decode::<Vec<Option<f64>>>(value)?
                .into_iter()
                .map(|v| v.map(float_json).unwrap_or(Value::Null))
                .collect(),
        ),
        "TEXT" | "VARCHAR" | "BPCHAR" | "NAME" => json!(decode::<Vec<Option<String>>>(value)?),
        "UUID" => json!(decode::<Vec<Option<uuid::Uuid>>>(value)?),
        // Date/time arrays are not decoded through chrono: its decoder panics
        // on infinity and out-of-range years. They fall back to text.
        _ => return None,
    })
}

/// Decodes the value at `index` of `row` into JSON.
pub fn decode_cell(row: &PgRow, index: usize) -> Value {
    let Ok(value) = row.try_get_raw(index) else {
        return Value::Null;
    };
    if value.is_null() {
        return Value::Null;
    }
    let type_info = value.type_info().into_owned();
    let type_name = type_info.name().to_ascii_uppercase();
    let raw_bytes = value.as_bytes().ok().map(<[u8]>::to_vec);

    let decoded: Option<Value> = match type_name.as_str() {
        "BOOL" => decode::<bool>(value).map(Value::Bool),
        "INT2" => decode::<i16>(value).map(|v| json!(v)),
        "INT4" => decode::<i32>(value).map(|v| json!(v)),
        "INT8" => decode::<i64>(value).map(int_json),
        "OID" => decode::<sqlx::postgres::types::Oid>(value).map(|v| json!(v.0)),
        "FLOAT4" => decode::<f32>(value).map(|v| float_json(v.into())),
        "FLOAT8" => decode::<f64>(value).map(float_json),
        "NUMERIC" => raw_bytes
            .as_deref()
            .and_then(numeric_to_string)
            .map(Value::String),
        "MONEY" => raw_bytes.as_deref().and_then(|b| {
            let cents = i64::from_be_bytes(b.try_into().ok()?);
            let sign = if cents < 0 { "-" } else { "" };
            Some(Value::String(format!(
                "{sign}{}.{:02}",
                (cents / 100).unsigned_abs(),
                (cents % 100).unsigned_abs()
            )))
        }),
        "TEXT" | "VARCHAR" | "BPCHAR" | "NAME" | "CHAR" | "XML" | "CITEXT" | "UNKNOWN" => {
            decode::<String>(value).map(Value::String)
        }
        "UUID" => decode::<uuid::Uuid>(value).map(|v| Value::String(v.to_string())),
        "JSON" | "JSONB" => decode::<Value>(value),
        // Dates and timestamps are decoded from their binary form with checked
        // arithmetic: sqlx's chrono decoders panic on ±infinity and on years
        // outside chrono's range, both of which PostgreSQL allows.
        "DATE" => raw_bytes
            .as_deref()
            .and_then(date_from_binary)
            .map(Value::String),
        "TIMESTAMP" => raw_bytes
            .as_deref()
            .and_then(|b| timestamp_from_binary(b, false))
            .map(Value::String),
        "TIMESTAMPTZ" => raw_bytes
            .as_deref()
            .and_then(|b| timestamp_from_binary(b, true))
            .map(Value::String),
        "TIME" => decode::<chrono::NaiveTime>(value).map(|v| Value::String(v.to_string())),
        "TIMETZ" => {
            decode::<PgTimeTz>(value).map(|v| Value::String(format!("{}{}", v.time, v.offset)))
        }
        "INTERVAL" => decode::<PgInterval>(value).map(|v| Value::String(interval_to_string(&v))),
        "BYTEA" => raw_bytes.as_deref().map(|b| Value::String(bytes_to_hex(b))),
        "INET" | "CIDR" => raw_bytes
            .as_deref()
            .and_then(inet_to_string)
            .map(Value::String),
        "VOID" => Some(Value::Null),
        name if name.ends_with("[]") => array_json(name, value),
        _ => None,
    };

    decoded.unwrap_or_else(|| fallback(&type_name, raw_bytes.as_deref()))
}

/// Values the decoder does not understand: special timestamps, enums and
/// other text-like types are shown as text; anything else as hex.
fn fallback(type_name: &str, bytes: Option<&[u8]>) -> Value {
    let Some(bytes) = bytes else {
        return Value::Null;
    };
    if matches!(type_name, "TIMESTAMP" | "TIMESTAMPTZ" | "DATE") {
        let special = match bytes.len() {
            8 => match i64::from_be_bytes(bytes.try_into().unwrap_or_default()) {
                i64::MAX => Some("infinity"),
                i64::MIN => Some("-infinity"),
                _ => None,
            },
            4 => match i32::from_be_bytes(bytes.try_into().unwrap_or_default()) {
                i32::MAX => Some("infinity"),
                i32::MIN => Some("-infinity"),
                _ => None,
            },
            _ => None,
        };
        if let Some(s) = special {
            return Value::String(s.into());
        }
    }
    match std::str::from_utf8(bytes) {
        Ok(text)
            if !text
                .chars()
                .any(|c| c.is_control() && c != '\n' && c != '\t') =>
        {
            Value::String(text.to_string())
        }
        _ => Value::String(bytes_to_hex(bytes)),
    }
}

/// Lowercase display name for a column type (`INT4[]` → `int4[]`).
pub fn type_label(type_info: &sqlx::postgres::PgTypeInfo) -> String {
    type_info.name().to_ascii_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn numeric(ndigits: i16, weight: i16, sign: u16, dscale: i16, digits: &[i16]) -> Vec<u8> {
        let mut b = Vec::new();
        b.extend_from_slice(&ndigits.to_be_bytes());
        b.extend_from_slice(&weight.to_be_bytes());
        b.extend_from_slice(&sign.to_be_bytes());
        b.extend_from_slice(&dscale.to_be_bytes());
        for d in digits {
            b.extend_from_slice(&d.to_be_bytes());
        }
        b
    }

    #[test]
    fn numeric_decoding() {
        // 12345.67 => digits [1, 2345, 6700], weight 1, dscale 2
        assert_eq!(
            numeric_to_string(&numeric(3, 1, 0, 2, &[1, 2345, 6700])).unwrap(),
            "12345.67"
        );
        // -0.001 => digits [10], weight -1, dscale 3
        assert_eq!(
            numeric_to_string(&numeric(1, -1, 0x4000, 3, &[10])).unwrap(),
            "-0.001"
        );
        // 0 with scale 2
        assert_eq!(
            numeric_to_string(&numeric(0, 0, 0, 2, &[])).unwrap(),
            "0.00"
        );
        // 100000000 => digits [1], weight 2
        assert_eq!(
            numeric_to_string(&numeric(1, 2, 0, 0, &[1])).unwrap(),
            "100000000"
        );
        // 0.00000123 => digits [123], weight -2, dscale 8
        assert_eq!(
            numeric_to_string(&numeric(1, -2, 0, 8, &[123])).unwrap(),
            "0.00000123"
        );
        assert_eq!(
            numeric_to_string(&numeric(0, 0, 0xC000, 0, &[])).unwrap(),
            "NaN"
        );
        assert_eq!(
            numeric_to_string(&numeric(0, 0, 0xD000, 0, &[])).unwrap(),
            "Infinity"
        );
        assert!(numeric_to_string(&[0, 1]).is_none());
    }

    #[test]
    fn int_precision() {
        assert_eq!(int_json(42), json!(42));
        assert_eq!(int_json(i64::MAX), json!("9223372036854775807"));
        assert_eq!(float_json(f64::NAN), json!("NaN"));
        assert_eq!(float_json(f64::NEG_INFINITY), json!("-Infinity"));
    }

    #[test]
    fn inet_and_bytes() {
        assert_eq!(
            inet_to_string(&[2, 32, 0, 4, 10, 0, 0, 1]).unwrap(),
            "10.0.0.1"
        );
        assert_eq!(
            inet_to_string(&[2, 24, 1, 4, 10, 0, 0, 0]).unwrap(),
            "10.0.0.0/24"
        );
        assert_eq!(bytes_to_hex(&[0xde, 0xad]), "\\xdead");
    }

    #[test]
    fn interval_format() {
        let iv = PgInterval {
            months: 14,
            days: 3,
            microseconds: 3_723_500_000,
        };
        assert_eq!(interval_to_string(&iv), "1 year 2 mons 3 days 01:02:03.5");
        let zero = PgInterval {
            months: 0,
            days: 0,
            microseconds: 0,
        };
        assert_eq!(interval_to_string(&zero), "00:00:00");
    }

    #[test]
    fn dates_and_timestamps_never_panic() {
        assert_eq!(date_from_binary(&0i32.to_be_bytes()).unwrap(), "2000-01-01");
        assert_eq!(
            date_from_binary(&i32::MAX.to_be_bytes()).unwrap(),
            "infinity"
        );
        assert_eq!(
            date_from_binary(&(-1i32).to_be_bytes()).unwrap(),
            "1999-12-31"
        );
        assert!(date_from_binary(&(i32::MAX - 1).to_be_bytes())
            .unwrap()
            .contains("out of range"));
        assert_eq!(
            timestamp_from_binary(&1_000_000i64.to_be_bytes(), true).unwrap(),
            "2000-01-01T00:00:01+00:00"
        );
        assert_eq!(
            timestamp_from_binary(&1_500_000i64.to_be_bytes(), false).unwrap(),
            "2000-01-01 00:00:01.500"
        );
        assert_eq!(
            timestamp_from_binary(&i64::MIN.to_be_bytes(), true).unwrap(),
            "-infinity"
        );
        assert!(timestamp_from_binary(&(i64::MAX - 1).to_be_bytes(), false)
            .unwrap()
            .contains("out of range"));
    }

    #[test]
    fn fallback_handles_infinity_and_binary() {
        assert_eq!(
            fallback("TIMESTAMPTZ", Some(&i64::MAX.to_be_bytes())),
            json!("infinity")
        );
        assert_eq!(fallback("MOOD", Some(b"happy")), json!("happy"));
        assert_eq!(fallback("POINT", Some(&[0, 1, 2])), json!("\\x000102"));
    }
}
