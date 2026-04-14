# MonoLens — VS Code Extension Context

## What this component is
The editor experience. Makes VS Code aware of the sparse monorepo so developers only see relevant code. Ghost folders in the sidebar, one-click hydration, inline impact warnings, and a status bar showing the active workspace. This is what makes the developer feel like they're working in a normal small repo, not a 14 GB monolith.

**This is the component that creates the "wow" moment without needing Xcode.**

## Language and tooling
- **TypeScript 5.x**, strict mode
- VS Code Extension API: `@types/vscode ^1.85.0`
- Build: `esbuild` (fast, single-file bundle)
- Package: `vsce` for .vsix packaging
- Lint: `eslint` with `@typescript-eslint`
- Test: `@vscode/test-electron`
- Extension ID: `sam-framework.monolens`
- Publisher: `sam-framework`

## Directory structure to build
```
monolens/
├── CLAUDE.md                  ← this file
├── package.json
├── tsconfig.json
├── .vscodeignore
├── esbuild.js
├── src/
│   ├── extension.ts           ← activation, command registration, disposables
│   ├── ghostTreeProvider.ts   ← custom TreeDataProvider for sidebar
│   ├── impactGutter.ts        ← CodeLensProvider for inline impact annotations
│   ├── statusBar.ts           ← status bar: "[SAM] sales-api · 4 domains · 340 MB"
│   ├── hydrationPanel.ts      ← webview panel: dep preview before hydrating
│   ├── samClient.ts           ← subprocess wrapper: runs sam CLI commands
│   ├── graphClient.ts         ← reads .sam/graph.json directly (no HTTP)
│   ├── profileWatcher.ts      ← fs.watch on .sam/profiles.yaml + workspace.yaml
│   └── types.ts               ← shared TypeScript interfaces
├── media/
│   ├── icon-ghost.svg         ← gray folder icon for unhydrated
│   ├── icon-loaded.svg        ← normal folder icon
│   ├── icon-loading.svg       ← spinner for in-progress
│   └── icon-shared.svg        ← blue-tinted icon for shared deps
└── test/
    └── suite/
        ├── extension.test.ts
        └── ghostTree.test.ts
```

## VS Code API surface — what to use

### 1. GhostTreeProvider (most important)
```typescript
// Implements vscode.TreeDataProvider<SamDomainItem>
// Shows all domains from .sam/graph.json
// Ghost state: grayed out, no expand arrow, click-to-hydrate
// Loaded state: normal, expandable, shows files inside

class SamDomainItem extends vscode.TreeItem {
  constructor(
    public readonly domain: string,
    public readonly state: 'ghost' | 'loading' | 'loaded' | 'shared'
  ) {
    super(domain, state === 'ghost'
      ? vscode.TreeItemCollapsibleState.None   // can't expand ghost
      : vscode.TreeItemCollapsibleState.Collapsed
    );

    // Visual treatment
    this.iconPath = getIconForState(state);
    this.contextValue = state;  // controls right-click menu
    this.tooltip = getTooltipForState(domain, state);

    // Click-to-hydrate on ghost items
    if (state === 'ghost') {
      this.command = {
        command: 'monolens.hydrateOnClick',
        title: 'Hydrate domain',
        arguments: [domain]
      };
    }
  }
}
```

### 2. FileDecorationProvider
```typescript
// Applies visual decoration to workspace folders in native explorer
// (separate from the custom sidebar tree)

class SamFileDecorator implements vscode.FileDecorationProvider {
  provideFileDecoration(uri: vscode.Uri): vscode.FileDecoration | undefined {
    const domain = uriToDomain(uri);
    if (!domain) return undefined;

    const state = workspaceState.getDomainState(domain);

    if (state === 'ghost') {
      return {
        badge: '○',
        color: new vscode.ThemeColor('disabledForeground'),
        tooltip: 'Not hydrated — click to fetch with SAM'
      };
    }
    if (state === 'shared') {
      return {
        badge: 'S',
        color: new vscode.ThemeColor('textLink.foreground'),
        tooltip: 'Shared dependency — auto-included'
      };
    }
    if (state === 'loaded') {
      return { badge: '●' };
    }
  }
}
```

### 3. CodeLensProvider (impact gutter)
```typescript
// Shows above exported functions in shared/ directories:
// ⚠ 9 domains depend on this · apis/payments (CRITICAL) · +8 more

class ImpactCodeLensProvider implements vscode.CodeLensProvider {
  provideCodeLenses(document: vscode.TextDocument): vscode.CodeLens[] {
    // Only activate in shared/ domains
    if (!isSharedDomain(document.uri)) return [];

    const symbols = getExportedSymbols(document);  // parse exports
    const graph = graphClient.getGraph();

    return symbols.map(symbol => {
      const dependents = graph.getDependents(document.uri.fsPath);
      if (dependents.length === 0) return null;

      const criticalCount = dependents.filter(d => d.risk === 'critical').length;
      const label = `⚠ ${dependents.length} domains depend on this` +
        (criticalCount > 0 ? ` · ${criticalCount} critical` : '');

      return new vscode.CodeLens(symbol.range, {
        title: label,
        command: 'monolens.showImpactPanel',
        arguments: [symbol.name, dependents]
      });
    }).filter(Boolean);
  }
}
```

### 4. Status bar
```typescript
// Bottom status bar: "[SAM] sales-api · 4 domains · 340 MB  [Hydrate more ▾]"
// Click → command palette with SAM commands
// Color: normal when daemon running, amber when daemon unreachable

const statusBarItem = vscode.window.createStatusBarItem(
  vscode.StatusBarAlignment.Left, 100
);
statusBarItem.command = 'monolens.showQuickPick';
```

## samClient.ts — subprocess wrapper
```typescript
// All sam CLI calls go through this module
// Never call MonoGraph HTTP directly from the extension — always via sam CLI

class SamClient {
  private async exec(args: string[]): Promise<{stdout: string, stderr: string}> {
    // Use child_process.execFile with repo root as cwd
    // Timeout: 30 seconds for fetch, 5 seconds for query commands
    // On error: return gracefully, never throw to VS Code
  }

  async getStatus(): Promise<WorkspaceStatus> {
    // Read .sam/workspace.yaml directly (no subprocess needed)
  }

  async hydrate(domain: string, withDeps: boolean): Promise<HydrateResult> {
    return this.exec(['fetch', domain, withDeps ? '--with-deps' : '']);
  }

  async plan(domain: string): Promise<PlanResult> {
    return this.exec(['plan', domain]);
  }

  async impact(): Promise<ImpactResult> {
    return this.exec(['impact', '--format', 'json']);
  }

  async useProfile(profile: string): Promise<void> {
    return this.exec(['use', '--profile', profile, '--no-ai']);
  }
}
```

## graphClient.ts — reads graph.json directly
```typescript
// Extension reads graph.json directly for performance
// No HTTP calls to MonoGraph — that's sam CLI's job

class GraphClient {
  private graph: Graph | null = null;
  private graphPath: string;

  constructor(repoRoot: string) {
    this.graphPath = path.join(repoRoot, '.sam', 'graph.json');
    this.watchForChanges();
  }

  private watchForChanges() {
    // fs.watch on graph.json — reload and fire onDidChange when updated
    vscode.workspace.createFileSystemWatcher(this.graphPath)
      .onDidChange(() => this.reload());
  }

  getDependencies(domain: string): Edge[] { ... }
  getDependents(domain: string): Edge[] { ... }  // reverse graph traversal
  getAllDomains(): string[] { ... }
}
```

## Commands to register (package.json contributes.commands)
```json
[
  { "command": "monolens.hydrateOnClick",  "title": "SAM: Hydrate domain" },
  { "command": "monolens.hydrateWithDeps", "title": "SAM: Hydrate with dependencies" },
  { "command": "monolens.showPlan",        "title": "SAM: Show fetch plan" },
  { "command": "monolens.useProfile",      "title": "SAM: Switch workspace profile" },
  { "command": "monolens.showImpact",      "title": "SAM: Show change impact" },
  { "command": "monolens.showImpactPanel", "title": "SAM: Open impact detail" },
  { "command": "monolens.refreshTree",     "title": "SAM: Refresh domain tree" },
  { "command": "monolens.showQuickPick",   "title": "SAM: SAM commands..." }
]
```

## Hydration UX flow — get this exactly right
```
Developer clicks ghost folder in SAM sidebar
  → MonoLens calls: sam plan <domain> (get dep list first)
  → Opens webview panel showing:
      "Hydrating apis/catalog will fetch:"
      ● apis/catalog    18 MB   (your selection)
      ● shared/auth      3 MB   (already have)
      ● shared/types     1 MB   (already have)
      ○ apis/search      22 MB  (AI-inferred co-change, score 0.71)
      ─────────────────────────
      Total new: 18 MB (apis/search optional)
      [Hydrate (18 MB)]   [Hydrate with deps (40 MB)]   [Cancel]

Developer clicks "Hydrate (18 MB)"
  → folder item switches to state: 'loading' (spinner icon)
  → MonoLens runs: sam fetch apis/catalog (subprocess)
  → On complete: state → 'loaded', tree refreshes
  → Status bar updates with new domain count
```

## package.json key sections
```json
{
  "name": "monolens",
  "displayName": "MonoLens — SAM Framework",
  "version": "0.1.0",
  "engines": { "vscode": "^1.85.0" },
  "activationEvents": ["workspaceContains:.sam/profiles.yaml"],
  "main": "./dist/extension.js",
  "contributes": {
    "viewsContainers": {
      "activitybar": [{
        "id": "sam-explorer",
        "title": "SAM Domains",
        "icon": "media/icon-ghost.svg"
      }]
    },
    "views": {
      "sam-explorer": [{
        "id": "samDomainTree",
        "name": "Domains"
      }]
    }
  }
}
```

## Build commands
```bash
npm install
npm run compile          # esbuild one-shot
npm run watch            # esbuild watch mode
npm run package          # vsce package → .vsix
code --install-extension monolens-0.1.0.vsix

# Run tests (opens VS Code test runner)
npm test
```

## Build order for this component
1. `types.ts` — interfaces (Graph, Edge, DomainState, WorkspaceStatus etc.)
2. `graphClient.ts` — read graph.json, in-memory graph queries
3. `profileWatcher.ts` — watch .sam/ files for changes
4. `samClient.ts` — subprocess wrapper
5. `ghostTreeProvider.ts` — the main sidebar tree (most important UI)
6. `statusBar.ts` — status bar item
7. `extension.ts` — wire everything together, register commands
8. `impactGutter.ts` — code lens for shared/ files
9. `hydrationPanel.ts` — webview dep preview panel
