"""Tests for the resolve endpoint logic."""

from __future__ import annotations

from monograph.graph import Edge, EdgeType, Graph
from monograph.resolver import resolve


def _make_graph(edges: list[tuple[str, str, EdgeType]]) -> Graph:
    all_domains = set()
    edge_objs = []
    for from_d, to_d, et in edges:
        all_domains.add(from_d)
        all_domains.add(to_d)
        edge_objs.append(
            Edge(from_domain=from_d, to_domain=to_d, edge_type=et, weight=1.0)
        )
    g = Graph.empty("/repo")
    g.domains = sorted(all_domains)
    g.edges = edge_objs
    return g


class TestResolve:
    def test_single_direct_dep(self):
        graph = _make_graph([("apis/sales", "shared/auth", EdgeType.STATIC)])
        result = resolve(graph, ["apis/sales"])
        assert "shared/auth" in result["resolved"]
        assert "shared/auth" in result["inferred"]

    def test_transitive_deps(self):
        # apis/sales → shared/auth → shared/types
        graph = _make_graph([
            ("apis/sales", "shared/auth", EdgeType.STATIC),
            ("shared/auth", "shared/types", EdgeType.STATIC),
        ])
        result = resolve(graph, ["apis/sales"])
        assert "shared/types" in result["resolved"]
        assert "shared/types" in result["inferred"]

    def test_seeds_not_in_inferred(self):
        graph = _make_graph([("apis/sales", "shared/auth", EdgeType.STATIC)])
        result = resolve(graph, ["apis/sales"])
        assert "apis/sales" not in result["inferred"]
        assert "apis/sales" in result["resolved"]

    def test_no_deps(self):
        graph = _make_graph([])
        graph.domains = ["apis/sales"]
        result = resolve(graph, ["apis/sales"])
        assert result["resolved"] == ["apis/sales"]
        assert result["inferred"] == []

    def test_cycle_safe(self):
        # A→B→A
        graph = _make_graph([
            ("a/x", "b/y", EdgeType.STATIC),
            ("b/y", "a/x", EdgeType.STATIC),
        ])
        result = resolve(graph, ["a/x"])
        assert "b/y" in result["resolved"]
        # Should terminate without infinite loop

    def test_multiple_seeds(self):
        graph = _make_graph([
            ("apis/sales", "shared/auth", EdgeType.STATIC),
            ("apis/orders", "shared/types", EdgeType.STATIC),
        ])
        result = resolve(graph, ["apis/sales", "apis/orders"])
        assert "shared/auth" in result["resolved"]
        assert "shared/types" in result["resolved"]

    def test_inference_detail_format(self):
        graph = _make_graph([("apis/sales", "shared/auth", EdgeType.STATIC)])
        result = resolve(graph, ["apis/sales"])
        detail = result["inference_detail"]
        assert len(detail) == 1
        assert detail[0]["domain"] == "shared/auth"
        assert detail[0]["from"] == "apis/sales"
        assert "reason" in detail[0]
