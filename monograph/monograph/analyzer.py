"""Orchestrates full graph build: walks repo, runs parsers, mines co-change."""

from __future__ import annotations

import logging
import os
from datetime import datetime, timezone
from pathlib import Path

from monograph.cache import acquire_lock, release_lock, save_graph
from monograph.cochange import mine_cochange, score_cochange
from monograph.graph import Edge, EdgeType, Graph
from monograph.parsers import parser_for_file

log = logging.getLogger(__name__)

# Directories to skip during the walk
_SKIP_DIRS = frozenset(
    {
        ".git",
        ".sam",
        "node_modules",
        "__pycache__",
        "vendor",
        ".venv",
        "venv",
        "env",
        "dist",
        "build",
        "target",
        "bin",
        "obj",
        ".idea",
        ".vscode",
    }
)


def build_graph(repo_path: Path) -> Graph:
    """
    Perform a full graph build for the repo at repo_path.

    Steps:
    1. Walk the source tree, run per-file parsers → static edges
    2. Mine git log → co-change edges
    3. Merge and return the complete Graph

    This is meant to be called in a background thread from the server.
    The caller is responsible for holding the lock (acquire_lock / release_lock).
    """
    log.info("Starting graph build for %s", repo_path)
    domains: set[str] = _discover_domains(repo_path)
    static_edges = _build_static_edges(repo_path)
    cochange_edges = _build_cochange_edges(repo_path, domains)

    graph = Graph(
        version="1",
        generated_at=datetime.now(timezone.utc).isoformat(),
        repo_root=str(repo_path.resolve()),
        domains=sorted(domains),
        edges=[],
    )
    graph.merge_edges(static_edges)
    graph.merge_edges(cochange_edges)

    log.info(
        "Graph built: %d domains, %d edges", len(graph.domains), len(graph.edges)
    )
    return graph


def build_graph_locked(repo_path: Path) -> Graph:
    """
    Convenience wrapper that acquires the .sam/graph.lock for the duration of
    the build and saves the result to .sam/graph.json.
    """
    acquire_lock(repo_path)
    try:
        graph = build_graph(repo_path)
        save_graph(graph, repo_path)
        return graph
    finally:
        release_lock(repo_path)


# ── Domain discovery ──────────────────────────────────────────────────────────


def _discover_domains(repo_path: Path) -> set[str]:
    """
    A domain is any two-level directory that contains at least one source file.
    e.g. apis/sales, shared/auth
    """
    domains: set[str] = set()
    try:
        top_dirs = [
            d for d in repo_path.iterdir()
            if d.is_dir() and d.name not in _SKIP_DIRS
        ]
    except OSError:
        return domains

    for top in top_dirs:
        try:
            for sub in top.iterdir():
                if sub.is_dir() and sub.name not in _SKIP_DIRS:
                    if _has_source_files(sub):
                        domains.add(f"{top.name}/{sub.name}")
        except OSError:
            continue

    return domains


def _has_source_files(directory: Path) -> bool:
    _SOURCE_EXTENSIONS = {".ts", ".tsx", ".js", ".jsx", ".py", ".go", ".java", ".cs", ".kt"}
    for _, _, files in os.walk(directory):
        for fname in files:
            if Path(fname).suffix in _SOURCE_EXTENSIONS:
                return True
    return False


# ── Static edge extraction ────────────────────────────────────────────────────


def _build_static_edges(repo_path: Path) -> list[Edge]:
    """Walk every source file and collect static import edges."""
    edges_map: dict[tuple[str, str], Edge] = {}

    for dirpath, dirnames, filenames in os.walk(repo_path):
        # Prune skip dirs in-place
        dirnames[:] = [d for d in dirnames if d not in _SKIP_DIRS]

        dir_path = Path(dirpath)
        rel_parts = dir_path.relative_to(repo_path).parts

        # Only process files that are inside a domain (at least 2 levels deep)
        if len(rel_parts) < 2:
            continue

        from_domain = f"{rel_parts[0]}/{rel_parts[1]}"

        for fname in filenames:
            file_path = dir_path / fname
            parser = parser_for_file(str(file_path))
            if parser is None:
                continue

            try:
                content = file_path.read_text(encoding="utf-8", errors="replace")
            except OSError:
                continue

            try:
                imported_domains = parser.get_domain_imports(
                    file_path, content, repo_path
                )
            except Exception as exc:
                log.debug("Parser error in %s: %s", file_path, exc)
                continue

            for to_domain in imported_domains:
                key = (from_domain, to_domain)
                if key in edges_map:
                    existing = edges_map[key]
                    rel_file = str(file_path.relative_to(repo_path))
                    if rel_file not in existing.source_files:
                        existing.source_files.append(rel_file)
                else:
                    rel_file = str(file_path.relative_to(repo_path))
                    edges_map[key] = Edge(
                        from_domain=from_domain,
                        to_domain=to_domain,
                        edge_type=EdgeType.STATIC,
                        weight=1.0,
                        source_files=[rel_file],
                    )

    return list(edges_map.values())


# ── Co-change edge extraction ─────────────────────────────────────────────────


def _build_cochange_edges(repo_path: Path, domains: set[str]) -> list[Edge]:
    """Mine git co-change and return co-change edges."""
    counts = mine_cochange(repo_path)
    scores = score_cochange(counts)

    edges: list[Edge] = []
    for from_domain, partners in scores.items():
        for to_domain, score in partners.items():
            # Avoid duplicating A→B and B→A
            if from_domain < to_domain:
                commit_count = counts.get(from_domain, {}).get(to_domain, 0)
                edges.append(
                    Edge(
                        from_domain=from_domain,
                        to_domain=to_domain,
                        edge_type=EdgeType.COCHANGE,
                        weight=score,
                        commit_count=commit_count,
                    )
                )

    return edges
