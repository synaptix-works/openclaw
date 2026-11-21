//! md_to_html + verify_html ports: markdown→HTML conversion and HTML validation.

use regex::Regex;
use std::path::Path;

pub fn convert_markdown_to_html(markdown_text: &str) -> (String, String) {
    let mut parts = markdown_text.splitn(2, "## Bibliography");
    let content_md = parts.next().unwrap_or("");
    let bibliography_md = parts.next().unwrap_or("");
    (
        convert_content_section(content_md),
        convert_bibliography_section(bibliography_md),
    )
}

fn convert_content_section(markdown: &str) -> String {
    // Skip everything until first "## " heading
    let mut processed: Vec<&str> = Vec::new();
    let mut skipping = true;
    for line in markdown.split('\n') {
        if skipping {
            if line.starts_with("## ") && !line.starts_with("### ") {
                skipping = false;
                processed.push(line);
            }
            continue;
        }
        processed.push(line);
    }
    let mut html = processed.join("\n");

    let h2 = Regex::new(r"(?m)^## (.+)$").unwrap();
    html = h2
        .replace_all(&html, r#"<div class="section"><h2 class="section-title">$1</h2>"#)
        .to_string();
    let h3 = Regex::new(r"(?m)^### (.+)$").unwrap();
    html = h3
        .replace_all(&html, r#"<h3 class="subsection-title">$1</h3>"#)
        .to_string();
    let h4 = Regex::new(r"(?m)^#### (.+)$").unwrap();
    html = h4
        .replace_all(&html, r#"<h4 class="subsubsection-title">$1</h4>"#)
        .to_string();
    html = Regex::new(r"\*\*(.+?)\*\*")
        .unwrap()
        .replace_all(&html, "<strong>$1</strong>")
        .to_string();
    html = Regex::new(r"\*(.+?)\*")
        .unwrap()
        .replace_all(&html, "<em>$1</em>")
        .to_string();
    html = Regex::new(r"`(.+?)`")
        .unwrap()
        .replace_all(&html, "<code>$1</code>")
        .to_string();

    html = convert_lists(&html);
    html = convert_tables(&html);
    html = convert_paragraphs(&html);
    html = close_sections(&html);

    html = html.replacen(
        r#"<h2 class="section-title">Executive Summary</h2>"#,
        r#"<div class="executive-summary"><h2 class="section-title">Executive Summary</h2>"#,
        1,
    );
    if html.contains(r#"<div class="executive-summary">"#) {
        html = html.replacen("</h2>\n<div class=\"section\">", "</h2></div>\n<div class=\"section\">", 1);
    }
    html
}

fn convert_bibliography_section(markdown: &str) -> String {
    if markdown.trim().is_empty() {
        return String::new();
    }
    let re = Regex::new(r"\[(\d+)\]\s*(.+?)\s*-\s*(https?://[^\s\)]+)").unwrap();
    let mut html = re
        .replace_all(
            markdown,
            r#"<div class="bib-entry"><span class="bib-number">[$1]</span> <a href="$3" target="_blank">$2</a></div>"#,
        )
        .to_string();
    html = Regex::new(r"\*\*(.+?)\*\*")
        .unwrap()
        .replace_all(&html, "<strong>$1</strong>")
        .to_string();
    format!(r#"<div class="bibliography-content">{html}</div>"#)
}

fn convert_lists(html: &str) -> String {
    let ordered = Regex::new(r"^\d+\.\s").unwrap();
    let mut result: Vec<String> = Vec::new();
    let mut in_list = false;
    let mut list_level = 0usize;
    for line in html.split('\n') {
        let stripped = line.trim();
        if stripped.starts_with("- ") || stripped.starts_with("* ") {
            if !in_list {
                result.push("<ul>".into());
                in_list = true;
                list_level = line.len() - line.trim_start().len();
            }
            result.push(format!("<li>{}</li>", &stripped[2..]));
        } else if ordered.is_match(stripped) {
            if !in_list {
                result.push("<ol>".into());
                in_list = true;
                list_level = line.len() - line.trim_start().len();
            }
            let content = ordered.replace(stripped, "");
            result.push(format!("<li>{content}</li>"));
        } else {
            if in_list {
                let current_level = line.len() - line.trim_start().len();
                if current_level > list_level && !stripped.is_empty() {
                    if let Some(last) = result.last_mut() {
                        if last.ends_with("</li>") {
                            let inner = last.trim_end_matches("</li>").to_string();
                            *last = format!("{inner} {stripped}</li>");
                            continue;
                        }
                    }
                    continue;
                }
                let close = if result.iter().rev().take(10).any(|l| l.contains("<ul>")) {
                    "</ul>"
                } else {
                    "</ol>"
                };
                result.push(close.into());
                in_list = false;
                list_level = 0;
            }
            result.push(line.to_string());
        }
    }
    if in_list {
        let close = if result.iter().rev().take(10).any(|l| l.contains("<ul>")) {
            "</ul>"
        } else {
            "</ol>"
        };
        result.push(close.into());
    }
    result.join("\n")
}

fn convert_tables(html: &str) -> String {
    let mut result: Vec<String> = Vec::new();
    let mut in_table = false;
    for line in html.split('\n') {
        if line.contains('|') && line.trim_start().starts_with('|') {
            if !in_table {
                result.push("<table>".into());
                in_table = true;
                let cells: Vec<&str> = line.split('|').collect();
                let inner = &cells[1..cells.len().saturating_sub(1).max(1)];
                result.push("<thead><tr>".into());
                for cell in inner {
                    result.push(format!("<th>{}</th>", cell.trim()));
                }
                result.push("</tr></thead>".into());
                result.push("<tbody>".into());
            } else if line.contains("---") {
                continue;
            } else {
                let cells: Vec<&str> = line.split('|').collect();
                let inner = &cells[1..cells.len().saturating_sub(1).max(1)];
                result.push("<tr>".into());
                for cell in inner {
                    result.push(format!("<td>{}</td>", cell.trim()));
                }
                result.push("</tr>".into());
            }
        } else {
            if in_table {
                result.push("</tbody></table>".into());
                in_table = false;
            }
            result.push(line.to_string());
        }
    }
    if in_table {
        result.push("</tbody></table>".into());
    }
    result.join("\n")
}

fn convert_paragraphs(html: &str) -> String {
    let mut result: Vec<String> = Vec::new();
    let mut in_paragraph = false;
    for line in html.split('\n') {
        let stripped = line.trim();
        if stripped.is_empty() {
            if in_paragraph {
                result.push("</p>".into());
                in_paragraph = false;
            }
            result.push(line.to_string());
            continue;
        }
        let is_html = (stripped.starts_with('<') && stripped.ends_with('>'))
            || stripped.starts_with("</")
            || ["<h", "<div", "<ul", "<ol", "<li", "<table", "</div>", "</ul>", "</ol>"]
                .iter()
                .any(|t| stripped.contains(t));
        if is_html {
            if in_paragraph {
                result.push("</p>".into());
                in_paragraph = false;
            }
            result.push(line.to_string());
            continue;
        }
        if !in_paragraph {
            result.push(format!("<p>{line}"));
            in_paragraph = true;
        } else {
            result.push(line.to_string());
        }
    }
    if in_paragraph {
        result.push("</p>".into());
    }
    result.join("\n")
}

fn close_sections(html: &str) -> String {
    let mut result: Vec<String> = Vec::new();
    let mut section_open = false;
    for line in html.split('\n') {
        if line.contains(r#"<div class="section">"#) {
            if section_open {
                result.push("</div>".into());
            }
            section_open = true;
        }
        result.push(line.to_string());
    }
    if section_open {
        result.push("</div>".into());
    }
    result.join("\n")
}

pub fn cmd_render(md_file: &str) {
    let path = Path::new(md_file);
    if !path.exists() {
        eprintln!("Error: File {md_file} not found");
        std::process::exit(1);
    }
    let markdown = std::fs::read_to_string(path).unwrap();
    let (content, bib) = convert_markdown_to_html(&markdown);
    println!("=== CONTENT HTML ===");
    println!("{}", &content[..content.len().min(1000)]);
    println!("\n=== BIBLIOGRAPHY HTML ===");
    println!("{}", &bib[..bib.len().min(500)]);
}

pub fn cmd_verify(html_path: &str, md_path: &str) {
    let (hp, mp) = (Path::new(html_path), Path::new(md_path));
    if !hp.exists() {
        eprintln!("Error: HTML file not found: {html_path}");
        std::process::exit(1);
    }
    if !mp.exists() {
        eprintln!("Error: Markdown file not found: {md_path}");
        std::process::exit(1);
    }
    let html = std::fs::read_to_string(hp).unwrap();
    let md = std::fs::read_to_string(mp).unwrap();
    let mut errors: Vec<String> = Vec::new();
    let mut warnings: Vec<String> = Vec::new();

    println!("\n{}\nHTML REPORT VERIFICATION\n{}\n", "=".repeat(60), "=".repeat(60));

    // Sections
    let md_sections: Vec<String> = Regex::new(r"(?m)^## (.+)$")
        .unwrap()
        .captures_iter(&md)
        .map(|c| c[1].to_string())
        .collect();
    let html_sections: Vec<String> = Regex::new(r#"<h2 class="section-title">(.+?)</h2>"#)
        .unwrap()
        .captures_iter(&html)
        .map(|c| c[1].to_string())
        .collect();
    let placeholders = Regex::new(r#"<div class="section">#</div>"#).unwrap().find_iter(&html).count();
    if placeholders > 0 {
        errors.push(format!(
            "Found {placeholders} placeholder sections (empty '#' divs) - content not converted properly"
        ));
    }
    if md_sections.len() > html_sections.len() + 1 {
        errors.push(format!(
            "Section count mismatch: MD has {} sections, HTML has only {} + bibliography",
            md_sections.len(),
            html_sections.len()
        ));
        let missing: Vec<&String> = md_sections.iter().filter(|s| !html_sections.contains(s)).collect();
        if !missing.is_empty() {
            errors.push(format!("Missing sections in HTML: {missing:?}"));
        }
    }
    if md.contains("Executive Summary") && !html.contains("Executive Summary") {
        errors.push("Executive Summary missing from HTML".into());
    }

    // Placeholders
    for p in [
        "{{TITLE}}", "{{DATE}}", "{{CONTENT}}", "{{BIBLIOGRAPHY}}", "{{METRICS_DASHBOARD}}",
        "{{SOURCE_COUNT}}", "TODO", "TBD", "PLACEHOLDER", "FIXME",
    ] {
        if html.contains(p) {
            errors.push(format!("Found unreplaced placeholder: {p}"));
        }
    }

    // Emojis
    let emoji_re = Regex::new(r"[\x{1F600}-\x{1F64F}\x{1F300}-\x{1F5FF}\x{1F680}-\x{1F6FF}\x{1F1E0}-\x{1F1FF}\x{2702}-\x{27B0}\x{24C2}-\x{1F251}]+").unwrap();
    let emojis: Vec<&str> = emoji_re.find_iter(&html).map(|m| m.as_str()).collect();
    if !emojis.is_empty() {
        let unique: std::collections::HashSet<&&str> = emojis.iter().collect();
        errors.push(format!("Found {} emojis in HTML (should be none): {unique:?}", emojis.len()));
    }

    // Structure
    for (el, name) in [
        ("<html", "HTML tag"), ("<head", "head tag"), ("<body", "body tag"),
        ("<title>", "title tag"), ("class=\"header\"", "header section"),
        ("class=\"content\"", "content section"), ("class=\"bibliography\"", "bibliography section"),
    ] {
        if !html.contains(el) {
            errors.push(format!("Missing {name} in HTML"));
        }
    }
    let open_divs = html.matches("<div").count();
    let close_divs = html.matches("</div>").count();
    if open_divs.abs_diff(close_divs) > 2 {
        warnings.push(format!("Possible unclosed divs: {open_divs} opening tags, {close_divs} closing tags"));
    }

    // Citations
    let cite_re = Regex::new(r"\[(\d+)\]").unwrap();
    let md_cites: std::collections::HashSet<String> =
        cite_re.captures_iter(&md).map(|c| c[1].to_string()).collect();
    let html_content = html.split("class=\"bibliography\"").next().unwrap_or(&html);
    let html_cites: std::collections::HashSet<String> =
        cite_re.captures_iter(html_content).map(|c| c[1].to_string()).collect();
    if !md_cites.is_empty() && html_cites.is_empty() {
        errors.push("No citations found in HTML content (but present in MD)".into());
    }
    if md_cites.len() as f64 > html_cites.len() as f64 * 1.5 {
        warnings.push(format!(
            "Fewer citations in HTML ({}) than MD ({})",
            html_cites.len(),
            md_cites.len()
        ));
    }

    // Bibliography
    if md.contains("## Bibliography") {
        if !html.contains("class=\"bibliography\"") {
            errors.push("Bibliography section missing from HTML".into());
        } else if !html.contains("class=\"bib-entry\"") {
            warnings.push("Bibliography present but entries not properly formatted".into());
        }
    }

    // Results
    println!("{}\nVERIFICATION RESULTS\n{}\n", "-".repeat(60), "-".repeat(60));
    if !errors.is_empty() {
        println!("ERRORS ({}):", errors.len());
        for (i, e) in errors.iter().enumerate() {
            println!("  {}. {}", i + 1, e);
        }
        println!();
    }
    if !warnings.is_empty() {
        println!("WARNINGS ({}):", warnings.len());
        for (i, w) in warnings.iter().enumerate() {
            println!("  {}. {}", i + 1, w);
        }
        println!();
    }
    if errors.is_empty() && warnings.is_empty() {
        println!("All checks passed! HTML report is valid.\n");
    }
    println!("{}\n", "-".repeat(60));
    std::process::exit(if errors.is_empty() { 0 } else { 1 });
}
