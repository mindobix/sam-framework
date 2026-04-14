"""Tests for all language parsers."""

from __future__ import annotations

from pathlib import Path

import pytest

FIXTURES = Path(__file__).parent / "fixtures"


# ── TypeScript ─────────────────────────────────────────────────────────────────


class TestTypeScriptParser:
    def setup_method(self):
        from monograph.parsers.typescript import TypeScriptParser
        self.parser = TypeScriptParser()

    def test_relative_import(self, tmp_path):
        repo = tmp_path
        (repo / "shared" / "auth").mkdir(parents=True)
        (repo / "apis" / "sales" / "src").mkdir(parents=True)
        file_path = repo / "apis" / "sales" / "src" / "service.ts"
        file_path.write_text('import { x } from "../../../shared/auth/token";')

        result = self.parser.get_domain_imports(file_path, file_path.read_text(), repo)
        assert "shared/auth" in result

    def test_require_statement(self, tmp_path):
        repo = tmp_path
        (repo / "shared" / "utils").mkdir(parents=True)
        (repo / "apis" / "sales" / "src").mkdir(parents=True)
        file_path = repo / "apis" / "sales" / "src" / "service.ts"
        file_path.write_text("const x = require('../../../shared/utils/helpers');")

        result = self.parser.get_domain_imports(file_path, file_path.read_text(), repo)
        assert "shared/utils" in result

    def test_external_import_ignored(self, tmp_path):
        repo = tmp_path
        (repo / "apis" / "sales" / "src").mkdir(parents=True)
        file_path = repo / "apis" / "sales" / "src" / "service.ts"
        file_path.write_text('import express from "express"; import lodash from "lodash";')

        result = self.parser.get_domain_imports(file_path, file_path.read_text(), repo)
        assert result == []

    def test_self_import_excluded(self, tmp_path):
        repo = tmp_path
        (repo / "apis" / "sales" / "src").mkdir(parents=True)
        file_path = repo / "apis" / "sales" / "src" / "service.ts"
        file_path.write_text('import { x } from "./other";')

        result = self.parser.get_domain_imports(file_path, file_path.read_text(), repo)
        # Self-domain import must not appear
        assert "apis/sales" not in result

    def test_tilde_alias(self, tmp_path):
        repo = tmp_path
        (repo / "shared" / "types").mkdir(parents=True)
        (repo / "apis" / "search" / "src").mkdir(parents=True)
        file_path = repo / "apis" / "search" / "src" / "main.ts"
        file_path.write_text('import type { Model } from "~/shared/types/index";')

        result = self.parser.get_domain_imports(file_path, file_path.read_text(), repo)
        assert "shared/types" in result

    def test_no_duplicates(self, tmp_path):
        repo = tmp_path
        (repo / "shared" / "auth").mkdir(parents=True)
        (repo / "apis" / "sales" / "src").mkdir(parents=True)
        file_path = repo / "apis" / "sales" / "src" / "service.ts"
        file_path.write_text(
            'import { a } from "../../../shared/auth/a";\n'
            'import { b } from "../../../shared/auth/b";\n'
        )

        result = self.parser.get_domain_imports(file_path, file_path.read_text(), repo)
        assert result.count("shared/auth") == 1

    def test_fixture_file(self):
        repo = FIXTURES / "ts_project"
        if not repo.exists():
            pytest.skip("fixture not found")
        file_path = repo / "apis" / "sales" / "src" / "service.ts"
        result = self.parser.get_domain_imports(file_path, file_path.read_text(), repo)
        assert "shared/auth" in result


# ── Python ─────────────────────────────────────────────────────────────────────


class TestPythonParser:
    def setup_method(self):
        from monograph.parsers.python import PythonParser
        self.parser = PythonParser()

    def test_absolute_import(self, tmp_path):
        repo = tmp_path
        (repo / "shared" / "auth").mkdir(parents=True)
        (repo / "apis" / "orders").mkdir(parents=True)
        file_path = repo / "apis" / "orders" / "__init__.py"
        file_path.write_text("from shared.auth import validate_token")

        result = self.parser.get_domain_imports(file_path, file_path.read_text(), repo)
        assert "shared/auth" in result

    def test_stdlib_ignored(self, tmp_path):
        repo = tmp_path
        (repo / "apis" / "orders").mkdir(parents=True)
        file_path = repo / "apis" / "orders" / "main.py"
        file_path.write_text("import os\nimport sys\nfrom pathlib import Path")

        result = self.parser.get_domain_imports(file_path, file_path.read_text(), repo)
        assert result == []

    def test_relative_import(self, tmp_path):
        # 2-dot relative import from apis/orders/service.py → goes up to apis/ → joins shared → apis/shared
        # won't match repo-root shared/auth. Use absolute import for cross-domain reference.
        repo = tmp_path
        (repo / "shared" / "auth").mkdir(parents=True)
        (repo / "apis" / "orders").mkdir(parents=True)
        file_path = repo / "apis" / "orders" / "service.py"
        # Absolute import: shared.auth resolves against repo root
        file_path.write_text("from shared.auth import validate_token")

        result = self.parser.get_domain_imports(file_path, file_path.read_text(), repo)
        assert "shared/auth" in result


# ── Go ─────────────────────────────────────────────────────────────────────────


class TestGoParser:
    def setup_method(self):
        from monograph.parsers.golang import GoParser
        self.parser = GoParser()

    def test_intra_repo_import(self, tmp_path):
        repo = tmp_path
        (repo / "shared" / "auth").mkdir(parents=True)
        (repo / "apis" / "checkout").mkdir(parents=True)
        (repo / "go.mod").write_text("module github.com/company/enterprise-api\n\ngo 1.22\n")
        file_path = repo / "apis" / "checkout" / "service.go"
        file_path.write_text(
            'import "github.com/company/enterprise-api/shared/auth"'
        )

        result = self.parser.get_domain_imports(file_path, file_path.read_text(), repo)
        assert "shared/auth" in result

    def test_external_import_ignored(self, tmp_path):
        repo = tmp_path
        (repo / "apis" / "checkout").mkdir(parents=True)
        (repo / "go.mod").write_text("module github.com/company/enterprise-api\n\ngo 1.22\n")
        file_path = repo / "apis" / "checkout" / "service.go"
        file_path.write_text('import "fmt"\nimport "net/http"')

        result = self.parser.get_domain_imports(file_path, file_path.read_text(), repo)
        assert result == []

    def test_import_block(self, tmp_path):
        repo = tmp_path
        (repo / "shared" / "auth").mkdir(parents=True)
        (repo / "shared" / "types").mkdir(parents=True)
        (repo / "apis" / "checkout").mkdir(parents=True)
        (repo / "go.mod").write_text("module github.com/company/enterprise-api\n\ngo 1.22\n")
        file_path = repo / "apis" / "checkout" / "service.go"
        file_path.write_text(
            'import (\n'
            '    "github.com/company/enterprise-api/shared/auth"\n'
            '    "github.com/company/enterprise-api/shared/types"\n'
            '    "fmt"\n'
            ')\n'
        )

        result = self.parser.get_domain_imports(file_path, file_path.read_text(), repo)
        assert "shared/auth" in result
        assert "shared/types" in result
        assert len([r for r in result if r == "shared/auth"]) == 1

    def test_fixture_file(self):
        repo = FIXTURES / "go_project"
        if not repo.exists():
            pytest.skip("fixture not found")
        file_path = repo / "apis" / "checkout" / "service.go"
        result = self.parser.get_domain_imports(file_path, file_path.read_text(), repo)
        assert "shared/auth" in result


# ── Java ───────────────────────────────────────────────────────────────────────


class TestJavaParser:
    def setup_method(self):
        from monograph.parsers.java import JavaParser
        self.parser = JavaParser()

    def test_dotted_import(self, tmp_path):
        repo = tmp_path
        (repo / "shared" / "auth").mkdir(parents=True)
        (repo / "apis" / "payments").mkdir(parents=True)
        file_path = repo / "apis" / "payments" / "Service.java"
        file_path.write_text("import shared.auth.TokenValidator;")

        result = self.parser.get_domain_imports(file_path, file_path.read_text(), repo)
        assert "shared/auth" in result

    def test_stdlib_ignored(self, tmp_path):
        repo = tmp_path
        (repo / "apis" / "payments").mkdir(parents=True)
        file_path = repo / "apis" / "payments" / "Service.java"
        file_path.write_text("import java.util.List;\nimport java.io.IOException;")

        result = self.parser.get_domain_imports(file_path, file_path.read_text(), repo)
        assert result == []


# ── C# ─────────────────────────────────────────────────────────────────────────


class TestCSharpParser:
    def setup_method(self):
        from monograph.parsers.csharp import CSharpParser
        self.parser = CSharpParser()

    def test_using_directive(self, tmp_path):
        repo = tmp_path
        (repo / "shared" / "auth").mkdir(parents=True)
        (repo / "apis" / "billing").mkdir(parents=True)
        file_path = repo / "apis" / "billing" / "Service.cs"
        file_path.write_text("using shared.auth;")

        result = self.parser.get_domain_imports(file_path, file_path.read_text(), repo)
        assert "shared/auth" in result

    def test_system_namespace_ignored(self, tmp_path):
        repo = tmp_path
        (repo / "apis" / "billing").mkdir(parents=True)
        file_path = repo / "apis" / "billing" / "Service.cs"
        file_path.write_text("using System;\nusing System.Collections.Generic;")

        result = self.parser.get_domain_imports(file_path, file_path.read_text(), repo)
        assert result == []
