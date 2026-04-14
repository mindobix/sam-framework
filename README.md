# SAM — Sparse API Monolith

**Enterprise monorepo velocity for everyone.**

Meta, Google, and Microsoft run massive monorepos because it eliminates dependency hell, enforces consistent tooling, and makes cross-team collaboration effortless. But they built billions of dollars of custom infrastructure to make it work at scale — virtual filesystems, distributed build caches, custom source control.

SAM brings that same architecture to every engineering team. Fortune 1000 companies with hundreds of API domains in a single repository can now give each developer the experience of working on a single microservice — fast clone, local editor, targeted tests, safe deploy — while keeping everything in one repo.

Developers fetch only the domains they own. MonoGraph resolves the dependencies they need. Ghost folders in Finder and VS Code show the full monorepo structure without downloading a single byte. Navigate into a folder, it hydrates with all its dependencies. Push your code, see the blast radius across the entire organization before it ships.

## The Experience

```bash
sam init git@company.com/enterprise-api     # blobless clone — seconds, not minutes
sam setup                                    # ghost folders in Finder (dimmed)
./sam-start.sh ~/Developer/enterprise-api    # start MonoGraph + watch daemon

# In Finder: double-click any dimmed folder → hydrates with dependencies
# In VS Code: click any ghost domain in SAM sidebar → hydrates
# In terminal: sam fetch bigquery --with-deps
```

## Quick Start

```bash
# Prerequisites
brew install go python@3.12 uv node
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Install
git clone https://github.com/sam-framework/sam.git
cd sam
./install.sh

# Clone a monorepo
sam init https://github.com/your-org/monorepo.git
cd monorepo

# Start everything (MonoGraph + Finder watch)
./sam-start.sh .

# Or start manually
sam setup                                    # create ghost folders
sam use --profile your-team-api              # hydrate your profile
```

## Commands

| Command | What it does |
|---------|-------------|
| `sam init <url>` | Blobless clone + sparse checkout + create `.sam/` |
| `sam use --profile <name>` | Hydrate a profile's domains + resolved deps |
| `sam fetch <domain>` | Hydrate a single domain |
| `sam fetch <domain> --with-deps` | Hydrate with resolved dependencies |
| `sam plan <domain>` | Show what fetching would hydrate (dry run) |
| `sam impact` | Show blast radius of your changes |
| `sam deploy --profile <name>` | Deploy only your changed services |
| `sam graph --domain <path>` | Show dependency graph |
| `sam setup` | Create ghost folders in Finder (dimmed) |
| `sam watch` | Auto-hydrate when you navigate into a ghost folder in Finder |
| `sam refresh` | Update ghost/hydrated state in Finder |
| `sam dehydrate <domain>` | Remove a domain's files, return to ghost state |
| `sam dehydrate --all` | Reset all domains to ghost state |

## Architecture

```
         Terminal              Finder              VS Code
            |                    |                    |
         sam CLI            sam watch            MonoLens
            |                    |                    |
     ┌──────┴────────────────────┴────────────────────┘
     |              sam-core (Rust)
     └──────┬────────────┬────────────────┐
            |            |                |
      git sparse    profiles.yaml    MonoGraph
      checkout      workspace.yaml   daemon :7474
```

| Component | Language | Purpose |
|-----------|----------|---------|
| **sam-core** | Rust | Core library: git ops, profile parsing, Finder integration |
| **sam CLI** | Rust | All user commands (4.4MB binary) |
| **MonoGraph** | Python | Dependency engine: static import analysis + git co-change mining |
| **MonoLens** | TypeScript | VS Code extension: ghost folder sidebar, click-to-hydrate, impact gutter |
| **MonoWatch** | Go | Pre-push git hook: impact analysis before every push |

## Finder Integration

SAM uses native macOS `chflags hidden` to dim non-hydrated folders:

1. **Enable hidden files**: press **Cmd+Shift+.** in Finder (one-time)
2. **`sam setup`** creates empty skeleton directories for every domain, dimmed
3. **`sam watch`** monitors which folder you navigate into via Finder
4. **Double-click a dimmed folder** → watch daemon detects it, runs `sam fetch --with-deps`, files appear, folder becomes normal
5. **`sam dehydrate <domain>`** removes files, folder returns to dimmed state

No extra files. No Icon? resources. No Finder extensions. Just `chflags` + Finder path polling.

```bash
# Start Finder integration
./sam-start.sh ~/Developer/monorepo

# Stop
./sam-stop.sh
```

## VS Code Integration (MonoLens)

MonoLens adds a SAM sidebar to VS Code with ghost folder visualization, click-to-hydrate, and inline impact annotations.

```bash
# Install
cd monolens && npm install && npm run compile
# In VS Code: Cmd+Shift+P → "Install from VSIX" → select monolens-0.1.0.vsix

# Open your SAM repo in VS Code — MonoLens activates automatically
```

Features:
- **SAM Domains sidebar** — all domains listed with ghost/hydrated/shared state
- **Click to hydrate** — click a ghost domain to fetch it
- **File decoration** — badges on folders in the native explorer (○ ghost, ● hydrated, S shared)
- **Impact gutter** — CodeLens annotations on shared code showing dependent domains
- **Profile switching** — `Cmd+Shift+P` → "SAM: Switch workspace profile"
- **Impact analysis** — `Cmd+Shift+P` → "SAM: Show change impact"

## Profile Config

Teams share `.sam/profiles.yaml` (committed to the repo):

```yaml
profiles:
  sales-api:
    domains:
      - apis/sales
      - apis/pricing
    auto_include:
      - shared/auth
      - shared/types
    ai_infer: true        # MonoGraph resolves additional deps
    watch:
      - apis/inventory    # alerts only, not hydrated

  platform:
    domains: "*"          # full monolith
    ai_infer: false
```

## How It Works

1. **`sam init`** — `git clone --filter=blob:none --sparse` downloads the full tree structure without file contents. Fast clone regardless of repo size.

2. **`sam setup`** — Creates empty directories for every domain. Sets `chflags hidden` on non-hydrated ones so they appear dimmed in Finder. Starts MonoGraph daemon.

3. **`sam use --profile`** — Reads your team's profile, calls MonoGraph to resolve dependencies via static import analysis and git co-change mining, then `git sparse-checkout add` to materialize only what you need.

4. **`sam watch`** — Background daemon that polls Finder's current window path every 500ms via AppleScript. When you navigate into a ghost folder, it resolves dependencies via MonoGraph and hydrates the folder with all its deps.

5. **`sam fetch --with-deps`** — Adds a domain to sparse checkout, calls `git checkout` to materialize files, updates workspace state, clears the hidden flag.

6. **`sam dehydrate`** — Removes a domain from sparse checkout, recreates the empty skeleton directory, sets the hidden flag. Terminal only — explicit and safe.

7. **`sam impact`** — Queries MonoGraph for the blast radius of your uncommitted changes. Shows affected domains with risk levels (critical/high/medium/low).

## MonoGraph — Dependency Engine

MonoGraph analyzes your monorepo to answer: "if I need this folder, what other folders do I also need?"

Two methods:
- **Static import analysis** — tree-sitter parsers (Go, TypeScript, Python, Java, C#) read your source code and extract import relationships
- **Co-change mining** — reads `git log` to find folders that historically change together in the same commits

API (localhost:7474):

```
GET  /health              → status + graph readiness
POST /analyze             → trigger graph build for a repo
POST /resolve             → resolve dependencies for domains
POST /impact              → blast radius for changed files
GET  /graph?domain=X      → dependency edges for a domain
GET  /cochange?file=X     → co-change partners for a file
```

SAM CLI works without MonoGraph running — falls back to static profile resolution.

## Design Rules

1. **sam CLI works without MonoGraph.** Always falls back to static resolution. Never blocks the developer.
2. **MonoGraph is read-only.** Only `sam fetch` writes to the working tree.
3. **profiles.yaml is committed.** Team-shared config.
4. **workspace.yaml is gitignored.** Local machine state.
5. **MonoWatch is advisory by default.** Warns but doesn't block pushes unless `block_on_critical: true`.
6. **Common operations under 3 seconds.** Network/git ops are the exception.
7. **Hydrate pulls deps automatically.** Dehydrate is explicit, single-domain only. Safe by design.

## Scripts

| Script | Purpose |
|--------|---------|
| `./install.sh` | Build and install all components |
| `./sam-start.sh <repo>` | Start MonoGraph + watch daemon for a repo |
| `./sam-stop.sh` | Stop all SAM background services |

## Manual Build

```bash
# sam CLI + sam-core (Rust)
cargo build --release
cp target/release/sam /usr/local/bin/

# MonoWatch pre-push hook (Go)
cd monowatch && go build -o /usr/local/bin/monowatch .

# MonoGraph dependency engine (Python)
cd monograph && uv sync

# MonoLens VS Code extension
cd monolens && npm install && npm run compile
```

## Command Examples

### Day 1: New developer joins the team

```bash
# Clone the company monorepo (takes seconds, not minutes)
sam init git@github.com:your-org/enterprise-api.git
cd enterprise-api

# See what profiles are available
cat .sam/profiles.yaml

# Your team lead says "use the sales-api profile"
sam use --profile sales-api
# ✓ Hydrated 4 domains: apis/sales, apis/pricing, shared/auth, shared/types

# Set up Finder ghost folders + start background services
./sam-start.sh .
```

### Daily workflow

```bash
# See what your changes might break before pushing
sam impact
# ⚠ 1 critical-risk domain: apis/orders
# ⚠ 3 high-risk domains: apis/payments, apis/shipping, apis/billing

# Check the dependency graph for a domain
sam graph --domain apis/sales
# apis/sales (root)
# ├── shared/auth (static import)
# ├── shared/types (static import)
# ├── apis/pricing (static import)
# └── apis/inventory (co-change, 12 commits)

# Preview what fetching a new domain would pull in
sam plan apis/orders
# Domain         Source        Files  Status
# apis/orders    profile       42     not hydrated
# apis/payments  auto_include  18     not hydrated
# shared/auth    auto_include  6      hydrated
```

### Hydrating and dehydrating

```bash
# Hydrate a single domain
sam fetch bigquery

# Hydrate with all dependencies (via MonoGraph)
sam fetch bigquery --with-deps

# Done working on bigquery? Free up space
sam dehydrate bigquery

# Nuclear option: dehydrate everything, start fresh
sam dehydrate --all
sam use --profile your-team-api
```

### Finder workflow

```bash
# Start ghost folders + auto-hydrate daemon
./sam-start.sh ~/Developer/enterprise-api

# In Finder: press Cmd+Shift+. to show dimmed folders
# Double-click any dimmed folder → it hydrates with dependencies
# In terminal: sam dehydrate <domain> to remove it

# Stop background services
./sam-stop.sh
```

### Working with profiles

```bash
# Switch to a different team's profile
sam use --profile orders-api

# Use the full monolith (everything hydrated)
sam use --profile platform

# Dry run — see what a profile would hydrate without doing it
sam use --profile orders-api --dry-run
```

### Impact analysis before push

```bash
# Edit some files in shared/auth
vim shared/auth/src/token.go

# Check blast radius
sam impact
# Domain            Risk      Calls/Day  Teams
# apis/orders       critical  214        orders-team
# apis/payments     critical  198        payments-team
# apis/sales        high      43         sales-team

# JSON output for CI integration
sam impact --format json
```

### Deploy

```bash
# Deploy only the domains in your profile
sam deploy --profile sales-api

# SAM runs the deploy command defined in profiles.yaml
# with SAM_DOMAIN and SAM_PROFILE environment variables
```

## Tech Stack

| Component | Language | Version | Purpose |
|-----------|----------|---------|---------|
| **sam-core** | Rust 1.77+ | 0.1.0 | Core library with C FFI. Git ops, profile parsing, workspace state, Finder integration |
| **sam CLI** | Rust 1.77+ | 0.1.0 | 4.4MB statically compiled binary. All user commands |
| **MonoGraph** | Python 3.12 | 0.1.0 | FastAPI daemon. tree-sitter for multi-language AST parsing (Go, TypeScript, Python, Java, C#). networkx for graph operations. git log mining for co-change analysis |
| **MonoLens** | TypeScript 5.x | 0.1.0 | VS Code Extension API (^1.85.0). esbuild bundled. 36KB VSIX |
| **MonoWatch** | Go 1.22 | 0.1.0 | 9.6MB binary. Pre-push git hook. cobra CLI |

### Dependencies

| Dependency | Used by | Purpose |
|------------|---------|---------|
| clap | sam CLI | CLI argument parsing (derive API) |
| reqwest | sam-core | HTTP client for MonoGraph API |
| serde + serde_yaml | sam-core | YAML/JSON serialization |
| cbindgen | sam-core | C header generation for FFI |
| FastAPI + uvicorn | MonoGraph | HTTP daemon |
| tree-sitter | MonoGraph | Multi-language import parsing |
| networkx | MonoGraph | Dependency graph operations |

### Build tools

| Tool | Purpose |
|------|---------|
| Cargo | Rust workspace (sam-core + sam CLI) |
| uv | Python package management (MonoGraph) |
| npm + esbuild | TypeScript bundling (MonoLens) |
| Go modules | Go build (MonoWatch) |

## System Requirements

| Requirement | Minimum |
|-------------|---------|
| **macOS** | 15.1 (Sequoia) or later |
| **Rust** | 1.77+ |
| **Python** | 3.12+ |
| **Go** | 1.22+ |
| **Node.js** | 18+ |
| **Git** | 2.25+ (sparse-checkout support) |
| **VS Code** | 1.85+ (for MonoLens extension) |
| **Xcode CLI Tools** | Required for git and system headers |

### macOS-specific features

- **Finder ghost folders** — uses `chflags hidden` (macOS native, no kernel extensions)
- **Finder path polling** — uses AppleScript to read Finder's current window path
- **Show hidden files** — requires Cmd+Shift+. enabled in Finder

### Not supported (yet)

- Windows
- Linux (sam-core and MonoGraph work, Finder integration is macOS only)

## License

MIT
