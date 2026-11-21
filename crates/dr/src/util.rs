//! Shared helpers: JSONL IO, hashing, whitespace normalization, UTC time.

use serde_json::{Map, Value};
use sha2::{Digest, Sha256};
use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::{SystemTime, UNIX_EPOCH};

pub type Json = Map<String, Value>;


/// Read a JSONL file into a Vec of objects. Missing file = empty vec.
pub fn read_jsonl(path: &Path) -> Vec<Json> {
    let mut rows = Vec::new();
    let Ok(f) = fs::File::open(path) else { return rows };
    for line in BufReader::new(f).lines().map_while(Result::ok) {
        let line = line.trim().to_string();
        if line.is_empty() {
            continue;
        }
        if let Ok(Value::Object(obj)) = serde_json::from_str::<Value>(&line) {
            rows.push(obj);
        }
    }
    rows
}

pub fn append_jsonl(path: &Path, obj: &Json) -> std::io::Result<()> {
    let mut f = OpenOptions::new().create(true).append(true).open(path)?;
    writeln!(f, "{}", Value::Object(obj.clone()))?;
    Ok(())
}

pub fn write_jsonl(path: &Path, rows: &[Json]) -> std::io::Result<()> {
    let mut out = String::new();
    for r in rows {
        out.push_str(&Value::Object(r.clone()).to_string());
        out.push('\n');
    }
    fs::write(path, out)
}

pub fn sha256_hex(s: &str) -> String {
    let mut h = Sha256::new();
    h.update(s.as_bytes());
    hex(&h.finalize())
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

pub fn normalize_ws(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ").to_lowercase()
}

pub fn get_str<'a>(obj: &'a Json, key: &str) -> Option<&'a str> {
    obj.get(key).and_then(|v| v.as_str())
}

/// Current UTC time as RFC-3339-ish string (seconds precision), e.g. 2026-11-21T12:34:56+00:00
/// Python's `datetime.now(timezone.utc).isoformat()` equivalent.
pub fn utc_now_iso() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    iso_from_epoch(secs)
}

pub fn unix_days_now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| (d.as_secs() / 86400) as i64)
        .unwrap_or(0)
}

/// days -> (year, month, day); Howard Hinnant civil_from_days.
pub fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

pub fn days_from_civil(y: i64, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = (y - era * 400) as u64;
    let mp = if m > 2 { m - 3 } else { m + 9 } as u64;
    let doy = (153 * mp + 2) / 5 + (d - 1) as u64;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe as i64 - 719_468
}

fn iso_from_epoch(secs: i64) -> String {
    let days = secs.div_euclid(86400);
    let rem = secs.rem_euclid(86400);
    let (y, m, d) = civil_from_days(days);
    let (hh, mm, ss) = (rem / 3600, (rem % 3600) / 60, rem % 60);
    format!("{y:04}-{m:02}-{d:02}T{hh:02}:{mm:02}:{ss:02}+00:00")
}

/// Parse an ISO-8601-ish date/datetime to unix days (date part only).
pub fn parse_date_days(s: &str) -> Option<i64> {
    let s = s.trim();
    let re = re_parse_date();
    let c = re.captures(s)?;
    let y: i64 = c[1].parse().ok()?;
    let m: u32 = c[2].parse().ok()?;
    let d: u32 = c[3].parse().ok()?;
    Some(days_from_civil(y, m, d))
}

pub fn current_year() -> i64 {
    civil_from_days(unix_days_now()).0
}

fn re_parse_date() -> &'static regex::Regex {
    static RE: OnceLock<regex::Regex> = OnceLock::new();
    RE.get_or_init(|| regex::Regex::new(r"(\d{4})-(\d{2})-(\d{2})").unwrap())
}

/// Minimal URL parse: returns (scheme_lower, host_lower, path, query).
pub fn parse_url(url: &str) -> (String, String, String, String) {
    static RE: OnceLock<regex::Regex> = OnceLock::new();
    let re = RE.get_or_init(|| {
        regex::Regex::new(r"(?i)^(?:([a-z][a-z0-9+.-]*)://)?(?:[^@/]*@)?([a-z0-9.\-]+(?::\d+)?)?(/[^?#]*)?(?:\?([^#]*))?").unwrap()
    });
    let c = re.captures(url.trim());
    match c {
        Some(c) => (
            c.get(1).map(|m| m.as_str().to_lowercase()).unwrap_or_default(),
            c.get(2).map(|m| m.as_str().to_lowercase()).unwrap_or_default(),
            c.get(3).map(|m| m.as_str().to_string()).unwrap_or_default(),
            c.get(4).map(|m| m.as_str().to_string()).unwrap_or_default(),
        ),
        None => (String::new(), String::new(), String::new(), String::new()),
    }
}

/// Percent-encode a string for a URL path segment (keep DOI-safe chars).
pub fn url_encode(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' | b'/' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

pub fn print_json(v: &Value) {
    println!("{}", serde_json::to_string_pretty(v).unwrap_or_else(|_| v.to_string()));
}

pub fn print_compact(v: &Value) {
    println!("{v}");
}

pub fn die(msg: &str) -> ! {
    eprintln!("{}", serde_json::json!({"error": msg}));
    std::process::exit(1);
}

/// Deduplicate rows by an id key, preserving first-seen order.
pub fn dedup(rows: Vec<Json>, key: &str) -> Vec<Json> {
    let mut seen = std::collections::HashSet::new();
    rows.into_iter()
        .filter(|r| match get_str(r, key) {
            Some(id) => seen.insert(id.to_string()),
            None => true,
        })
        .collect()
}

/// Find the run-directory path for a named artifact.
pub fn artifact(dir: &str, name: &str) -> PathBuf {
    PathBuf::from(dir).join(name)
}
