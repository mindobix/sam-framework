import * as vscode from 'vscode';
import * as fs from 'fs';
import * as path from 'path';
import { ProfilesConfig, WorkspaceStatus } from './types';

/**
 * ProfileWatcher monitors `.sam/profiles.yaml` and `.sam/workspace.yaml` for
 * changes and exposes their parsed content.  Other components subscribe to
 * `onDidChange` to refresh when the user switches profiles or hydrates domains.
 */
export class ProfileWatcher implements vscode.Disposable {
  private profiles: ProfilesConfig | null = null;
  private status: WorkspaceStatus | null = null;

  private readonly _onDidChange = new vscode.EventEmitter<void>();
  readonly onDidChange = this._onDidChange.event;

  private readonly profilesPath: string;
  private readonly workspacePath: string;
  private watchers: vscode.FileSystemWatcher[] = [];

  constructor(private readonly repoRoot: string) {
    this.profilesPath = path.join(repoRoot, '.sam', 'profiles.yaml');
    this.workspacePath = path.join(repoRoot, '.sam', 'workspace.yaml');

    this.loadAll();
    this.startWatchers();
  }

  // ─── Public API ─────────────────────────────────────────────────────────────

  getProfiles(): ProfilesConfig | null {
    return this.profiles;
  }

  getWorkspaceStatus(): WorkspaceStatus | null {
    return this.status;
  }

  /** Returns true if the given domain path is currently hydrated. */
  isDomainHydrated(domain: string): boolean {
    return this.status?.hydratedDomains.includes(domain) ?? false;
  }

  /** Returns the active profile name, or '' if none. */
  getActiveProfile(): string {
    return this.status?.activeProfile ?? '';
  }

  /** All profile names defined in profiles.yaml. */
  getProfileNames(): string[] {
    if (!this.profiles) { return []; }
    return Object.keys(this.profiles.profiles).sort();
  }

  // ─── Internal ────────────────────────────────────────────────────────────────

  private loadAll(): void {
    this.loadProfiles();
    this.loadWorkspace();
  }

  private loadProfiles(): void {
    try {
      const raw = fs.readFileSync(this.profilesPath, 'utf8');
      this.profiles = parseProfilesYAML(raw);
    } catch {
      this.profiles = null;
    }
  }

  private loadWorkspace(): void {
    try {
      const raw = fs.readFileSync(this.workspacePath, 'utf8');
      this.status = parseWorkspaceYAML(raw);
    } catch {
      this.status = null;
    }
  }

  private startWatchers(): void {
    const watch = (relGlob: string, handler: () => void): void => {
      const pattern = new vscode.RelativePattern(this.repoRoot, relGlob);
      const w = vscode.workspace.createFileSystemWatcher(pattern);
      w.onDidChange(handler);
      w.onDidCreate(handler);
      w.onDidDelete(handler);
      this.watchers.push(w);
    };

    watch('.sam/profiles.yaml', () => {
      this.loadProfiles();
      this._onDidChange.fire();
    });

    watch('.sam/workspace.yaml', () => {
      this.loadWorkspace();
      this._onDidChange.fire();
    });
  }

  dispose(): void {
    this.watchers.forEach(w => w.dispose());
    this._onDidChange.dispose();
  }
}

// ─── Minimal YAML parsers ─────────────────────────────────────────────────────
// The extension ships no YAML library to keep bundle size small.
// We only need to extract specific scalar/sequence fields.

/**
 * Parse `.sam/workspace.yaml` into WorkspaceStatus.
 * Format is simple flat YAML; we parse just what we need.
 */
function parseWorkspaceYAML(raw: string): WorkspaceStatus {
  const lines = raw.split('\n');
  const result: WorkspaceStatus = {
    activeProfile: '',
    hydratedDomains: [],
    lastUpdated: '',
    monographAnalyzed: false,
  };

  let inDomains = false;
  for (const line of lines) {
    const trimmed = line.trim();

    if (trimmed.startsWith('active_profile:')) {
      result.activeProfile = extractScalar(trimmed, 'active_profile:');
      inDomains = false;
      continue;
    }
    if (trimmed.startsWith('last_updated:')) {
      result.lastUpdated = extractScalar(trimmed, 'last_updated:');
      inDomains = false;
      continue;
    }
    if (trimmed.startsWith('monograph_analyzed:')) {
      result.monographAnalyzed = extractScalar(trimmed, 'monograph_analyzed:') === 'true';
      inDomains = false;
      continue;
    }
    if (trimmed.startsWith('hydrated_domains:')) {
      inDomains = true;
      continue;
    }
    if (inDomains && trimmed.startsWith('- ')) {
      result.hydratedDomains.push(trimmed.slice(2).trim().replace(/^["']|["']$/g, ''));
      continue;
    }
    if (inDomains && trimmed !== '' && !trimmed.startsWith('#')) {
      inDomains = false;
    }
  }

  return result;
}

/**
 * Parse `.sam/profiles.yaml` into ProfilesConfig.
 * We extract profile names and their domains lists.
 */
function parseProfilesYAML(raw: string): ProfilesConfig {
  const result: ProfilesConfig = { version: '', profiles: {} };
  const lines = raw.split('\n');

  let currentProfile: string | null = null;
  let inDomains = false;

  for (const line of lines) {
    const trimmed = line.trim();
    const indent = line.search(/\S/);

    if (trimmed.startsWith('version:')) {
      result.version = extractScalar(trimmed, 'version:');
      continue;
    }

    // Profile names are at indent 2 under "profiles:" key
    if (indent === 2 && trimmed.endsWith(':') && !trimmed.startsWith('-')) {
      currentProfile = trimmed.slice(0, -1);
      result.profiles[currentProfile] = { domains: [] };
      inDomains = false;
      continue;
    }

    if (!currentProfile) { continue; }
    const prof = result.profiles[currentProfile];

    if (trimmed.startsWith('domains:')) {
      const inline = trimmed.slice('domains:'.length).trim();
      if (inline === '"*"' || inline === "'*'" || inline === '*') {
        prof.domains = '*';
        inDomains = false;
      } else if (inline === '') {
        inDomains = true;
      } else {
        prof.domains = [inline.replace(/^["']|["']$/g, '')];
        inDomains = false;
      }
      continue;
    }

    if (inDomains && trimmed.startsWith('- ')) {
      if (Array.isArray(prof.domains)) {
        prof.domains.push(trimmed.slice(2).trim().replace(/^["']|["']$/g, ''));
      }
      continue;
    }

    if (trimmed.startsWith('ai_infer:')) {
      prof.ai_infer = extractScalar(trimmed, 'ai_infer:') === 'true';
      inDomains = false;
      continue;
    }

    if (trimmed !== '' && !trimmed.startsWith('-') && !trimmed.startsWith('#')) {
      inDomains = false;
    }
  }

  return result;
}

function extractScalar(line: string, prefix: string): string {
  return line.slice(prefix.length).trim().replace(/^["']|["']$/g, '');
}
