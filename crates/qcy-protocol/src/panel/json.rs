//! Lenient accessors for control panel JSON.
//!
//! The app reads panels with Android's `org.json` `optInt`/`optString`, which
//! coerce freely: `10`, `10.0` and `"10"` are all the integer 10, and a
//! missing or malformed value falls back to a default. Real panels rely on
//! this (one has every number as a string, others have `103.0`), so these
//! helpers do the same instead of using strict typed deserialisation.

use serde_json::Value;

/// `optInt`: integer value of `key`, coercing floats and numeric strings.
pub fn int(v: &Value, key: &str) -> Option<i64> {
    as_int(v.get(key)?)
}

/// Integer value of a JSON value, coercing like `optInt`.
pub fn as_int(v: &Value) -> Option<i64> {
    match v {
        Value::Number(n) => n.as_i64().or_else(|| n.as_f64().map(|f| f as i64)),
        Value::String(s) => {
            let s = s.trim();
            s.parse::<i64>()
                .ok()
                .or_else(|| s.parse::<f64>().ok().map(|f| f as i64))
        }
        Value::Bool(b) => Some(i64::from(*b)),
        _ => None,
    }
}

/// Floating point value of `key` (numbers and numeric strings).
pub fn float(v: &Value, key: &str) -> Option<f64> {
    match v.get(key)? {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => s.trim().parse().ok(),
        _ => None,
    }
}

/// `optBoolean`: `true`, `"true"`, `1` and `"1"` are true.
pub fn boolean(v: &Value, key: &str) -> bool {
    match v.get(key) {
        Some(Value::Bool(b)) => *b,
        Some(Value::String(s)) => s.eq_ignore_ascii_case("true") || s.trim() == "1",
        Some(other) => as_int(other) == Some(1),
        None => false,
    }
}

/// Non-empty string value of `key` (numbers are rendered as text).
pub fn string(v: &Value, key: &str) -> Option<String> {
    let s = match v.get(key)? {
        Value::String(s) => s.trim().to_string(),
        Value::Number(n) => n.to_string(),
        _ => return None,
    };
    (!s.is_empty()).then_some(s)
}

/// The array under `key`, or an empty slice.
pub fn array<'a>(v: &'a Value, key: &str) -> &'a [Value] {
    v.get(key)
        .and_then(Value::as_array)
        .map_or(&[], Vec::as_slice)
}

/// A comma separated list such as `"31,62,125"` or `"0.7, 1, 1.5"`.
pub fn csv(v: &Value, key: &str) -> Vec<String> {
    string(v, key)
        .map(|s| {
            s.split(',')
                .map(|p| p.trim().to_string())
                .filter(|p| !p.is_empty())
                .collect()
        })
        .unwrap_or_default()
}

/// A comma separated list of numbers; entries that do not parse are dropped.
pub fn csv_numbers(v: &Value, key: &str) -> Vec<f64> {
    csv(v, key).iter().filter_map(|p| p.parse().ok()).collect()
}

/// Parses a frequency label: `"125"`, `"1k"`, `"1.5K"`, `"16k"`.
pub fn frequency(label: &str) -> Option<u32> {
    let label = label.trim();
    let (number, scale) = match label.strip_suffix(['k', 'K']) {
        Some(n) => (n, 1000.0),
        None => (label, 1.0),
    };
    let hz = number.trim().parse::<f64>().ok()? * scale;
    (hz > 0.0 && hz < 100_000.0).then(|| hz.round() as u32)
}

/// Decodes a hex string such as `"FF03090101"` (whitespace ignored).
pub fn hex(s: &str) -> Option<Vec<u8>> {
    let s: String = s.chars().filter(|c| !c.is_whitespace()).collect();
    if s.is_empty() || s.len() % 2 != 0 {
        return None;
    }
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).ok())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn coerces_like_opt_int() {
        let v = json!({"a": 10, "b": "10", "c": 103.0, "d": "x", "e": " 7 "});
        assert_eq!(int(&v, "a"), Some(10));
        assert_eq!(int(&v, "b"), Some(10));
        assert_eq!(int(&v, "c"), Some(103));
        assert_eq!(int(&v, "d"), None);
        assert_eq!(int(&v, "e"), Some(7));
        assert_eq!(int(&v, "missing"), None);
    }

    #[test]
    fn lists_and_hex() {
        let v = json!({"f": "31, 62,1k,16K", "q": "0.7,x,1"});
        assert_eq!(csv(&v, "f"), ["31", "62", "1k", "16K"]);
        assert_eq!(csv_numbers(&v, "q"), [0.7, 1.0]);
        assert_eq!(frequency("1k"), Some(1000));
        assert_eq!(frequency("1.5K"), Some(1500));
        assert_eq!(frequency("abc"), None);
        assert_eq!(hex("FF03090101"), Some(vec![0xFF, 3, 9, 1, 1]));
        assert_eq!(hex("F"), None);
    }
}
