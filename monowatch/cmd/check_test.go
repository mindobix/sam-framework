package cmd

import (
	"testing"
)

// ─────────────────────────────────────────────────────────────────────────────
// filterSkippedFiles
// ─────────────────────────────────────────────────────────────────────────────

func TestFilterSkippedFiles_NoSkip(t *testing.T) {
	files := []string{"apis/sales/handler.go", "shared/auth/jwt.go"}
	got := filterSkippedFiles(files, nil)
	if len(got) != 2 {
		t.Errorf("got %d files, want 2", len(got))
	}
}

func TestFilterSkippedFiles_SkipsPrefix(t *testing.T) {
	files := []string{
		"apis/legacy/old.go",
		"apis/sales/handler.go",
		"internal/tools/gen.go",
		"shared/auth/jwt.go",
	}
	skip := []string{"apis/legacy", "internal/tools"}

	got := filterSkippedFiles(files, skip)
	if len(got) != 2 {
		t.Fatalf("got %v, want 2 files", got)
	}
	if got[0] != "apis/sales/handler.go" {
		t.Errorf("got[0] = %q", got[0])
	}
	if got[1] != "shared/auth/jwt.go" {
		t.Errorf("got[1] = %q", got[1])
	}
}

func TestFilterSkippedFiles_AllSkipped(t *testing.T) {
	files := []string{"apis/legacy/a.go", "apis/legacy/b.go"}
	got := filterSkippedFiles(files, []string{"apis/legacy"})
	if len(got) != 0 {
		t.Errorf("expected empty slice, got %v", got)
	}
}

func TestFilterSkippedFiles_EmptyInput(t *testing.T) {
	got := filterSkippedFiles(nil, []string{"apis/legacy"})
	if got != nil {
		t.Errorf("expected nil, got %v", got)
	}
}
