import * as assert from 'assert';

// ─────────────────────────────────────────────────────────────────────────────
// Inline the pure helper logic under test so we can run these without VS Code.
// The production code lives in ghostTreeProvider.ts — keep these in sync.
// ─────────────────────────────────────────────────────────────────────────────

// Re-implementation of dedup/merge logic from buildTopLevelItems so we can
// unit test it without an extension host.

type DomainState = 'ghost' | 'loading' | 'loaded' | 'shared';

function classifyDomains(
  allDomains: string[],
  hydrated: string[],
  shared: string[],
  loading: string[]
): Map<string, DomainState> {
  const hydratedSet = new Set(hydrated);
  const sharedSet = new Set(shared);
  const loadingSet = new Set(loading);

  const result = new Map<string, DomainState>();
  for (const domain of allDomains) {
    let state: DomainState;
    if (loadingSet.has(domain)) {
      state = 'loading';
    } else if (hydratedSet.has(domain) && sharedSet.has(domain)) {
      state = 'shared';
    } else if (hydratedSet.has(domain)) {
      state = 'loaded';
    } else {
      state = 'ghost';
    }
    result.set(domain, state);
  }
  return result;
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

suite('GhostTreeProvider — domain classification', () => {
  test('unhydrated domain is ghost', () => {
    const states = classifyDomains(['apis/sales'], [], [], []);
    assert.strictEqual(states.get('apis/sales'), 'ghost');
  });

  test('hydrated domain is loaded', () => {
    const states = classifyDomains(['apis/sales'], ['apis/sales'], [], []);
    assert.strictEqual(states.get('apis/sales'), 'loaded');
  });

  test('hydrated shared domain is shared', () => {
    const states = classifyDomains(
      ['shared/auth'],
      ['shared/auth'],
      ['shared/auth'],
      []
    );
    assert.strictEqual(states.get('shared/auth'), 'shared');
  });

  test('loading takes precedence over hydrated', () => {
    const states = classifyDomains(
      ['apis/sales'],
      ['apis/sales'],
      [],
      ['apis/sales']
    );
    assert.strictEqual(states.get('apis/sales'), 'loading');
  });

  test('mixed: ghost and loaded in same result', () => {
    const states = classifyDomains(
      ['apis/sales', 'apis/pricing', 'shared/auth'],
      ['apis/sales', 'shared/auth'],
      ['shared/auth'],
      []
    );
    assert.strictEqual(states.get('apis/sales'), 'loaded');
    assert.strictEqual(states.get('apis/pricing'), 'ghost');
    assert.strictEqual(states.get('shared/auth'), 'shared');
  });
});
