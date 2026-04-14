"""Go import parser — extracts intra-repo imports from .go files."""

from __future__ import annotations

import re
from pathlib import Path
from typing import Any

from monograph.parsers.base import BaseParser, path_to_domain

# Match: import "module/path" and import alias "module/path"
# Also handles import blocks: import ( "a" "b" )
_SINGLE_IMPORT_RE = re.compile(r"""import\s+(?:\w+\s+)?["'`]([^"'`]+)["'`]""")
_BLOCK_IMPORT_RE = re.compile(
    r"""import\s*\(([^)]+)\)""", re.DOTALL
)
_BLOCK_LINE_RE = re.compile(r"""(?:\w+\s+)?["'`]([^"'`]+)["'`]""")


def _try_load_go_parser() -> Any | None:
    try:
        import tree_sitter_go as tsgo
        from tree_sitter import Language, Parser

        go_language = Language(tsgo.language_go())
        return Parser(go_language)
    except Exception:
        return None


_GO_PARSER: Any | None = None
_GO_PARSER_LOADED = False


def _get_parser() -> Any | None:
    global _GO_PARSER, _GO_PARSER_LOADED
    if not _GO_PARSER_LOADED:
        _GO_PARSER = _try_load_go_parser()
        _GO_PARSER_LOADED = True
    return _GO_PARSER


def _read_module_path(repo_root: Path) -> str | None:
    """Read the module path from go.mod in repo_root."""
    go_mod = repo_root / "go.mod"
    if not go_mod.exists():
        return None
    try:
        for line in go_mod.read_text(encoding="utf-8").splitlines():
            if line.startswith("module "):
                return line[len("module "):].strip()
    except OSError:
        pass
    return None


class GoParser(BaseParser):
    """
    Handles .go files.

    Import forms:
      import "github.com/company/enterprise-api/shared/auth"
      import auth "github.com/company/enterprise-api/shared/auth"
      import (
          "github.com/company/enterprise-api/shared/auth"
          _ "github.com/company/enterprise-api/shared/types"
      )
    """

    def __init__(self) -> None:
        self._module_cache: dict[Path, str | None] = {}

    def _module_path(self, repo_root: Path) -> str | None:
        if repo_root not in self._module_cache:
            self._module_cache[repo_root] = _read_module_path(repo_root)
        return self._module_cache[repo_root]

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
                if node.type == "import_spec":
                    path_node = node.child_by_field_name("path")
                    if path_node:
                        raw = path_node.text.decode("utf-8", errors="replace").strip('"\'`')
                        results.append(raw)
                for child in node.children:
                    walk(child)

            walk(tree.root_node)
        except Exception:
            return self._extract_regex(content)
        return results

    def _extract_regex(self, content: str) -> list[str]:
        results: list[str] = []
        for m in _SINGLE_IMPORT_RE.finditer(content):
            results.append(m.group(1))
        for block_m in _BLOCK_IMPORT_RE.finditer(content):
            for line_m in _BLOCK_LINE_RE.finditer(block_m.group(1)):
                results.append(line_m.group(1))
        return results

    def resolve_import_to_domain(
        self,
        import_str: str,
        file_path: Path,
        repo_root: Path,
    ) -> str | None:
        module_path = self._module_path(repo_root)
        if not module_path:
            return None

        # Only process imports that start with our module path
        if not import_str.startswith(module_path):
            return None

        # Strip the module prefix to get a repo-relative path
        rest = import_str[len(module_path):].lstrip("/")
        if not rest:
            return None

        candidate = (repo_root / rest).resolve()
        if not _is_under(candidate, repo_root):
            return None
        return path_to_domain(candidate, repo_root)


def _is_under(path: Path, repo_root: Path) -> bool:
    try:
        path.relative_to(repo_root.resolve())
        return True
    except ValueError:
        return False
