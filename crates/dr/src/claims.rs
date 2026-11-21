//! extract_claims port: decompose report sections into typed claims.

use crate::util::*;
use regex::Regex;
use serde_json::json;
use std::collections::HashSet;
use std::sync::OnceLock;

fn citation_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\[(\d+(?:,\s*\d+)*)\]").unwrap())
}

/// Split on whitespace that follows a sentence-ending char and precedes an
/// uppercase letter — equivalent to Python's split on `(?<=[.!?])\s+(?=[A-Z])`
/// (Rust regex has no look-around).
fn split_sentences(text: &str) -> Vec<String> {
    let chars: Vec<char> = text.chars().collect();
    let mut out = Vec::new();
    let mut start = 0usize;
    let mut i = 0usize;
    while i < chars.len() {
        if matches!(chars[i], '.' | '!' | '?') {
            let mut j = i + 1;
            let ws_start = j;
            while j < chars.len() && chars[j].is_whitespace() {
                j += 1;
            }
            if j > ws_start && j < chars.len() && chars[j].is_uppercase() {
                out.push(chars[start..i + 1].iter().collect());
                start = j;
                i = j;
                continue;
            }
        }
        i += 1;
    }
    out.push(chars[start..].iter().collect());
    out
}

fn section_res() -> &'static [Regex] {
    static RE: OnceLock<Vec<Regex>> = OnceLock::new();
    RE.get_or_init(|| {
        vec![
            Regex::new(r"(?i)^##\s+Executive\s+Summary").unwrap(),
            Regex::new(r"(?i)^##\s+Introduction").unwrap(),
            Regex::new(r"(?i)^##\s+Finding\s+(\d+)").unwrap(),
            Regex::new(r"(?i)^##\s+Synthesis").unwrap(),
            Regex::new(r"(?i)^##\s+Limitations").unwrap(),
            Regex::new(r"(?i)^##\s+Recommendations").unwrap(),
            Regex::new(r"(?i)^##\s+Conclusion").unwrap(),
            Regex::new(r"(?i)^##\s+(.+)").unwrap(),
        ]
    })
}

pub fn compute_claim_id(section_id: &str, text: &str) -> String {
    let payload = format!("{}{}", section_id, normalize_ws(text));
    sha256_hex(&payload)[..16].to_string()
}

fn classify_claim(text: &str, section_id: &str) -> &'static str {
    let lower = text.to_lowercase();
    for w in ["should", "recommend", "suggest", "advise", "consider"] {
        if lower.contains(w) {
            return "recommendation";
        }
    }
    for w in [
        "might",
        "could potentially",
        "it is possible",
        "may eventually",
        "hypothetically",
        "speculatively",
    ] {
        if lower.contains(w) {
            return "speculation";
        }
    }
    if matches!(section_id, "synthesis" | "conclusion" | "limitations") {
        for w in [
            "overall",
            "taken together",
            "collectively",
            "the evidence suggests",
            "this implies",
        ] {
            if lower.contains(w) {
                return "synthesis";
            }
        }
    }
    "factual"
}

pub fn parse_sections(markdown: &str) -> Vec<(String, String)> {
    let mut sections = Vec::new();
    let mut current_id = "preamble".to_string();
    let mut current_lines: Vec<&str> = Vec::new();
    let generic = &section_res()[7];
    for line in markdown.split('\n') {
        let mut matched = false;
        for (i, re) in section_res().iter().enumerate() {
            if let Some(m) = re.captures(line) {
                if !current_lines.is_empty() {
                    sections.push((current_id.clone(), current_lines.join("\n")));
                }
                current_id = match i {
                    0 => "executive_summary".into(),
                    1 => "introduction".into(),
                    2 => format!("finding_{}", &m[1]),
                    3 => "synthesis".into(),
                    4 => "limitations".into(),
                    5 => "recommendations".into(),
                    6 => "conclusion".into(),
                    _ => {
                        let s: String = m[1]
                            .trim()
                            .to_lowercase()
                            .chars()
                            .map(|c| if c.is_alphanumeric() { c } else { '_' })
                            .collect();
                        let collapsed: String = s
                            .split('_')
                            .filter(|p| !p.is_empty())
                            .collect::<Vec<_>>()
                            .join("_");
                        collapsed.chars().take(30).collect()
                    }
                };
                current_lines = Vec::new();
                matched = true;
                break;
            }
        }
        if !matched {
            current_lines.push(line);
        }
    }
    let _ = generic;
    if !current_lines.is_empty() {
        sections.push((current_id, current_lines.join("\n")));
    }
    sections
}

pub fn extract_sentences(text: &str) -> Vec<String> {
    let bullet = Regex::new(r"(?m)^[-*]\s+").unwrap();
    let bold = Regex::new(r"\*\*([^*]+)\*\*").unwrap();
    let italic = Regex::new(r"\*([^*]+)\*").unwrap();
    let t = bullet.replace_all(text, "");
    let t = bold.replace_all(&t, "$1");
    let t = italic.replace_all(&t, "$1");
    split_sentences(&t)
        .into_iter()
        .map(|s| s.trim().to_string())
        .filter(|s| s.len() > 30 && !s.starts_with('#') && !s.starts_with('|'))
        .collect()
}

pub fn cmd_extract(dir: &str, report: &str) {
    let markdown = match std::fs::read_to_string(report) {
        Ok(m) => m,
        Err(_) => die(&format!("Report not found: {report}")),
    };
    let claims_path = artifact(dir, "claims.jsonl");
    let mut existing: HashSet<String> = read_jsonl(&claims_path)
        .iter()
        .filter_map(|r| get_str(r, "claim_id").map(str::to_string))
        .collect();
    let mut added = 0;
    let mut skipped = 0;
    for (section_id, content) in parse_sections(&markdown) {
        if section_id == "preamble" {
            continue;
        }
        for sentence in extract_sentences(&content) {
            let claim_id = compute_claim_id(&section_id, &sentence);
            if existing.contains(&claim_id) {
                skipped += 1;
                continue;
            }
            let mut nums = Vec::new();
            for m in citation_re().captures_iter(&sentence) {
                for n in m[1].split(',') {
                    if let Ok(v) = n.trim().parse::<i64>() {
                        nums.push(json!(v));
                    }
                }
            }
            let claim = json!({
                "claim_id": claim_id,
                "section_id": section_id,
                "text": sentence,
                "claim_type": classify_claim(&sentence, &section_id),
                "cited_source_ids": [],
                "evidence_ids": [],
                "support_status": "unverified",
                "extracted_at": utc_now_iso(),
                "_citation_numbers": nums,
            });
            let obj = claim.as_object().unwrap().clone();
            append_jsonl(&claims_path, &obj).ok();
            existing.insert(claim_id);
            added += 1;
        }
    }
    print_compact(&json!({
        "status": "ok", "claims_added": added, "claims_skipped": skipped,
        "total_claims": existing.len(),
    }));
}

pub fn cmd_add(dir: &str, json_arg: &str) {
    let data: Json = serde_json::from_str(json_arg).unwrap_or_else(|_| die("invalid --json"));
    let section_id = get_str(&data, "section_id").unwrap_or("unknown").to_string();
    let text = get_str(&data, "text").unwrap_or("").to_string();
    if text.is_empty() {
        die("text is required");
    }
    let claim_id = compute_claim_id(&section_id, &text);
    let path = artifact(dir, "claims.jsonl");
    for row in read_jsonl(&path) {
        if get_str(&row, "claim_id") == Some(claim_id.as_str()) {
            print_compact(&json!({"status":"duplicate","claim_id":claim_id}));
            return;
        }
    }
    let ct = get_str(&data, "claim_type").unwrap_or("factual");
    let ct = if ["factual", "synthesis", "recommendation", "speculation"].contains(&ct) {
        ct
    } else {
        "factual"
    };
    let mut claim = data.clone();
    for (k, v) in [
        ("claim_id", json!(claim_id)),
        ("section_id", json!(section_id)),
        ("text", json!(text)),
        ("claim_type", json!(ct)),
        ("support_status", json!("unverified")),
        ("extracted_at", json!(utc_now_iso())),
    ] {
        claim.insert(k.into(), v);
    }
    claim.entry("cited_source_ids").or_insert(json!([]));
    claim.entry("evidence_ids").or_insert(json!([]));
    append_jsonl(&path, &claim).ok();
    print_compact(&json!({"status":"added","claim_id":claim_id}));
}

pub fn cmd_list(dir: &str, section: Option<&str>, ctype: Option<&str>, status: Option<&str>) {
    let mut rows = read_jsonl(&artifact(dir, "claims.jsonl"));
    if let Some(s) = section {
        rows.retain(|r| get_str(r, "section_id") == Some(s));
    }
    if let Some(t) = ctype {
        rows.retain(|r| get_str(r, "claim_type") == Some(t));
    }
    if let Some(s) = status {
        rows.retain(|r| get_str(r, "support_status") == Some(s));
    }
    let unique = dedup(rows, "claim_id");
    print_json(&json!({"count": unique.len(), "claims": unique}));
}

pub fn cmd_stats(dir: &str) {
    let unique = dedup(read_jsonl(&artifact(dir, "claims.jsonl")), "claim_id");
    let mut by_type = serde_json::Map::new();
    let mut by_status = serde_json::Map::new();
    let mut by_section = serde_json::Map::new();
    for r in &unique {
        for (map, key, def) in [
            (&mut by_type, "claim_type", "unknown"),
            (&mut by_status, "support_status", "unknown"),
            (&mut by_section, "section_id", "unknown"),
        ] {
            let k = get_str(r, key).unwrap_or(def).to_string();
            let e = map.entry(k).or_insert(json!(0));
            *e = json!(e.as_i64().unwrap_or(0) + 1);
        }
    }
    print_json(&json!({
        "total": unique.len(),
        "by_type": by_type, "by_status": by_status, "by_section": by_section,
    }));
}
