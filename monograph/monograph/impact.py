"""Impact analysis — given changed files, determine which domains are affected."""

from __future__ import annotations

from pathlib import Path

from monograph.graph import Edge, EdgeType, Graph
from monograph.parsers.base import path_to_domain


def analyze_impact(graph: Graph, changed_files: list[str]) -> dict:
    """
    Given a list of changed file paths (repo-root-relative), return the domains
    that are transitively affected.

    Risk levels:
      critical — static dependency (direct import)
      high     — co-change (git pattern, high score ≥ 0.7)
      medium   — co-change (score 0.3–0.7)

    Returns a dict matching the /impact response schema:
    {
        "affected": [{"domain": ..., "risk": ..., "type": ..., "calls_per_day": ...}, ...],
        "not_affected": [...]
    }
    """
    repo_root = Path(graph.repo_root) if graph.repo_root else None
    changed_domains: set[str] = set()

    for f in changed_files:
        if repo_root:
            domain = path_to_domain(repo_root / f, repo_root)
        else:
            # Fallback: take first two path segments
            parts = Path(f).parts
            domain = f"{parts[0]}/{parts[1]}" if len(parts) >= 2 else None
        if domain:
            changed_domains.add(domain)

    if not changed_domains:
        return {
            "affected": [],
            "not_affected": sorted(graph.domains),
        }

    # For each changed domain, find all domains that depend on it (reverse edges).
    # Gather (affected_domain, best_edge) pairs.
    affected: dict[str, dict] = {}

    for changed_domain in changed_domains:
        # Direct dependents
        for edge in graph.get_dependents(changed_domain):
            dep_domain = edge.from_domain
            if dep_domain in changed_domains:
                continue  # Don't report changed domains as affected
            risk = _risk_level(edge)
            existing = affected.get(dep_domain)
            if existing is None or _risk_rank(risk) > _risk_rank(existing["risk"]):
                affected[dep_domain] = {
                    "domain": dep_domain,
                    "risk": risk,
                    "type": edge.edge_type.value,
                    "calls_per_day": _estimate_calls(edge),
                }

    affected_list = sorted(
        affected.values(),
        key=lambda x: (_risk_rank(x["risk"]), x.get("calls_per_day", 0)),
        reverse=True,
    )

    not_affected = sorted(
        d for d in graph.domains
        if d not in affected and d not in changed_domains
    )

    return {
        "affected": affected_list,
        "not_affected": not_affected,
    }


def _risk_level(edge: Edge) -> str:
    if edge.edge_type == EdgeType.STATIC:
        return "critical"
    if edge.weight >= 0.7:
        return "high"
    return "medium"


def _risk_rank(level: str) -> int:
    return {"critical": 3, "high": 2, "medium": 1, "low": 0}.get(level, 0)


def _estimate_calls(edge: Edge) -> int:
    """
    Placeholder for calls_per_day.  In production this would query an APM system.
    For now, use commit_count as a rough proxy (high co-change ≈ high coupling).
    """
    return edge.commit_count * 5  # heuristic: ~5 calls per co-change commit
