package internal

import (
	"os"
	"path/filepath"

	"gopkg.in/yaml.v3"
)

// Config is the subset of .sam/config.yaml that MonoWatch cares about.
// Missing file → safe defaults (never block, warn only).
type Config struct {
	MonoWatch MonoWatchConfig `yaml:"monowatch"`
}

// MonoWatchConfig controls MonoWatch behaviour.
type MonoWatchConfig struct {
	// BlockOnCritical causes `monowatch check` to exit 1 when a CRITICAL-risk
	// domain is affected.  Default: false (advisory only).
	BlockOnCritical bool `yaml:"block_on_critical"`

	// SkipDomains lists domain paths that should never be reported as affected.
	SkipDomains []string `yaml:"skip_domains"`
}

// defaultConfig returns safe out-of-the-box defaults.
func defaultConfig() Config {
	return Config{
		MonoWatch: MonoWatchConfig{
			BlockOnCritical: false,
		},
	}
}

// LoadConfig reads .sam/config.yaml from repoRoot.
// A missing file is not an error — defaults are applied silently.
// A present but malformed file IS an error.
func LoadConfig(repoRoot string) (Config, error) {
	cfg := defaultConfig()

	path := filepath.Join(repoRoot, ".sam", "config.yaml")
	data, err := os.ReadFile(path)
	if os.IsNotExist(err) {
		return cfg, nil
	}
	if err != nil {
		return cfg, err
	}

	if err := yaml.Unmarshal(data, &cfg); err != nil {
		return cfg, err
	}
	return cfg, nil
}
