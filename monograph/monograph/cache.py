"""Atomic read/write of .sam/graph.json with stale-detection."""

from __future__ import annotations

import json
import os
import time
from pathlib import Path

from monograph.graph import Graph

GRAPH_CACHE_PATH = ".sam/graph.json"
GRAPH_LOCK_PATH = ".sam/graph.lock"
_GRAPH_TMP_PATH = ".sam/graph.json.tmp"

# Source file extensions that, when newer than graph.json, mark it stale.
_SOURCE_EXTENSIONS = frozenset(
    {".ts", ".tsx", ".js", ".jsx", ".py", ".go", ".java", ".cs", ".kt"}
)


def _graph_path(repo_path: Path) -> Path:
    return repo_path / GRAPH_CACHE_PATH


def _lock_path(repo_path: Path) -> Path:
    return repo_path / GRAPH_LOCK_PATH


def _tmp_path(repo_path: Path) -> Path:
    return repo_path / _GRAPH_TMP_PATH


def lock_exists(repo_path: Path) -> bool:
    """True if a graph build is in progress."""
    return _lock_path(repo_path).exists()


def acquire_lock(repo_path: Path) -> None:
    _lock_path(repo_path).parent.mkdir(parents=True, exist_ok=True)
    _lock_path(repo_path).write_text(str(os.getpid()))


def release_lock(repo_path: Path) -> None:
    try:
        _lock_path(repo_path).unlink()
    except FileNotFoundError:
        pass


def is_graph_stale(repo_path: Path) -> bool:
    """
    Return True if graph.json doesn't exist, is being written (lock present),
    or is older than any source file under repo_path.
    """
    graph_file = _graph_path(repo_path)
    if not graph_file.exists():
        return True
    if lock_exists(repo_path):
        return False  # build in progress — callers should wait, not rebuild
    graph_mtime = graph_file.stat().st_mtime

    # Walk the repo looking for source files newer than the graph.
    for dirpath, dirnames, filenames in os.walk(repo_path):
        # Skip hidden dirs and common noise directories
        dirnames[:] = [
            d for d in dirnames
            if not d.startswith(".") and d not in ("node_modules", "__pycache__", "vendor")
        ]
        for fname in filenames:
            if Path(fname).suffix in _SOURCE_EXTENSIONS:
                fpath = Path(dirpath) / fname
                try:
                    if fpath.stat().st_mtime > graph_mtime:
                        return True
                except OSError:
                    pass
    return False


def save_graph(graph: Graph, repo_path: Path) -> None:
    """Atomic write: tmp file then os.rename (POSIX atomic on same filesystem)."""
    sam_dir = repo_path / ".sam"
    sam_dir.mkdir(parents=True, exist_ok=True)

    tmp = _tmp_path(repo_path)
    data = json.dumps(graph.to_json(), indent=2, ensure_ascii=False)
    tmp.write_text(data, encoding="utf-8")
    tmp.rename(_graph_path(repo_path))


def load_graph(repo_path: Path) -> Graph | None:
    """Return the cached Graph, or None if missing or corrupt."""
    graph_file = _graph_path(repo_path)
    if not graph_file.exists():
        return None
    try:
        data = json.loads(graph_file.read_text(encoding="utf-8"))
        return Graph.from_json(data)
    except (json.JSONDecodeError, KeyError, ValueError):
        return None


def graph_age_seconds(repo_path: Path) -> float | None:
    """Seconds since graph.json was last written, or None if it doesn't exist."""
    graph_file = _graph_path(repo_path)
    if not graph_file.exists():
        return None
    return time.time() - graph_file.stat().st_mtime
