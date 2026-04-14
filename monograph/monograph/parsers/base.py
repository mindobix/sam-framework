"""Abstract base class for all language parsers."""

from __future__ import annotations

from abc import ABC, abstractmethod
from pathlib import Path


class BaseParser(ABC):
    """
    Extracts intra-repo import relationships from a single source file.

    Subclasses must implement:
      - extract_imports(file_path, content) -> list of raw import strings
      - resolve_import_to_domain(import_str, file_path, repo_root) -> domain path | None

    Domain paths are repo-root-relative directory paths, e.g. "shared/auth" or "apis/sales".
    External imports (npm packages, stdlib, etc.) must return None.
    """

    @abstractmethod
    def extract_imports(self, file_path: Path, content: str) -> list[str]:
        """Return raw import strings found in the file."""
        raise NotImplementedError

    @abstractmethod
    def resolve_import_to_domain(
        self,
        import_str: str,
        file_path: Path,
        repo_root: Path,
    ) -> str | None:
        """
        Map a raw import string to a domain path relative to repo_root.

        Returns None if:
        - The import points outside the repo (external package)
        - The import cannot be resolved

        The domain path is the top two path segments: e.g. if the resolved
        absolute path is /repo/shared/auth/token.ts, the domain is "shared/auth".
        """
        raise NotImplementedError

    def get_domain_imports(
        self,
        file_path: Path,
        content: str,
        repo_root: Path,
    ) -> list[str]:
        """
        High-level helper: returns deduplicated domain paths imported by this file.
        The file's own domain is excluded (self-references).
        """
        file_domain = _file_domain(file_path, repo_root)
        raw_imports = self.extract_imports(file_path, content)
        seen: set[str] = set()
        result: list[str] = []
        for imp in raw_imports:
            domain = self.resolve_import_to_domain(imp, file_path, repo_root)
            if domain and domain != file_domain and domain not in seen:
                seen.add(domain)
                result.append(domain)
        return result


def _file_domain(file_path: Path, repo_root: Path) -> str | None:
    """Return the domain of a file."""
    return path_to_domain(file_path, repo_root)


# Cache for two-level parent detection
_two_level_cache: dict[str, set[str]] = {}


def _get_two_level_parents(repo_root: Path) -> set[str]:
    """Detect which top-level dirs use two-level domain paths (from profiles.yaml)."""
    key = str(repo_root)
    if key in _two_level_cache:
        return _two_level_cache[key]

    parents: set[str] = set()
    profiles_path = repo_root / ".sam" / "profiles.yaml"
    if profiles_path.exists():
        try:
            for line in profiles_path.read_text().splitlines():
                trimmed = line.strip().lstrip("- ")
                if "/" in trimmed and ":" not in trimmed and not trimmed.startswith("#"):
                    parents.add(trimmed.split("/")[0])
        except Exception:
            pass

    _two_level_cache[key] = parents
    return parents


def path_to_domain(abs_path: Path, repo_root: Path) -> str | None:
    """
    Convert a file/directory path to its domain path.

    Detects repo structure from .sam/profiles.yaml:
    - Two-level repos (apis/sales): returns first two segments for matching parents
    - Single-level repos (bigquery): returns first segment only
    """
    try:
        rel = abs_path.resolve().relative_to(repo_root.resolve())
    except ValueError:
        return None
    parts = rel.parts
    if not parts or parts[0].startswith("."):
        return None

    two_level = _get_two_level_parents(repo_root)
    if parts[0] in two_level and len(parts) >= 2:
        return f"{parts[0]}/{parts[1]}"
    return parts[0]
