"""Java import parser — extracts intra-repo imports from .java files."""

from __future__ import annotations

import re
from pathlib import Path
from typing import Any

from monograph.parsers.base import BaseParser, path_to_domain

# import com.company.shared.auth.TokenValidator;
# import static com.company.shared.auth.TokenValidator.validate;
_IMPORT_RE = re.compile(
    r"""^import\s+(?:static\s+)?([\w.]+)\s*;""", re.MULTILINE
)


def _try_load_java_parser() -> Any | None:
    try:
        import tree_sitter_java as tsjava
        from tree_sitter import Language, Parser

        java_language = Language(tsjava.language_java())
        return Parser(java_language)
    except Exception:
        return None


_JAVA_PARSER: Any | None = None
_JAVA_PARSER_LOADED = False


def _get_parser() -> Any | None:
    global _JAVA_PARSER, _JAVA_PARSER_LOADED
    if not _JAVA_PARSER_LOADED:
        _JAVA_PARSER = _try_load_java_parser()
        _JAVA_PARSER_LOADED = True
    return _JAVA_PARSER


def _read_group_id(repo_root: Path) -> str | None:
    """Try to read groupId from pom.xml or build.gradle to identify intra-repo imports."""
    pom = repo_root / "pom.xml"
    if pom.exists():
        try:
            text = pom.read_text(encoding="utf-8")
            m = re.search(r"<groupId>([\w.]+)</groupId>", text)
            if m:
                return m.group(1)
        except OSError:
            pass
    gradle = repo_root / "build.gradle"
    if gradle.exists():
        try:
            text = gradle.read_text(encoding="utf-8")
            m = re.search(r"""group\s*=\s*['"]([^'"]+)['"]""", text)
            if m:
                return m.group(1)
        except OSError:
            pass
    return None


class JavaParser(BaseParser):
    """
    Handles .java files.

    Import forms:
      import com.company.shared.auth.TokenValidator;
      import static com.company.shared.auth.TokenValidator.validate;
    """

    def __init__(self) -> None:
        self._group_cache: dict[Path, str | None] = {}

    def _group_id(self, repo_root: Path) -> str | None:
        if repo_root not in self._group_cache:
            self._group_cache[repo_root] = _read_group_id(repo_root)
        return self._group_cache[repo_root]

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
                if node.type == "import_declaration":
                    # Child: scoped_identifier or identifier
                    for child in node.children:
                        if child.type in ("scoped_identifier", "identifier"):
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
        for m in _IMPORT_RE.finditer(content):
            results.append(m.group(1))
        return results

    def resolve_import_to_domain(
        self,
        import_str: str,
        file_path: Path,
        repo_root: Path,
    ) -> str | None:
        group_id = self._group_id(repo_root)
        s = import_str.strip()

        prefix = group_id + "." if group_id else None

        if prefix and s.startswith(prefix):
            rest = s[len(prefix):]
        else:
            rest = s

        # Convert dotted package to path segments and try longest matching prefix.
        # e.g. "shared.auth.TokenValidator" → try shared/auth/TokenValidator, shared/auth, shared
        path_parts = rest.split(".")
        for n in range(min(4, len(path_parts)), 0, -1):
            candidate = (repo_root / "/".join(path_parts[:n])).resolve()
            if _is_under(candidate, repo_root) and candidate.exists():
                return path_to_domain(candidate, repo_root)

        return None


def _is_under(path: Path, repo_root: Path) -> bool:
    try:
        path.relative_to(repo_root.resolve())
        return True
    except ValueError:
        return False
