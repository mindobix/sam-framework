package internal

import (
	"bytes"
	"fmt"
	"os/exec"
	"path/filepath"
	"strings"
)

// gitCmd runs a git command in repoRoot and returns trimmed stdout.
// Stderr is captured and included in the error.
func gitCmd(repoRoot string, args ...string) (string, error) {
	cmd := exec.Command("git", args...)
	cmd.Dir = repoRoot

	var stdout, stderr bytes.Buffer
	cmd.Stdout = &stdout
	cmd.Stderr = &stderr

	if err := cmd.Run(); err != nil {
		msg := strings.TrimSpace(stderr.String())
		if msg == "" {
			msg = err.Error()
		}
		return "", fmt.Errorf("git %s: %s", strings.Join(args, " "), msg)
	}

	return strings.TrimSpace(stdout.String()), nil
}

// splitLines splits newline-separated output into non-empty trimmed lines.
func splitLines(raw string) []string {
	var lines []string
	for _, l := range strings.Split(raw, "\n") {
		if t := strings.TrimSpace(l); t != "" {
			lines = append(lines, t)
		}
	}
	return lines
}

// TopLevel returns the absolute repo root by running `git rev-parse --show-toplevel`.
func TopLevel(startPath string) (string, error) {
	abs, err := filepath.Abs(startPath)
	if err != nil {
		return "", err
	}
	out, err := gitCmd(abs, "rev-parse", "--show-toplevel")
	if err != nil {
		return "", err
	}
	return out, nil
}

// CurrentBranch returns the abbreviated name of HEAD.
func CurrentBranch(repoRoot string) (string, error) {
	return gitCmd(repoRoot, "rev-parse", "--abbrev-ref", "HEAD")
}

// remoteTrackingBranch returns the remote-tracking branch for the given local
// branch, e.g. "origin/main".  Returns ("", nil) if none is configured.
func remoteTrackingBranch(repoRoot, branch string) (string, error) {
	out, err := gitCmd(repoRoot,
		"rev-parse", "--abbrev-ref", "--symbolic-full-name", branch+"@{u}")
	if err != nil {
		// No upstream configured — not a hard error.
		return "", nil
	}
	return out, nil
}

// hasRef reports whether ref exists in the repo.
func hasRef(repoRoot, ref string) bool {
	_, err := gitCmd(repoRoot, "rev-parse", "--verify", ref)
	return err == nil
}

// diffNames returns the files listed by `git diff --name-only <range>`.
func diffNames(repoRoot, rangeSpec string) ([]string, error) {
	out, err := gitCmd(repoRoot, "diff", "--name-only", rangeSpec)
	if err != nil {
		return nil, err
	}
	return splitLines(out), nil
}

// GetUnpushedChangedFiles returns the set of files changed in commits that
// exist locally but have not yet been pushed to the remote tracking branch.
//
// Strategy (in order):
//  1. Compare HEAD to the upstream tracking branch  (e.g. origin/main).
//  2. If no upstream, compare HEAD~1..HEAD (last commit only).
//  3. If HEAD~1 doesn't exist (first commit), list all tracked files.
//
// A non-fatal failure at any step is swallowed; an empty slice is returned so
// that the push is never blocked by tooling errors.
func GetUnpushedChangedFiles(repoRoot string) ([]string, error) {
	branch, err := CurrentBranch(repoRoot)
	if err != nil {
		// Detached HEAD or unborn — nothing we can do.
		return nil, nil
	}

	// 1. Try upstream tracking branch.
	upstream, _ := remoteTrackingBranch(repoRoot, branch)
	if upstream != "" && hasRef(repoRoot, upstream) {
		files, err := diffNames(repoRoot, upstream+"..HEAD")
		if err == nil {
			return dedup(files), nil
		}
		// Fall through on error.
	}

	// 2. Fallback: try well-known remote branches.
	for _, ref := range []string{"origin/main", "origin/master"} {
		if hasRef(repoRoot, ref) {
			files, err := diffNames(repoRoot, ref+"..HEAD")
			if err == nil {
				return dedup(files), nil
			}
		}
	}

	// 3. No remote ref — compare last commit only.
	if hasRef(repoRoot, "HEAD~1") {
		files, err := diffNames(repoRoot, "HEAD~1..HEAD")
		if err == nil {
			return dedup(files), nil
		}
	}

	// 4. First commit — list all tracked files.
	out, err := gitCmd(repoRoot, "ls-files")
	if err != nil {
		return nil, nil
	}
	return dedup(splitLines(out)), nil
}

// RemoteURL returns the push URL of origin (for display only).
func RemoteURL(repoRoot string) string {
	url, _ := gitCmd(repoRoot, "remote", "get-url", "--push", "origin")
	return url
}

// dedup removes duplicate strings while preserving order.
func dedup(ss []string) []string {
	seen := make(map[string]struct{}, len(ss))
	out := make([]string, 0, len(ss))
	for _, s := range ss {
		if _, ok := seen[s]; !ok {
			seen[s] = struct{}{}
			out = append(out, s)
		}
	}
	return out
}
