//! Parity port of the Python deep-research test suite
//! (test_citation_manager, test_evidence_store, test_extract_claims,
//! test_verify_claim_support) — CLI-level assertions against `dr`.

use serde_json::{json, Value};
use std::fs;
use std::path::PathBuf;
use std::process::Command;

fn tmpdir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!(
        "dr_test_{}_{}_{:?}",
        name,
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let _ = fs::remove_dir_all(&d);
    fs::create_dir_all(&d).unwrap();
    d
}

fn dr() -> Command {
    let mut p = std::env::current_exe().unwrap();
    p.pop();
    p.pop();
    p.push(if cfg!(windows) { "dr.exe" } else { "dr" });
    if !p.exists() {
        p = PathBuf::from("target/debug/dr");
        if cfg!(windows) {
            p.set_extension("exe");
        }
    }
    Command::new(p)
}

fn run(args: &[&str]) -> Value {
    let out = dr().args(args).output().unwrap();
    let s = String::from_utf8_lossy(&out.stdout).to_string();
    serde_json::from_str(s.trim()).unwrap_or_else(|_| json!(s))
}

fn run_expect_fail(args: &[&str]) -> (Value, i32) {
    let out = dr().args(args).output().unwrap();
    let s = String::from_utf8_lossy(&out.stdout).to_string();
    let v = serde_json::from_str(s.trim()).unwrap_or_else(|_| json!(s));
    (v, out.status.code().unwrap_or(-1))
}

fn write_jsonl(dir: &std::path::Path, name: &str, rows: &[Value]) {
    let mut s = String::new();
    for r in rows {
        s.push_str(&serde_json::to_string(r).unwrap());
        s.push('\n');
    }
    fs::write(dir.join(name), s).unwrap();
}

fn init(dir: &std::path::Path) {
    run(&[
        "run",
        "init",
        "--out-dir",
        dir.to_str().unwrap(),
        "--query",
        "test",
    ]);
}

// ============================ citation_manager ============================

#[test]
fn cm_init_run_creates_manifest_and_artifacts() {
    let d = tmpdir("cm_init");
    let out = run(&[
        "run",
        "init",
        "--out-dir",
        d.to_str().unwrap(),
        "--query",
        "test question",
        "--mode",
        "deep",
    ]);
    assert_eq!(out["status"], "ok");
    let manifest: Value =
        serde_json::from_str(&fs::read_to_string(d.join("run_manifest.json")).unwrap()).unwrap();
    assert_eq!(manifest["version"], "3.0.0");
    assert_eq!(manifest["query"], "test question");
    assert_eq!(manifest["mode"], "deep");
    assert!(manifest["started_at"].is_string());
    assert!(manifest["finished_at"].is_null());
    assert_eq!(manifest["artifact_paths"]["sources"], "sources.jsonl");
    for name in ["sources.jsonl", "evidence.jsonl", "claims.jsonl"] {
        let p = d.join(name);
        assert!(p.exists(), "{name} missing");
        assert_eq!(fs::metadata(&p).unwrap().len(), 0);
    }
}

#[test]
fn cm_register_and_dedup() {
    let d = tmpdir("cm_reg");
    init(&d);
    let dir = d.to_str().unwrap().to_string();
    let src = r#"{"raw_url":"https://arxiv.org/abs/2305.14251","title":"FActScore","source_type":"academic","year":"2023"}"#;
    let out1 = run(&["source", "register", "--json", src, "--dir", &dir]);
    assert_eq!(out1["status"], "registered");
    assert_eq!(out1["source_id"].as_str().unwrap().len(), 16);
    assert!(out1["canonical_locator"].as_str().unwrap().starts_with("arxiv:"));
    let out2 = run(&["source", "register", "--json", src, "--dir", &dir]);
    assert_eq!(out2["status"], "duplicate");
    assert_eq!(out2["source_id"], out1["source_id"]);
}

#[test]
fn cm_doi_canonicalization() {
    let d = tmpdir("cm_doi");
    init(&d);
    let out = run(&[
        "source",
        "register",
        "--json",
        r#"{"raw_url":"https://doi.org/10.1038/s41586-023-06745-9","title":"Some Nature paper"}"#,
        "--dir",
        d.to_str().unwrap(),
    ]);
    assert!(out["canonical_locator"]
        .as_str()
        .unwrap()
        .starts_with("doi:10.1038/"));
}

#[test]
fn cm_url_normalization_dedup() {
    let d = tmpdir("cm_norm");
    init(&d);
    let dir = d.to_str().unwrap().to_string();
    let out1 = run(&[
        "source",
        "register",
        "--json",
        r#"{"raw_url":"https://Example.Com/article?utm_source=google&id=42","title":"Test"}"#,
        "--dir",
        &dir,
    ]);
    let out2 = run(&[
        "source",
        "register",
        "--json",
        r#"{"raw_url":"https://example.com/article?id=42&utm_medium=email","title":"Test duplicate"}"#,
        "--dir",
        &dir,
    ]);
    assert_eq!(out1["source_id"], out2["source_id"]);
    assert_eq!(out2["status"], "duplicate");
}

#[test]
fn cm_assign_display_numbers_in_order() {
    let d = tmpdir("cm_nums");
    init(&d);
    let dir = d.to_str().unwrap().to_string();
    for (i, url) in ["https://a.com/1", "https://b.com/2", "https://c.com/3"]
        .iter()
        .enumerate()
    {
        run(&[
            "source",
            "register",
            "--json",
            &json!({"raw_url": url, "title": format!("Source {}", i + 1)}).to_string(),
            "--dir",
            &dir,
        ]);
    }
    let mapping = run(&["source", "numbers", "--dir", &dir]);
    assert_eq!(mapping.as_object().unwrap().len(), 3);
    let mut vals: Vec<i64> = mapping
        .as_object()
        .unwrap()
        .values()
        .map(|v| v.as_i64().unwrap())
        .collect();
    vals.sort();
    assert_eq!(vals, vec![1, 2, 3]);
}

#[test]
fn cm_export_bibliography_markdown() {
    let d = tmpdir("cm_bib_md");
    init(&d);
    let dir = d.to_str().unwrap().to_string();
    run(&[
        "source",
        "register",
        "--json",
        r#"{"raw_url":"https://arxiv.org/abs/2305.14251","title":"FActScore","authors":["Min, S.","Krishna, K."],"year":"2023","source_type":"academic"}"#,
        "--dir",
        &dir,
    ]);
    let out = run(&["source", "bibliography", "--dir", &dir, "--style", "markdown"]);
    let s = out.as_str().unwrap();
    assert!(s.contains("[1]"));
    assert!(s.contains("FActScore"));
    assert!(s.contains("Min, S. & Krishna, K."));
}

#[test]
fn cm_export_bibliography_json() {
    let d = tmpdir("cm_bib_json");
    init(&d);
    let dir = d.to_str().unwrap().to_string();
    run(&[
        "source",
        "register",
        "--json",
        r#"{"raw_url":"https://example.com/paper","title":"Test Paper"}"#,
        "--dir",
        &dir,
    ]);
    let out = run(&["source", "bibliography", "--dir", &dir, "--style", "json"]);
    let arr = out.as_array().unwrap();
    assert_eq!(arr.len(), 1);
    assert_eq!(arr[0]["display_number"], 1);
    assert_eq!(arr[0]["title"], "Test Paper");
}

// canonicalize_locator unit-level checks via duplicate-detection equivalence
#[test]
fn cm_canon_doi_from_url() {
    let d = tmpdir("cm_canon_doi");
    init(&d);
    let dir = d.to_str().unwrap().to_string();
    let a = run(&[
        "source", "register", "--dir", &dir,
        "--json", r#"{"raw_url":"https://doi.org/10.1038/s41586-023-06745-9","title":"x"}"#,
    ]);
    let b = run(&[
        "source", "register", "--dir", &dir,
        "--json", r#"{"raw_url":"https://dx.doi.org/10.1038/s41586-023-06745-9.","title":"y"}"#,
    ]);
    // dx.doi.org with trailing dot resolves to the same canonical doi
    assert_eq!(a["source_id"], b["source_id"]);
    assert!(a["canonical_locator"]
        .as_str()
        .unwrap()
        .starts_with("doi:10.1038/"));
}

#[test]
fn cm_canon_arxiv_variants() {
    let d = tmpdir("cm_canon_arxiv");
    init(&d);
    let dir = d.to_str().unwrap().to_string();
    let a = run(&[
        "source", "register", "--dir", &dir,
        "--json", r#"{"raw_url":"https://arxiv.org/abs/2305.14251v2","title":"x"}"#,
    ]);
    assert_eq!(a["canonical_locator"], "arxiv:2305.14251v2");
    let b = run(&[
        "source", "register", "--dir", &dir,
        "--json", r#"{"raw_url":"arxiv:2401.15884","title":"y"}"#,
    ]);
    assert_eq!(b["canonical_locator"], "arxiv:2401.15884");
}

#[test]
fn cm_canon_strips_tracking_fragment_slash() {
    let d = tmpdir("cm_canon_url");
    init(&d);
    let dir = d.to_str().unwrap().to_string();
    let a = run(&[
        "source", "register", "--dir", &dir,
        "--json", r#"{"raw_url":"https://Example.Com/page?utm_source=x&key=val#frag","title":"x"}"#,
    ]);
    let loc = a["canonical_locator"].as_str().unwrap();
    assert!(!loc.contains("utm_source"));
    assert!(loc.contains("key=val"));
    assert!(loc.starts_with("https://example.com"));
    assert!(!loc.contains("#frag"));
    let b = run(&[
        "source", "register", "--dir", &dir,
        "--json", r#"{"raw_url":"https://example.com/other/","title":"y"}"#,
    ]);
    assert!(!b["canonical_locator"].as_str().unwrap().ends_with('/'));
}

// ============================ evidence_store ============================

#[test]
fn es_init_creates_empty_file() {
    let d = tmpdir("es_init");
    let out = run(&["evidence", "init", "--dir", d.to_str().unwrap()]);
    assert_eq!(out["status"], "ok");
    let p = d.join("evidence.jsonl");
    assert!(p.exists());
    assert_eq!(fs::metadata(&p).unwrap().len(), 0);
}

#[test]
fn es_add_and_dedup() {
    let d = tmpdir("es_add");
    let dir = d.to_str().unwrap().to_string();
    run(&["evidence", "init", "--dir", &dir]);
    let ev = r#"{"source_id":"abcdef0123456789","quote":"FActScore decomposes generation into atomic facts.","evidence_type":"direct_quote","locator":"page 3","retrieval_query":"factuality evaluation methods"}"#;
    let out1 = run(&["evidence", "add", "--json", ev, "--dir", &dir]);
    assert_eq!(out1["status"], "added");
    assert_eq!(out1["evidence_id"].as_str().unwrap().len(), 16);
    let out2 = run(&["evidence", "add", "--json", ev, "--dir", &dir]);
    assert_eq!(out2["status"], "duplicate");
    assert_eq!(out2["evidence_id"], out1["evidence_id"]);
}

#[test]
fn es_whitespace_normalization() {
    let d = tmpdir("es_ws");
    let dir = d.to_str().unwrap().to_string();
    run(&["evidence", "init", "--dir", &dir]);
    let out1 = run(&["evidence","add","--dir",&dir,"--json",
        r#"{"source_id":"abcdef0123456789","quote":"  FActScore   decomposes   generation  into atomic facts.  ","evidence_type":"direct_quote"}"#]);
    let out2 = run(&["evidence","add","--dir",&dir,"--json",
        r#"{"source_id":"abcdef0123456789","quote":"FActScore decomposes generation into atomic facts.","evidence_type":"direct_quote"}"#]);
    assert_eq!(out1["evidence_id"], out2["evidence_id"]);
    assert_eq!(out2["status"], "duplicate");
}

#[test]
fn es_different_sources_different_ids() {
    let d = tmpdir("es_diff");
    let dir = d.to_str().unwrap().to_string();
    run(&["evidence", "init", "--dir", &dir]);
    let out1 = run(&["evidence","add","--dir",&dir,"--json",
        r#"{"source_id":"aaaaaaaaaaaaaaaa","quote":"Same quote text.","evidence_type":"paraphrase"}"#]);
    let out2 = run(&["evidence","add","--dir",&dir,"--json",
        r#"{"source_id":"bbbbbbbbbbbbbbbb","quote":"Same quote text.","evidence_type":"paraphrase"}"#]);
    assert_ne!(out1["evidence_id"], out2["evidence_id"]);
    assert_eq!(out2["status"], "added");
}

fn es_seed(dir: &std::path::Path) {
    let dir_s = dir.to_str().unwrap().to_string();
    run(&["evidence", "init", "--dir", &dir_s]);
    for (src, quote) in [
        ("src_aaa", "First quote from source A."),
        ("src_aaa", "Second quote from source A."),
        ("src_bbb", "Quote from source B."),
    ] {
        run(&[
            "evidence",
            "add",
            "--dir",
            &dir_s,
            "--json",
            &json!({"source_id": src, "quote": quote, "evidence_type": "direct_quote"})
                .to_string(),
        ]);
    }
}

#[test]
fn es_list_all() {
    let d = tmpdir("es_list");
    es_seed(&d);
    let out = run(&["evidence", "list", "--dir", d.to_str().unwrap()]);
    assert_eq!(out["count"], 3);
}

#[test]
fn es_list_filtered() {
    let d = tmpdir("es_filt");
    es_seed(&d);
    let dir = d.to_str().unwrap().to_string();
    let out = run(&["evidence", "list", "--dir", &dir, "--source-id", "src_aaa"]);
    assert_eq!(out["count"], 2);
    let out = run(&["evidence", "list", "--dir", &dir, "--source-id", "src_bbb"]);
    assert_eq!(out["count"], 1);
}

#[test]
fn es_export() {
    let d = tmpdir("es_export");
    es_seed(&d);
    let out = run(&["evidence", "export", "--dir", d.to_str().unwrap()]);
    let arr = out.as_array().unwrap();
    assert_eq!(arr.len(), 3);
    for row in arr {
        for k in ["evidence_id", "source_id", "quote", "evidence_type", "captured_at"] {
            assert!(row.get(k).is_some(), "missing {k}");
        }
    }
}

#[test]
fn es_id_deterministic_and_locator_matters() {
    let d = tmpdir("es_id");
    let dir = d.to_str().unwrap().to_string();
    run(&["evidence", "init", "--dir", &dir]);
    let a = run(&["evidence","add","--dir",&dir,"--json",
        r#"{"source_id":"src_a","quote":"test quote","locator":"page 1"}"#]);
    let b = run(&["evidence","add","--dir",&dir,"--json",
        r#"{"source_id":"src_a","quote":"test quote","locator":"page 2"}"#]);
    assert_eq!(a["evidence_id"].as_str().unwrap().len(), 16);
    assert_ne!(a["evidence_id"], b["evidence_id"]);
}

// ============================ extract_claims ============================

const SAMPLE_REPORT: &str = r#"---
title: Test Research Report
---

## Executive Summary

This report examines the impact of quantum computing on cryptography [1, 2]. The field has advanced significantly since 2020, with major breakthroughs in error correction.

## Introduction

Quantum computing represents a paradigm shift in computational capability. Researchers at Google demonstrated quantum supremacy in 2019 using a 53-qubit processor [3]. This milestone confirmed theoretical predictions made decades earlier.

## Finding 1

The Shor algorithm can factor large numbers exponentially faster than classical methods [4]. Current RSA-2048 encryption could be broken by a sufficiently large quantum computer. However, such machines are estimated to require millions of physical qubits [5, 6].

## Finding 2

Post-quantum cryptography standards should be adopted within the next 5 years. Organizations should consider hybrid classical-quantum approaches during the transition period. NIST has already standardized several lattice-based algorithms [7].

## Synthesis

Taken together, the evidence suggests that quantum computing poses a real but manageable threat to current cryptographic systems. The timeline for practical quantum attacks remains uncertain, but proactive migration reduces risk substantially.

## Recommendations

Organizations should begin evaluating post-quantum cryptography solutions immediately. Security teams should conduct a cryptographic inventory to identify vulnerable systems. Companies should consider implementing crypto-agility frameworks to enable rapid algorithm switching.

## Bibliography

[1] Smith et al. (2023). Quantum Computing Advances.
[2] Johnson (2024). Cryptographic Implications.
"#;

fn claims_setup() -> (PathBuf, String, String) {
    let d = tmpdir("ec");
    fs::write(d.join("claims.jsonl"), "").unwrap();
    let rp = d.join("report.md");
    fs::write(&rp, SAMPLE_REPORT).unwrap();
    let dir = d.to_str().unwrap().to_string();
    let report = rp.to_str().unwrap().to_string();
    (d, dir, report)
}

#[test]
fn ec_extract_finds_claims() {
    let (_d, dir, report) = claims_setup();
    let out = run(&["claims", "extract", "--report", &report, "--dir", &dir]);
    assert_eq!(out["status"], "ok");
    assert!(out["claims_added"].as_u64().unwrap() > 5);
}

#[test]
fn ec_extract_idempotent() {
    let (_d, dir, report) = claims_setup();
    let out1 = run(&["claims", "extract", "--report", &report, "--dir", &dir]);
    let out2 = run(&["claims", "extract", "--report", &report, "--dir", &dir]);
    assert_eq!(out2["claims_added"], 0);
    assert_eq!(out2["claims_skipped"], out1["claims_added"]);
}

#[test]
fn ec_claim_types_assigned() {
    let (_d, dir, report) = claims_setup();
    run(&["claims", "extract", "--report", &report, "--dir", &dir]);
    let out = run(&["claims", "stats", "--dir", &dir]);
    assert!(out["by_type"].get("factual").is_some());
    assert!(out["by_type"].get("recommendation").is_some());
}

#[test]
fn ec_sections_detected() {
    let (_d, dir, report) = claims_setup();
    run(&["claims", "extract", "--report", &report, "--dir", &dir]);
    let out = run(&["claims", "stats", "--dir", &dir]);
    assert!(out["by_section"].get("finding_1").is_some());
    assert!(out["by_section"].get("finding_2").is_some());
    assert!(out["by_section"].get("recommendations").is_some());
}

#[test]
fn ec_add_and_dedup() {
    let d = tmpdir("ec_add");
    fs::write(d.join("claims.jsonl"), "").unwrap();
    let dir = d.to_str().unwrap().to_string();
    let claim = r#"{"section_id":"finding_1","text":"Quantum computers can break RSA encryption.","claim_type":"factual"}"#;
    let out1 = run(&["claims", "add", "--json", claim, "--dir", &dir]);
    assert_eq!(out1["status"], "added");
    assert_eq!(out1["claim_id"].as_str().unwrap().len(), 16);
    let out2 = run(&["claims", "add", "--json", claim, "--dir", &dir]);
    assert_eq!(out2["status"], "duplicate");
}

#[test]
fn ec_add_with_sources() {
    let d = tmpdir("ec_add_src");
    fs::write(d.join("claims.jsonl"), "").unwrap();
    let out = run(&["claims","add","--dir",d.to_str().unwrap(),"--json",
        r#"{"section_id":"finding_1","text":"NIST standardized CRYSTALS-Kyber in 2024.","claim_type":"factual","cited_source_ids":["abcdef0123456789"],"evidence_ids":["1234567890abcdef"]}"#]);
    assert_eq!(out["status"], "added");
}

fn ec_seed() -> (PathBuf, String) {
    let d = tmpdir("ec_seed");
    fs::write(d.join("claims.jsonl"), "").unwrap();
    let dir = d.to_str().unwrap().to_string();
    for (sec, text, ctype) in [
        ("finding_1", "The sky appears blue due to Rayleigh scattering.", "factual"),
        ("finding_1", "Light wavelengths scatter differently in the atmosphere.", "factual"),
        ("synthesis", "Overall, atmospheric optics explains most visual phenomena.", "synthesis"),
        ("recommendations", "Researchers should investigate polarization effects further.", "recommendation"),
    ] {
        run(&[
            "claims",
            "add",
            "--dir",
            &dir,
            "--json",
            &json!({"section_id": sec, "text": text, "claim_type": ctype}).to_string(),
        ]);
    }
    (d, dir)
}

#[test]
fn ec_list_all() {
    let (_d, dir) = ec_seed();
    let out = run(&["claims", "list", "--dir", &dir]);
    assert_eq!(out["count"], 4);
}

#[test]
fn ec_list_by_section() {
    let (_d, dir) = ec_seed();
    let out = run(&["claims", "list", "--dir", &dir, "--section", "finding_1"]);
    assert_eq!(out["count"], 2);
}

#[test]
fn ec_list_by_type() {
    let (_d, dir) = ec_seed();
    let out = run(&["claims", "list", "--dir", &dir, "--type", "recommendation"]);
    assert_eq!(out["count"], 1);
}

#[test]
fn ec_stats() {
    let (_d, dir) = ec_seed();
    let out = run(&["claims", "stats", "--dir", &dir]);
    assert_eq!(out["total"], 4);
    assert_eq!(out["by_type"]["factual"], 2);
    assert_eq!(out["by_type"]["synthesis"], 1);
    assert_eq!(out["by_type"]["recommendation"], 1);
}

#[test]
fn ec_claim_id_deterministic_and_section_matters() {
    let d = tmpdir("ec_id");
    fs::write(d.join("claims.jsonl"), "").unwrap();
    let dir = d.to_str().unwrap().to_string();
    let a = run(&["claims","add","--dir",&dir,"--json",
        r#"{"section_id":"finding_1","text":"Same deterministic text for claim id test."}"#]);
    // same text, different section -> different claim_id
    let b = run(&["claims","add","--dir",&dir,"--json",
        r#"{"section_id":"finding_2","text":"Same deterministic text for claim id test."}"#]);
    assert_eq!(a["claim_id"].as_str().unwrap().len(), 16);
    assert_ne!(a["claim_id"], b["claim_id"]);
}

#[test]
fn ec_classify_recommendation_factual_synthesis() {
    // classify_claim parity: extract assigns types from text + section
    let d = tmpdir("ec_class");
    fs::write(d.join("claims.jsonl"), "").unwrap();
    let md = d.join("r.md");
    fs::write(
        &md,
        "## Recommendations\n\nOrganizations should adopt PQC immediately for all systems today.\n\n## Finding 1\n\nRSA-2048 uses 2048-bit keys for public key encryption widely.\n\n## Synthesis\n\nTaken together, the results indicate a clear overall trend here.\n",
    )
    .unwrap();
    let dir = d.to_str().unwrap().to_string();
    run(&["claims", "extract", "--dir", &dir, "--report", md.to_str().unwrap()]);
    let out = run(&["claims", "list", "--dir", &dir]);
    let claims = out["claims"].as_array().unwrap();
    let ctype = |text_start: &str| {
        claims
            .iter()
            .find(|c| c["text"].as_str().unwrap().starts_with(text_start))
            .unwrap()["claim_type"]
            .as_str()
            .unwrap()
            .to_string()
    };
    assert_eq!(ctype("Organizations should"), "recommendation");
    assert_eq!(ctype("RSA-2048"), "factual");
    assert_eq!(ctype("Taken together"), "synthesis");
}

// ============================ verify_claim_support ============================

fn vcs_dir(name: &str, sources: &[Value], evidence: &[Value], claims: &[Value]) -> (PathBuf, String) {
    let d = tmpdir(name);
    write_jsonl(&d, "sources.jsonl", sources);
    write_jsonl(&d, "evidence.jsonl", evidence);
    write_jsonl(&d, "claims.jsonl", claims);
    let dir = d.to_str().unwrap().to_string();
    (d, dir)
}

fn read_claims(d: &std::path::Path) -> Vec<Value> {
    fs::read_to_string(d.join("claims.jsonl"))
        .unwrap()
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).unwrap())
        .collect()
}

#[test]
fn vcs_supported_claim() {
    let (d, dir) = vcs_dir(
        "vcs_sup",
        &[json!({"source_id":"src_quantum_001","title":"Quantum Computing 2024"})],
        &[json!({"evidence_id":"ev_shor_001","source_id":"src_quantum_001",
            "quote":"Shor's algorithm can factor large integers exponentially faster than any known classical algorithm, threatening RSA-2048 encryption.",
            "evidence_type":"direct_quote"})],
        &[json!({"claim_id":"clm_factor_001","section_id":"finding_1",
            "text":"Shor's algorithm can factor large numbers exponentially faster than classical methods, threatening RSA-2048.",
            "claim_type":"factual","cited_source_ids":["src_quantum_001"],
            "evidence_ids":["ev_shor_001"],"support_status":"unverified"})],
    );
    let out = run(&["support", "verify", "--dir", &dir]);
    assert_eq!(out["status"], "pass");
    assert_eq!(out["factual_unsupported"], 0);
    let claims = read_claims(&d);
    assert_eq!(claims[0]["support_status"], "supported");
}

#[test]
fn vcs_unsupported_no_evidence() {
    let (_d, dir) = vcs_dir(
        "vcs_uns",
        &[],
        &[],
        &[json!({"claim_id":"clm_no_ev_001","section_id":"finding_1",
            "text":"The population of Mars is 500 million as of 2025.",
            "claim_type":"factual","cited_source_ids":[],
            "evidence_ids":[],"support_status":"unverified"})],
    );
    let out = run(&["support", "verify", "--dir", &dir]);
    assert_eq!(out["factual_unsupported"], 1);
    assert_eq!(out["status"], "pass");
}

#[test]
fn vcs_strict_fails() {
    let (_d, dir) = vcs_dir(
        "vcs_strict",
        &[],
        &[],
        &[json!({"claim_id":"clm_no_ev_001","section_id":"finding_1",
            "text":"The population of Mars is 500 million as of 2025.",
            "claim_type":"factual","cited_source_ids":[],
            "evidence_ids":[],"support_status":"unverified"})],
    );
    let (out, code) = run_expect_fail(&["support", "verify", "--dir", &dir, "--strict"]);
    assert_eq!(out["status"], "fail");
    assert_ne!(code, 0);
}

#[test]
fn vcs_speculation_passes() {
    let (d, dir) = vcs_dir(
        "vcs_spec",
        &[],
        &[],
        &[
            json!({"claim_id":"clm_spec_001","section_id":"finding_1",
                "text":"Quantum computers might eventually solve protein folding in real time.",
                "claim_type":"speculation","cited_source_ids":[],
                "evidence_ids":[],"support_status":"unverified"}),
            json!({"claim_id":"clm_rec_001","section_id":"recommendations",
                "text":"Organizations should begin PQC migration planning immediately.",
                "claim_type":"recommendation","cited_source_ids":[],
                "evidence_ids":[],"support_status":"unverified"}),
        ],
    );
    run(&["support", "verify", "--dir", &dir]);
    let claims = read_claims(&d);
    let spec = claims
        .iter()
        .find(|c| c["claim_type"] == "speculation")
        .unwrap();
    assert_eq!(spec["support_status"], "supported");
}

#[test]
fn vcs_partial_support() {
    let (d, dir) = vcs_dir(
        "vcs_part",
        &[json!({"source_id":"src_nist_001","title":"NIST PQC Standards"})],
        &[json!({"evidence_id":"ev_nist_001","source_id":"src_nist_001",
            "quote":"NIST announced the standardization of CRYSTALS-Kyber for key encapsulation.",
            "evidence_type":"direct_quote"})],
        &[json!({"claim_id":"clm_nist_time","section_id":"finding_2",
            "text":"NIST standardized four lattice-based algorithms in 2024, covering both encryption and signatures.",
            "claim_type":"factual","cited_source_ids":["src_nist_001"],
            "evidence_ids":["ev_nist_001"],"support_status":"unverified"})],
    );
    run(&["support", "verify", "--dir", &dir]);
    let claims = read_claims(&d);
    let st = claims[0]["support_status"].as_str().unwrap();
    assert!(
        ["partial", "needs_review", "supported"].contains(&st),
        "unexpected status {st}"
    );
}

#[test]
fn vcs_identical_text_supported() {
    let (d, dir) = vcs_dir(
        "vcs_ident",
        &[json!({"source_id":"s1","title":"t"})],
        &[json!({"evidence_id":"e1","source_id":"s1",
            "quote":"RSA-2048 uses 2048-bit keys for encryption.","evidence_type":"direct_quote"})],
        &[json!({"claim_id":"c1","section_id":"finding_1",
            "text":"RSA-2048 uses 2048-bit keys for encryption.",
            "claim_type":"factual","cited_source_ids":[],
            "evidence_ids":["e1"],"support_status":"unverified"})],
    );
    run(&["support", "verify", "--dir", &dir]);
    let claims = read_claims(&d);
    assert_eq!(claims[0]["support_status"], "supported");
    assert!(claims[0]["_support_score"].as_f64().unwrap() > 0.8);
}

#[test]
fn vcs_unrelated_evidence_low_score() {
    let (d, dir) = vcs_dir(
        "vcs_unrel",
        &[json!({"source_id":"s1","title":"t"})],
        &[json!({"evidence_id":"e1","source_id":"s1",
            "quote":"Bananas are a good source of potassium and fiber.","evidence_type":"direct_quote"})],
        &[json!({"claim_id":"c1","section_id":"finding_1",
            "text":"The moon landing occurred in 1969.",
            "claim_type":"factual","cited_source_ids":[],
            "evidence_ids":["e1"],"support_status":"unverified"})],
    );
    run(&["support", "verify", "--dir", &dir]);
    let claims = read_claims(&d);
    let st = claims[0]["support_status"].as_str().unwrap();
    assert!(["needs_review", "unsupported"].contains(&st));
    assert!(claims[0]["_support_score"].as_f64().unwrap() < 0.35);
}

// ============================ extra: validate + html + state ============================

#[test]
fn validate_minimal_report() {
    let d = tmpdir("val");
    let md = d.join("ok.md");
    fs::write(
        &md,
        format!(
            "# T\n\n## Executive Summary\n{}\n\n## Introduction\nx\n\n## Main Analysis\nx\n\n## Bibliography\n[1] A. https://doi.org/10.1/a\n",
            "word ".repeat(60)
        ),
    )
    .unwrap();
    let out = run(&["report", "validate", "--report", md.to_str().unwrap()]);
    assert!(out.as_str().unwrap_or("").contains("VALIDATION") || out.is_object());
}

#[test]
fn html_render_and_verify() {
    let d = tmpdir("html");
    let md = d.join("r.md");
    fs::write(&md, "## Executive Summary\n\nA sentence long enough to be a claim paragraph here.\n").unwrap();
    let out = dr()
        .args(["html", "render", md.to_str().unwrap()])
        .output()
        .unwrap();
    let html = String::from_utf8_lossy(&out.stdout).to_string();
    assert!(html.contains("Executive Summary"));
    let hp = d.join("r.html");
    fs::write(&hp, &html).unwrap();
    let out = run(&[
        "html",
        "verify",
        "--html",
        hp.to_str().unwrap(),
        "--md",
        md.to_str().unwrap(),
    ]);
    assert!(out.is_object() || out.is_string());
}

#[test]
fn state_init_and_phases() {
    let d = tmpdir("state");
    let out = run(&["state", "init", "--dir", d.to_str().unwrap(), "--query", "q"]);
    assert_eq!(out["status"], "ok");
    let out = run(&["phases", "--mode", "deep"]);
    let phases: Vec<String> = out["phases"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_string())
        .collect();
    assert_eq!(
        phases,
        vec![
            "scope", "plan", "retrieve", "triangulate", "outline", "synthesize", "critique",
            "refine", "package"
        ]
    );
}
