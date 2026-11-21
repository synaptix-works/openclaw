//! research_engine port: state scaffold + phase listing.
//! The agent is the orchestrator; this only persists state and prints the
//! phase sequence for a mode. Phase instruction templates live in
//! reference/methodology.md.

use crate::util::*;
use serde_json::json;

pub fn phases_for_mode(mode: &str) -> Vec<&'static str> {
    match mode {
        "quick" => vec!["scope", "retrieve", "package"],
        "standard" => vec!["scope", "plan", "retrieve", "triangulate", "synthesize", "package"],
        "deep" | "ultradeep" => vec![
            "scope", "plan", "retrieve", "triangulate", "outline", "synthesize", "critique",
            "refine", "package",
        ],
        _ => vec!["scope", "plan", "retrieve", "triangulate", "synthesize", "package"],
    }
}

pub fn cmd_phases(mode: &str) {
    print_json(&json!({"mode": mode, "phases": phases_for_mode(mode)}));
}

pub fn cmd_init(dir: &str, query: &str, mode: &str) {
    std::fs::create_dir_all(dir).ok();
    let state = json!({
        "query": query,
        "mode": mode,
        "phase": "scope",
        "scope": {}, "plan": {}, "sources": [], "findings": [],
        "synthesis": {}, "critique": {}, "report": "",
        "metadata": {"started_at": utc_now_iso(), "version": "1.0"},
        "phases": phases_for_mode(mode),
    });
    let path = artifact(dir, "research_state.json");
    std::fs::write(&path, serde_json::to_string_pretty(&state).unwrap() + "\n").ok();
    print_compact(&json!({"status":"ok","state":path}));
}

pub fn cmd_show(file: &str) {
    match std::fs::read_to_string(file) {
        Ok(s) => match serde_json::from_str::<serde_json::Value>(&s) {
            Ok(v) => print_json(&v),
            Err(e) => die(&format!("invalid state file: {e}")),
        },
        Err(e) => die(&format!("cannot read state file: {e}")),
    }
}

/// Update the "phase" field of research_state.json (checkpoint advance).
pub fn cmd_set_phase(dir: &str, phase: &str) {
    let path = artifact(dir, "research_state.json");
    let Ok(s) = std::fs::read_to_string(&path) else {
        die("research_state.json not found; run `dr state init` first")
    };
    let Ok(mut v) = serde_json::from_str::<serde_json::Value>(&s) else {
        die("invalid research_state.json")
    };
    if let Some(obj) = v.as_object_mut() {
        obj.insert("phase".into(), json!(phase));
        obj.insert("phase_updated_at".into(), json!(utc_now_iso()));
    }
    std::fs::write(&path, serde_json::to_string_pretty(&v).unwrap() + "\n").ok();
    print_compact(&json!({"status":"ok","phase":phase,"state":path}));
}
