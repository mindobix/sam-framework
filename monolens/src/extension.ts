import * as vscode from 'vscode';
import * as path from 'path';
import { GraphClient } from './graphClient';
import { ProfileWatcher } from './profileWatcher';
import { SamClient } from './samClient';
import { GhostTreeProvider } from './ghostTreeProvider';
import { StatusBar } from './statusBar';
import { HydrationPanel } from './hydrationPanel';
import { ImpactCodeLensProvider } from './impactGutter';

/** All disposables created during activation are collected here. */
const disposables: vscode.Disposable[] = [];

export function activate(context: vscode.ExtensionContext): void {
  // ─── Resolve repo root ──────────────────────────────────────────────────────
  const repoRoot = resolveRepoRoot();
  if (!repoRoot) {
    // No workspace open or no .sam/ folder — stay silent.
    return;
  }

  // ─── Core services ──────────────────────────────────────────────────────────
  const graphClient = new GraphClient(repoRoot);
  const profileWatcher = new ProfileWatcher(repoRoot);
  const samClient = new SamClient(repoRoot);

  disposables.push(graphClient, profileWatcher);

  // ─── UI components ──────────────────────────────────────────────────────────
  const treeProvider = new GhostTreeProvider(
    graphClient,
    profileWatcher,
    context.extensionUri
  );

  const statusBar = new StatusBar(profileWatcher, graphClient);

  const hydrationPanel = new HydrationPanel(
    samClient,
    profileWatcher,
    treeProvider,
    statusBar,
    context.extensionUri
  );

  disposables.push(treeProvider, statusBar, hydrationPanel);

  // ─── Tree view registration ─────────────────────────────────────────────────
  const treeView = vscode.window.createTreeView('samDomainTree', {
    treeDataProvider: treeProvider,
    showCollapseAll: true,
  });
  disposables.push(treeView);

  // ─── CodeLens (impact gutter) ───────────────────────────────────────────────
  const impactLens = new ImpactCodeLensProvider(graphClient, repoRoot);
  disposables.push(
    vscode.languages.registerCodeLensProvider(
      [
        { scheme: 'file', language: 'typescript' },
        { scheme: 'file', language: 'javascript' },
        { scheme: 'file', language: 'go' },
        { scheme: 'file', language: 'python' },
        { scheme: 'file', language: 'java' },
      ],
      impactLens
    ),
    impactLens
  );

  // ─── File decoration (native explorer badges) ───────────────────────────────
  const fileDecorator = new SamFileDecorator(profileWatcher, repoRoot);
  disposables.push(
    vscode.window.registerFileDecorationProvider(fileDecorator),
    fileDecorator
  );

  // ─── Commands ───────────────────────────────────────────────────────────────
  register(context, 'monolens.hydrateOnClick', async (domain: string) => {
    await hydrationPanel.show(domain);
  });

  register(context, 'monolens.hydrateWithDeps', async (item: unknown) => {
    const domain = domainFromArg(item);
    if (domain) {
      await runHydrate(samClient, treeProvider, statusBar, domain, true);
    }
  });

  register(context, 'monolens.showPlan', async (item: unknown) => {
    const domain = domainFromArg(item);
    if (domain) {
      await hydrationPanel.show(domain);
    }
  });

  register(context, 'monolens.useProfile', async () => {
    const profiles = samClient.listProfiles();
    if (profiles.length === 0) {
      vscode.window.showWarningMessage('No profiles found in .sam/profiles.yaml');
      return;
    }
    const picked = await vscode.window.showQuickPick(profiles, {
      placeHolder: 'Select a SAM workspace profile',
    });
    if (!picked) { return; }

    await vscode.window.withProgress(
      { location: vscode.ProgressLocation.Notification, title: `SAM: Switching to ${picked}...` },
      async () => {
        const result = await samClient.useProfile(picked);
        if (result.success) {
          treeProvider.refresh();
          statusBar.refresh();
          vscode.window.showInformationMessage(`SAM: Now using profile "${picked}"`);
        } else {
          vscode.window.showErrorMessage(`SAM: Failed to switch profile: ${result.errorMessage}`);
        }
      }
    );
  });

  register(context, 'monolens.showImpact', async () => {
    await vscode.window.withProgress(
      { location: vscode.ProgressLocation.Notification, title: 'SAM: Running impact analysis...' },
      async () => {
        const result = await samClient.impact();
        if (!result) {
          vscode.window.showWarningMessage(
            'SAM: Could not run impact analysis. Is MonoGraph running?'
          );
          return;
        }
        if (result.entries.length === 0) {
          vscode.window.showInformationMessage('SAM: No cross-domain impact detected.');
          return;
        }

        const criticalCount = result.entries.filter(e => e.risk === 'critical').length;
        const summary = `${result.entries.length} domains affected` +
          (criticalCount > 0 ? ` · ${criticalCount} CRITICAL` : '');

        vscode.window.showWarningMessage(`SAM Impact: ${summary}`, 'View in Terminal')
          .then(sel => {
            if (sel === 'View in Terminal') {
              const term = vscode.window.createTerminal('SAM Impact');
              term.show();
              term.sendText('sam impact');
            }
          });
      }
    );
  });

  register(context, 'monolens.refreshTree', () => {
    treeProvider.refresh();
    statusBar.refresh();
  });

  register(context, 'monolens.showQuickPick', async () => {
    const items: vscode.QuickPickItem[] = [
      { label: '$(arrow-down) Hydrate domain',        description: 'sam fetch <domain>' },
      { label: '$(list-tree) Switch profile',         description: 'sam use --profile' },
      { label: '$(warning) Show impact analysis',     description: 'sam impact' },
      { label: '$(refresh) Refresh domain tree',      description: 'Re-read workspace state' },
    ];
    const picked = await vscode.window.showQuickPick(items, {
      placeHolder: 'SAM commands',
    });
    if (!picked) { return; }

    const cmdMap: Record<string, string> = {
      '$(arrow-down) Hydrate domain':     'monolens.hydrateOnClick',
      '$(list-tree) Switch profile':      'monolens.useProfile',
      '$(warning) Show impact analysis':  'monolens.showImpact',
      '$(refresh) Refresh domain tree':   'monolens.refreshTree',
    };
    const cmd = cmdMap[picked.label];
    if (cmd) { vscode.commands.executeCommand(cmd); }
  });

  // ─── Push all disposables into context ──────────────────────────────────────
  context.subscriptions.push(...disposables);
}

export function deactivate(): void {
  // VS Code disposes context.subscriptions automatically.
}

// ─── FileDecorationProvider ───────────────────────────────────────────────────

class SamFileDecorator
  implements vscode.FileDecorationProvider, vscode.Disposable {

  private readonly _onDidChangeFileDecorations = new vscode.EventEmitter<vscode.Uri[]>();
  readonly onDidChangeFileDecorations = this._onDidChangeFileDecorations.event;

  private disposables: vscode.Disposable[] = [];

  constructor(
    private readonly profileWatcher: ProfileWatcher,
    private readonly repoRoot: string
  ) {
    this.disposables.push(
      profileWatcher.onDidChange(() => this._onDidChangeFileDecorations.fire([]))
    );
  }

  provideFileDecoration(uri: vscode.Uri): vscode.FileDecoration | undefined {
    const status = this.profileWatcher.getWorkspaceStatus();
    if (!status) { return undefined; }

    const rel = normalizePath(path.relative(this.repoRoot, uri.fsPath));
    const domain = toDomainPrefix(rel);
    if (!domain) { return undefined; }

    const profiles = this.profileWatcher.getProfiles();
    const sharedSet = new Set<string>();
    if (profiles && status.activeProfile) {
      const prof = profiles.profiles[status.activeProfile];
      prof?.auto_include?.forEach(d => sharedSet.add(d));
    }

    const isHydrated = status.hydratedDomains.some(d => rel.startsWith(d));
    const isShared = Array.from(sharedSet).some(d => rel.startsWith(d));

    if (isShared && isHydrated) {
      return {
        badge: 'S',
        color: new vscode.ThemeColor('textLink.foreground'),
        tooltip: 'Shared dependency — auto-included by your profile',
      };
    }
    if (isHydrated) {
      return { badge: '●', tooltip: 'Hydrated domain' };
    }

    // Only decorate directories at the domain level, not every file.
    // (prevents decorating thousands of non-domain paths)
    if (isDomainRoot(rel, domain)) {
      return {
        badge: '○',
        color: new vscode.ThemeColor('disabledForeground'),
        tooltip: 'Not hydrated — click to fetch with SAM',
      };
    }

    return undefined;
  }

  dispose(): void {
    this.disposables.forEach(d => d.dispose());
    this._onDidChangeFileDecorations.dispose();
  }
}

// ─── Helpers ──────────────────────────────────────────────────────────────────

function resolveRepoRoot(): string | undefined {
  const folders = vscode.workspace.workspaceFolders;
  if (!folders || folders.length === 0) { return undefined; }
  // Return the first workspace folder that contains .sam/profiles.yaml.
  for (const folder of folders) {
    const samPath = path.join(folder.uri.fsPath, '.sam', 'profiles.yaml');
    try {
      require('fs').accessSync(samPath);
      return folder.uri.fsPath;
    } catch {
      // Not this one.
    }
  }
  return folders[0].uri.fsPath;
}

function register(
  context: vscode.ExtensionContext,
  command: string,
  handler: (...args: unknown[]) => unknown
): void {
  context.subscriptions.push(
    vscode.commands.registerCommand(command, handler)
  );
}

function domainFromArg(arg: unknown): string | undefined {
  if (typeof arg === 'string') { return arg; }
  if (arg && typeof arg === 'object' && 'domain' in arg) {
    return (arg as { domain: string }).domain;
  }
  return undefined;
}

async function runHydrate(
  samClient: SamClient,
  treeProvider: GhostTreeProvider,
  statusBar: StatusBar,
  domain: string,
  withDeps: boolean
): Promise<void> {
  treeProvider.setLoading(domain, true);
  await vscode.window.withProgress(
    {
      location: vscode.ProgressLocation.Notification,
      title: `SAM: Hydrating ${domain}${withDeps ? ' with deps' : ''}...`,
      cancellable: false,
    },
    async () => {
      const result = await samClient.hydrate(domain, withDeps);
      treeProvider.setLoading(domain, false);
      statusBar.refresh();
      treeProvider.refresh();
      if (result.success) {
        vscode.window.showInformationMessage(
          `SAM: ${domain} hydrated (${result.filesAdded} files)`
        );
      } else {
        vscode.window.showErrorMessage(
          `SAM: Failed to hydrate ${domain}: ${result.errorMessage}`
        );
      }
    }
  );
}

function normalizePath(p: string): string {
  return p.replace(/\\/g, '/');
}

function toDomainPrefix(rel: string): string | undefined {
  const parts = rel.split('/');
  if (parts.length < 2) { return undefined; }
  return parts.slice(0, 2).join('/');
}

function isDomainRoot(rel: string, domain: string): boolean {
  return rel === domain || rel.startsWith(domain + '/');
}
