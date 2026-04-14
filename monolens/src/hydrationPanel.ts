import * as vscode from 'vscode';
import { SamClient } from './samClient';
import { ProfileWatcher } from './profileWatcher';
import { GhostTreeProvider } from './ghostTreeProvider';
import { StatusBar } from './statusBar';

/**
 * HydrationPanel shows a webview preview before the user commits to fetching a
 * domain.  It calls `sam plan <domain>` to get the dependency list and renders
 * a "what will be fetched" table with Hydrate / Hydrate with deps / Cancel buttons.
 */
export class HydrationPanel implements vscode.Disposable {
  private panel: vscode.WebviewPanel | undefined;
  private currentDomain: string | undefined;
  private disposables: vscode.Disposable[] = [];

  constructor(
    private readonly samClient: SamClient,
    private readonly profileWatcher: ProfileWatcher,
    private readonly treeProvider: GhostTreeProvider,
    private readonly statusBar: StatusBar,
    private readonly extensionUri: vscode.Uri
  ) {}

  // ─── Public API ──────────────────────────────────────────────────────────────

  async show(domain: string): Promise<void> {
    this.currentDomain = domain;

    if (this.panel) {
      this.panel.reveal(vscode.ViewColumn.Beside);
    } else {
      this.panel = vscode.window.createWebviewPanel(
        'samHydrationPanel',
        `SAM: Hydrate ${domain}`,
        vscode.ViewColumn.Beside,
        {
          enableScripts: true,
          localResourceRoots: [vscode.Uri.joinPath(this.extensionUri, 'media')],
        }
      );

      this.panel.onDidDispose(() => {
        this.panel = undefined;
      }, null, this.disposables);

      this.panel.webview.onDidReceiveMessage(
        async (msg: { command: string }) => {
          switch (msg.command) {
            case 'hydrate':
              await this.hydrate(domain, false);
              break;
            case 'hydrateWithDeps':
              await this.hydrate(domain, true);
              break;
            case 'cancel':
              this.panel?.dispose();
              break;
          }
        },
        null,
        this.disposables
      );
    }

    this.panel.title = `SAM: Hydrate ${domain}`;
    this.panel.webview.html = this.loadingHtml(domain);

    // Fetch plan in background and update panel.
    const planOutput = await this.samClient.plan(domain);
    if (this.panel && this.currentDomain === domain) {
      this.panel.webview.html = this.buildHtml(domain, planOutput);
    }
  }

  // ─── Hydration ───────────────────────────────────────────────────────────────

  private async hydrate(domain: string, withDeps: boolean): Promise<void> {
    this.panel?.dispose();

    this.treeProvider.setLoading(domain, true);

    await vscode.window.withProgress(
      {
        location: vscode.ProgressLocation.Notification,
        title: `SAM: Hydrating ${domain}${withDeps ? ' with deps' : ''}...`,
        cancellable: false,
      },
      async () => {
        const result = await this.samClient.hydrate(domain, withDeps);

        this.treeProvider.setLoading(domain, false);
        this.statusBar.refresh();
        this.treeProvider.refresh();

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

  // ─── HTML ────────────────────────────────────────────────────────────────────

  private loadingHtml(domain: string): string {
    return `<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
  <title>SAM: Hydrate ${escapeHtml(domain)}</title>
  <style>${sharedStyles()}</style>
</head>
<body>
  <h2>Hydrating <code>${escapeHtml(domain)}</code></h2>
  <p class="muted">Loading fetch plan...</p>
</body>
</html>`;
  }

  private buildHtml(domain: string, planOutput: string): string {
    const hydratedDomains = new Set(
      this.profileWatcher.getWorkspaceStatus()?.hydratedDomains ?? []
    );

    return `<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
  <title>SAM: Hydrate ${escapeHtml(domain)}</title>
  <style>${sharedStyles()}</style>
</head>
<body>
  <h2>Fetch plan for <code>${escapeHtml(domain)}</code></h2>

  ${hydratedDomains.has(domain)
    ? `<p class="info">✓ Already hydrated. Fetching again will refresh any changes.</p>`
    : ''}

  <pre class="plan-output">${escapeHtml(planOutput || 'No plan output available.')}</pre>

  <div class="actions">
    <button onclick="send('hydrate')" class="primary">Hydrate</button>
    <button onclick="send('hydrateWithDeps')">Hydrate with deps</button>
    <button onclick="send('cancel')" class="secondary">Cancel</button>
  </div>

  <script>
    const vscode = acquireVsCodeApi();
    function send(command) {
      vscode.postMessage({ command });
    }
  </script>
</body>
</html>`;
  }

  dispose(): void {
    this.panel?.dispose();
    this.disposables.forEach(d => d.dispose());
  }
}

// ─── Helpers ──────────────────────────────────────────────────────────────────

function escapeHtml(s: string): string {
  return s
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;');
}

function sharedStyles(): string {
  return `
    body {
      font-family: var(--vscode-font-family);
      font-size: var(--vscode-font-size);
      color: var(--vscode-foreground);
      background: var(--vscode-editor-background);
      padding: 20px;
      max-width: 800px;
    }
    h2 { margin-bottom: 8px; }
    code {
      font-family: var(--vscode-editor-font-family);
      background: var(--vscode-textCodeBlock-background);
      padding: 2px 6px;
      border-radius: 3px;
    }
    pre.plan-output {
      font-family: var(--vscode-editor-font-family);
      font-size: 0.9em;
      background: var(--vscode-textCodeBlock-background);
      padding: 12px;
      border-radius: 4px;
      overflow-x: auto;
      white-space: pre-wrap;
      border-left: 3px solid var(--vscode-focusBorder);
    }
    .actions {
      display: flex;
      gap: 8px;
      margin-top: 16px;
    }
    button {
      padding: 6px 16px;
      border: none;
      border-radius: 3px;
      cursor: pointer;
      font-size: var(--vscode-font-size);
      background: var(--vscode-button-secondaryBackground);
      color: var(--vscode-button-secondaryForeground);
    }
    button.primary {
      background: var(--vscode-button-background);
      color: var(--vscode-button-foreground);
    }
    button.secondary {
      background: transparent;
      color: var(--vscode-descriptionForeground);
    }
    button:hover { opacity: 0.85; }
    .muted { color: var(--vscode-descriptionForeground); }
    .info {
      color: var(--vscode-notificationsInfoIcon-foreground);
      margin-bottom: 8px;
    }
  `;
}
