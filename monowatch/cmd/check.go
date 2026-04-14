package cmd

import (
	"flag"
	"fmt"
	"os"
	"time"

	"github.com/sam-framework/monowatch/internal"
)

// hookTimeout is the hard cap on the MonoGraph HTTP call.
const hookTimeout = 2 * time.Second

// newCheckCmd returns the `monowatch check` subcommand.
//
// This is the command invoked by the pre-push hook:
//
//	monowatch check --repo "$(git rev-parse --show-toplevel)"
func newCheckCmd() *subcommand {
	return &subcommand{
		name: "check",
		run:  runCheck,
	}
}

func runCheck(args []string) error {
	fs := flag.NewFlagSet("check", flag.ContinueOnError)
	repoFlag := fs.String("repo", "", "path to the repo root (default: current directory)")
	dryRun := fs.Bool("dry-run", false, "print impact report but always exit 0")
	monographURL := fs.String("monograph-url", "", "MonoGraph base URL (default: http://127.0.0.1:7474)")

	if err := fs.Parse(args); err != nil {
		if err == flag.ErrHelp {
			return nil
		}
		return err
	}

	// Resolve repo root.
	repoRoot := *repoFlag
	if repoRoot == "" {
		var err error
		repoRoot, err = internal.TopLevel(".")
		if err != nil {
			// Not inside a git repo — skip silently.
			return nil
		}
	}

	// Load config — never fail on missing file.
	cfg, cfgErr := internal.LoadConfig(repoRoot)
	if cfgErr != nil {
		// Malformed config: warn but apply safe defaults so the push is never blocked.
		fmt.Fprintf(os.Stderr, "SAM MonoWatch: could not read .sam/config.yaml: %v\n", cfgErr)
		cfg = internal.Config{} // zero value = safe defaults (BlockOnCritical: false)
	}

	// Get changed files from unpushed commits.
	changedFiles, err := internal.GetUnpushedChangedFiles(repoRoot)
	if err != nil || len(changedFiles) == 0 {
		internal.NoImpact(os.Stderr)
		return nil
	}

	// Apply skip_domains filter — remove files that belong to skipped domains.
	changedFiles = filterSkippedFiles(changedFiles, cfg.MonoWatch.SkipDomains)
	if len(changedFiles) == 0 {
		internal.NoImpact(os.Stderr)
		return nil
	}

	// Query MonoGraph.
	baseURL := *monographURL
	result, err := internal.QueryImpact(baseURL, changedFiles, hookTimeout)
	if err != nil {
		// Daemon not running — advisory only, never block.
		internal.DaemonUnreachable(os.Stderr)
		return nil
	}

	// Nothing affected across domain boundaries — clean exit.
	if len(result.Entries) == 0 {
		internal.NoImpact(os.Stderr)
		return nil
	}

	// Render table.
	blocked := !*dryRun && cfg.MonoWatch.BlockOnCritical && result.HasRisk("critical")
	internal.ImpactTable(os.Stderr, result, changedFiles, cfg.MonoWatch.BlockOnCritical, blocked)

	// Exit code.
	if blocked {
		os.Exit(1)
	}
	return nil
}

// filterSkippedFiles removes files that start with any of the skipped domain
// prefixes.  If skipDomains is empty every file passes through.
func filterSkippedFiles(files []string, skipDomains []string) []string {
	if len(skipDomains) == 0 {
		return files
	}
	out := files[:0:len(files)]
	for _, f := range files {
		skip := false
		for _, d := range skipDomains {
			if len(f) >= len(d) && f[:len(d)] == d {
				skip = true
				break
			}
		}
		if !skip {
			out = append(out, f)
		}
	}
	return out
}
