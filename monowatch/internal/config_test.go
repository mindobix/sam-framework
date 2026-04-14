package internal

import (
	"os"
	"path/filepath"
	"testing"
)

// makeSAMDir creates a temp directory with a .sam/ subdirectory and returns
// the root.
func makeSAMDir(t *testing.T) string {
	t.Helper()
	root := t.TempDir()
	if err := os.MkdirAll(filepath.Join(root, ".sam"), 0o755); err != nil {
		t.Fatal(err)
	}
	return root
}

func TestLoadConfig_Defaults(t *testing.T) {
	root := makeSAMDir(t)

	cfg, err := LoadConfig(root)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if cfg.MonoWatch.BlockOnCritical {
		t.Error("BlockOnCritical default should be false")
	}
	if len(cfg.MonoWatch.SkipDomains) != 0 {
		t.Errorf("SkipDomains default should be empty, got %v", cfg.MonoWatch.SkipDomains)
	}
}

func TestLoadConfig_BlockOnCritical(t *testing.T) {
	root := makeSAMDir(t)

	yaml := "monowatch:\n  block_on_critical: true\n"
	if err := os.WriteFile(
		filepath.Join(root, ".sam", "config.yaml"),
		[]byte(yaml), 0o644,
	); err != nil {
		t.Fatal(err)
	}

	cfg, err := LoadConfig(root)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if !cfg.MonoWatch.BlockOnCritical {
		t.Error("BlockOnCritical should be true")
	}
}

func TestLoadConfig_SkipDomains(t *testing.T) {
	root := makeSAMDir(t)

	yaml := "monowatch:\n  skip_domains:\n    - apis/legacy\n    - internal/tools\n"
	if err := os.WriteFile(
		filepath.Join(root, ".sam", "config.yaml"),
		[]byte(yaml), 0o644,
	); err != nil {
		t.Fatal(err)
	}

	cfg, err := LoadConfig(root)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if len(cfg.MonoWatch.SkipDomains) != 2 {
		t.Fatalf("SkipDomains = %v, want 2 entries", cfg.MonoWatch.SkipDomains)
	}
	if cfg.MonoWatch.SkipDomains[0] != "apis/legacy" {
		t.Errorf("SkipDomains[0] = %q", cfg.MonoWatch.SkipDomains[0])
	}
}

func TestLoadConfig_InvalidYAML(t *testing.T) {
	root := makeSAMDir(t)

	if err := os.WriteFile(
		filepath.Join(root, ".sam", "config.yaml"),
		[]byte("monowatch: [invalid: yaml: {"), 0o644,
	); err != nil {
		t.Fatal(err)
	}

	_, err := LoadConfig(root)
	if err == nil {
		t.Fatal("expected error for invalid YAML, got nil")
	}
}
