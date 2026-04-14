use std::env;
use std::path::PathBuf;
use std::process;

use clap::{Parser, Subcommand};

mod commands;

#[derive(Parser)]
#[command(
    name = "sam",
    about = "Sparse API Monolith — work in a monorepo without downloading everything",
    version
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// Path to repo (defaults to current directory)
    #[arg(long, global = true)]
    repo: Option<PathBuf>,
}

#[derive(Subcommand)]
enum Commands {
    /// Clone a monorepo and initialize SAM workspace
    Init {
        /// Git URL of the repository
        url: String,
    },
    /// Activate a profile (hydrate its domains)
    Use {
        /// Profile name (or unique prefix)
        #[arg(long)]
        profile: String,

        /// Show what would be hydrated without doing it
        #[arg(long, default_value_t = false)]
        dry_run: bool,
    },
    /// Fetch a single domain into the working tree
    Fetch {
        /// Domain path (e.g. apis/sales)
        domain: String,

        /// Also fetch resolved dependencies
        #[arg(long, default_value_t = false)]
        with_deps: bool,

        /// Re-fetch even if already hydrated
        #[arg(long, default_value_t = false)]
        force: bool,
    },
    /// Show what fetching a domain would hydrate
    Plan {
        /// Domain path (e.g. apis/sales)
        domain: String,
    },
    /// Show impact analysis for changed files
    Impact {
        /// Output format: table or json
        #[arg(long, default_value = "table")]
        format: String,
    },
    /// Deploy domains in a profile
    Deploy {
        /// Profile name (or unique prefix)
        #[arg(long)]
        profile: String,
    },
    /// Show dependency graph for a domain
    Graph {
        /// Domain path (defaults to active profile's first domain)
        #[arg(long)]
        domain: Option<String>,

        /// Output format: tree or json
        #[arg(long)]
        output: Option<String>,
    },
    /// Set up Finder integration (ghost folders + auto-hydrate on open)
    Setup {
        /// Max directory depth for ghost folders (default: 4)
        #[arg(long, default_value_t = 4)]
        depth: usize,
    },
    /// Refresh Finder tags based on current hydration state
    Refresh,
    /// Watch Finder and auto-hydrate when you navigate into a ghost folder
    Watch,
    /// Dehydrate a domain (remove files, re-hide folder)
    Dehydrate {
        /// Domain to dehydrate (e.g. bigquery)
        domain: Option<String>,

        /// Dehydrate all hydrated domains
        #[arg(long, default_value_t = false)]
        all: bool,
    },
}

fn resolve_repo(repo_arg: Option<PathBuf>) -> anyhow::Result<PathBuf> {
    match repo_arg {
        Some(p) => Ok(p),
        None => {
            let cwd = env::current_dir()?;
            Ok(sam_core::profile::find_repo_root(&cwd)?)
        }
    }
}

fn main() {
    let cli = Cli::parse();

    let result = match cli.command {
        Commands::Init { ref url } => commands::init::run(url),
        Commands::Use {
            ref profile,
            dry_run,
        } => {
            let repo = require_repo(cli.repo.clone());
            commands::use_cmd::run(&repo, profile, dry_run)
        }
        Commands::Fetch {
            ref domain,
            with_deps,
            force,
        } => {
            let repo = require_repo(cli.repo.clone());
            commands::fetch::run(&repo, domain, with_deps, force)
        }
        Commands::Plan { ref domain } => {
            let repo = require_repo(cli.repo.clone());
            commands::plan::run(&repo, domain)
        }
        Commands::Impact { ref format } => {
            let repo = require_repo(cli.repo.clone());
            commands::impact::run(&repo, format)
        }
        Commands::Deploy { ref profile } => {
            let repo = require_repo(cli.repo.clone());
            commands::deploy::run(&repo, profile)
        }
        Commands::Graph {
            ref domain,
            ref output,
        } => {
            let repo = require_repo(cli.repo.clone());
            commands::graph::run(&repo, domain.as_deref(), output.as_deref())
        }
        Commands::Setup { depth } => {
            let repo = require_repo(cli.repo.clone());
            commands::setup::run(&repo, depth)
        }
        Commands::Refresh => {
            let repo = require_repo(cli.repo.clone());
            commands::refresh::run(&repo)
        }
        Commands::Watch => {
            let repo = require_repo(cli.repo.clone());
            commands::watch::run(&repo)
        }
        Commands::Dehydrate {
            ref domain,
            all,
        } => {
            let repo = require_repo(cli.repo.clone());
            commands::dehydrate::run(&repo, domain.as_deref(), all)
        }
    };

    if let Err(e) = result {
        eprintln!("{}", sam_core::output::error_msg(&format!("{e}")));
        process::exit(1);
    }
}

fn require_repo(repo_arg: Option<PathBuf>) -> PathBuf {
    match resolve_repo(repo_arg) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("{}", sam_core::output::error_msg(&format!("{e}")));
            process::exit(1);
        }
    }
}
