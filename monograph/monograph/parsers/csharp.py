"""C# import parser — extracts intra-repo using/namespace references from .cs files."""

from __future__ import annotations

import re
from pathlib import Path
from typing import Any

from monograph.parsers.base import BaseParser, path_to_domain

# using Company.Shared.Auth;
# using static Company.Shared.Auth.TokenValidator;
# using AuthAlias = Company.Shared.Auth;
_USING_RE = re.compile(
    r"""^using\s+(?:static\s+)?(?:\w+\s*=\s*)?([\w.]+)\s*;""", re.MULTILINE
)


def _try_load_cs_parser() -> Any | None:
    try:
        import tree_sitter_c_sharp as tscs
        from tree_sitter import Language, Parser

        cs_language = Language(tscs.language_c_sharp())
        return Parser(cs_language)
    except Exception:
        return None


_CS_PARSER: Any | None = None
_CS_PARSER_LOADED = False


def _get_parser() -> Any | None:
    global _CS_PARSER, _CS_PARSER_LOADED
    if not _CS_PARSER_LOADED:
        _CS_PARSER = _try_load_cs_parser()
        _CS_PARSER_LOADED = True
    return _CS_PARSER


def _read_root_namespace(repo_root: Path) -> str | None:
    """Try to determine the root namespace from a .csproj file."""
    for csproj in repo_root.rglob("*.csproj"):
        try:
            text = csproj.read_text(encoding="utf-8")
            m = re.search(r"<RootNamespace>([\w.]+)</RootNamespace>", text)
            if m:
                return m.group(1)
            m = re.search(r"<AssemblyName>([\w.]+)</AssemblyName>", text)
            if m:
                return m.group(1)
        except OSError:
            continue
    return None


class CSharpParser(BaseParser):
    """
    Handles .cs files.

    Import forms:
      using Company.Shared.Auth;
      using static Company.Shared.Auth.TokenValidator;
      using AuthAlias = Company.Shared.Auth;
    """

    def __init__(self) -> None:
        self._ns_cache: dict[Path, str | None] = {}

    def _root_namespace(self, repo_root: Path) -> str | None:
        if repo_root not in self._ns_cache:
            self._ns_cache[repo_root] = _read_root_namespace(repo_root)
        return self._ns_cache[repo_root]

    def extract_imports(self, file_path: Path, content: str) -> list[str]:
        parser = _get_parser()
        if parser is not None:
            return self._extract_tree_sitter(content, parser)
        return self._extract_regex(content)

    def _extract_tree_sitter(self, content: str, parser: Any) -> list[str]:
        results: list[str] = []
        try:
            tree = parser.parse(content.encode("utf-8", errors="replace"))

            def walk(node: Any) -> None:
                if node.type == "using_directive":
                    for child in node.children:
                        if child.type in ("qualified_name", "identifier"):
                            raw = child.text.decode("utf-8", errors="replace").strip()
                            if raw:
                                results.append(raw)
                for child in node.children:
                    walk(child)

            walk(tree.root_node)
        except Exception:
            return self._extract_regex(content)
        return results

    def _extract_regex(self, content: str) -> list[str]:
        results: list[str] = []
        for m in _USING_RE.finditer(content):
            results.append(m.group(1))
        return results

    def resolve_import_to_domain(
        self,
        import_str: str,
        file_path: Path,
        repo_root: Path,
    ) -> str | None:
        ns = self._root_namespace(repo_root)
        s = import_str.strip()

        # Strip root namespace prefix if known
        if ns and s.startswith(ns + "."):
            rest = s[len(ns) + 1:]
        else:
            rest = s

        # Convert dotted namespace to directory path segments
        parts = rest.split(".")
        if len(parts) < 1:
            return None

        # Try matching against actual directories, longest prefix first
        for n in range(min(4, len(parts)), 0, -1):
            candidate = (repo_root / "/".join(parts[:n])).resolve()
            if _is_under(candidate, repo_root) and candidate.exists():
                return path_to_domain(candidate, repo_root)

        return None


def _is_under(path: Path, repo_root: Path) -> bool:
    try:
        path.relative_to(repo_root.resolve())
        return True
    except ValueError:
        return False
