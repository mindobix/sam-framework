import * as vscode from 'vscode';
import * as cp from 'child_process';
import * as path from 'path';
import * as fs from 'fs';
import { ImpactResult, HydrateResult, MonoLensConfig } from './types';

const FETCH_TIMEOUT_MS = 30_000;
const QUERY_TIMEOUT_MS = 5_000;

/**
 * SamClient wraps the `sam` CLI binary.  All interactions with the monorepo
 * go through this class — no direct git or HTTP calls from the extension.
 */
export class SamClient {
  private repoRoot: string;

  constructor(repoRoot: string) {
    this.repoRoot = repoRoot;
  }

  // ─── Public commands ─────────────────────────────────────────────────────────

  /** Fetch (hydrate) a domain, optionally with its AI-resolved deps. */
  async hydrate(domain: string, withDeps: boolean): Promise<HydrateResult> {
    const args = ['fetch', domain];
    if (withDeps) { args.push('--with-deps'); }

    const { stdout, stderr, exitCode } = await this.exec(args, FETCH_TIMEOUT_MS);
    if (exitCode !== 0) {
      return { domain, filesAdded: 0, success: false, errorMessage: stderr || stdout };
    }

    // Parse "Fetched N files" from stderr output (human-readable goes to stderr).
    const match = (stderr + stdout).match(/(\d+)\s+files?/i);
    const filesAdded = match ? parseInt(match[1], 10) : 0;
    return { domain, filesAdded, success: true };
  }

  /** Switch to a workspace profile. */
  async useProfile(profile: string): Promise<{ success: boolean; errorMessage?: string }> {
    const { exitCode, stderr, stdout } = await this.exec(
      ['use', '--profile', profile, '--no-ai'],
      FETCH_TIMEOUT_MS
    );
    if (exitCode !== 0) {
      return { success: false, errorMessage: stderr || stdout };
    }
    return { success: true };
  }

  /** Run `sam impact --format json` and return parsed entries. */
  async impact(): Promise<ImpactResult | null> {
    const { stdout, exitCode } = await this.exec(
      ['impact', '--format', 'json'],
      QUERY_TIMEOUT_MS
    );
    if (exitCode !== 0 || !stdout.trim()) { return null; }

    try {
      return JSON.parse(stdout) as ImpactResult;
    } catch {
      return null;
    }
  }

  /** Run `sam plan <domain>` and return raw stderr output for display. */
  async plan(domain: string): Promise<string> {
    const { stdout, stderr } = await this.exec(
      ['plan', domain],
      QUERY_TIMEOUT_MS
    );
    return stderr || stdout;
  }

  /** List available profiles by reading profiles.yaml (no subprocess). */
  listProfiles(): string[] {
    const profilesPath = path.join(this.repoRoot, '.sam', 'profiles.yaml');
    try {
      const raw = fs.readFileSync(profilesPath, 'utf8');
      return extractProfileNames(raw);
    } catch {
      return [];
    }
  }

  // ─── Binary discovery ────────────────────────────────────────────────────────

  private resolveBinary(): string {
    const cfg = vscode.workspace.getConfiguration('monolens') as unknown as MonoLensConfig;
    if (cfg.samBinaryPath) { return cfg.samBinaryPath; }

    // Search common install locations.
    for (const p of [
      '/usr/local/bin/sam',
      `${process.env.HOME}/.sam/bin/sam`,
      `${process.env.HOME}/.local/bin/sam`,
    ]) {
      if (fs.existsSync(p)) { return p; }
    }

    // Fall back to PATH lookup.
    return 'sam';
  }

  // ─── exec helper ─────────────────────────────────────────────────────────────

  private exec(
    args: string[],
    timeoutMs: number
  ): Promise<{ stdout: string; stderr: string; exitCode: number }> {
    return new Promise((resolve) => {
      const bin = this.resolveBinary();
      const proc = cp.spawn(bin, args, {
        cwd: this.repoRoot,
        env: { ...process.env },
      });

      let stdout = '';
      let stderr = '';

      proc.stdout.on('data', (d: Buffer) => { stdout += d.toString(); });
      proc.stderr.on('data', (d: Buffer) => { stderr += d.toString(); });

      const timer = setTimeout(() => {
        proc.kill();
        resolve({ stdout, stderr: `[timeout after ${timeoutMs}ms]`, exitCode: -1 });
      }, timeoutMs);

      proc.on('close', (code) => {
        clearTimeout(timer);
        resolve({ stdout, stderr, exitCode: code ?? -1 });
      });

      proc.on('error', (err) => {
        clearTimeout(timer);
        resolve({ stdout: '', stderr: err.message, exitCode: -1 });
      });
    });
  }
}

// ─── Helpers ──────────────────────────────────────────────────────────────────

/** Extract profile names from profiles.yaml without a YAML parser. */
function extractProfileNames(raw: string): string[] {
  const names: string[] = [];
  let inProfiles = false;

  for (const line of raw.split('\n')) {
    const trimmed = line.trim();
    if (trimmed === 'profiles:') { inProfiles = true; continue; }
    if (!inProfiles) { continue; }

    const indent = line.search(/\S/);
    // Profile entries are at indent 2, end with ":"
    if (indent === 2 && trimmed.endsWith(':') && !trimmed.startsWith('-')) {
      names.push(trimmed.slice(0, -1));
    }
    // Stop at next top-level key
    if (indent === 0 && trimmed !== '' && !trimmed.startsWith('#')) {
      inProfiles = false;
    }
  }

  return names.sort();
}
