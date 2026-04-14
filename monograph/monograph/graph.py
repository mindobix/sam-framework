"""Graph data model — nodes are domain paths, edges are static imports or co-change pairs."""

from __future__ import annotations

from collections import defaultdict, deque
from dataclasses import dataclass, field
from datetime import datetime, timezone
from enum import Enum
from typing import Any


class EdgeType(Enum):
    STATIC = "static"      # direct import statement in source code
    COCHANGE = "cochange"  # git log co-occurrence pattern


@dataclass
class Edge:
    from_domain: str
    to_domain: str
    edge_type: EdgeType
    weight: float            # 1.0 for static; 0.0–1.0 for cochange
    commit_count: int = 0    # meaningful only for cochange edges
    source_files: list[str] = field(default_factory=list)  # meaningful only for static edges

    def to_json(self) -> dict[str, Any]:
        return {
            "from": self.from_domain,
            "to": self.to_domain,
            "type": self.edge_type.value,
            "weight": round(self.weight, 4),
            "commit_count": self.commit_count,
            "source_files": self.source_files,
        }

    @classmethod
    def from_json(cls, data: dict[str, Any]) -> "Edge":
        return cls(
            from_domain=data["from"],
            to_domain=data["to"],
            edge_type=EdgeType(data["type"]),
            weight=float(data["weight"]),
            commit_count=int(data.get("commit_count", 0)),
            source_files=list(data.get("source_files", [])),
        )


@dataclass
class Graph:
    version: str
    generated_at: str
    repo_root: str
    domains: list[str]
    edges: list[Edge]

    # Internal adjacency indices — built lazily on first access
    _out_index: dict[str, list[Edge]] = field(default_factory=dict, repr=False, compare=False)
    _in_index: dict[str, list[Edge]] = field(default_factory=dict, repr=False, compare=False)
    _indexed: bool = field(default=False, repr=False, compare=False)

    @classmethod
    def empty(cls, repo_root: str) -> "Graph":
        return cls(
            version="1",
            generated_at=datetime.now(timezone.utc).isoformat(),
            repo_root=repo_root,
            domains=[],
            edges=[],
        )

    # ── Serialization ─────────────────────────────────────────────────────────

    def to_json(self) -> dict[str, Any]:
        return {
            "version": self.version,
            "generated_at": self.generated_at,
            "repo_root": self.repo_root,
            "domains": sorted(self.domains),
            "edges": [e.to_json() for e in self.edges],
        }

    @classmethod
    def from_json(cls, data: dict[str, Any]) -> "Graph":
        g = cls(
            version=data.get("version", "1"),
            generated_at=data.get("generated_at", ""),
            repo_root=data.get("repo_root", ""),
            domains=list(data.get("domains", [])),
            edges=[Edge.from_json(e) for e in data.get("edges", [])],
        )
        return g

    # ── Index ─────────────────────────────────────────────────────────────────

    def _build_index(self) -> None:
        if self._indexed:
            return
        out: dict[str, list[Edge]] = defaultdict(list)
        inp: dict[str, list[Edge]] = defaultdict(list)
        for edge in self.edges:
            out[edge.from_domain].append(edge)
            inp[edge.to_domain].append(edge)
        self._out_index = dict(out)
        self._in_index = dict(inp)
        self._indexed = True

    # ── Queries ───────────────────────────────────────────────────────────────

    def get_dependencies(self, domain: str) -> list[Edge]:
        """Edges where from_domain == domain — what this domain depends on."""
        self._build_index()
        return self._out_index.get(domain, [])

    def get_dependents(self, domain: str) -> list[Edge]:
        """Edges where to_domain == domain — what depends on this domain."""
        self._build_index()
        return self._in_index.get(domain, [])

    def resolve_transitive(self, domains: list[str]) -> list[str]:
        """BFS over the dependency graph; returns all transitively required domains."""
        self._build_index()
        visited: set[str] = set()
        queue: deque[str] = deque(domains)
        while queue:
            current = queue.popleft()
            if current in visited:
                continue
            visited.add(current)
            for edge in self._out_index.get(current, []):
                if edge.to_domain not in visited:
                    queue.append(edge.to_domain)
        return sorted(visited)

    def merge_edges(self, new_edges: list[Edge]) -> None:
        """Merge a batch of edges, deduplicating by (from, to, type) and keeping
        the highest weight when there are duplicates."""
        key_map: dict[tuple[str, str, str], Edge] = {}
        for e in self.edges:
            k = (e.from_domain, e.to_domain, e.edge_type.value)
            key_map[k] = e
        for e in new_edges:
            k = (e.from_domain, e.to_domain, e.edge_type.value)
            existing = key_map.get(k)
            if existing is None:
                key_map[k] = e
            else:
                # Keep higher weight; merge source_files; accumulate commit_count
                key_map[k] = Edge(
                    from_domain=e.from_domain,
                    to_domain=e.to_domain,
                    edge_type=e.edge_type,
                    weight=max(existing.weight, e.weight),
                    commit_count=existing.commit_count + e.commit_count,
                    source_files=sorted(set(existing.source_files + e.source_files)),
                )
        self.edges = list(key_map.values())
        self._indexed = False  # invalidate index
