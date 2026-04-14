"""Co-change miner — finds files that are frequently committed together."""

from __future__ import annotations

import subprocess
from collections import defaultdict
from pathlib import Path

from monograph.parsers.base import path_to_domain

MIN_COCHANGE_SCORE = 0.3
DEFAULT_MAX_COMMITS = 500


def mine_cochange(
    repo_path: Path,
    max_commits: int = DEFAULT_MAX_COMMITS,
) -> dict[str, dict[str, int]]:
    """
    Mine git log for co-change relationships at the **domain** level.

    Returns:
        {domain_a: {domain_b: co_change_commit_count, ...}, ...}

    Only intra-repo domain pairs are tracked; external files are ignored.
    """
    raw_commits = _get_commit_file_lists(repo_path, max_commits)
    domain_pairs: dict[str, dict[str, int]] = defaultdict(lambda: defaultdict(int))

    for file_list in raw_commits:
        domains: set[str] = set()
        for f in file_list:
            if f.startswith(".sam/") or f.startswith(".git/"):
                continue
            domain = path_to_domain(repo_path / f, repo_path)
            if domain:
                domains.add(domain)

        # Generate all ordered pairs within the commit
        domain_list = sorted(domains)
        for i, a in enumerate(domain_list):
            for b in domain_list[i + 1:]:
                domain_pairs[a][b] += 1
                domain_pairs[b][a] += 1

    return {k: dict(v) for k, v in domain_pairs.items()}


def score_cochange(
    counts: dict[str, dict[str, int]],
    min_score: float = MIN_COCHANGE_SCORE,
) -> dict[str, dict[str, float]]:
    """
    Normalize raw co-change counts to 0.0–1.0 scores.

    score(a, b) = count(a, b) / max_count_in_repo

    Pairs below min_score are dropped to keep the graph sparse.
    """
    max_count = 0
    for inner in counts.values():
        for count in inner.values():
            if count > max_count:
                max_count = count

    if max_count == 0:
        return {}

    scores: dict[str, dict[str, float]] = {}
    for domain_a, partners in counts.items():
        scored_partners: dict[str, float] = {}
        for domain_b, count in partners.items():
            score = count / max_count
            if score >= min_score:
                scored_partners[domain_b] = round(score, 4)
        if scored_partners:
            scores[domain_a] = scored_partners

    return scores


def file_cochange_partners(
    repo_path: Path,
    target_file: str,
    max_commits: int = DEFAULT_MAX_COMMITS,
) -> list[dict]:
    """
    Return per-file co-change partners for a specific file (not domain-level).

    Used by the /cochange endpoint.

    Returns list of {"file": ..., "score": ..., "commit_count": ...} dicts,
    sorted by score descending.
    """
    raw_commits = _get_commit_file_lists(repo_path, max_commits)
    partner_counts: dict[str, int] = defaultdict(int)

    for file_list in raw_commits:
        if target_file not in file_list:
            continue
        for f in file_list:
            if f != target_file and not f.startswith(".sam/") and not f.startswith(".git/"):
                partner_counts[f] += 1

    if not partner_counts:
        return []

    max_count = max(partner_counts.values())
    result = [
        {
            "file": f,
            "score": round(count / max_count, 4),
            "commit_count": count,
        }
        for f, count in partner_counts.items()
    ]
    result.sort(key=lambda x: x["score"], reverse=True)
    return result


def _get_commit_file_lists(
    repo_path: Path, max_commits: int
) -> list[list[str]]:
    """
    Run `git log --name-only --format=""` and parse output into per-commit file lists.

    Commits are separated by blank lines in the output.
    """
    try:
        result = subprocess.run(
            [
                "git",
                "log",
                "--name-only",
                "--format=",
                f"-n{max_commits}",
            ],
            cwd=repo_path,
            capture_output=True,
            text=True,
            timeout=30,
        )
    except (FileNotFoundError, subprocess.TimeoutExpired):
        return []

    if result.returncode != 0:
        return []

    commits: list[list[str]] = []
    current: list[str] = []

    for line in result.stdout.splitlines():
        line = line.strip()
        if not line:
            if current:
                commits.append(current)
                current = []
        else:
            current.append(line)

    if current:
        commits.append(current)

    return commits
