import * as vscode from 'vscode';
import * as fs from 'fs';
import * as path from 'path';
import { Graph, GraphEdge, Dependency, EdgeType } from './types';

/**
 * GraphClient reads .sam/graph.json directly from disk (no HTTP) and answers
 * dependency queries.  It watches the file for changes and fires onDidChange
 * when the graph is updated by MonoGraph.
 */
export class GraphClient implements vscode.Disposable {
  private graph: Graph | null = null;
  private readonly graphPath: string;
  private readonly _onDidChange = new vscode.EventEmitter<void>();
  private watcher: vscode.FileSystemWatcher | undefined;

  readonly onDidChange = this._onDidChange.event;

  constructor(private readonly repoRoot: string) {
    this.graphPath = path.join(repoRoot, '.sam', 'graph.json');
    this.loadGraph();
    this.startWatcher();
  }

  // ─── Public API ─────────────────────────────────────────────────────────────

  /** All domain paths known to the graph. Returns [] if graph not loaded. */
  getAllDomains(): string[] {
    if (!this.graph) { return []; }
    return this.graph.domains ?? [];
  }

  /** Domains that the given domain imports (forward edges). */
  getDependencies(domain: string): Dependency[] {
    if (!this.graph) { return []; }
    return this.graph.edges
      .filter(e => e.from === domain)
      .map(edgeToDependency);
  }

  /** Domains that depend on the given domain (reverse edges). */
  getDependents(domain: string): Dependency[] {
    if (!this.graph) { return []; }
    return this.graph.edges
      .filter(e => e.to === domain)
      .map(e => ({ domain: e.from, type: e.type, score: e.score }));
  }

  /** Returns the raw graph, or null if not yet loaded. */
  getGraph(): Graph | null {
    return this.graph;
  }

  /** True if graph.json exists and was parsed successfully. */
  isLoaded(): boolean {
    return this.graph !== null;
  }

  // ─── Internal ────────────────────────────────────────────────────────────────

  private loadGraph(): void {
    try {
      const raw = fs.readFileSync(this.graphPath, 'utf8');
      this.graph = JSON.parse(raw) as Graph;
    } catch {
      // graph.json missing or malformed — not an error at extension startup.
      this.graph = null;
    }
  }

  private startWatcher(): void {
    // Use VS Code's watcher so it respects the workspace file system.
    const pattern = new vscode.RelativePattern(
      this.repoRoot,
      '.sam/graph.json'
    );
    this.watcher = vscode.workspace.createFileSystemWatcher(pattern);

    this.watcher.onDidChange(() => this.reload());
    this.watcher.onDidCreate(() => this.reload());
    this.watcher.onDidDelete(() => {
      this.graph = null;
      this._onDidChange.fire();
    });
  }

  private reload(): void {
    this.loadGraph();
    this._onDidChange.fire();
  }

  dispose(): void {
    this.watcher?.dispose();
    this._onDidChange.dispose();
  }
}

// ─── Helpers ──────────────────────────────────────────────────────────────────

function edgeToDependency(e: GraphEdge): Dependency {
  return { domain: e.to, type: e.type as EdgeType, score: e.weight ?? e.score ?? 0 };
}
