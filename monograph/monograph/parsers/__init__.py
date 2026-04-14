"""Language-specific import parsers."""

from monograph.parsers.base import BaseParser
from monograph.parsers.typescript import TypeScriptParser
from monograph.parsers.python import PythonParser
from monograph.parsers.golang import GoParser
from monograph.parsers.java import JavaParser
from monograph.parsers.csharp import CSharpParser

__all__ = [
    "BaseParser",
    "TypeScriptParser",
    "PythonParser",
    "GoParser",
    "JavaParser",
    "CSharpParser",
]


def parser_for_file(file_path: str) -> "BaseParser | None":
    """Return the right parser for a file extension, or None for unknown types."""
    from pathlib import Path

    ext = Path(file_path).suffix.lower()
    if ext in {".ts", ".tsx", ".js", ".jsx", ".mjs", ".cjs"}:
        return TypeScriptParser()
    if ext == ".py":
        return PythonParser()
    if ext == ".go":
        return GoParser()
    if ext == ".java":
        return JavaParser()
    if ext == ".cs":
        return CSharpParser()
    return None
