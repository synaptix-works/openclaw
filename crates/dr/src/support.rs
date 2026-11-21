//! verify_claim_support port: deterministic claim-support scoring.

use crate::util::*;
use regex::Regex;
use serde_json::json;
use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;

fn number_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\b\d+(?:\.\d+)?(?:%|x|X)?\b").unwrap())
}
fn year_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\b(?:19|20)\d{2}\b").unwrap())
}
fn entity_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\b[A-Z][a-z]+(?:\s+[A-Z][a-z]+)*\b").unwrap())
}
fn token_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\b[a-z]{4,}\b").unwrap())
}

const STOP_ENTITIES: &[&str] = &[
    "The", "This", "That", "These", "However", "Furthermore", "Moreover", "Additionally",
    "Therefore", "Nevertheless",
];

fn extract_tokens(text: &str) -> HashSet<String> {
    token_re()
        .find_iter(&text.to_lowercase())
        .map(|m| m.as_str().to_string())
        .collect()
}
fn extract_numbers(text: &str) -> HashSet<String> {
    number_re().find_iter(text).map(|m| m.as_str().to_string()).collect()
}
fn extract_years(text: &str) -> HashSet<String> {
    year_re().find_iter(text).map(|m| m.as_str().to_string()).collect()
}
fn extract_entities(text: &str) -> HashSet<String> {
    entity_re()
        .find_iter(text)
        .map(|m| m.as_str().to_string())
        .filter(|e| !STOP_ENTITIES.contains(&e.as_str()))
        .collect()
}

fn jaccard_over(a: &HashSet<String>, b: &HashSet<String>) -> f64 {
    if a.is_empty() {
        return 0.0;
    }
    a.intersection(b).count() as f64 / a.len() as f64
}

/// Returns (status, score, notes).
pub fn compute_support_score(claim_text: &str, quotes: &[String]) -> (String, f64, String) {
    if quotes.is_empty() {
        return ("unsupported".into(), 0.0, "no evidence linked".into());
    }
    let ct = extract_tokens(claim_text);
    let cn = extract_numbers(claim_text);
    let cy = extract_years(claim_text);
    let ce = extract_entities(claim_text);

    let mut best_score = 0.0f64;
    let mut best_notes: Vec<&str> = Vec::new();
    for quote in quotes {
        let et = extract_tokens(quote);
        let en = extract_numbers(quote);
        let ey = extract_years(quote);
        let ee = extract_entities(quote);

        let token_overlap = if ct.is_empty() { 0.0 } else { jaccard_over(&ct, &et) };
        let number_match = if cn.is_empty() { 1.0 } else { jaccard_over(&cn, &en) };
        let year_match = if cy.is_empty() { 1.0 } else { jaccard_over(&cy, &ey) };
        let entity_match = if ce.is_empty() { 1.0 } else { jaccard_over(&ce, &ee) };

        let score = 0.4 * token_overlap + 0.25 * number_match + 0.15 * year_match + 0.2 * entity_match;
        if score > best_score {
            best_score = score;
            best_notes = Vec::new();
            if token_overlap < 0.3 {
                best_notes.push("low lexical overlap");
            }
            if !cn.is_empty() && number_match < 0.5 {
                best_notes.push("number mismatch");
            }
            if !cy.is_empty() && year_match < 1.0 {
                best_notes.push("year mismatch");
            }
            if !ce.is_empty() && entity_match < 0.3 {
                best_notes.push("entity mismatch");
            }
        }
    }
    let status = if best_score >= 0.6 {
        "supported"
    } else if best_score >= 0.35 {
        "partial"
    } else {
        "needs_review"
    };
    let notes = if best_notes.is_empty() {
        "adequate overlap".to_string()
    } else {
        best_notes.join("; ")
    };
    (status.into(), (best_score * 1000.0).round() / 1000.0, notes)
}

pub fn cmd_verify(dir: &str, strict: bool) {
    let claims_path = artifact(dir, "claims.jsonl");
    let claims = read_jsonl(&claims_path);
    let evidence = read_jsonl(&artifact(dir, "evidence.jsonl"));

    let mut ev_by_source: HashMap<String, Vec<String>> = HashMap::new();
    let mut ev_by_id: HashMap<String, String> = HashMap::new();
    for ev in &evidence {
        let sid = get_str(ev, "source_id").unwrap_or("").to_string();
        let eid = get_str(ev, "evidence_id").unwrap_or("").to_string();
        let quote = get_str(ev, "quote").unwrap_or("").to_string();
        ev_by_source.entry(sid).or_default().push(quote.clone());
        ev_by_id.insert(eid, quote);
    }

    let unique = dedup(claims, "claim_id");
    let mut updated: Vec<Json> = Vec::new();
    for claim in unique {
        let mut claim = claim;
        let claim_type = get_str(&claim, "claim_type").unwrap_or("factual").to_string();
        let cited: Vec<String> = claim
            .get("cited_source_ids")
            .and_then(|v| v.as_array())
            .map(|a| a.iter().filter_map(|v| v.as_str().map(str::to_string)).collect())
            .unwrap_or_default();
        let evids: Vec<String> = claim
            .get("evidence_ids")
            .and_then(|v| v.as_array())
            .map(|a| a.iter().filter_map(|v| v.as_str().map(str::to_string)).collect())
            .unwrap_or_default();

        let mut quotes: Vec<String> = Vec::new();
        for eid in &evids {
            if let Some(q) = ev_by_id.get(eid) {
                quotes.push(q.clone());
            }
        }
        for sid in &cited {
            if let Some(qs) = ev_by_source.get(sid) {
                quotes.extend(qs.iter().cloned());
            }
        }

        if quotes.is_empty() && cited.is_empty() && evids.is_empty() {
            let st = if claim_type == "speculation" { "supported" } else { "unsupported" };
            claim.insert("support_status".into(), json!(st));
        } else if quotes.is_empty() {
            claim.insert("support_status".into(), json!("needs_review"));
        } else {
            let (status, score, notes) = compute_support_score(get_str(&claim, "text").unwrap_or(""), &quotes);
            claim.insert("support_status".into(), json!(status));
            claim.insert("_support_score".into(), json!(score));
            claim.insert("_support_notes".into(), json!(notes));
        }
        updated.push(claim);
    }
    let verified = updated.len();
    write_jsonl(&claims_path, &updated).ok();

    let mut counts: HashMap<String, i64> = HashMap::new();
    let mut factual_unsupported = 0i64;
    let mut total_factual = 0i64;
    for c in &updated {
        *counts
            .entry(get_str(c, "support_status").unwrap_or("unknown").to_string())
            .or_insert(0) += 1;
        if get_str(c, "claim_type") == Some("factual") {
            total_factual += 1;
            if get_str(c, "support_status") == Some("unsupported") {
                factual_unsupported += 1;
            }
        }
    }
    let passed = !(strict && factual_unsupported > 0);
    print_json(&json!({
        "status": if passed { "pass" } else { "fail" },
        "verified": verified,
        "support_status_counts": counts,
        "factual_unsupported": factual_unsupported,
        "total_factual": total_factual,
        "unsupported_rate": (factual_unsupported as f64 / total_factual.max(1) as f64 * 1000.0).round() / 1000.0,
    }));
    if !passed {
        std::process::exit(1);
    }
}

pub fn cmd_report(dir: &str) {
    let unique = dedup(read_jsonl(&artifact(dir, "claims.jsonl")), "claim_id");
    let mut status_counts: HashMap<String, i64> = HashMap::new();
    let mut type_counts: HashMap<String, i64> = HashMap::new();
    for c in &unique {
        *status_counts
            .entry(get_str(c, "support_status").unwrap_or("unknown").to_string())
            .or_insert(0) += 1;
        *type_counts
            .entry(get_str(c, "claim_type").unwrap_or("unknown").to_string())
            .or_insert(0) += 1;
    }
    let mut lines = vec![
        "# Claim Support Verification Report".to_string(),
        String::new(),
        format!("**Total claims:** {}", unique.len()),
        format!("**By type:** {type_counts:?}"),
        format!("**By status:** {status_counts:?}"),
        String::new(),
    ];
    let unsupported: Vec<&Json> = unique
        .iter()
        .filter(|c| {
            get_str(c, "claim_type") == Some("factual")
                && matches!(get_str(c, "support_status"), Some("unsupported") | Some("needs_review"))
        })
        .collect();
    if !unsupported.is_empty() {
        lines.push("## Unsupported/Review-needed Factual Claims".into());
        lines.push(String::new());
        for c in &unsupported {
            let text = get_str(c, "text").unwrap_or("");
            let snippet: String = text.chars().take(100).collect();
            lines.push(format!(
                "- [{}] `{}`: {}...",
                get_str(c, "support_status").unwrap_or("?"),
                get_str(c, "section_id").unwrap_or("?"),
                snippet
            ));
            if let Some(notes) = get_str(c, "_support_notes") {
                lines.push(format!("  Notes: {notes}"));
            }
        }
        lines.push(String::new());
    } else {
        lines.push("## All factual claims have adequate support.".into());
        lines.push(String::new());
    }
    println!("{}", lines.join("\n"));
}
