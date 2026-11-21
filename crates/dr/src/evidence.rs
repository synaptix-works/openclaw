//! evidence_store port: append-only evidence persistence.

use crate::util::*;
use serde_json::json;
use std::fs;

const VALID_TYPES: &[&str] = &[
    "direct_quote",
    "paraphrase",
    "data_point",
    "figure_reference",
    "methodology",
];

pub fn compute_evidence_id(source_id: &str, quote: &str, locator: Option<&str>) -> String {
    let payload = format!("{}{}{}", source_id, normalize_ws(quote), locator.unwrap_or(""));
    sha256_hex(&payload)[..16].to_string()
}

pub fn cmd_init(dir: &str) {
    fs::create_dir_all(dir).ok();
    let path = artifact(dir, "evidence.jsonl");
    if !path.exists() {
        fs::write(&path, "").ok();
    }
    print_compact(&json!({"status":"ok","path":path}));
}

pub fn cmd_add(dir: &str, json_arg: &str) {
    let data: Json = serde_json::from_str(json_arg).unwrap_or_else(|_| die("invalid --json"));
    let source_id = get_str(&data, "source_id").unwrap_or("").to_string();
    let quote = get_str(&data, "quote").unwrap_or("").to_string();
    if source_id.is_empty() || quote.is_empty() {
        die("source_id and quote are required");
    }
    let locator = get_str(&data, "locator").map(str::to_string);
    let evidence_id = compute_evidence_id(&source_id, &quote, locator.as_deref());
    let path = artifact(dir, "evidence.jsonl");
    for row in read_jsonl(&path) {
        if get_str(&row, "evidence_id") == Some(evidence_id.as_str()) {
            print_compact(&json!({"status":"duplicate","evidence_id":evidence_id}));
            return;
        }
    }
    let et = get_str(&data, "evidence_type").unwrap_or("direct_quote");
    let et = if VALID_TYPES.contains(&et) { et } else { "direct_quote" };
    let mut row = data.clone();
    for (k, v) in [
        ("evidence_id", json!(evidence_id)),
        ("source_id", json!(source_id)),
        ("quote", json!(quote)),
        ("evidence_type", json!(et)),
        ("captured_at", json!(utc_now_iso())),
    ] {
        row.insert(k.into(), v);
    }
    match locator {
        Some(l) => row.insert("locator".into(), json!(l)),
        None => row.insert("locator".into(), serde_json::Value::Null),
    };
    append_jsonl(&path, &row).ok();
    print_compact(&json!({"status":"added","evidence_id":evidence_id,"source_id":source_id}));
}

pub fn cmd_list(dir: &str, source_id: Option<&str>) {
    let mut rows = read_jsonl(&artifact(dir, "evidence.jsonl"));
    if let Some(sid) = source_id {
        rows.retain(|r| get_str(r, "source_id") == Some(sid));
    }
    let unique = dedup(rows, "evidence_id");
    print_json(&json!({"count": unique.len(), "evidence": unique}));
}

pub fn cmd_export(dir: &str) {
    let unique = dedup(read_jsonl(&artifact(dir, "evidence.jsonl")), "evidence_id");
    print_json(&json!(unique));
}
