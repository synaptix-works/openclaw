//! dr — Deep Research evidence substrate CLI.
//! Rust port of the deep-research skill's Python scripts.

mod citation;
mod citeverify;
mod claims;
mod eval;
mod evidence;
mod mdhtml;
mod state;
mod support;
mod util;
mod validate;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "dr", version, about = "Deep Research evidence substrate")]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Run manifest + artifact initialization
    Run {
        #[command(subcommand)]
        sub: RunCmd,
    },
    /// Source registration, display numbers, bibliography, credibility
    Source {
        #[command(subcommand)]
        sub: SourceCmd,
    },
    /// Append-only evidence store
    Evidence {
        #[command(subcommand)]
        sub: EvidenceCmd,
    },
    /// Atomic claim ledger
    Claims {
        #[command(subcommand)]
        sub: ClaimsCmd,
    },
    /// Claim-support verification
    Support {
        #[command(subcommand)]
        sub: SupportCmd,
    },
    /// Report structure validation
    Report {
        #[command(subcommand)]
        sub: ReportCmd,
    },
    /// Bibliography citation verification (DOI/URL/hallucination)
    Citations {
        #[command(subcommand)]
        sub: CitationsCmd,
    },
    /// Markdown→HTML render and HTML verification
    Html {
        #[command(subcommand)]
        sub: HtmlCmd,
    },
    /// Research state scaffold
    State {
        #[command(subcommand)]
        sub: StateCmd,
    },
    /// Print the phase sequence for a mode
    Phases {
        #[arg(long, default_value = "standard")]
        mode: String,
    },
}

#[derive(Subcommand)]
enum RunCmd {
    /// Create run_manifest.json + empty artifact files
    Init {
        #[arg(long)]
        out_dir: String,
        #[arg(long, default_value = "")]
        query: String,
        #[arg(long, default_value = "standard", value_parser = ["quick","standard","deep","ultradeep"])]
        mode: String,
    },
}

#[derive(Subcommand)]
enum SourceCmd {
    /// Register a source, return stable source_id
    Register {
        #[arg(long)]
        dir: String,
        #[arg(long)]
        json: String,
    },
    /// Map stable source_ids to display numbers
    Numbers {
        #[arg(long)]
        dir: String,
    },
    /// Export bibliography
    Bibliography {
        #[arg(long)]
        dir: String,
        #[arg(long, default_value = "markdown", value_parser = ["markdown","json"])]
        style: String,
    },
    /// Credibility-score a source
    Evaluate {
        #[arg(long)]
        url: String,
        #[arg(long)]
        title: String,
        #[arg(long)]
        date: Option<String>,
        #[arg(long)]
        author: Option<String>,
        #[arg(long)]
        content: Option<String>,
    },
}

#[derive(Subcommand)]
enum EvidenceCmd {
    /// Create empty evidence.jsonl
    Init {
        #[arg(long)]
        dir: String,
    },
    /// Append an evidence row
    Add {
        #[arg(long)]
        dir: String,
        #[arg(long)]
        json: String,
    },
    /// List evidence rows
    List {
        #[arg(long)]
        dir: String,
        #[arg(long)]
        source_id: Option<String>,
    },
    /// Export evidence as JSON array
    Export {
        #[arg(long)]
        dir: String,
    },
}

#[derive(Subcommand)]
enum ClaimsCmd {
    /// Extract atomic claims from report.md
    Extract {
        #[arg(long)]
        dir: String,
        #[arg(long)]
        report: String,
    },
    /// Manually add a claim
    Add {
        #[arg(long)]
        dir: String,
        #[arg(long)]
        json: String,
    },
    /// List claims
    List {
        #[arg(long)]
        dir: String,
        #[arg(long)]
        section: Option<String>,
        #[arg(long = "type")]
        ctype: Option<String>,
        #[arg(long)]
        status: Option<String>,
    },
    /// Claim statistics
    Stats {
        #[arg(long)]
        dir: String,
    },
}

#[derive(Subcommand)]
enum SupportCmd {
    /// Verify claims against evidence, rewrite claims.jsonl
    Verify {
        #[arg(long)]
        dir: String,
        #[arg(long)]
        strict: bool,
    },
    /// Human-readable verification report
    Report {
        #[arg(long)]
        dir: String,
    },
}

#[derive(Subcommand)]
enum ReportCmd {
    /// 9-check quality validator
    Validate {
        #[arg(long)]
        report: String,
    },
}

#[derive(Subcommand)]
enum CitationsCmd {
    /// DOI/URL/hallucination verification
    Verify {
        #[arg(long)]
        report: String,
        #[arg(long)]
        strict: bool,
        /// Skip network checks
        #[arg(long)]
        offline: bool,
    },
}

#[derive(Subcommand)]
enum HtmlCmd {
    /// Convert markdown report to HTML fragments
    Render {
        md_file: String,
    },
    /// Verify HTML report against its markdown source
    Verify {
        #[arg(long)]
        html: String,
        #[arg(long)]
        md: String,
    },
}

#[derive(Subcommand)]
enum StateCmd {
    /// Initialize research_state.json
    Init {
        #[arg(long)]
        dir: String,
        #[arg(long)]
        query: String,
        #[arg(long, default_value = "standard")]
        mode: String,
    },
    /// Print a saved state file
    Show {
        #[arg(long)]
        file: String,
    },
    /// Advance the phase checkpoint
    SetPhase {
        #[arg(long)]
        dir: String,
        #[arg(long)]
        phase: String,
    },
}

fn main() {
    let cli = Cli::parse();
    match cli.cmd {
        Cmd::Run { sub } => match sub {
            RunCmd::Init { out_dir, query, mode } => citation::cmd_init_run(&out_dir, &query, &mode),
        },
        Cmd::Source { sub } => match sub {
            SourceCmd::Register { dir, json } => citation::cmd_register_source(&dir, &json),
            SourceCmd::Numbers { dir } => citation::cmd_display_numbers(&dir),
            SourceCmd::Bibliography { dir, style } => citation::cmd_bibliography(&dir, &style),
            SourceCmd::Evaluate { url, title, date, author, content } => {
                eval::cmd_evaluate(&url, &title, date.as_deref(), author.as_deref(), content.as_deref())
            }
        },
        Cmd::Evidence { sub } => match sub {
            EvidenceCmd::Init { dir } => evidence::cmd_init(&dir),
            EvidenceCmd::Add { dir, json } => evidence::cmd_add(&dir, &json),
            EvidenceCmd::List { dir, source_id } => evidence::cmd_list(&dir, source_id.as_deref()),
            EvidenceCmd::Export { dir } => evidence::cmd_export(&dir),
        },
        Cmd::Claims { sub } => match sub {
            ClaimsCmd::Extract { dir, report } => claims::cmd_extract(&dir, &report),
            ClaimsCmd::Add { dir, json } => claims::cmd_add(&dir, &json),
            ClaimsCmd::List { dir, section, ctype, status } => {
                claims::cmd_list(&dir, section.as_deref(), ctype.as_deref(), status.as_deref())
            }
            ClaimsCmd::Stats { dir } => claims::cmd_stats(&dir),
        },
        Cmd::Support { sub } => match sub {
            SupportCmd::Verify { dir, strict } => support::cmd_verify(&dir, strict),
            SupportCmd::Report { dir } => support::cmd_report(&dir),
        },
        Cmd::Report { sub } => match sub {
            ReportCmd::Validate { report } => validate::cmd_validate(&report),
        },
        Cmd::Citations { sub } => match sub {
            CitationsCmd::Verify { report, strict, offline } => {
                citeverify::cmd_verify(&report, strict, offline)
            }
        },
        Cmd::Html { sub } => match sub {
            HtmlCmd::Render { md_file } => mdhtml::cmd_render(&md_file),
            HtmlCmd::Verify { html, md } => mdhtml::cmd_verify(&html, &md),
        },
        Cmd::State { sub } => match sub {
            StateCmd::Init { dir, query, mode } => state::cmd_init(&dir, &query, &mode),
            StateCmd::Show { file } => state::cmd_show(&file),
            StateCmd::SetPhase { dir, phase } => state::cmd_set_phase(&dir, &phase),
        },
        Cmd::Phases { mode } => state::cmd_phases(&mode),
    }
}
