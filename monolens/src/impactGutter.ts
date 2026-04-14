import * as vscode from 'vscode';
import * as path from 'path';
import { GraphClient } from './graphClient';
import { Dependency } from './types';

/**
 * ImpactCodeLensProvider annotates exported symbols in shared/ domains with
 * cross-domain dependency counts.
 *
 * Example annotation (appears above exported functions/classes/consts):
 *   ⚠ 9 domains depend on this · apis/payments (CRITICAL) · +7 more
 *
 * Only active in shared/ directories (configurable via monolens.showImpactGutter).
 */
export class ImpactCodeLensProvider
  implements vscode.CodeLensProvider, vscode.Disposable {

  private readonly _onDidChangeCodeLenses = new vscode.EventEmitter<void>();
  readonly onDidChangeCodeLenses = this._onDidChangeCodeLenses.event;

  private disposables: vscode.Disposable[] = [];

  constructor(
    private readonly graphClient: GraphClient,
    private readonly repoRoot: string
  ) {
    // Refresh lenses when graph changes.
    this.disposables.push(
      graphClient.onDidChange(() => this._onDidChangeCodeLenses.fire())
    );
  }

  // ─── CodeLensProvider ────────────────────────────────────────────────────────

  provideCodeLenses(document: vscode.TextDocument): vscode.CodeLens[] {
    if (!this.shouldAnnotate(document)) { return []; }

    const domain = this.fileToDomain(document.uri.fsPath);
    if (!domain) { return []; }

    const dependents = this.graphClient.getDependents(domain);
    if (dependents.length === 0) { return []; }

    const config = vscode.workspace.getConfiguration('monolens');
    if (!config.get<boolean>('showImpactGutter', true)) { return []; }

    // Find exported symbol lines.
    const exportLines = findExportLines(document);
    if (exportLines.length === 0) { return []; }

    const label = buildLabel(dependents);

    return exportLines.map(line => {
      const range = new vscode.Range(line, 0, line, 0);
      return new vscode.CodeLens(range, {
        title: label,
        command: 'monolens.showImpact',
        arguments: [domain, dependents],
        tooltip: `${dependents.length} domains import from ${domain}`,
      });
    });
  }

  // ─── Internal ────────────────────────────────────────────────────────────────

  /** Only annotate files inside shared/ directories. */
  private shouldAnnotate(document: vscode.TextDocument): boolean {
    const rel = path.relative(this.repoRoot, document.uri.fsPath);
    return rel.startsWith('shared/') || rel.startsWith('shared\\');
  }

  /** Map a file path to its domain (first two path components). */
  private fileToDomain(fsPath: string): string | undefined {
    const rel = path.relative(this.repoRoot, fsPath);
    const parts = rel.split(path.sep);
    if (parts.length < 2) { return undefined; }
    return parts.slice(0, 2).join('/');
  }

  dispose(): void {
    this.disposables.forEach(d => d.dispose());
    this._onDidChangeCodeLenses.dispose();
  }
}

// ─── Helpers ──────────────────────────────────────────────────────────────────

/**
 * Find line numbers of exported declarations.
 * Handles TypeScript/JavaScript `export function|class|const|type|interface`.
 */
function findExportLines(document: vscode.TextDocument): number[] {
  const lines: number[] = [];
  const exportPattern =
    /^\s*export\s+(default\s+)?(function|class|const|let|var|type|interface|enum|abstract)/;

  for (let i = 0; i < document.lineCount; i++) {
    if (exportPattern.test(document.lineAt(i).text)) {
      lines.push(i);
    }
  }
  return lines;
}

/**
 * Build a human-readable CodeLens title from the list of dependents.
 * e.g. "⚠ 9 domains depend on this · apis/payments · +8 more"
 */
function buildLabel(dependents: Dependency[]): string {
  const count = dependents.length;
  if (count === 0) { return ''; }

  // Sort by score descending so the most coupled domain appears first.
  const sorted = [...dependents].sort((a, b) => b.score - a.score);
  const first = sorted[0].domain;

  if (count === 1) {
    return `⚠ 1 domain depends on this · ${first}`;
  }

  return `⚠ ${count} domains depend on this · ${first} · +${count - 1} more`;
}
