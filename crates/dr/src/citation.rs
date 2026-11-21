//! citation_manager port: run manifest + stable source identity.

use crate::util::*;
use serde_json::{json, Value};
use std::fs;
use std::path::Path;
use std::sync::OnceLock;

fn doi_re() -> &'static regex::Regex {
    static RE: OnceLock<regex::Regex> = OnceLock::new();
    RE.get_or_init(|| {
        regex::Regex::new(r"(?i)(?:https?://(?:dx\.)?doi\.org/|doi:)(10\.\d{4,}/\S+)").unwrap()
    })
}

fn arxiv_re() -> &'static regex::Regex {
    static RE: OnceLock<regex::Regex> = OnceLock::new();
    RE.get_or_init(|| {
        regex::Regex::new(r"(?i)(?:https?://arxiv\.org/abs/|arxiv:)(\d{4}\.\d{4,}(?:v\d+)?)").unwrap()
    })
}

const TRACKING_PARAMS: &[&str] = &[
    "utm_source", "utm_medium", "utm_campaign", "utm_term", "utm_content", "ref", "source",
    "fbclid", "gclid", "mc_cid", "mc_eid",
];

/// Priority: DOI > arXiv > normalized URL.
pub fn canonicalize_locator(raw: &str) -> String {
    if let Some(m) = doi_re().captures(raw) {
        return format!("doi:{}", m[1].trim_end_matches('.'));
    }
    if let Some(m) = arxiv_re().captures(raw) {
        return format!("arxiv:{}", &m[1]);
    }
    let (scheme, host, path, query) = parse_url(raw);
    let scheme = if scheme.is_empty() { "https".into() } else { scheme };
    let path = path.trim_end_matches('/').to_string();
    let query = if query.is_empty() {
        String::new()
    } else {
        let mut keep: Vec<&str> = query
            .split('&')
            .filter(|p| {
                let k = p.split('=').next().unwrap_or("").to_lowercase();
                !TRACKING_PARAMS.contains(&k.as_str())
            })
            .collect();
        keep.sort();
        keep.join("&")
    };
    let mut out = format!("{scheme}://{host}{path}");
    if !query.is_empty() {
        out.push('?');
        out.push_str(&query);
    }
    out
}

pub fn compute_source_id(canonical: &str) -> String {
    sha256_hex(canonical)[..16].to_string()
}

/// Default report dir: `$DR_RESEARCH_DIR/<Topic>_Research_<YYYYMMDD>`,
/// else `~/Documents/<Topic>_Research_<YYYYMMDD>`. Point DR_RESEARCH_DIR at an
/// Obsidian vault folder (e.g. a Drive-synced vault) to file research into it.
pub fn default_out_dir(query: &str) -> String {
    let base = std::env::var("DR_RESEARCH_DIR")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            std::env::var_os("USERPROFILE")
                .or_else(|| std::env::var_os("HOME"))
                .map(std::path::PathBuf::from)
                .unwrap_or_else(|| std::path::PathBuf::from("."))
                .join("Documents")
        });
    let slug: String = query
        .split_whitespace()
        .take(8)
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '_' })
        .collect::<String>()
        .split('_')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("_");
    let slug = if slug.is_empty() { "Research".to_string() } else { slug };
    let date = utc_now_iso()[..10].replace('-', "");
    base.join(format!("{slug}_Research_{date}"))
        .to_string_lossy()
        .to_string()
}

pub fn cmd_init_run(out_dir: &str, query: &str, mode: &str) {
    fs::create_dir_all(out_dir).ok();
    let dir = std::fs::canonicalize(out_dir).unwrap_or_else(|_| Path::new(out_dir).to_path_buf());
    let artifacts = json!({
        "sources": "sources.jsonl", "evidence": "evidence.jsonl",
        "claims": "claims.jsonl", "report": "report.md"
    });
    let manifest = json!({
        "version": "3.0.0", "query": query, "mode": mode,
        "started_at": utc_now_iso(), "finished_at": Value::Null,
        "assumptions": [],
        "provider_config": {"primary": "search-cli", "scholarly": Value::Null},
        "report_dir": dir.to_string_lossy(),
        "artifact_paths": artifacts,
        "continuation": Value::Null,
    });
    let manifest_path = dir.join("run_manifest.json");
    fs::write(&manifest_path, serde_json::to_string_pretty(&manifest).unwrap() + "\n").ok();
    for name in ["sources.jsonl", "evidence.jsonl", "claims.jsonl"] {
        let p = dir.join(name);
        if !p.exists() {
            fs::write(&p, "").ok();
        }
    }
    print_compact(&json!({"status":"ok","manifest":manifest_path,"dir":dir}));
}

pub fn cmd_register_source(dir: &str, json_arg: &str) {
    let data: Json = serde_json::from_str(json_arg).unwrap_or_else(|_| die("invalid --json"));
    let raw_url = get_str(&data, "raw_url")
        .or_else(|| get_str(&data, "url"))
        .unwrap_or("")
        .to_string();
    if raw_url.is_empty() {
        die("raw_url is required");
    }
    let canonical = get_str(&data, "canonical_locator")
        .map(str::to_string)
        .unwrap_or_else(|| canonicalize_locator(&raw_url));
    let source_id = compute_source_id(&canonical);
    let path = artifact(dir, "sources.jsonl");
    for row in read_jsonl(&path) {
        if get_str(&row, "source_id") == Some(source_id.as_str()) {
            print_compact(&json!({"status":"duplicate","source_id":source_id,"canonical_locator":canonical}));
            return;
        }
    }
    let mut source = data.clone();
    for (k, v) in [
        ("source_id", json!(source_id)),
        ("canonical_locator", json!(canonical)),
        ("raw_url", json!(raw_url)),
        ("source_type", data.get("source_type").cloned().unwrap_or(json!("web"))),
        ("metadata_status", data.get("metadata_status").cloned().unwrap_or(json!("unverified"))),
        ("registered_at", json!(utc_now_iso())),
    ] {
        source.insert(k.into(), v);
    }
    source.entry("title").or_insert(json!(""));
    append_jsonl(&path, &source).ok();
    print_compact(&json!({"status":"registered","source_id":source_id,"canonical_locator":canonical}));
}

pub fn cmd_display_numbers(dir: &str) {
    let sources = read_jsonl(&artifact(dir, "sources.jsonl"));
    let mut mapping = serde_json::Map::new();
    for (i, src) in sources.iter().enumerate() {
        if let Some(sid) = get_str(src, "source_id") {
            mapping.entry(sid.to_string()).or_insert(json!(i + 1));
        }
    }
    print_json(&Value::Object(mapping));
}

pub fn cmd_bibliography(dir: &str, style: &str) {
    let unique = dedup(read_jsonl(&artifact(dir, "sources.jsonl")), "source_id");
    match style {
        "markdown" => {
            let mut lines = vec!["## Bibliography".to_string(), String::new()];
            for (i, src) in unique.iter().enumerate() {
                let author_str = match src.get("authors").and_then(|a| a.as_array()) {
                    Some(a) if a.len() == 1 => format!("{}. ", a[0].as_str().unwrap_or("")),
                    Some(a) if a.len() == 2 => format!(
                        "{} & {}. ",
                        a[0].as_str().unwrap_or(""),
                        a[1].as_str().unwrap_or("")
                    ),
                    Some(a) if !a.is_empty() => {
                        format!("{} et al. ", a[0].as_str().unwrap_or(""))
                    }
                    _ => String::new(),
                };
                let year = match src.get("year") {
                    Some(Value::String(s)) => format!("({s})"),
                    Some(v) => format!("({v})"),
                    None => "(n.d.)".into(),
                };
                let title = get_str(src, "title").unwrap_or("Untitled");
                let url = get_str(src, "raw_url").unwrap_or("");
                lines.push(format!("[{}] {}{}. [{}]({})", i + 1, author_str, year, title, url));
            }
            println!("{}", lines.join("\n"));
        }
        "json" => {
            let out: Vec<Value> = unique
                .iter()
                .enumerate()
                .map(|(i, s)| {
                    json!({
                        "display_number": i + 1,
                        "source_id": s.get("source_id"),
                        "canonical_locator": s.get("canonical_locator"),
                        "title": get_str(s, "title").unwrap_or(""),
                        "authors": s.get("authors"),
                        "year": s.get("year"),
                        "raw_url": get_str(s, "raw_url").unwrap_or(""),
                    })
                })
                .collect();
            print_json(&json!(out));
        }
        other => die(&format!("Unknown style: {other}")),
    }
}
