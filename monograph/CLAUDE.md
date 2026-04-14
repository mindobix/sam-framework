# MonoGraph — Component Context

## What this component is
The AI brain of SAM. A local HTTP daemon that builds and serves the dependency graph for the monorepo. It runs silently in the background and answers two key questions:
1. "To work on domain X, what else do I need?" (`/resolve`)
2. "If I change these files, what will break?" (`/impact`)

All analysis is local. No code leaves the machine.

## Language and tooling
- **Python 3.12**
- Package manager: `uv` (not pip, not poetry)
- Web framework: `FastAPI` with `uvicorn`
- AST parsing: `tree-sitter` + language bindings
- Data: `networkx` for graph operations
- Git: `gitpython` for log mining
- Serialization: stdlib `json`
- Process management: macOS `launchd` (installed by `sam init`)

## Directory structure to build
```
monograph/
├── CLAUDE.md              ← this file
├── pyproject.toml
├── uv.lock
├── monograph/
│   ├── __init__.py
│   ├── __main__.py        ← entry: python -m monograph
│   ├── server.py          ← FastAPI app + uvicorn runner
│   ├── cli.py             ← typer CLI: monograph serve / analyze / status
│   ├── analyzer.py        ← orchestrates full graph build pipeline
│   ├── parsers/
│   │   ├── __init__.py
│   │   ├── base.py        ← BaseParser abstract class
│   │   ├── typescript.py  ← .ts/.tsx/.js/.jsx
│   │   ├── python.py      ← .py
│   │   ├── java.py        ← .java
│   │   ├── golang.py      ← .go
│   │   └── csharp.py      ← .cs
│   ├── cochange.py        ← git log miner + co-change scorer
│   ├── graph.py           ← Graph data model, node/edge types
│   ├── resolver.py        ← /resolve endpoint logic
│   ├── impact.py          ← /impact endpoint logic
│   └── cache.py           ← graph.json read/write + invalidation
└── tests/
    ├── fixtures/           ← tiny fake repos per language
    ├── test_parsers.py
    ├── test_cochange.py
    ├── test_resolver.py
    └── test_impact.py
```

## HTTP API — implement exactly this

### `GET /health`
```json
{"status": "ok", "graph_ready": true, "graph_age_seconds": 342}
```

### `POST /analyze`
```json
Request:  {"repo_path": "/Users/dev/enterprise-api"}
Response: {"status": "started", "estimated_seconds": 45}
```
Runs graph build in background thread. Non-blocking.

### `GET /graph?domain=apis/sales`
```json
{
  "domain": "apis/sales",
  "edges": [
    {"to": "shared/auth", "type": "static", "weight": 1.0},
    {"to": "apis/pricing", "type": "cochange", "weight": 0.87, "commit_count": 43}
  ]
}
```

### `POST /resolve`
```json
Request:  {"domains": ["apis/sales", "apis/pricing"]}
Response: {
  "resolved": ["apis/sales", "apis/pricing", "shared/auth", "shared/types"],
  "inferred": ["shared/auth", "shared/types"],
  "inference_detail": [
    {"domain": "shared/auth", "reason": "static import", "from": "apis/sales"},
    {"domain": "shared/types", "reason": "static import", "from": "apis/sales"}
  ]
}
```

### `POST /impact`
```json
Request:  {"changed_files": ["shared/auth/token-validator.ts"]}
Response: {
  "affected": [
    {"domain": "apis/payments", "risk": "critical", "type": "static", "calls_per_day": 214},
    {"domain": "apis/checkout", "risk": "critical", "type": "static", "calls_per_day": 198},
    {"domain": "apis/sales",    "risk": "high",     "type": "cochange", "calls_per_day": 43}
  ],
  "not_affected": ["apis/catalog", "apis/search"]
}
```

### `GET /cochange?file=shared/auth/token-validator.ts`
```json
{
  "file": "shared/auth/token-validator.ts",
  "partners": [
    {"file": "apis/sales/src/auth-wrapper.ts", "score": 0.87, "commit_count": 43},
    {"file": "shared/types/user.ts", "score": 0.72, "commit_count": 36}
  ]
}
```

## Parser implementation details

### BaseParser interface
```python
class BaseParser:
    def extract_imports(self, file_path: Path, content: str) -> list[str]:
        """Return list of imported domain paths (e.g. 'shared/auth')"""
        raise NotImplementedError

    def resolve_import_to_domain(self, import_str: str, file_path: Path, repo_root: Path) -> str | None:
        """Map an import string to a domain folder path, or None if external"""
        raise NotImplementedError
```

### TypeScript/JavaScript parser
Key import patterns to handle:
```typescript
import { thing } from '@company/shared-auth'           → shared/auth
import { thing } from '../../../shared/auth/index'     → shared/auth
import { thing } from '~/shared/auth'                  → shared/auth
const x = require('../../shared/types')                → shared/types
```

Strategy:
1. Use tree-sitter to get import_statement and call_expression nodes
2. Extract string literal from `from "..."` or `require("...")`
3. Resolve relative paths relative to file_path
4. Map path aliases (read tsconfig.json paths if present)
5. Strip to domain root (first 2 path segments after repo root)

### Python parser
```python
from company.shared.auth import token     → shared/auth
from ...shared import types               → shared/types
import company.shared.auth               → shared/auth
```

### Go parser
```go
import "github.com/company/enterprise-api/shared/auth"    → shared/auth
import auth "github.com/company/enterprise-api/shared/auth" → shared/auth
```

### Handling unknown/external imports
Any import that resolves outside the repo root = external dependency = ignore.
Only track intra-repo dependencies.

## Co-change miner

```python
def mine_cochange(repo_path: Path, max_commits: int = 500) -> dict[str, dict[str, int]]:
    """
    Returns: {file_a: {file_b: co_change_count, file_c: co_change_count, ...}, ...}
    
    Algorithm:
    1. git log --name-only --format="" -n {max_commits}
    2. Split output into per-commit file lists (blank line separates commits)
    3. For each commit, generate all (file_a, file_b) pairs
    4. Increment co_change[a][b] and co_change[b][a]
    5. Normalize to domain level: map file paths to domain root
    """
```

```python
def score_cochange(counts: dict) -> dict[str, dict[str, float]]:
    """
    Normalize raw counts to 0.0–1.0 scores.
    score = count / max_count_in_repo
    Only include pairs where score >= MIN_COCHANGE_SCORE (default 0.3)
    """
```

## Graph data model

```python
from dataclasses import dataclass, field
from enum import Enum

class EdgeType(Enum):
    STATIC   = "static"    # direct import statement
    COCHANGE = "cochange"  # git log co-change pattern

@dataclass
class Edge:
    from_domain: str
    to_domain:   str
    edge_type:   EdgeType
    weight:      float         # 1.0 for static, 0.0–1.0 for cochange
    commit_count: int = 0      # for cochange edges
    source_files: list[str] = field(default_factory=list)  # for static edges

@dataclass
class Graph:
    version:      str
    generated_at: str
    repo_root:    str
    domains:      list[str]
    edges:        list[Edge]

    def to_json(self) -> dict: ...
    
    @classmethod
    def from_json(cls, data: dict) -> "Graph": ...
    
    def get_dependencies(self, domain: str) -> list[Edge]:
        """Edges where from_domain == domain (what domain depends on)"""
    
    def get_dependents(self, domain: str) -> list[Edge]:
        """Edges where to_domain == domain (what depends on domain) — for impact"""
    
    def resolve_transitive(self, domains: list[str]) -> list[str]:
        """BFS: all transitive dependencies of given domains"""
```

## Cache management

```python
GRAPH_CACHE_PATH = ".sam/graph.json"
GRAPH_LOCK_PATH  = ".sam/graph.lock"

def is_graph_stale(repo_path: Path) -> bool:
    """
    Graph is stale if:
    - graph.json doesn't exist
    - graph.json is older than any source file in repo
    - graph.lock exists (build in progress — wait, don't rebuild)
    """

def save_graph(graph: Graph, repo_path: Path) -> None:
    """Atomic write: write to .sam/graph.json.tmp, then rename"""

def load_graph(repo_path: Path) -> Graph | None:
    """Return None if graph.json doesn't exist or is corrupt"""
```

## pyproject.toml to create
```toml
[project]
name = "monograph"
version = "0.1.0"
requires-python = ">=3.12"
dependencies = [
    "fastapi>=0.110.0",
    "uvicorn[standard]>=0.27.0",
    "tree-sitter>=0.21.0",
    "tree-sitter-javascript>=0.21.0",
    "tree-sitter-typescript>=0.21.0",
    "tree-sitter-python>=0.21.0",
    "tree-sitter-java>=0.21.0",
    "tree-sitter-go>=0.21.0",
    "networkx>=3.2.0",
    "gitpython>=3.1.40",
    "typer>=0.9.0",
]

[project.scripts]
monograph = "monograph.cli:app"

[build-system]
requires = ["hatchling"]
build-backend = "hatchling.build"
```

## Run commands
```bash
# Install deps
uv sync

# Start daemon
uv run monograph serve --port 7474 --repo /path/to/repo

# Run analysis on a repo
uv run monograph analyze --repo /path/to/repo

# Check status
uv run monograph status

# Run tests
uv run pytest tests/ -v
```

## Build order for this component
1. `graph.py` — data model (no deps)
2. `cache.py` — read/write graph.json
3. `parsers/base.py` + `parsers/typescript.py` (most common)
4. `cochange.py` — git log miner
5. `analyzer.py` — orchestrates 3 + 4 → graph
6. `resolver.py` + `impact.py` — query logic
7. `server.py` — FastAPI app wiring all the above
8. `cli.py` — typer CLI wrapping server + analyze
9. Add remaining parsers: python, java, golang, csharp
10. Tests throughout

## Critical correctness rules
- Never modify any file in the repo. Read-only always.
- graph.json write must be atomic (tmp file + rename)
- Daemon startup must not block if graph is already cached
- `/resolve` must return in under 200ms (use cached graph only — never trigger rebuild on request)
- If graph.json is missing, return empty resolve with warning header `X-Graph-Ready: false`
