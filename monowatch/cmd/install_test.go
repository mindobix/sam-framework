package cmd

import (
	"os"
	"path/filepath"
	"strings"
	"testing"
)

// ─────────────────────────────────────────────────────────────────────────────
// containsMarker
// ─────────────────────────────────────────────────────────────────────────────

func TestContainsMarker_SAMHook(t *testing.T) {
	if !containsMarker(hookScript) {
		t.Error("hookScript should contain the SAM marker")
	}
}

func TestContainsMarker_UnknownHook(t *testing.T) {
	foreign := "#!/bin/sh\n# some other hook\nexit 0\n"
	if containsMarker(foreign) {
		t.Error("foreign hook should not contain SAM marker")
	}
}

func TestContainsMarker_Empty(t *testing.T) {
	if containsMarker("") {
		t.Error("empty string should not contain SAM marker")
	}
}

// ─────────────────────────────────────────────────────────────────────────────
// install — happy path
// ─────────────────────────────────────────────────────────────────────────────

// makeGitRepo creates a temp directory that looks like a git repo root
// (has a .git/hooks directory).
func makeGitRepo(t *testing.T) string {
	t.Helper()
	root := t.TempDir()
	if err := os.MkdirAll(filepath.Join(root, ".git", "hooks"), 0o755); err != nil {
		t.Fatal(err)
	}
	return root
}

func TestRunInstall_WritesHook(t *testing.T) {
	root := makeGitRepo(t)

	if err := runInstall([]string{"--repo", root}); err != nil {
		t.Fatalf("runInstall error = %v", err)
	}

	hookPath := filepath.Join(root, ".git", "hooks", "pre-push")
	data, err := os.ReadFile(hookPath)
	if err != nil {
		t.Fatalf("hook file not written: %v", err)
	}
	if !strings.Contains(string(data), samHookMarker) {
		t.Error("hook does not contain SAM marker")
	}
}

func TestRunInstall_Idempotent(t *testing.T) {
	// Writing twice should succeed without --force because our own hook is
	// being replaced.
	root := makeGitRepo(t)

	if err := runInstall([]string{"--repo", root}); err != nil {
		t.Fatalf("first install error = %v", err)
	}
	if err := runInstall([]string{"--repo", root}); err != nil {
		t.Fatalf("second install error = %v", err)
	}
}

func TestRunInstall_RefusesForeignHook(t *testing.T) {
	root := makeGitRepo(t)

	// Write a foreign hook.
	hookPath := filepath.Join(root, ".git", "hooks", "pre-push")
	if err := os.WriteFile(hookPath, []byte("#!/bin/sh\nexit 0\n"), 0o755); err != nil {
		t.Fatal(err)
	}

	err := runInstall([]string{"--repo", root})
	if err == nil {
		t.Fatal("expected error when clobbering foreign hook, got nil")
	}
}

func TestRunInstall_ForceClobbers(t *testing.T) {
	root := makeGitRepo(t)

	hookPath := filepath.Join(root, ".git", "hooks", "pre-push")
	if err := os.WriteFile(hookPath, []byte("#!/bin/sh\nexit 0\n"), 0o755); err != nil {
		t.Fatal(err)
	}

	if err := runInstall([]string{"--repo", root, "--force"}); err != nil {
		t.Fatalf("force install error = %v", err)
	}

	// Backup should have been written.
	backupPath := hookPath + ".bak"
	if _, err := os.Stat(backupPath); os.IsNotExist(err) {
		t.Error("backup file was not created")
	}
}

func TestRunInstall_NotAGitRepo(t *testing.T) {
	plain := t.TempDir() // no .git directory
	err := runInstall([]string{"--repo", plain})
	if err == nil {
		t.Fatal("expected error for non-git directory, got nil")
	}
}
