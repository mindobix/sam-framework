# SAM Framework — Architecture

## Problem statement

Mid-market engineering teams (500–5,000 engineers) need monorepo benefits (shared code, atomic commits, unified CI) but can't build Google-scale internal tooling. Full repo clones take 20+ minutes. Developers on the Sales API team have no reason to have 14 GB of Payments, Catalog, and Inventory code on their laptop. But splitting into polyrepos loses cross-team visibility and shared dependency management.

**SAM solves this with intelligent sparse checkout: the full monolith is always the source of truth, but each developer only materializes what they need.**

---

## High-level architecture

```
┌─────────────────────────────────────────────────────────┐
│                    Developer's Mac                       │
│                                                         │
│  ┌─────────────┐    ┌──────────────┐   ┌────────────┐  │
│  │  Finder     │    │   VS Code    │   │  Terminal  │  │
│  │ (MonoShell) │    │  (MonoLens)  │   │  (sam CLI) │  │
│  └──────┬──────┘    └──────┬───────┘   └─────┬──────┘  │
│         │                  │                 │          │
│         └──────────────────┼─────────────────┘          │
│                            │ subprocess / HTTP           │
│                   ┌────────▼────────┐                   │
│                   │  sam CLI binary │                   │
│                   │   (Go, /usr/   │                   │
│                   │   local/bin)   │                   │
│                   └────────┬────────┘                   │
│                            │                            │
│              ┌─────────────┼──────────────┐             │
│              │             │              │             │
│     ┌────────▼───┐  ┌──────▼──────┐  ┌───▼──────────┐  │
│     │git sparse- │  │  MonoGraph  │  │  .sam/       │  │
│     │ checkout   │  │  :7474      │  │  profiles    │  │
│     │ (native)   │  │  (Python)   │  │  .yaml       │  │
│     └────────────┘  └──────┬──────┘  └──────────────┘  │
│                            │                            │
│                   ┌────────▼────────┐                   │
│                   │ .sam/graph.json │                   │
│                   │ (local cache)   │                   │
│                   └─────────────────┘                   │
│                                                         │
└─────────────────────────────────────────────────────────┘
              │ git fetch (sparse)
              ▼
┌─────────────────────────┐
│   Git remote (GitHub /  │
│   GitLab / Bitbucket)   │
│                         │
│  enterprise-api/        │
│  ├── shared/            │
│  ├── apis/              │
│  └── services/          │
└─────────────────────────┘
```

---

## Component deep dives

### 1. sam CLI (Go)

The single binary developers interact with. Must be fast, offline-capable for basic operations, and installable via Homebrew.

**Commands:**

```
sam init <repo-url>
  1. git clone --filter=blob:none --sparse <repo-url>
  2. git sparse-checkout set --no-cone (empty pattern — no files yet)
  3. Run MonoGraph analyze in background (non-blocking)
  4. Install MonoWatch pre-push hook
  5. Print: "Ready. Run: sam use --profile <name>"

sam use --profile <name>
  1. Read .sam/profiles.yaml → resolve profile domains
  2. POST /resolve to MonoGraph → get full dep list (or fall back to static)
  3. git sparse-checkout add <domain1> <domain2> ...
  4. Print summary: X domains, Y MB, Z deps inferred by AI

sam fetch <domain> [--with-deps] [--dry-run]
  1. Optional: POST /graph?domain=X to MonoGraph
  2. git sparse-checkout add <domain> [+ deps]
  3. Print: fetched N files, M MB

sam plan <domain>
  1. Resolve deps without fetching
  2. Print: "Would fetch: N domains, M MB, K files"
  3. Show co-change partners with scores

sam impact [--format json|table]
  1. git diff --name-only HEAD (staged + unstaged)
  2. POST /impact {changed_files} to MonoGraph
  3. Print impact table: domain | risk | calls/day | last touched

sam deploy --profile <name>
  1. Validate only hydrated domains are touched
  2. Run profile's deploy command (from profiles.yaml)
  3. Stream output

sam graph [--domain X] [--output json]
  Pretty-print the dependency graph for a domain
```

**Offline fallback:**
When MonoGraph daemon is unreachable, `sam use` resolves deps from `auto_include` in profiles.yaml only. No AI inference. Prints warning.

---

### 2. MonoGraph (Python + FastAPI)

The intelligence layer. Builds and serves the dependency graph. Runs as a persistent local daemon started by `sam init` and kept alive by launchd (macOS Launch Agent).

**Graph build pipeline:**

```
repo root
    │
    ▼
[1] File walker
    → finds all source files by extension
    → ignores: node_modules, .git, __pycache__, dist, build
    │
    ▼
[2] Language router (tree-sitter)
    → .ts/.tsx/.js/.jsx  → TypeScript/JS parser
    → .py               → Python parser
    → .java             → Java parser
    → .go               → Go parser
    → .cs               → C# parser
    → others            → skip (no analysis)
    │
    ▼
[3] Import extractor (per-language rules)
    → maps import path → domain folder
    → example: "import { auth } from '@company/shared-auth'"
               → shared/auth
    │
    ▼
[4] Co-change miner (git log)
    → git log --name-only --format="" -n 500
    → pair files that appear in same commit
    → score = (co-change count) / (max co-change count in repo)
    │
    ▼
[5] Graph builder
    → nodes: domain folders
    → edges: import dependency (type: "static", weight: 1.0)
             co-change dependency (type: "cochange", weight: 0.0–1.0)
    │
    ▼
[6] Serializer → .sam/graph.json
```

**graph.json schema:**
```json
{
  "version": "1.0",
  "generated_at": "2026-04-12T10:00:00Z",
  "repo_root": "/Users/dev/enterprise-api",
  "domains": ["apis/sales", "apis/pricing", "shared/auth"],
  "edges": [
    {
      "from": "apis/sales",
      "to": "shared/auth",
      "type": "static",
      "weight": 1.0,
      "files": ["apis/sales/src/handler.ts"]
    },
    {
      "from": "apis/sales",
      "to": "apis/pricing",
      "type": "cochange",
      "weight": 0.87,
      "commit_count": 43
    }
  ]
}
```

**Impact scoring algorithm:**
```
risk_score(domain) =
  IF type == "static":  base_risk = 0.9
  IF type == "cochange": base_risk = edge.weight

  final_risk = base_risk × log10(calls_per_day + 1) / 5

  risk_label:
    final_risk > 0.7  → "critical"
    final_risk > 0.4  → "high"
    final_risk > 0.2  → "medium"
    else              → "low"
```

---

### 3. MonoLens VS Code Extension (TypeScript)

Transforms VS Code's file explorer into a ghost-folder browser.

**Key VS Code APIs used:**
- `TreeDataProvider` — custom sidebar tree with ghost/loaded states
- `FileDecorationProvider` — grayed-out opacity + badge icons on unhydrated folders
- `CodeLensProvider` — "N teams use this" inline annotations
- `StatusBarItem` — current profile + hydration status in status bar
- `tasks.executeTask` — runs `sam` CLI subprocess

**Ghost folder visual states:**
```
● (filled green dot)   = hydrated — code on disk
○ (empty gray dot)     = ghost — stub only, no code
⟳ (spinning)          = loading — sam fetch in progress
⚠ (amber triangle)    = stale — graph.json newer than local files
```

**File decoration CSS-like rules:**
```typescript
// Unhydrated folders: muted color + italic
{ color: new ThemeColor('disabledForeground'), italic: true, badge: '○' }

// Hydrated folders: normal
{ color: undefined, badge: '●' }

// Shared deps (auto-included): subtle blue tint
{ color: new ThemeColor('textLink.foreground'), badge: 'S' }
```

**Impact gutter annotation:**
When a file in `shared/` is open, show above each exported function:
```
⚠ 9 domains depend on this · apis/payments (critical) · apis/checkout (critical) · +7 more
```

**Status bar:**
```
[SAM] sales-api · 4 domains · 340 MB  [Hydrate more ▾]
```

---

### 4. MonoWatch (Go — git pre-push hook)

Installed to `.git/hooks/pre-push` by `sam init`.

**Flow:**
```
git push
  → pre-push hook fires
  → monowatch reads: git diff --name-only origin/main..HEAD
  → POST /impact {changed_files} to MonoGraph :7474
  → if daemon unreachable: exit 0 (never block, print warning)
  → render impact table to terminal
  → if block_on_critical=true AND any critical: exit 1
  → else: exit 0
```

**Terminal output format:**
```
SAM MonoWatch — impact analysis
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
Changed: shared/auth/token-validator.ts

Domain              Risk       Calls/day   Team
─────────────────────────────────────────────────
apis/payments       CRITICAL   214         @payments-team
apis/checkout       CRITICAL   198         @checkout-team
apis/user-mgmt      HIGH        87         @platform-team
apis/sales          HIGH        43         @sales-team
apis/loyalty        LOW         12         @loyalty-team
─────────────────────────────────────────────────
2 critical · 2 high · 1 low · 4 not affected

Consider notifying: @payments-team @checkout-team
Push proceeding (set block_on_critical: true to block)
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
```

---

### 5. MonoShell (Swift — macOS)

Two separate Apple extensions packaged in one app bundle.

**Extension 1: FileProvider**
- Registers the sparse repo as a "cloud drive" with macOS
- Unhydrated folders are marked as "not downloaded" (same mechanism as iCloud)
- Opening a folder triggers `NSFileProviderExtension.providePlaceholder` → calls `sam fetch`
- Requires entitlement: `com.apple.developer.fileprovider.server-mode`

**Extension 2: FinderSync**
- Watches the repo root directory
- Applies badge images to folders based on hydration state
- Right-click context menu items:
  - "Hydrate with SAM" → `sam fetch <domain>`
  - "Show dependencies" → opens terminal with `sam graph <domain>`
  - "Check impact" → `sam impact` for this domain
- Communicates with main app via XPC

**macOS launchd agent:**
MonoGraph daemon is kept alive via a launchd plist:
```xml
<!-- ~/Library/LaunchAgents/com.sam-framework.monograph.plist -->
<key>ProgramArguments</key>
<array>
  <string>/usr/local/bin/monograph</string>
  <string>serve</string>
  <string>--port</string>
  <string>7474</string>
</array>
<key>RunAtLoad</key><true/>
<key>KeepAlive</key><true/>
```

---

## Data flows

### Flow 1: Developer clones and sets up workspace
```
dev: sam init git@company.com/enterprise-api
  CLI: git clone --filter=blob:none --sparse .
  CLI: git sparse-checkout set (empty)
  CLI: start MonoGraph daemon (launchd)
  CLI: install .git/hooks/pre-push (MonoWatch)
  CLI: MonoGraph analyze (background, 30–120 sec)
  CLI: print "Ready"

dev: sam use --profile sales-api
  CLI: read .sam/profiles.yaml
  CLI: POST /resolve {domains: ["apis/sales","apis/pricing"]} → MonoGraph
  MonoGraph: add auto_include + ai_infer deps
  MonoGraph: return {fetch: ["apis/sales","apis/pricing","shared/auth","shared/types"]}
  CLI: git sparse-checkout add apis/sales apis/pricing shared/auth shared/types
  CLI: print summary
```

### Flow 2: Developer opens unhydrated folder in VS Code
```
dev: clicks ghost folder "apis/catalog" in MonoLens sidebar
  MonoLens: detects folder is unhydrated (not in sparse-checkout list)
  MonoLens: shows "Hydrate apis/catalog?" dialog
  dev: clicks "Hydrate with deps"
  MonoLens: runs `sam fetch apis/catalog --with-deps` subprocess
  CLI: POST /resolve {domains: ["apis/catalog"]} → MonoGraph
  MonoGraph: returns deps: ["shared/auth", "shared/types", "apis/search"]
  CLI: git sparse-checkout add apis/catalog shared/auth shared/types apis/search
  MonoLens: refreshes tree, folder becomes ● hydrated
```

### Flow 3: Developer pushes shared code change
```
dev: git push
  MonoWatch: git diff --name-only origin/main..HEAD
  MonoWatch: finds "shared/auth/token-validator.ts" changed
  MonoWatch: POST /impact {files: ["shared/auth/token-validator.ts"]} → MonoGraph
  MonoGraph: traverses reverse graph from shared/auth
  MonoGraph: returns {affected: [{domain: "apis/payments", risk: "critical", calls: 214}, ...]}
  MonoWatch: renders impact table
  MonoWatch: exit 0 (advisory)
  git: push proceeds
```

---

## .sam/ directory layout

```
.sam/                        ← gitignored except profiles.yaml and config.yaml
├── profiles.yaml            ← COMMITTED. Team workspace definitions.
├── config.yaml              ← COMMITTED. Repo-level SAM settings.
├── graph.json               ← GITIGNORED. Local MonoGraph cache.
├── graph.lock               ← GITIGNORED. Graph build in progress flag.
└── workspace.yaml           ← GITIGNORED. Current developer's active profile/domains.
```

**config.yaml schema:**
```yaml
# .sam/config.yaml — committed to repo
version: "1.0"
monograph:
  port: 7474
  min_cochange_score: 0.3    # ignore co-changes below this threshold
  max_git_log_commits: 500
monowatch:
  block_on_critical: false   # set true to block pushes on critical impact
  notify_slack: false        # future: Slack webhook
deploy:
  command: "kubectl apply -f k8s/{domain}/"  # template, {domain} replaced
  ci_trigger: "github_actions"
```

---

## Security considerations

- MonoGraph daemon binds to localhost only (127.0.0.1:7474). Never 0.0.0.0.
- sam CLI never sends code to any external service. All analysis is local.
- MonoShell XPC communication is sandboxed to the app group.
- No telemetry in MVP. Opt-in analytics planned for v2.
- profiles.yaml should never contain secrets. Use `.sam/config.yaml` for tokens (gitignored).

---

## Performance targets

| Operation | Target | Measurement |
|-----------|--------|-------------|
| `sam init` (clone tree) | < 15 sec | 10k-file repo |
| `sam use --profile X` | < 5 sec | excluding git fetch I/O |
| `sam plan X` | < 1 sec | graph already cached |
| MonoGraph `/resolve` | < 200ms | cached graph |
| MonoGraph `/impact` | < 500ms | cached graph |
| MonoGraph full analyze | < 120 sec | 10k-file repo |
| MonoWatch pre-push | < 3 sec | |
| MonoLens tree refresh | < 500ms | |

---

## MVP scope (what is NOT in v1)

- No semantic code embeddings (planned v2 — requires embedding model)
- No OpenTelemetry runtime trace integration (planned v2)
- No Windows support (Mac only)
- No CI/CD pipeline integration beyond git hooks
- No Slack/Teams notifications
- No web dashboard
- No multi-remote support (single origin only)
- No LFS support
