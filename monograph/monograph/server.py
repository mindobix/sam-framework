"""FastAPI server — the MonoGraph HTTP daemon."""

from __future__ import annotations

import logging
import threading
from pathlib import Path
from typing import Any

import uvicorn
from fastapi import FastAPI, HTTPException, Query, Response
from fastapi.middleware.cors import CORSMiddleware
from pydantic import BaseModel

from monograph import __version__
from monograph.analyzer import build_graph_locked
from monograph.cache import (
    graph_age_seconds,
    is_graph_stale,
    load_graph,
    lock_exists,
)
from monograph.cochange import file_cochange_partners
from monograph.graph import Graph
from monograph.impact import analyze_impact
from monograph.resolver import resolve

log = logging.getLogger(__name__)

app = FastAPI(title="MonoGraph", version=__version__)

app.add_middleware(
    CORSMiddleware,
    allow_origins=["*"],
    allow_methods=["*"],
    allow_headers=["*"],
)

# ── State ─────────────────────────────────────────────────────────────────────

_repo_path: Path | None = None
_graph: Graph | None = None
_graph_lock = threading.Lock()
_build_thread: threading.Thread | None = None


def get_graph() -> Graph | None:
    with _graph_lock:
        return _graph


def set_graph(g: Graph) -> None:
    global _graph
    with _graph_lock:
        _graph = g


def get_repo_path() -> Path:
    if _repo_path is None:
        raise HTTPException(
            status_code=503,
            detail="No repo path configured. POST /analyze to set one.",
        )
    return _repo_path


# ── Models ────────────────────────────────────────────────────────────────────


class AnalyzeRequest(BaseModel):
    repo_path: str


class ResolveRequest(BaseModel):
    domains: list[str]


class ImpactRequest(BaseModel):
    changed_files: list[str]


# ── Endpoints ─────────────────────────────────────────────────────────────────


@app.get("/health")
def health() -> dict[str, Any]:
    graph = get_graph()
    repo = _repo_path

    age = None
    if repo:
        age = graph_age_seconds(repo)

    return {
        "status": "ok",
        "version": __version__,
        "graph_ready": graph is not None,
        "graph_age_seconds": round(age, 1) if age is not None else None,
        "repo_path": str(repo) if repo else None,
    }


@app.post("/analyze")
def analyze(req: AnalyzeRequest) -> dict[str, Any]:
    """Trigger a background graph build for the given repo."""
    global _repo_path, _build_thread

    repo = Path(req.repo_path)
    if not repo.is_dir():
        raise HTTPException(
            status_code=400, detail=f"repo_path does not exist: {repo}"
        )

    _repo_path = repo

    # Try to load cached graph first
    cached = load_graph(repo)
    if cached is not None:
        set_graph(cached)
        if not is_graph_stale(repo):
            return {
                "status": "cached",
                "message": "Loaded from cache. Graph is up-to-date.",
            }

    # Don't start a second build if one is in progress
    if lock_exists(repo):
        return {"status": "building", "message": "Build already in progress."}

    def _build() -> None:
        try:
            g = build_graph_locked(repo)
            set_graph(g)
            log.info("Graph build complete: %d domains", len(g.domains))
        except Exception as exc:
            log.error("Graph build failed: %s", exc, exc_info=True)

    _build_thread = threading.Thread(target=_build, daemon=True)
    _build_thread.start()

    return {
        "status": "started",
        "message": "Graph build started in background.",
        "estimated_seconds": 30,
    }


@app.get("/graph")
def get_graph_domain(
    domain: str = Query(..., description="Domain path, e.g. 'apis/sales'"),
    response: Response = None,
) -> dict[str, Any]:
    graph = get_graph()
    if graph is None:
        if response:
            response.headers["X-Graph-Ready"] = "false"
        return {"domain": domain, "edges": [], "warning": "Graph not yet built."}

    edges = graph.get_dependencies(domain)
    return {
        "domain": domain,
        "edges": [
            {
                "to": e.to_domain,
                "type": e.edge_type.value,
                "weight": e.weight,
                "commit_count": e.commit_count,
            }
            for e in edges
        ],
    }


@app.post("/resolve")
def resolve_domains(req: ResolveRequest, response: Response = None) -> dict[str, Any]:
    graph = get_graph()
    if graph is None:
        if response:
            response.headers["X-Graph-Ready"] = "false"
        return {
            "resolved": sorted(req.domains),
            "inferred": [],
            "inference_detail": [],
            "warning": "Graph not yet built — returning seeds only.",
        }
    return resolve(graph, req.domains)


@app.post("/impact")
def impact(req: ImpactRequest) -> dict[str, Any]:
    graph = get_graph()
    if graph is None:
        raise HTTPException(
            status_code=503,
            detail="Graph not yet built. POST /analyze first.",
        )
    return analyze_impact(graph, req.changed_files)


@app.get("/cochange")
def cochange(
    file: str = Query(..., description="Repo-relative file path"),
) -> dict[str, Any]:
    repo = _repo_path
    if repo is None:
        raise HTTPException(status_code=503, detail="No repo configured.")

    partners = file_cochange_partners(repo, file)
    return {
        "file": file,
        "partners": partners,
    }


# ── Server runner ─────────────────────────────────────────────────────────────


def run(
    host: str = "127.0.0.1",
    port: int = 7474,
    repo_path: Path | None = None,
    log_level: str = "info",
) -> None:
    """Start uvicorn. Optionally pre-load a repo's graph."""
    global _repo_path

    if repo_path:
        _repo_path = repo_path
        cached = load_graph(repo_path)
        if cached:
            set_graph(cached)
            log.info(
                "Loaded cached graph: %d domains, %d edges",
                len(cached.domains),
                len(cached.edges),
            )
        elif not lock_exists(repo_path):
            # Kick off a build in the background
            def _startup_build() -> None:
                try:
                    g = build_graph_locked(repo_path)
                    set_graph(g)
                except Exception as exc:
                    log.error("Startup graph build failed: %s", exc)

            threading.Thread(target=_startup_build, daemon=True).start()

    uvicorn.run(app, host=host, port=port, log_level=log_level)
