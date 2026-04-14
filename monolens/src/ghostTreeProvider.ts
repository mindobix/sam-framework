import * as vscode from 'vscode';
import * as path from 'path';
import { DomainState } from './types';
import { GraphClient } from './graphClient';
import { ProfileWatcher } from './profileWatcher';

// ─── Tree item ────────────────────────────────────────────────────────────────

export class SamDomainItem extends vscode.TreeItem {
  constructor(
    public readonly domain: string,
    public readonly state: DomainState,
    extensionUri: vscode.Uri
  ) {
    super(
      path.basename(domain),
      state === 'ghost'
        ? vscode.TreeItemCollapsibleState.None
        : vscode.TreeItemCollapsibleState.Collapsed
    );

    this.id = `sam-domain-${domain}`;
    this.description = state === 'ghost' ? domain : undefined;
    this.tooltip = tooltipForState(domain, state);
    this.contextValue = state;
    this.iconPath = iconForState(state, extensionUri);

    // Ghost items hydrate on single-click.
    if (state === 'ghost') {
      this.command = {
        command: 'monolens.hydrateOnClick',
        title: 'Hydrate domain',
        arguments: [domain],
      };
    }
  }
}

// ─── Provider ─────────────────────────────────────────────────────────────────

/**
 * GhostTreeProvider drives the "SAM Domains" sidebar view.
 *
 * It merges three sources to build the list:
 *   1. Domains from .sam/graph.json (all known domains)
 *   2. Hydrated domains from .sam/workspace.yaml (state = loaded/shared)
 *   3. Active profile's auto_include domains (state = shared)
 *
 * Domains not in graph.json but in profiles.yaml are shown with state=ghost.
 */
export class GhostTreeProvider
  implements vscode.TreeDataProvider<SamDomainItem>, vscode.Disposable {

  private readonly _onDidChangeTreeData = new vscode.EventEmitter<
    SamDomainItem | undefined | null | void
  >();
  readonly onDidChangeTreeData = this._onDidChangeTreeData.event;

  /** Domains currently being hydrated (show spinner). */
  private readonly loadingDomains = new Set<string>();

  private disposables: vscode.Disposable[] = [];

  constructor(
    private readonly graphClient: GraphClient,
    private readonly profileWatcher: ProfileWatcher,
    private readonly extensionUri: vscode.Uri
  ) {
    // Refresh when graph or workspace changes.
    this.disposables.push(
      graphClient.onDidChange(() => this.refresh()),
      profileWatcher.onDidChange(() => this.refresh())
    );
  }

  // ─── TreeDataProvider ───────────────────────────────────────────────────────

  getTreeItem(element: SamDomainItem): vscode.TreeItem {
    return element;
  }

  getChildren(element?: SamDomainItem): vscode.ProviderResult<SamDomainItem[]> {
    if (element) {
      // Expanded loaded domain — show files as plain text items.
      return this.getFilesForDomain(element.domain);
    }
    return this.buildTopLevelItems();
  }

  // ─── Public API ──────────────────────────────────────────────────────────────

  refresh(item?: SamDomainItem): void {
    this._onDidChangeTreeData.fire(item);
  }

  /** Mark a domain as loading (spinner).  Call again with loading=false when done. */
  setLoading(domain: string, loading: boolean): void {
    if (loading) {
      this.loadingDomains.add(domain);
    } else {
      this.loadingDomains.delete(domain);
    }
    this._onDidChangeTreeData.fire();
  }

  // ─── Internal ────────────────────────────────────────────────────────────────

  private buildTopLevelItems(): SamDomainItem[] {
    const hydratedSet = new Set(
      this.profileWatcher.getWorkspaceStatus()?.hydratedDomains ?? []
    );

    // Build shared set from active profile auto_include.
    const sharedSet = new Set<string>();
    const profiles = this.profileWatcher.getProfiles();
    const activeProfile = this.profileWatcher.getActiveProfile();
    if (profiles && activeProfile) {
      const prof = profiles.profiles[activeProfile];
      if (prof?.auto_include) {
        prof.auto_include.forEach(d => sharedSet.add(d));
      }
    }

    // All known domains: graph + anything hydrated that graph hasn't seen yet.
    const allDomains = new Set<string>(this.graphClient.getAllDomains());
    hydratedSet.forEach(d => allDomains.add(d));
    if (profiles) {
      for (const prof of Object.values(profiles.profiles)) {
        if (Array.isArray(prof.domains)) {
          prof.domains.forEach(d => allDomains.add(d));
        }
      }
    }

    return Array.from(allDomains)
      .sort()
      .map(domain => {
        let state: DomainState;
        if (this.loadingDomains.has(domain)) {
          state = 'loading';
        } else if (hydratedSet.has(domain) && sharedSet.has(domain)) {
          state = 'shared';
        } else if (hydratedSet.has(domain)) {
          state = 'loaded';
        } else {
          state = 'ghost';
        }
        return new SamDomainItem(domain, state, this.extensionUri);
      });
  }

  private getFilesForDomain(domain: string): vscode.TreeItem[] {
    // Show dependency info rather than actual files (we avoid touching git here).
    const deps = this.graphClient.getDependencies(domain);
    if (deps.length === 0) {
      const empty = new vscode.TreeItem('No known dependencies');
      empty.description = 'run sam fetch to hydrate';
      return [empty];
    }

    return deps
      .filter(d => d.score >= 0.4)
      .map(d => {
        const item = new vscode.TreeItem(d.domain);
        item.description = d.type === 'co_change'
          ? `co-change (${Math.round(d.score * 100)}%)`
          : 'static import';
        item.contextValue = 'dependency';
        item.iconPath = new vscode.ThemeIcon(
          d.type === 'co_change' ? 'link' : 'references'
        );
        return item;
      });
  }

  dispose(): void {
    this.disposables.forEach(d => d.dispose());
    this._onDidChangeTreeData.dispose();
  }
}

// ─── Helpers ──────────────────────────────────────────────────────────────────

function tooltipForState(domain: string, state: DomainState): string {
  switch (state) {
    case 'ghost':
      return `${domain}\nNot hydrated — click to fetch with SAM`;
    case 'loading':
      return `${domain}\nFetching...`;
    case 'shared':
      return `${domain}\nShared dependency — auto-included by your profile`;
    case 'loaded':
      return `${domain}\nHydrated`;
  }
}

function iconForState(
  state: DomainState,
  extensionUri: vscode.Uri
): vscode.Uri | vscode.ThemeIcon {
  const iconName = {
    ghost: 'icon-ghost.svg',
    loading: 'icon-loading.svg',
    loaded: 'icon-loaded.svg',
    shared: 'icon-shared.svg',
  }[state];

  return vscode.Uri.joinPath(extensionUri, 'media', iconName);
}
