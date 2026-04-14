"""TypeScript / JavaScript import parser using tree-sitter."""

from __future__ import annotations

import json
import re
from pathlib import Path
from typing import Any

from monograph.parsers.base import BaseParser, path_to_domain

# Regex fallback for when tree-sitter language bindings are unavailable.
# Handles: import ... from '...', require('...')
_IMPORT_RE = re.compile(
    r"""(?:import\s+(?:[^'"]*\s+from\s+)?['"]([^'"]+)['"]"""
    r"""|require\s*\(\s*['"]([^'"]+)['"]\s*\))""",
    re.MULTILINE,
)


def _try_load_ts_parser() -> Any | None:
    """Attempt to load the tree-sitter TypeScript grammar. Returns parser or None."""
    try:
        import tree_sitter_typescript as tsts
        from tree_sitter import Language, Parser

        ts_language = Language(tsts.language_typescript())
        parser = Parser(ts_language)
        return parser
    except Exception:
        pass

    try:
        import tree_sitter_javascript as tsjs
        from tree_sitter import Language, Parser

        js_language = Language(tsjs.language_javascript())
        parser = Parser(js_language)
        return parser
    except Exception:
        return None


_TS_PARSER: Any | None = None
_TS_PARSER_LOADED = False


def _get_parser() -> Any | None:
    global _TS_PARSER, _TS_PARSER_LOADED
    if not _TS_PARSER_LOADED:
        _TS_PARSER = _try_load_ts_parser()
        _TS_PARSER_LOADED = True
    return _TS_PARSER


# ── Alias resolution ──────────────────────────────────────────────────────────


def _load_tsconfig_paths(repo_root: Path) -> dict[str, str]:
    """
    Read tsconfig.json (and tsconfig.base.json) from repo_root to extract
    compilerOptions.paths aliases.  Returns {alias_prefix: resolved_dir}.
    """
    aliases: dict[str, str] = {}
    for fname in ("tsconfig.json", "tsconfig.base.json"):
        cfg = repo_root / fname
        if not cfg.exists():
            continue
        try:
            data = json.loads(cfg.read_text(encoding="utf-8"))
        except (json.JSONDecodeError, OSError):
            continue
        paths = (
            data.get("compilerOptions", {}).get("paths", {})
            or data.get("compilerOptions", {}).get("paths", {})
        )
        base_url = data.get("compilerOptions", {}).get("baseUrl", ".")
        base = (repo_root / base_url).resolve()
        for alias, targets in paths.items():
            if isinstance(targets, list) and targets:
                target = str(targets[0]).rstrip("/*").rstrip("/")
                alias_key = alias.rstrip("/*")
                try:
                    resolved = str((base / target).resolve().relative_to(repo_root.resolve()))
                    aliases[alias_key] = resolved
                except ValueError:
                    pass
    return aliases


# ── Parser class ──────────────────────────────────────────────────────────────


class TypeScriptParser(BaseParser):
    """
    Handles .ts, .tsx, .js, .jsx files.

    Import forms recognised:
      import { x } from '../../../shared/auth'
      import x from '@company/shared-auth'
      import type { X } from '~/shared/types'
      const x = require('../../shared/types')
      export { x } from '../utils'
      dynamic import() expressions
    """

    def __init__(self) -> None:
        self._aliases: dict[str, str] | None = None
        self._aliases_repo: Path | None = None

    def _get_aliases(self, repo_root: Path) -> dict[str, str]:
        if self._aliases_repo != repo_root:
            self._aliases = _load_tsconfig_paths(repo_root)
            self._aliases_repo = repo_root
        return self._aliases or {}

    # ── extract_imports ───────────────────────────────────────────────────────

    def extract_imports(self, file_path: Path, content: str) -> list[str]:
        parser = _get_parser()
        if parser is not None:
            return self._extract_tree_sitter(content, parser)
        return self._extract_regex(content)

    def _extract_tree_sitter(self, content: str, parser: Any) -> list[str]:
        """Use tree-sitter AST to find import/require sources."""
        results: list[str] = []
        try:
            tree = parser.parse(content.encode("utf-8", errors="replace"))
            root = tree.root_node

            def walk(node: Any) -> None:
                # import_statement: import ... from "source"
                if node.type in (
                    "import_statement",
                    "export_statement",
                    "import_declaration",
                ):
                    for child in node.children:
                        if child.type in ("string", "string_literal"):
                            src = _unquote(child.text.decode("utf-8", errors="replace"))
                            if src:
                                results.append(src)
                # call_expression: require("source") or import("source")
                elif node.type == "call_expression":
                    fn = node.child_by_field_name("function")
                    args = node.child_by_field_name("arguments")
                    if fn and fn.text in (b"require", b"import"):
                        if args:
                            for child in args.children:
                                if child.type in ("string", "string_literal"):
                                    src = _unquote(
                                        child.text.decode("utf-8", errors="replace")
                                    )
                                    if src:
                                        results.append(src)
                for child in node.children:
                    walk(child)

            walk(root)
        except Exception:
            # Fall back to regex on any tree-sitter error
            return self._extract_regex(content)
        return results

    def _extract_regex(self, content: str) -> list[str]:
        results: list[str] = []
        for m in _IMPORT_RE.finditer(content):
            src = m.group(1) or m.group(2)
            if src:
                results.append(src)
        return results

    # ── resolve_import_to_domain ──────────────────────────────────────────────

    def resolve_import_to_domain(
        self,
        import_str: str,
        file_path: Path,
        repo_root: Path,
    ) -> str | None:
        aliases = self._get_aliases(repo_root)
        resolved = self._resolve_path(import_str, file_path, repo_root, aliases)
        if resolved is None:
            return None
        return path_to_domain(resolved, repo_root)

    def _resolve_path(
        self,
        import_str: str,
        file_path: Path,
        repo_root: Path,
        aliases: dict[str, str],
    ) -> Path | None:
        s = import_str.strip()

        # 1. Relative import: starts with ./ or ../
        if s.startswith("./") or s.startswith("../"):
            base = file_path.parent
            resolved = (base / s).resolve()
            return resolved if _is_under(resolved, repo_root) else None

        # 2. Tilde alias (~/ or ~) — common in Vue/Nuxt projects
        if s.startswith("~/") or s == "~":
            rest = s[2:] if s.startswith("~/") else ""
            resolved = (repo_root / rest).resolve()
            return resolved if _is_under(resolved, repo_root) else None

        # 3. tsconfig paths aliases
        for alias, target in aliases.items():
            if s == alias or s.startswith(alias + "/"):
                rest = s[len(alias):].lstrip("/")
                resolved = (repo_root / target / rest).resolve()
                return resolved if _is_under(resolved, repo_root) else None

        # 4. Bare path that looks like it's inside the repo (no scope/@, no dots)
        #    e.g. "shared/auth" — used in monorepos with root-relative imports
        if "/" in s and not s.startswith("@") and not s.startswith("."):
            candidate = (repo_root / s).resolve()
            if _is_under(candidate, repo_root):
                return candidate

        # 5. Scoped package @company/... — try to map to monorepo domain
        #    e.g. @company/shared-auth → shared/auth (heuristic: dash → slash in 2nd segment)
        if s.startswith("@"):
            parts = s.lstrip("@").split("/", 2)
            if len(parts) >= 2:
                # Try @scope/domain-sub → domain/sub
                domain_guess = parts[1].replace("-", "/", 1)
                candidate = (repo_root / domain_guess).resolve()
                if _is_under(candidate, repo_root) and candidate.exists():
                    return candidate

        # Everything else is an external package
        return None


# ── Utilities ─────────────────────────────────────────────────────────────────


def _unquote(s: str) -> str:
    s = s.strip()
    if (s.startswith('"') and s.endswith('"')) or (
        s.startswith("'") and s.endswith("'")
    ):
        return s[1:-1]
    if s.startswith("`") and s.endswith("`"):
        return s[1:-1]
    return s


def _is_under(path: Path, repo_root: Path) -> bool:
    try:
        path.relative_to(repo_root.resolve())
        return True
    except ValueError:
        return False
