//! source_evaluator port: deterministic credibility scoring.

use crate::util::*;
use serde_json::json;

const HIGH_AUTHORITY: &[&str] = &[
    "arxiv.org", "nature.com", "science.org", "cell.com", "nejm.org", "thelancet.com",
    "springer.com", "sciencedirect.com", "plos.org", "ieee.org", "acm.org",
    "pubmed.ncbi.nlm.nih.gov", "nih.gov", "cdc.gov", "who.int", "fda.gov", "nasa.gov",
    "gov.uk", "europa.eu", "un.org", "docs.python.org", "developer.mozilla.org",
    "docs.microsoft.com", "cloud.google.com", "aws.amazon.com", "kubernetes.io",
    "reuters.com", "apnews.com", "bbc.com", "economist.com", "nature.com/news",
    "scientificamerican.com",
];

const MODERATE_AUTHORITY: &[&str] = &[
    "techcrunch.com", "theverge.com", "arstechnica.com", "wired.com", "zdnet.com", "cnet.com",
    "forbes.com", "bloomberg.com", "wsj.com", "ft.com", "wikipedia.org", "britannica.com",
    "khanacademy.org", "medium.com", "dev.to", "stackoverflow.com", "github.com",
];

const LOW_INDICATORS: &[&str] = &["blogspot.com", "wordpress.com", "wix.com", "substack.com"];

fn domain_of(url: &str) -> String {
    let (_, host, _, _) = parse_url(url);
    host.replacen("www.", "", 1)
}

fn domain_authority(domain: &str) -> f64 {
    if HIGH_AUTHORITY.contains(&domain) {
        90.0
    } else if MODERATE_AUTHORITY.contains(&domain) {
        70.0
    } else if LOW_INDICATORS.iter().any(|i| domain.contains(i)) {
        40.0
    } else {
        55.0
    }
}

fn recency(publication_date: Option<&str>) -> f64 {
    let Some(d) = publication_date else { return 50.0 };
    let Some(days) = parse_date_days(d) else { return 50.0 };
    let age = unix_days_now() - days;
    if age < 90 {
        100.0
    } else if age < 365 {
        85.0
    } else if age < 730 {
        70.0
    } else if age < 1825 {
        50.0
    } else {
        30.0
    }
}

fn expertise(domain: &str, title: &str, author: Option<&str>) -> f64 {
    let mut score: f64 = 50.0;
    if ["arxiv", "nature", "science", "ieee", "acm"].iter().any(|d| domain.contains(d)) {
        score += 30.0;
    }
    if domain.contains(".gov") || domain.contains("who.int") {
        score += 25.0;
    }
    if domain.contains("docs.") || title.to_lowercase().contains("documentation") {
        score += 20.0;
    }
    if let Some(a) = author {
        let a = a.to_lowercase();
        if ["dr.", "phd", "professor"].iter().any(|t| a.contains(t)) {
            score += 15.0;
        }
    }
    score.min(100.0)
}

fn bias(domain: &str, title: &str, content: Option<&str>) -> f64 {
    let mut score: f64 = 70.0;
    let tl = title.to_lowercase();
    for ind in [
        "!", "shocking", "unbelievable", "you won't believe", "secret",
        "they don't want you to know",
    ] {
        if tl.contains(ind) {
            score -= 20.0;
            break;
        }
    }
    if ["arxiv", "nature", "science", "ieee"].iter().any(|d| domain.contains(d)) {
        score += 20.0;
    }
    if let Some(c) = content {
        let cl = c.to_lowercase();
        if ["however", "although", "on the other hand", "critics argue"]
            .iter()
            .any(|i| cl.contains(i))
        {
            score += 10.0;
        }
    }
    score.clamp(0.0, 100.0)
}

fn factors(domain_score: f64, recency_score: f64, expertise_score: f64, bias_score: f64) -> serde_json::Map<String, serde_json::Value> {
    let mut f = serde_json::Map::new();
    if domain_score >= 85.0 {
        f.insert("domain".into(), json!("High authority domain"));
    } else if domain_score <= 45.0 {
        f.insert("domain".into(), json!("Low authority domain - verify claims"));
    }
    if recency_score >= 85.0 {
        f.insert("recency".into(), json!("Recent information"));
    } else if recency_score <= 40.0 {
        f.insert("recency".into(), json!("Outdated information - verify currency"));
    }
    if expertise_score >= 80.0 {
        f.insert("expertise".into(), json!("Expert source"));
    } else if expertise_score <= 45.0 {
        f.insert("expertise".into(), json!("Limited expertise indicators"));
    }
    if bias_score >= 80.0 {
        f.insert("bias".into(), json!("Balanced perspective"));
    } else if bias_score <= 50.0 {
        f.insert("bias".into(), json!("Potential bias detected"));
    }
    f
}

fn recommendation(overall: f64) -> &'static str {
    if overall >= 80.0 {
        "high_trust"
    } else if overall >= 60.0 {
        "moderate_trust"
    } else if overall >= 40.0 {
        "low_trust"
    } else {
        "verify"
    }
}

pub fn cmd_evaluate(url: &str, title: &str, date: Option<&str>, author: Option<&str>, content: Option<&str>) {
    let domain = domain_of(url);
    let ds = domain_authority(&domain);
    let rs = recency(date);
    let es = expertise(&domain, title, author);
    let bs = bias(&domain, title, content);
    let overall = ds * 0.35 + rs * 0.20 + es * 0.25 + bs * 0.20;
    let r2 = |v: f64| (v * 100.0).round() / 100.0;
    print_json(&json!({
        "overall_score": r2(overall),
        "domain_authority": r2(ds),
        "recency": r2(rs),
        "expertise": r2(es),
        "bias_score": r2(bs),
        "factors": factors(ds, rs, es, bs),
        "recommendation": recommendation(overall),
    }));
}
