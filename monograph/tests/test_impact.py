"""Tests for impact analysis logic."""

from __future__ import annotations

from monograph.graph import Edge, EdgeType, Graph
from monograph.impact import analyze_impact, _risk_level


def _make_graph(edges: list[tuple[str, str, EdgeType, float]]) -> Graph:
    all_domains = set()
    edge_objs = []
    for from_d, to_d, et, weight in edges:
        all_domains.add(from_d)
        all_domains.add(to_d)
        edge_objs.append(
            Edge(from_domain=from_d, to_domain=to_d, edge_type=et, weight=weight)
        )
    g = Graph.empty("/repo")
    g.domains = sorted(all_domains)
    g.edges = edge_objs
    return g


class TestAnalyzeImpact:
    def test_static_dep_is_critical(self):
        # apis/payments depends on shared/auth
        graph = _make_graph([("apis/payments", "shared/auth", EdgeType.STATIC, 1.0)])
        graph.repo_root = ""  # disable path resolution
        result = analyze_impact(graph, ["shared/auth/token.ts"])
        # apis/payments imports shared/auth → static change → critical risk
        # But without repo_root we can't resolve the file to a domain, so test differently
        # Use a graph with known repo_root
        graph2 = _make_graph([("apis/payments", "shared/auth", EdgeType.STATIC, 1.0)])
        import tempfile, os
        with tempfile.TemporaryDirectory() as tmp:
            os.makedirs(os.path.join(tmp, "shared", "auth"))
            graph2.repo_root = tmp
            result2 = analyze_impact(graph2, ["shared/auth/token.ts"])
            affected_domains = [a["domain"] for a in result2["affected"]]
            assert "apis/payments" in affected_domains
            affected_by_payments = next(a for a in result2["affected"] if a["domain"] == "apis/payments")
            assert affected_by_payments["risk"] == "critical"

    def test_cochange_high_score(self):
        graph = _make_graph([("apis/sales", "shared/auth", EdgeType.COCHANGE, 0.85)])
        import tempfile, os
        with tempfile.TemporaryDirectory() as tmp:
            os.makedirs(os.path.join(tmp, "shared", "auth"))
            graph.repo_root = tmp
            result = analyze_impact(graph, ["shared/auth/token.ts"])
            affected_domains = [a["domain"] for a in result["affected"]]
            assert "apis/sales" in affected_domains
            entry = next(a for a in result["affected"] if a["domain"] == "apis/sales")
            assert entry["risk"] == "high"

    def test_cochange_medium_score(self):
        graph = _make_graph([("apis/sales", "shared/auth", EdgeType.COCHANGE, 0.45)])
        import tempfile, os
        with tempfile.TemporaryDirectory() as tmp:
            os.makedirs(os.path.join(tmp, "shared", "auth"))
            graph.repo_root = tmp
            result = analyze_impact(graph, ["shared/auth/token.ts"])
            entry = next(a for a in result["affected"] if a["domain"] == "apis/sales")
            assert entry["risk"] == "medium"

    def test_unchanged_domain_not_affected(self):
        graph = _make_graph([("apis/payments", "shared/auth", EdgeType.STATIC, 1.0)])
        import tempfile, os
        with tempfile.TemporaryDirectory() as tmp:
            os.makedirs(os.path.join(tmp, "shared", "auth"))
            os.makedirs(os.path.join(tmp, "apis", "catalog"))
            graph.domains = sorted(["apis/payments", "shared/auth", "apis/catalog"])
            graph.repo_root = tmp
            result = analyze_impact(graph, ["shared/auth/token.ts"])
            assert "apis/catalog" in result["not_affected"]

    def test_empty_changed_files(self):
        graph = _make_graph([("apis/payments", "shared/auth", EdgeType.STATIC, 1.0)])
        graph.repo_root = "/repo"
        result = analyze_impact(graph, [])
        assert result["affected"] == []

    def test_risk_level_helper(self):
        static_edge = Edge("a", "b", EdgeType.STATIC, 1.0)
        high_cc_edge = Edge("a", "b", EdgeType.COCHANGE, 0.8)
        med_cc_edge = Edge("a", "b", EdgeType.COCHANGE, 0.5)
        assert _risk_level(static_edge) == "critical"
        assert _risk_level(high_cc_edge) == "high"
        assert _risk_level(med_cc_edge) == "medium"
