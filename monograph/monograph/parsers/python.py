"""Python import parser using tree-sitter with regex fallback."""

from __future__ import annotations

import re
from pathlib import Path
from typing import Any

from monograph.parsers.base import BaseParser, path_to_domain

_IMPORT_RE = re.compile(
    r"""^(?:from\s+([\w.]+)\s+import|import\s+([\w.,\s]+))""",
    re.MULTILINE,
)
_FROM_RELATIVE_RE = re.compile(r"""^from\s+(\.+)([\w.]*)\s+import""", re.MULTILINE)


def _try_load_py_parser() -> Any | None:
    try:
        import tree_sitter_python as tspy
        from tree_sitter import Language, Parser

        py_language = Language(tspy.language_python())
        return Parser(py_language)
    except Exception:
        return None


_PY_PARSER: Any | None = None
_PY_PARSER_LOADED = False


def _get_parser() -> Any | None:
    global _PY_PARSER, _PY_PARSER_LOADED
    if not _PY_PARSER_LOADED:
        _PY_PARSER = _try_load_py_parser()
        _PY_PARSER_LOADED = True
    return _PY_PARSER


class PythonParser(BaseParser):
    """
    Handles .py files.

    Import forms:
      import company.shared.auth
      from company.shared.auth import token
      from ...shared import types        (relative — resolved against file_path)
    """

    def extract_imports(self, file_path: Path, content: str) -> list[str]:
        parser = _get_parser()
        if parser is not None:
            return self._extract_tree_sitter(content, parser, file_path)
        return self._extract_regex(content, file_path)

    def _extract_tree_sitter(
        self, content: str, parser: Any, file_path: Path
    ) -> list[str]:
        results: list[str] = []
        try:
            tree = parser.parse(content.encode("utf-8", errors="replace"))

            def walk(node: Any) -> None:
                if node.type == "import_statement":
                    # import a.b.c
                    for child in node.children:
                        if child.type in ("dotted_name", "aliased_import"):
                            text = child.text.decode("utf-8", errors="replace")
                            results.append(text.split(" as ")[0].strip())
                elif node.type == "import_from_statement":
                    # from X import Y — grab the module part
                    module_node = node.child_by_field_name("module_name")
                    if module_node:
                        module = module_node.text.decode("utf-8", errors="replace")
                        # Count leading dots for relative imports
                        dots = sum(1 for c in node.children if c.type == ".")
                        if dots:
                            results.append("." * dots + module)
                        else:
                            results.append(module)
                for child in node.children:
                    walk(child)

            walk(tree.root_node)
        except Exception:
            return self._extract_regex(content, file_path)
        return results

    def _extract_regex(self, content: str, file_path: Path) -> list[str]:
        results: list[str] = []
        for m in _IMPORT_RE.finditer(content):
            if m.group(1):
                results.append(m.group(1))
            elif m.group(2):
                for part in m.group(2).split(","):
                    results.append(part.strip())
        for m in _FROM_RELATIVE_RE.finditer(content):
            dots = m.group(1)
            rest = m.group(2)
            results.append(dots + rest)
        return results

    def resolve_import_to_domain(
        self,
        import_str: str,
        file_path: Path,
        repo_root: Path,
    ) -> str | None:
        s = import_str.strip()

        # Relative import: starts with one or more dots
        if s.startswith("."):
            dots = len(s) - len(s.lstrip("."))
            rest = s.lstrip(".").replace(".", "/")
            base = file_path.parent
            for _ in range(dots - 1):
                base = base.parent
            candidate = (base / rest).resolve() if rest else base.resolve()
            if _is_under(candidate, repo_root):
                return path_to_domain(candidate, repo_root)
            return None

        # Absolute dotted module path: try to map to a repo directory
        # e.g. "company.shared.auth" or "shared.auth"
        # Strategy: try longest-to-shortest prefix matches against repo dirs
        parts = s.split(".")
        # Skip obvious stdlib / external packages (single segment, no slash)
        if len(parts) == 1:
            return None

        # Try subsets of parts as directory paths
        for i in range(len(parts), 0, -1):
            candidate_path = "/".join(parts[:i])
            candidate = (repo_root / candidate_path).resolve()
            if _is_under(candidate, repo_root) and candidate.exists():
                return path_to_domain(candidate, repo_root)

        # Heuristic: drop the first segment (company prefix) and retry
        if len(parts) > 2:
            for i in range(len(parts) - 1, 0, -1):
                candidate_path = "/".join(parts[1 : 1 + i])
                candidate = (repo_root / candidate_path).resolve()
                if _is_under(candidate, repo_root) and candidate.exists():
                    return path_to_domain(candidate, repo_root)

        return None


def _is_under(path: Path, repo_root: Path) -> bool:
    try:
        path.relative_to(repo_root.resolve())
        return True
    except ValueError:
        return False
