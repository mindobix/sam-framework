use std::path::Path;
use std::time::Duration;

use sam_core::monograph::Client;
use sam_core::output;
use sam_core::profile;

const BLUE: &str = "\x1b[34m";
const GREEN: &str = "\x1b[32m";
const YELLOW: &str = "\x1b[33m";
const RESET: &str = "\x1b[0m";
const BOLD: &str = "\x1b[1m";
const DIM: &str = "\x1b[2m";

pub fn run(repo: &Path, domain: Option<&str>, output_format: Option<&str>) -> anyhow::Result<()> {
    let domain = match domain {
        Some(d) => d.to_string(),
        None => {
            let state = sam_core::workspace::load(repo)?;
            if let Some(ref profile_name) = state.active_profile {
                let profiles = profile::load_profiles(repo)?;
                let prof = profile::get_profile(&profiles, profile_name)?;
                match &prof.domains {
                    profile::Domains::List(list) if !list.is_empty() => list[0].clone(),
                    _ => anyhow::bail!("no domain specified. Use --domain"),
                }
            } else if !state.hydrated_domains.is_empty() {
                state.hydrated_domains[0].clone()
            } else {
                anyhow::bail!("no domain specified. Use --domain");
            }
        }
    };

    let repo_config = profile::load_repo_config(repo)?;
    let client = Client::new(&repo_config.monograph.address, Duration::from_secs(5));

    let mut spinner = output::Spinner::new(&format!("Loading graph for {}...", domain));

    let graph = match client.graph(&domain) {
        Ok(resp) => {
            spinner.stop_with_message(&output::success("Graph loaded"));
            resp
        }
        Err(e) => {
            spinner.stop_with_message(&output::error_msg(&format!("MonoGraph unavailable: {e}")));
            eprintln!(
                "{}",
                output::hint("Start MonoGraph for dependency graph: monograph serve --port 7474")
            );
            return Ok(());
        }
    };

    if output_format == Some("json") {
        println!("{}", serde_json::to_string_pretty(&graph)?);
        return Ok(());
    }

    // Render edge list as a tree
    eprintln!(
        "{}",
        output::header(&format!("Dependency graph: {}", output::domain(&domain)))
    );

    eprintln!("  {BLUE}{BOLD}{}{RESET} (root)", graph.domain);

    let edge_count = graph.edges.len();
    for (i, edge) in graph.edges.iter().enumerate() {
        let is_last = i == edge_count - 1;
        let connector = if is_last { "└── " } else { "├── " };

        let (color, type_label) = match edge.edge_type.as_deref() {
            Some("import") => (GREEN, "import"),
            Some("cochange") => (YELLOW, "co-change"),
            Some(t) => (RESET, t),
            None => (RESET, "dep"),
        };

        let score_str = match edge.weight {
            Some(w) if w < 1.0 => format!(" {DIM}score: {:.2}{RESET}", w),
            _ => String::new(),
        };

        let commit_str = match edge.commit_count {
            Some(c) if c > 0 => format!(" {DIM}({} commits){RESET}", c),
            _ => String::new(),
        };

        eprintln!(
            "  {connector}{color}{}{RESET} ({type_label}){score_str}{commit_str}",
            edge.to
        );
    }

    eprintln!();
    eprintln!(
        "{}",
        output::info(&format!("{} dependencies from {}", edge_count, graph.domain))
    );

    Ok(())
}
