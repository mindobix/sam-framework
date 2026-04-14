"""Tests for co-change miner."""

from __future__ import annotations

from unittest.mock import patch

import pytest

from monograph.cochange import mine_cochange, score_cochange


class TestMineCochange:
    def test_two_domains_commited_together(self, tmp_path):
        git_output = (
            "\n"  # blank after format=""
            "apis/sales/service.ts\n"
            "shared/auth/token.ts\n"
            "\n"
            "apis/orders/handler.py\n"
            "shared/auth/token.ts\n"
        )

        (tmp_path / ".git").mkdir()
        with patch("subprocess.run") as mock_run:
            mock_run.return_value.returncode = 0
            mock_run.return_value.stdout = git_output

            counts = mine_cochange(tmp_path, max_commits=10)

        # apis/sales and shared/auth co-changed once
        assert counts.get("apis/sales", {}).get("shared/auth", 0) == 1
        # apis/orders and shared/auth co-changed once
        assert counts.get("apis/orders", {}).get("shared/auth", 0) == 1
        # apis/sales and apis/orders never co-changed
        assert counts.get("apis/sales", {}).get("apis/orders", 0) == 0

    def test_empty_repo(self, tmp_path):
        with patch("subprocess.run") as mock_run:
            mock_run.return_value.returncode = 0
            mock_run.return_value.stdout = ""
            counts = mine_cochange(tmp_path)
        assert counts == {}

    def test_git_failure(self, tmp_path):
        with patch("subprocess.run") as mock_run:
            mock_run.return_value.returncode = 128
            mock_run.return_value.stdout = ""
            counts = mine_cochange(tmp_path)
        assert counts == {}


class TestScoreCochange:
    def test_normalization(self):
        counts = {
            "a/x": {"b/y": 10, "c/z": 5},
            "b/y": {"a/x": 10},
            "c/z": {"a/x": 5},
        }
        scores = score_cochange(counts, min_score=0.0)
        # max is 10, so a/x→b/y = 1.0, a/x→c/z = 0.5
        assert scores["a/x"]["b/y"] == pytest.approx(1.0)
        assert scores["a/x"]["c/z"] == pytest.approx(0.5)

    def test_min_score_filter(self):
        counts = {
            "a/x": {"b/y": 10, "c/z": 2},
            "b/y": {"a/x": 10},
            "c/z": {"a/x": 2},
        }
        scores = score_cochange(counts, min_score=0.3)
        assert "b/y" in scores.get("a/x", {})
        assert "c/z" not in scores.get("a/x", {})

    def test_empty_counts(self):
        assert score_cochange({}) == {}
