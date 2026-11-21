//! validate_report port: 9-check report quality validator.

use regex::Regex;
use std::collections::HashSet;
use std::path::Path;

pub fn cmd_validate(report_path: &str) {
    let path = Path::new(report_path);
    if !path.exists() {
        eprintln!("ERROR: Report file not found: {report_path}");
        std::process::exit(1);
    }
    let content = std::fs::read_to_string(path).unwrap_or_else(|e| {
        eprintln!("ERROR: Cannot read report: {e}");
        std::process::exit(1);
    });

    let mut errors: Vec<String> = Vec::new();
    let mut warnings: Vec<String> = Vec::new();

    println!("\n{}", "=".repeat(60));
    println!("VALIDATING REPORT: {}", path.file_name().unwrap_or_default().to_string_lossy());
    println!("{}\n", "=".repeat(60));

    // 1. Executive Summary
    {
        let ok = check_exec_summary(&content, &mut errors, &mut warnings);
        report_check("Executive Summary", ok);
    }
    // 2. Required Sections
    {
        let ok = check_sections(&content, &mut errors, &mut warnings);
        report_check("Required Sections", ok);
    }
    // 3. Citations
    {
        let ok = check_citations(&content, &mut errors, &mut warnings);
        report_check("Citations", ok);
    }
    // 4. Bibliography
    {
        let ok = check_bibliography(&content, &mut errors, &mut warnings);
        report_check("Bibliography", ok);
    }
    // 5. Placeholders
    {
        let placeholders = [
            "TBD", "TODO", "FIXME", "XXX", "[citation needed]", "[needs citation]",
            "[placeholder]", "[TODO]", "[TBD]",
        ];
        let found: Vec<&str> = placeholders.iter().copied().filter(|p| content.contains(p)).collect();
        let ok = found.is_empty();
        if !ok {
            errors.push(format!("Found placeholder text: {}", found.join(", ")));
        }
        report_check("Placeholder Text", ok);
    }
    // 6. Content truncation
    {
        let patterns = [
            (r"Content continues", "Phrase \"Content continues\""),
            (r"Due to length", "Phrase \"Due to length\""),
            (r"would continue", "Phrase \"would continue\""),
            (r"\[Sections \d+-\d+", "Pattern \"[Sections X-Y\""),
            (r"Additional sections", "Phrase \"Additional sections\""),
            (r"(?i)comprehensive.*word document that continues", "Pattern \"comprehensive...document that continues\""),
        ];
        let mut ok = true;
        for (pat, desc) in patterns {
            if Regex::new(pat).unwrap().is_match(&content) {
                errors.push(format!("CRITICAL: Content truncation detected: {desc}"));
                errors.push("   Report is INCOMPLETE and UNUSABLE - regenerate with progressive assembly".into());
                ok = false;
                break;
            }
        }
        report_check("Content Truncation", ok);
    }
    // 7. Word count
    {
        let wc = content.split_whitespace().count();
        if wc < 500 {
            warnings.push(format!("Report is very short: {wc} words (consider expanding)"));
        }
        report_check("Word Count", true);
    }
    // 8. Source count
    {
        if let Some(bib) = bib_section(&content) {
            let re = Regex::new(r"(?m)^\[(\d+)\]").unwrap();
            let count: HashSet<String> = re.captures_iter(&bib).map(|c| c[1].to_string()).collect();
            if count.len() < 10 {
                warnings.push(format!("Only {} sources (recommended: >=10)", count.len()));
            }
        }
        report_check("Source Count", true);
    }
    // 9. Broken internal links
    {
        let re = Regex::new(r"\[.*?\]\((\./.*?)\)").unwrap();
        let mut broken = Vec::new();
        for c in re.captures_iter(&content) {
            let link = c[1].split('#').next().unwrap_or("");
            if !path.parent().unwrap_or(Path::new(".")).join(link).exists() {
                broken.push(link.to_string());
            }
        }
        let ok = broken.is_empty();
        if !ok {
            errors.push(format!("Broken internal links: {}", broken.join(", ")));
        }
        report_check("Broken Links", ok);
    }

    // Summary
    println!("\n{}", "=".repeat(60));
    println!("VALIDATION SUMMARY");
    println!("{}\n", "=".repeat(60));
    if !errors.is_empty() {
        println!("ERRORS ({}):", errors.len());
        for e in &errors {
            println!("   - {e}");
        }
        println!();
    }
    if !warnings.is_empty() {
        println!("WARNINGS ({}):", warnings.len());
        for w in &warnings {
            println!("   - {w}");
        }
        println!();
    }
    if errors.is_empty() && warnings.is_empty() {
        println!("ALL CHECKS PASSED - Report meets quality standards!\n");
    } else if errors.is_empty() {
        println!("VALIDATION PASSED (with warnings)\n");
    } else {
        println!("VALIDATION FAILED - Please fix errors before delivery\n");
    }
    std::process::exit(if errors.is_empty() { 0 } else { 1 });
}

fn report_check(name: &str, ok: bool) {
    println!("Checking: {name}... {}", if ok { "PASS" } else { "FAIL" });
}

fn section<'a>(content: &'a str, name: &str) -> Option<String> {
    let re = Regex::new(&format!(r"(?is)##\s*{}\s*(.*?)(?:##|\z)", name)).unwrap();
    re.captures(content).map(|c| c[1].to_string())
}

fn bib_section(content: &str) -> Option<String> {
    section(content, "Bibliography")
}

fn check_exec_summary(content: &str, errors: &mut Vec<String>, warnings: &mut Vec<String>) -> bool {
    match section(content, "Executive Summary") {
        None => {
            errors.push("Missing 'Executive Summary' section".into());
            false
        }
        Some(s) => {
            let wc = s.trim().split_whitespace().count();
            if wc > 400 {
                warnings.push(format!("Executive summary too long: {wc} words (should be <=400)"));
            }
            if wc < 50 {
                warnings.push(format!("Executive summary too short: {wc} words (should be >=50)"));
            }
            true
        }
    }
}

fn check_sections(content: &str, errors: &mut Vec<String>, warnings: &mut Vec<String>) -> bool {
    let required = [
        "Executive Summary", "Introduction", "Main Analysis", "Synthesis", "Limitations",
        "Recommendations", "Bibliography", "Methodology",
    ];
    let recommended = ["Counterevidence Register", "Claims-Evidence Table"];
    let mut missing = Vec::new();
    for s in required {
        let re = Regex::new(&format!(r"(?i)##.*{}", regex::escape(s))).unwrap();
        if !re.is_match(content) {
            missing.push(s);
        }
    }
    let mut ok = true;
    if !missing.is_empty() {
        errors.push(format!("Missing sections: {}", missing.join(", ")));
        ok = false;
    } else {
        let miss_rec: Vec<&str> = recommended
            .iter()
            .copied()
            .filter(|s| {
                !Regex::new(&format!(r"(?i)##.*{}", regex::escape(s)))
                    .unwrap()
                    .is_match(content)
            })
            .collect();
        if !miss_rec.is_empty() {
            warnings.push(format!(
                "Missing recommended sections (for academic rigor): {}",
                miss_rec.join(", ")
            ));
        }
    }
    ok
}

fn check_citations(content: &str, errors: &mut Vec<String>, warnings: &mut Vec<String>) -> bool {
    let re = Regex::new(r"\[(\d+)\]").unwrap();
    let cites: Vec<i64> = re
        .captures_iter(content)
        .filter_map(|c| c[1].parse().ok())
        .collect();
    if cites.is_empty() {
        errors.push("No citations found in report".into());
        return false;
    }
    let unique: HashSet<i64> = cites.iter().copied().collect();
    if unique.len() < 10 {
        warnings.push(format!(
            "Only {} unique sources cited (recommended: >=10)",
            unique.len()
        ));
    }
    let max = *unique.iter().max().unwrap_or(&0);
    let missing: Vec<i64> = (1..=max).filter(|n| !unique.contains(n)).collect();
    if !missing.is_empty() {
        warnings.push(format!("Non-consecutive citation numbers, missing: {missing:?}"));
    }
    true
}

fn check_bibliography(content: &str, errors: &mut Vec<String>, warnings: &mut Vec<String>) -> bool {
    let bib = match bib_section(content) {
        None => {
            errors.push("Missing 'Bibliography' section".into());
            return false;
        }
        Some(b) => b,
    };
    let trunc = [
        (r"\[\d+-\d+\]", "Citation range (e.g., [8-75])"),
        (r"(?i)Additional.*citations", "Phrase \"Additional citations\""),
        (r"would be included", "Phrase \"would be included\""),
        (r"\[\.\.\.continue", "Pattern \"[...continue\""),
        (r"\[Continue with", "Pattern \"[Continue with\""),
        (r"etc\.(?:[^\w]|$)", "Standalone \"etc.\""),
        (r"and so on", "Phrase \"and so on\""),
    ];
    for (pat, desc) in trunc {
        if Regex::new(pat).unwrap().is_match(&bib) {
            errors.push(format!("CRITICAL: Bibliography contains truncation placeholder: {desc}"));
            errors.push("   This makes the report UNUSABLE - complete bibliography required".into());
            return false;
        }
    }
    let entry_re = Regex::new(r"(?m)^\[(\d+)\]").unwrap();
    let bib_nums: Vec<i64> = entry_re
        .captures_iter(&bib)
        .filter_map(|c| c[1].parse().ok())
        .collect();
    if bib_nums.is_empty() {
        errors.push("Bibliography has no entries".into());
        return false;
    }
    let max = *bib_nums.iter().max().unwrap();
    let actual: HashSet<i64> = bib_nums.iter().copied().collect();
    let missing: Vec<i64> = (1..=max).filter(|n| !actual.contains(n)).collect();
    if !missing.is_empty() {
        errors.push(format!("Bibliography has gaps in numbering: missing {missing:?}"));
        return false;
    }
    let text_re = Regex::new(r"\[(\d+)\]").unwrap();
    let text_cites: HashSet<i64> = text_re
        .captures_iter(content)
        .filter_map(|c| c[1].parse().ok())
        .collect();
    let missing_in_bib: Vec<i64> = text_cites.difference(&actual).copied().collect();
    if !missing_in_bib.is_empty() {
        let mut m = missing_in_bib;
        m.sort();
        errors.push(format!("Citations missing from bibliography: {m:?}"));
        return false;
    }
    let unused: Vec<i64> = actual.difference(&text_cites).copied().collect();
    if !unused.is_empty() {
        let mut u = unused;
        u.sort();
        warnings.push(format!("Unused bibliography entries: {u:?}"));
    }
    true
}
