import * as vscode from 'vscode';
import { ProfileWatcher } from './profileWatcher';
import { GraphClient } from './graphClient';

/**
 * StatusBar manages the bottom-left status bar item:
 *   [SAM] sales-api · 4 domains · graph ready
 *   [SAM] sales-api · 4 domains · no graph
 *   [SAM] No profile active
 */
export class StatusBar implements vscode.Disposable {
  private readonly item: vscode.StatusBarItem;
  private disposables: vscode.Disposable[] = [];

  constructor(
    private readonly profileWatcher: ProfileWatcher,
    private readonly graphClient: GraphClient
  ) {
    this.item = vscode.window.createStatusBarItem(
      vscode.StatusBarAlignment.Left,
      100
    );
    this.item.command = 'monolens.showQuickPick';
    this.item.tooltip = 'SAM workspace — click for commands';
    this.item.show();

    this.update();

    this.disposables.push(
      profileWatcher.onDidChange(() => this.update()),
      graphClient.onDidChange(() => this.update())
    );
  }

  // ─── Public API ──────────────────────────────────────────────────────────────

  /** Force a refresh (e.g. after hydrating a domain). */
  refresh(): void {
    this.update();
  }

  /** Show an amber warning when the daemon is unreachable. */
  setDaemonStatus(reachable: boolean): void {
    if (reachable) {
      this.item.color = undefined;
      this.item.backgroundColor = undefined;
    } else {
      this.item.color = new vscode.ThemeColor('statusBarItem.warningForeground');
      this.item.backgroundColor = new vscode.ThemeColor('statusBarItem.warningBackground');
    }
  }

  // ─── Internal ────────────────────────────────────────────────────────────────

  private update(): void {
    const status = this.profileWatcher.getWorkspaceStatus();
    const graphLoaded = this.graphClient.isLoaded();

    if (!status || !status.activeProfile) {
      this.item.text = '$(file-directory) SAM — No profile';
      this.item.tooltip = 'No SAM profile active. Run "SAM: Switch workspace profile".';
      return;
    }

    const domainCount = status.hydratedDomains.length;
    const graphLabel = graphLoaded ? 'graph ready' : 'no graph';

    this.item.text = [
      '$(file-directory)',
      `SAM: ${status.activeProfile}`,
      `· ${domainCount} domain${domainCount === 1 ? '' : 's'}`,
      `· ${graphLabel}`,
    ].join(' ');

    this.item.tooltip = [
      `Profile: ${status.activeProfile}`,
      `Domains: ${status.hydratedDomains.join(', ') || 'none'}`,
      graphLoaded
        ? `Graph: loaded (${this.graphClient.getAllDomains().length} total domains)`
        : 'Graph: not built — run sam fetch to trigger analysis',
      '',
      'Click for SAM commands',
    ].join('\n');
  }

  dispose(): void {
    this.item.dispose();
    this.disposables.forEach(d => d.dispose());
  }
}
