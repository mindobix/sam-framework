# MonoWatch — Component Context

## What this component is
The safety net. A git pre-push hook installed by `sam init` that shows developers the blast radius of their changes before they push. Non-blocking by default — it warns, never blocks (unless explicitly configured). Fast: must complete in under 3 seconds.

## Language and tooling
- **Go 1.22** (same module as sam-cli or separate — separate is cleaner)
- Module: `github.com/sam-framework/monowatch`
- No external dependencies beyond stdlib
- Queries MonoGraph via HTTP on :7474
- Single binary: `monowatch`
- Installed to: `.git/hooks/pre-push` (as a shell script that calls the binary)

## What gets installed

### `.git/hooks/pre-push` (shell wrapper)
```bash
#!/bin/sh
# SAM MonoWatch — installed by sam init
# Runs impact analysis before every push

MONOWATCH_BIN=$(which monowatch 2>/dev/null || echo "$HOME/.sam/bin/monowatch")

if [ ! -f "$MONOWATCH_BIN" ]; then
  echo "SAM: monowatch not found, skipping impact check"
  exit 0
fi

"$MONOWATCH_BIN" check --repo "$(git rev-parse --show-toplevel)"
exit $?
```

## Directory structure to build
```
monowatch/
├── CLAUDE.md
├── go.mod
├── main.go
├── cmd/
│   ├── check.go       ← the main command (called by hook)
│   └── install.go     ← monowatch install --repo <path>
└── internal/
    ├── git.go         ← git diff, rev-parse, remote info
    ├── impact.go      ← HTTP call to MonoGraph /impact
    ├── render.go      ← terminal table renderer (colored)
    └── config.go      ← read .sam/config.yaml for block_on_critical
```

## Core logic — cmd/check.go
```go
func runCheck(repoPath string) (exitCode int) {
    // 1. Get changed files (not-yet-pushed commits)
    changedFiles, err := git.GetUnpushedChangedFiles(repoPath)
    if err != nil || len(changedFiles) == 0 {
        return 0  // nothing changed or can't determine — let push proceed
    }

    // 2. Load config — check block_on_critical setting
    cfg := config.Load(repoPath)  // never fail on missing config

    // 3. Query MonoGraph
    result, err := impact.Query("http://127.0.0.1:7474", changedFiles, 2*time.Second)
    if err != nil {
        // Daemon not running — skip check, never block
        fmt.Println("SAM: MonoGraph unreachable, skipping impact check")
        return 0
    }

    // 4. Check if anything actually affected
    if len(result.Affected) == 0 {
        fmt.Println("SAM MonoWatch: no cross-domain impact detected")
        return 0
    }

    // 5. Render impact table
    render.ImpactTable(result)

    // 6. Decide exit code
    hasCritical := result.HasRisk("critical")
    if hasCritical && cfg.MonoWatch.BlockOnCritical {
        fmt.Println("\nSAM MonoWatch: blocking push (block_on_critical = true)")
        fmt.Println("To override: git push --no-verify")
        return 1
    }

    return 0
}
```

## git.go — changed files detection
```go
// Get files changed in commits not yet on remote
// This is the right approach: local commits vs remote/origin/main

func GetUnpushedChangedFiles(repoPath string) ([]string, error) {
    // git rev-parse --abbrev-ref HEAD → current branch
    // git rev-parse --verify origin/<branch> → check if remote exists
    // git diff --name-only origin/<branch>..HEAD → changed files
    
    // Fallback if no remote tracking branch:
    // git diff --name-only HEAD~1..HEAD (just last commit)
}
```

## render.go — terminal output
```
SAM MonoWatch — impact analysis
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
Changed: shared/auth/token-validator.ts
         shared/types/user-session.ts

Domain              Risk        Calls/day   Type
─────────────────────────────────────────────────────
apis/payments       CRITICAL    214         static import
apis/checkout       CRITICAL    198         static import
apis/user-mgmt      HIGH         87         static import
apis/sales          HIGH         43         co-change (0.87)
apis/loyalty        LOW          12         co-change (0.31)
─────────────────────────────────────────────────────
2 critical · 2 high · 1 low · 5 not affected

Consider notifying: owners of apis/payments, apis/checkout
Push proceeding (block_on_critical is false in .sam/config.yaml)
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
```

Color rules:
- CRITICAL → red bold
- HIGH → yellow bold
- LOW → normal
- co-change edges → slightly dimmer than static

## go.mod
```
module github.com/sam-framework/monowatch

go 1.22

require (
    github.com/fatih/color v1.16.0
)
```

## Install command
```go
// monowatch install --repo /path/to/repo
// Copies the hook script to .git/hooks/pre-push
// Makes it executable
// Verifies it works with: monowatch check --dry-run

func installHook(repoPath string) error {
    hookPath := filepath.Join(repoPath, ".git", "hooks", "pre-push")
    // Write hook script
    // chmod +x
    // Print confirmation
}
```

## Build and test commands
```bash
go build -o monowatch .
./monowatch install --repo /path/to/test-repo
./monowatch check --repo /path/to/test-repo --dry-run
go test ./...
```

## Critical rules
- Exit 0 when MonoGraph is unreachable. Never block a push because of tooling.
- Exit 0 when changed files are all within one domain (no cross-domain impact).
- Complete in under 3 seconds total. 2s timeout on MonoGraph HTTP call.
- Never read or write any files in the repo working tree.
- `git push --no-verify` always bypasses the hook. Document this clearly.
