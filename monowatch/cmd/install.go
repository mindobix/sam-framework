package cmd

import (
	"flag"
	"fmt"
	"os"
	"path/filepath"
)

// hookScript is the content written to .git/hooks/pre-push.
// It locates the monowatch binary at runtime and calls `monowatch check`.
const hookScript = `#!/bin/sh
# SAM MonoWatch — installed by sam init / monowatch install
# Runs impact analysis before every push.
# To skip: git push --no-verify

MONOWATCH_BIN=$(which monowatch 2>/dev/null || echo "$HOME/.sam/bin/monowatch")

if [ ! -x "$MONOWATCH_BIN" ]; then
  echo "SAM: monowatch not found, skipping impact check"
  exit 0
fi

"$MONOWATCH_BIN" check --repo "$(git rev-parse --show-toplevel)"
exit $?
`

// samHookMarker is a unique string we embed in the hook so we can detect hooks
// that were NOT installed by SAM and avoid clobbering them.
const samHookMarker = "SAM MonoWatch — installed by sam init"

// newInstallCmd returns the `monowatch install` subcommand.
func newInstallCmd() *subcommand {
	return &subcommand{
		name: "install",
		run:  runInstall,
	}
}

func runInstall(args []string) error {
	fs := flag.NewFlagSet("install", flag.ContinueOnError)
	repoFlag := fs.String("repo", "", "path to the repo root (required)")
	force := fs.Bool("force", false, "overwrite an existing non-SAM hook")

	if err := fs.Parse(args); err != nil {
		if err == flag.ErrHelp {
			return nil
		}
		return err
	}

	repoRoot := *repoFlag
	if repoRoot == "" {
		var err error
		repoRoot, err = os.Getwd()
		if err != nil {
			return fmt.Errorf("cannot determine current directory: %w", err)
		}
	}

	// Verify this looks like a git repo.
	gitDir := filepath.Join(repoRoot, ".git")
	if _, err := os.Stat(gitDir); os.IsNotExist(err) {
		return fmt.Errorf("%s is not a git repository (no .git directory)", repoRoot)
	}

	hookDir := filepath.Join(gitDir, "hooks")
	if err := os.MkdirAll(hookDir, 0o755); err != nil {
		return fmt.Errorf("create hooks directory: %w", err)
	}

	hookPath := filepath.Join(hookDir, "pre-push")

	// Check for an existing hook we must not clobber.
	if existing, err := os.ReadFile(hookPath); err == nil {
		// File exists.
		if !containsMarker(string(existing)) {
			if !*force {
				return fmt.Errorf(
					"pre-push hook already exists and was not installed by SAM.\n"+
						"Inspect it at %s, then re-run with --force to replace it.",
					hookPath,
				)
			}
			// --force: back up the old hook.
			backupPath := hookPath + ".bak"
			if err := os.WriteFile(backupPath, existing, 0o755); err != nil {
				return fmt.Errorf("back up existing hook: %w", err)
			}
			fmt.Fprintf(os.Stderr, "Existing hook backed up to %s\n", backupPath)
		}
	}

	// Write the hook script.
	if err := os.WriteFile(hookPath, []byte(hookScript), 0o755); err != nil {
		return fmt.Errorf("write hook: %w", err)
	}

	// Ensure it is executable (WriteFile mode bits may be masked by umask on
	// some systems — chmod explicitly).
	if err := os.Chmod(hookPath, 0o755); err != nil {
		return fmt.Errorf("chmod hook: %w", err)
	}

	fmt.Fprintf(os.Stderr, "MonoWatch hook installed → %s\n", hookPath)
	fmt.Fprintln(os.Stderr, "Run 'git push --no-verify' to bypass the hook when needed.")
	return nil
}

// containsMarker reports whether the hook content was written by SAM.
func containsMarker(content string) bool {
	return len(content) >= len(samHookMarker) &&
		contains(content, samHookMarker)
}

// contains is strings.Contains without importing strings.
func contains(s, substr string) bool {
	for i := 0; i <= len(s)-len(substr); i++ {
		if s[i:i+len(substr)] == substr {
			return true
		}
	}
	return false
}
