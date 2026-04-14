"""Resolve endpoint logic — given a set of seed domains, return all transitive deps."""

from __future__ import annotations

from monograph.graph import Edge, Graph


def resolve(graph: Graph, seed_domains: list[str]) -> dict:
    """
    BFS over the dependency graph to find all domains that must be hydrated
    when working on the seed domains.

    Returns a dict matching the /resolve response schema:
    {
        "resolved": [...],   # all required domains (seeds + inferred)
        "inferred": [...],   # domains added beyond the seeds
        "inference_detail": [{"domain": ..., "reason": ..., "from": ...}, ...]
    }
    """
    seed_set = set(seed_domains)
    resolved_set: set[str] = set(seed_domains)
    inference_detail: list[dict] = []

    # BFS
    queue = list(seed_domains)
    visited: set[str] = set()

    while queue:
        current = queue.pop(0)
        if current in visited:
            continue
        visited.add(current)

        for edge in graph.get_dependencies(current):
            dep = edge.to_domain
            if dep not in resolved_set:
                resolved_set.add(dep)
                reason = _edge_reason(edge)
                inference_detail.append(
                    {"domain": dep, "reason": reason, "from": current}
                )
                queue.append(dep)

    inferred = sorted(resolved_set - seed_set)
    resolved = sorted(resolved_set)

    return {
        "resolved": resolved,
        "inferred": inferred,
        "inference_detail": inference_detail,
    }


def _edge_reason(edge: Edge) -> str:
    return edge.edge_type.value
