//! verify_citations port: DOI/URL checks + hallucination pattern detection.
//! Network checks use ureq (10s timeouts); --offline skips them.

use crate::util::*;
use regex::Regex;
use serde_json::{json, Value};
use std::collections::HashSet;
use std::path::Path;

#[derive(Default, Clone)]
struct Entry {
    num: String,
    raw: String,
    year: Option<i64>,
    title: Option<String>,
    doi: Option<String>,
    url: Option<String>,
}

const SUSPICIOUS: &[(&str, &str)] = &[
    (
        r"(?i)^(A |An |The )?(Study|Analysis|Review|Survey|Investigation) (of|on|into)",
        "Generic academic title pattern",
    ),
    (
        r"(?i)^(Recent|Current|Modern|Contemporary) (Advances|Developments|Trends) in",
        "Generic 'advances' title pattern",
    ),
    (
        r"^[A-Z][a-z]+ [A-Z][a-z]+: A (Comprehensive|Complete|Systematic) (Review|Analysis|Guide)$",
        "Too perfect, templated structure",
    ),
];

fn extract_bibliography(content: &str) -> Vec<Entry> {
    let re = Regex::new(r"(?is)##\s*Bibliography(.*?)(?:##|\z)").unwrap();
    let Some(cap) = re.captures(content) else { return Vec::new() };
    let bib = &cap[1];
    let entry_re = Regex::new(r"(?m)^\[(\d+)\]\s+(.+)$").unwrap();
    let year_re = Regex::new(r"\((\d{4})\)").unwrap();
    let title_re = Regex::new("\"([^\"]+)\"").unwrap();
    let doi_re = Regex::new(r"doi\.org/(10\.\S+)").unwrap();
    let url_re = Regex::new(r#"https?://[^\s\)]+"#).unwrap();

    let mut entries = Vec::new();
    let mut current: Option<Entry> = None;
    for line in bib.trim().split('\n') {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if let Some(m) = entry_re.captures(line) {
            if let Some(e) = current.take() {
                entries.push(e);
            }
            let rest = m[2].to_string();
            current = Some(Entry {
                num: m[1].to_string(),
                raw: rest.clone(),
                year: year_re.captures(&rest).and_then(|c| c[1].parse().ok()),
                title: title_re.captures(&rest).map(|c| c[1].to_string()),
                doi: doi_re.captures(&rest).map(|c| c[1].to_string()),
                url: url_re.find(&rest).map(|m| m.as_str().to_string()),
            });
        } else if let Some(e) = current.as_mut() {
            e.raw.push(' ');
            e.raw.push_str(line);
        }
    }
    if let Some(e) = current {
        entries.push(e);
    }
    entries
}

fn verify_doi(doi: &str) -> Result<Value, String> {
    let url = format!("https://doi.org/{}", url_encode(doi));
    let resp = ureq::get(&url)
        .set("Accept", "application/vnd.citationstyles.csl+json")
        .timeout(std::time::Duration::from_secs(10))
        .call();
    match resp {
        Ok(r) => {
            let data: Value = r.into_json().map_err(|e| e.to_string())?;
            Ok(json!({
                "title": data.get("title").and_then(|v| v.as_str()).unwrap_or(""),
                "year": data.pointer("/issued/date-parts/0/0").and_then(|v| v.as_i64()),
                "authors": data.get("author").and_then(|a| a.as_array()).map(|arr| {
                    arr.iter().map(|a| format!("{} {}",
                        a.get("family").and_then(|v| v.as_str()).unwrap_or(""),
                        a.get("given").and_then(|v| v.as_str()).unwrap_or(""))).collect::<Vec<_>>()
                }).unwrap_or_default(),
                "venue": data.get("container-title").and_then(|v| v.as_str()).unwrap_or(""),
            }))
        }
        Err(ureq::Error::Status(404, _)) => Err("DOI not found (404)".into()),
        Err(ureq::Error::Status(code, _)) => Err(format!("HTTP {code}")),
        Err(e) => Err(e.to_string()),
    }
}

fn verify_url(url: &str) -> Result<(), String> {
    let resp = ureq::head(url)
        .set("User-Agent", "Mozilla/5.0 (Research Citation Verifier)")
        .timeout(std::time::Duration::from_secs(10))
        .call();
    match resp {
        Ok(_) => Ok(()),
        Err(ureq::Error::Status(code, _)) => Err(format!("HTTP {code}")),
        Err(e) => Err(format!("URL error: {e}")),
    }
}

fn detect_hallucination(entry: &Entry) -> Vec<String> {
    let mut issues = Vec::new();
    let Some(title) = &entry.title else { return issues };
    for (pat, desc) in SUSPICIOUS {
        if Regex::new(pat).unwrap().is_match(title) {
            issues.push(format!("Suspicious title pattern: {desc}"));
        }
    }
    let generic = ["overview", "introduction", "guide", "handbook", "manual"];
    if generic.iter().any(|w| title.to_lowercase().contains(w)) && title.split_whitespace().count() < 5 {
        issues.push("Very generic short title".into());
    }
    if ["tbd", "todo", "placeholder", "example"]
        .iter()
        .any(|x| title.to_lowercase().contains(x))
    {
        issues.push("Placeholder text in title".into());
    }
    if let Some(year) = entry.year {
        let cy = current_year();
        if year >= cy - 1 && entry.doi.is_none() && entry.url.is_none() {
            issues.push(format!("Recent year ({year}) with no verification method"));
        }
        if year > cy {
            issues.push(format!("Future year: {year} (current: {cy})"));
        }
        if year < 2000
            && ["ai", "llm", "gpt", "transformer"]
                .iter()
                .any(|w| title.to_lowercase().contains(w))
        {
            issues.push(format!("Anachronistic: pre-2000 ({year}) citation mentioning modern AI terms"));
        }
    }
    issues
}

fn title_similarity(a: &str, b: &str) -> f64 {
    let norm = |s: &str| -> HashSet<String> {
        Regex::new(r"[^\w\s]")
            .unwrap()
            .replace_all(&s.to_lowercase(), " ")
            .split_whitespace()
            .map(str::to_string)
            .collect()
    };
    let (w1, w2) = (norm(a), norm(b));
    if w1.is_empty() || w2.is_empty() {
        return 0.0;
    }
    w1.intersection(&w2).count() as f64 / w1.union(&w2).count() as f64
}

fn verify_entry(entry: &Entry, offline: bool) -> Json {
    let mut issues: Vec<String> = Vec::new();
    let mut methods: Vec<String> = Vec::new();
    let mut status = "unknown".to_string();
    let mut metadata = json!({});

    let hallu = detect_hallucination(entry);
    if !hallu.is_empty() {
        issues.extend(hallu);
        status = "suspicious".into();
    }

    if let Some(doi) = &entry.doi {
        if offline {
            issues.push("DOI check skipped (offline)".into());
            if status == "unknown" {
                status = "unverified".into();
            }
        } else {
            match verify_doi(doi) {
                Ok(meta) => {
                    methods.push("DOI".into());
                    status = "verified".into();
                    if let (Some(t), Some(mt)) = (&entry.title, meta.get("title").and_then(|v| v.as_str())) {
                        let sim = title_similarity(t, mt);
                        if sim < 0.5 {
                            issues.push(format!("Title mismatch (similarity: {:.0}%)", sim * 100.0));
                            status = "suspicious".into();
                        }
                    }
                    if let (Some(y), Some(my)) = (entry.year, meta.get("year").and_then(|v| v.as_i64())) {
                        if y != my {
                            issues.push(format!("Year mismatch: report says {y}, DOI says {my}"));
                            status = "suspicious".into();
                        }
                    }
                    metadata = meta;
                }
                Err(e) => {
                    status = "unverified".into();
                    issues.push(format!("DOI resolution failed: {e}"));
                }
            }
        }
    }

    if entry.url.is_some() && status != "verified" {
        if offline {
            methods.push("URL(offline-skip)".into());
        } else {
            match verify_url(entry.url.as_ref().unwrap()) {
                Ok(()) => {
                    methods.push("URL".into());
                    if ["unknown", "no_doi", "unverified"].contains(&status.as_str()) {
                        status = "url_verified".into();
                    }
                }
                Err(e) => issues.push(format!("URL check failed: {e}")),
            }
        }
    }

    if entry.doi.is_none() && entry.url.is_none() {
        issues.push("No DOI or URL - cannot verify".into());
        status = "suspicious".into();
    }

    let mut obj = serde_json::Map::new();
    obj.insert("num".into(), json!(entry.num));
    obj.insert("status".into(), json!(status));
    obj.insert("issues".into(), json!(issues));
    obj.insert("metadata".into(), metadata);
    obj.insert("verification_methods".into(), json!(methods));
    obj
}

pub fn cmd_verify(report: &str, strict: bool, offline: bool) {
    let path = Path::new(report);
    if !path.exists() {
        eprintln!("ERROR: Report file not found: {report}");
        std::process::exit(1);
    }
    let content = std::fs::read_to_string(path).unwrap_or_else(|e| {
        eprintln!("ERROR: Cannot read report: {e}");
        std::process::exit(1);
    });

    println!("\n{}\nCITATION VERIFICATION: {}\n{}", "=".repeat(60), path.file_name().unwrap_or_default().to_string_lossy(), "=".repeat(60));
    if offline {
        println!("OFFLINE MODE: DOI/URL network checks skipped\n");
    }

    let entries = extract_bibliography(&content);
    if entries.is_empty() {
        println!("No bibliography entries found\n");
        std::process::exit(1);
    }
    println!("Found {} citations\n", entries.len());

    let results: Vec<Json> = entries.iter().map(|e| verify_entry(e, offline)).collect();

    let count = |st: &str| results.iter().filter(|r| get_str(r, "status") == Some(st)).count();
    let verified = count("verified");
    let url_verified = count("url_verified");
    let suspicious = count("suspicious");
    let unverified: Vec<&Json> = results
        .iter()
        .filter(|r| matches!(get_str(r, "status"), Some("unverified") | Some("no_doi") | Some("unknown")))
        .collect();

    println!("\n{}\nVERIFICATION SUMMARY\n{}\n", "=".repeat(60), "=".repeat(60));
    println!("DOI Verified: {verified}/{}", results.len());
    println!("URL Verified: {url_verified}/{}", results.len());
    println!("Suspicious: {suspicious}/{}", results.len());
    println!("Unverified: {}/{}\n", unverified.len(), results.len());

    if suspicious > 0 {
        println!("SUSPICIOUS CITATIONS (Manual Review Needed):");
        for r in results.iter().filter(|r| get_str(r, "status") == Some("suspicious")) {
            println!("\n  [{}]", get_str(r, "num").unwrap_or("?"));
            if let Some(arr) = r.get("issues").and_then(|v| v.as_array()) {
                for i in arr {
                    println!("    - {}", i.as_str().unwrap_or(""));
                }
            }
        }
        println!();
    }
    if !unverified.is_empty() {
        println!("UNVERIFIED CITATIONS (Could not check):");
        for r in &unverified {
            let first = r
                .get("issues")
                .and_then(|v| v.as_array())
                .and_then(|a| a.first())
                .and_then(|v| v.as_str())
                .unwrap_or("Unknown");
            println!("  [{}] {}", get_str(r, "num").unwrap_or("?"), first);
        }
        println!();
    }

    let total_verified = verified + url_verified;
    let mut passed = true;
    if suspicious > 0 {
        println!("WARNING: Suspicious citations detected");
        if strict {
            println!("  STRICT MODE: Failing due to suspicious citations");
            passed = false;
        }
    }
    if strict && !unverified.is_empty() {
        println!("STRICT MODE: Unverified citations found");
        passed = false;
    }
    if (total_verified as f64 / results.len() as f64) < 0.5 {
        println!("WARNING: Less than 50% citations verified");
    } else if passed {
        println!("CITATION VERIFICATION PASSED");
    }
    std::process::exit(if passed { 0 } else { 1 });
}
