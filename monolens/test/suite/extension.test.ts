import * as assert from 'assert';

// ─────────────────────────────────────────────────────────────────────────────
// Unit tests for pure helpers that do not require the VS Code extension host.
// ─────────────────────────────────────────────────────────────────────────────

// ─── profileWatcher helpers (inline reimplementation for testing) ─────────────

function extractScalar(line: string, prefix: string): string {
  return line.slice(prefix.length).trim().replace(/^["']|["']$/g, '');
}

function parseWorkspaceYAML(raw: string): {
  activeProfile: string;
  hydratedDomains: string[];
  monographAnalyzed: boolean;
} {
  const lines = raw.split('\n');
  const result = { activeProfile: '', hydratedDomains: [] as string[], monographAnalyzed: false };
  let inDomains = false;

  for (const line of lines) {
    const trimmed = line.trim();
    if (trimmed.startsWith('active_profile:')) {
      result.activeProfile = extractScalar(trimmed, 'active_profile:');
      inDomains = false;
    } else if (trimmed.startsWith('monograph_analyzed:')) {
      result.monographAnalyzed = extractScalar(trimmed, 'monograph_analyzed:') === 'true';
      inDomains = false;
    } else if (trimmed.startsWith('hydrated_domains:')) {
      inDomains = true;
    } else if (inDomains && trimmed.startsWith('- ')) {
      result.hydratedDomains.push(trimmed.slice(2).trim().replace(/^["']|["']$/g, ''));
    } else if (inDomains && trimmed !== '' && !trimmed.startsWith('#')) {
      inDomains = false;
    }
  }
  return result;
}

// ─── Tests ────────────────────────────────────────────────────────────────────

suite('ProfileWatcher — workspace.yaml parser', () => {
  test('parses active_profile', () => {
    const ws = parseWorkspaceYAML('active_profile: sales-api\n');
    assert.strictEqual(ws.activeProfile, 'sales-api');
  });

  test('parses hydrated_domains list', () => {
    const raw = [
      'active_profile: sales-api',
      'hydrated_domains:',
      '  - apis/sales',
      '  - shared/auth',
    ].join('\n');
    const ws = parseWorkspaceYAML(raw);
    assert.deepStrictEqual(ws.hydratedDomains, ['apis/sales', 'shared/auth']);
  });

  test('parses monograph_analyzed flag', () => {
    const ws = parseWorkspaceYAML('monograph_analyzed: true\n');
    assert.strictEqual(ws.monographAnalyzed, true);
  });

  test('empty file returns safe defaults', () => {
    const ws = parseWorkspaceYAML('');
    assert.strictEqual(ws.activeProfile, '');
    assert.deepStrictEqual(ws.hydratedDomains, []);
    assert.strictEqual(ws.monographAnalyzed, false);
  });

  test('quoted profile name is unquoted', () => {
    const ws = parseWorkspaceYAML('active_profile: "my-profile"\n');
    assert.strictEqual(ws.activeProfile, 'my-profile');
  });
});

// ─── impactGutter helpers ─────────────────────────────────────────────────────

// Inline reimplementation of buildLabel for unit testing.
interface Dependency {
  domain: string;
  score: number;
}

function buildLabel(dependents: Dependency[]): string {
  const count = dependents.length;
  if (count === 0) { return ''; }
  const sorted = [...dependents].sort((a, b) => b.score - a.score);
  const first = sorted[0].domain;
  if (count === 1) {
    return `⚠ 1 domain depends on this · ${first}`;
  }
  return `⚠ ${count} domains depend on this · ${first} · +${count - 1} more`;
}

suite('ImpactCodeLensProvider — label builder', () => {
  test('no dependents → empty string', () => {
    assert.strictEqual(buildLabel([]), '');
  });

  test('one dependent', () => {
    const label = buildLabel([{ domain: 'apis/payments', score: 0.9 }]);
    assert.strictEqual(label, '⚠ 1 domain depends on this · apis/payments');
  });

  test('multiple dependents — highest score first', () => {
    const label = buildLabel([
      { domain: 'apis/sales', score: 0.5 },
      { domain: 'apis/payments', score: 0.95 },
    ]);
    assert.match(label, /⚠ 2 domains depend on this · apis\/payments · \+1 more/);
  });

  test('three dependents — correct +N count', () => {
    const label = buildLabel([
      { domain: 'a', score: 0.9 },
      { domain: 'b', score: 0.7 },
      { domain: 'c', score: 0.5 },
    ]);
    assert.match(label, /\+2 more/);
  });
});
