import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { mkdtemp, mkdir, readFile, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import test from 'node:test';

import {
  buildEvidence,
  digestScope,
  validateEvidence,
} from './feasibility-evidence.mjs';

const DESIGN_COMMIT = '5893d4340a4815677da79f74223642ac855519e7';
const COMMON_SCOPE = [
  'docs/feasibility/evidence-scopes.json',
  'docs/feasibility/evidence.schema.json',
  'scripts/feasibility-evidence.mjs',
];

const withCommon = (paths) => [...new Set([...COMMON_SCOPE, ...paths])].sort();

const TASK_4_SCOPE = [
  'package.json',
  'scripts/verify-config.mjs',
  'scripts/verify-default-artifacts.mjs',
  'src/App.svelte',
  'src/App.test.ts',
  'src/lib/feasibility/FeasibilityPanel.svelte',
  'src/lib/feasibility/GdProbe.svelte',
  'src/vite-env.d.ts',
  'src-tauri/Cargo.lock',
  'src-tauri/Cargo.toml',
  'src-tauri/build.rs',
  'src-tauri/capabilities/feasibility-main.json',
  'src-tauri/permissions/feasibility.toml',
  'src-tauri/src/feasibility/gd_live.rs',
  'src-tauri/src/feasibility/mod.rs',
  'src-tauri/src/feasibility/signature_webview.rs',
  'src-tauri/src/feasibility/webview_resource_policy.rs',
  'src-tauri/src/lib.rs',
  'src-tauri/tauri.conf.json',
  'src-tauri/tauri.feasibility.conf.json',
  'vite.config.ts',
];

const TASK_5_SCOPE = [
  'src-tauri/Cargo.lock',
  'src-tauri/Cargo.toml',
  'src-tauri/src/feasibility/gd_live.rs',
  'src-tauri/src/feasibility/mod.rs',
  'src-tauri/src/feasibility/network_policy.rs',
  'src-tauri/tests/network_policy.rs',
];

const TASK_8_SCOPE = [
  'scripts/slow-update-server.mjs',
  'scripts/verify-config.mjs',
  'src/lib/feasibility/FeasibilityPanel.svelte',
  'src-tauri/Cargo.lock',
  'src-tauri/Cargo.toml',
  'src-tauri/build.rs',
  'src-tauri/permissions/feasibility.toml',
  'src-tauri/src/feasibility/mod.rs',
  'src-tauri/src/feasibility/updater_probe.rs',
  'src-tauri/src/lib.rs',
  'src-tauri/tauri.feasibility.conf.json',
  'src-tauri/tests/updater_probe.rs',
];

const EXPECTED_SCOPES = {
  'atomic-commit': withCommon([
    'src-tauri/Cargo.lock',
    'src-tauri/Cargo.toml',
    'src-tauri/src/bin/atomic_commit_worker.rs',
    'src-tauri/src/feasibility/atomic_commit.rs',
    'src-tauri/src/feasibility/mod.rs',
    'src-tauri/tests/atomic_commit.rs',
  ]),
  'gd-contract-pagination': withCommon([
    ...TASK_4_SCOPE,
    ...TASK_5_SCOPE,
    'src-tauri/src/music/contract.rs',
    'src-tauri/src/music/mod.rs',
    'src-tauri/tests/fixtures/gd/README.md',
    'src-tauri/tests/fixtures/gd/explicit_error.json',
    'src-tauri/tests/fixtures/gd/lyric_empty.json',
    'src-tauri/tests/fixtures/gd/lyric_success.json',
    'src-tauri/tests/fixtures/gd/pic_success.json',
    'src-tauri/tests/fixtures/gd/search_empty.json',
    'src-tauri/tests/fixtures/gd/search_incompatible.json',
    'src-tauri/tests/fixtures/gd/search_mixed.json',
    'src-tauri/tests/fixtures/gd/url_empty.json',
    'src-tauri/tests/fixtures/gd/url_lower_bitrate.json',
    'src-tauri/tests/fixtures/gd/url_missing_bitrate.json',
    'src-tauri/tests/fixtures/gd/url_success.json',
    'src-tauri/tests/gd_contract.rs',
  ]),
  'media-containers': withCommon([
    'scripts/generate-media-fixtures.mjs',
    'src-tauri/Cargo.lock',
    'src-tauri/Cargo.toml',
    'src-tauri/src/feasibility/media_probe.rs',
    'src-tauri/src/feasibility/mod.rs',
    'src-tauri/tests/fixtures/media/README.md',
    'src-tauri/tests/fixtures/media/cover.png',
    'src-tauri/tests/fixtures/media/minimal-320.mp3',
    'src-tauri/tests/fixtures/media/minimal.flac',
    'src-tauri/tests/fixtures/media/minimal.mp2',
    'src-tauri/tests/fixtures/media/minimal.mp3',
    'src-tauri/tests/fixtures/media/truncated-flac.bin',
    'src-tauri/tests/fixtures/media/truncated-id3.bin',
    'src-tauri/tests/media_probe.rs',
  ]),
  'network-policy': withCommon(TASK_5_SCOPE),
  'result-list-performance': withCommon([
    '.github/workflows/perf-results.yml',
    'benchmarks/results-1000/BenchmarkApp.svelte',
    'benchmarks/results-1000/ResultsTablePrototype.svelte',
    'benchmarks/results-1000/budgets.ts',
    'benchmarks/results-1000/dataset.test.ts',
    'benchmarks/results-1000/dataset.ts',
    'benchmarks/results-1000/env.d.ts',
    'benchmarks/results-1000/index.html',
    'benchmarks/results-1000/main.ts',
    'benchmarks/results-1000/metrics.test.ts',
    'benchmarks/results-1000/metrics.ts',
    'benchmarks/results-1000/playwright-reporter.ts',
    'benchmarks/results-1000/playwright.config.ts',
    'benchmarks/results-1000/report.schema.json',
    'benchmarks/results-1000/report.test.ts',
    'benchmarks/results-1000/report.ts',
    'benchmarks/results-1000/results-1000.spec.ts',
    'benchmarks/results-1000/selection.test.ts',
    'benchmarks/results-1000/selection.ts',
    'benchmarks/results-1000/tsconfig.json',
    'benchmarks/results-1000/virtual-range.test.ts',
    'benchmarks/results-1000/virtual-range.ts',
    'benchmarks/results-1000/vite.config.ts',
    'package.json',
    'pnpm-lock.yaml',
    'scripts/perf/capture-windows-baseline.ps1',
    'scripts/perf/run-browser.mjs',
    'scripts/perf/validate-report.mjs',
    'scripts/perf/validate-report.test.mjs',
    'scripts/verify-config.mjs',
    'src-tauri/tauri.perf.conf.json',
  ]),
  'signature-webview': withCommon([...TASK_4_SCOPE, ...TASK_8_SCOPE]),
  'toolchain-ci': withCommon([
    '.github/workflows/platform-smoke.yml',
    '.github/workflows/quality.yml',
    'package.json',
    'pnpm-lock.yaml',
    'scripts/feasibility-evidence.test.mjs',
    'scripts/verify-ci.mjs',
  ]),
  'updater-exit-barrier': withCommon(TASK_8_SCOPE),
};

const DECISION_PATHS = {
  'atomic-commit': 'docs/decisions/0004-atomic-no-clobber.md',
  'gd-contract-pagination': 'docs/decisions/0001-gd-pagination.md',
  'media-containers': 'docs/decisions/0005-media-container-allowlist.md',
  'network-policy': 'docs/decisions/0003-network-ssrf-policy.md',
  'signature-webview': 'docs/decisions/0002-signature-webview.md',
  'updater-exit-barrier': 'docs/decisions/0006-updater-exit-barrier.md',
};

function git(cwd, ...args) {
  return execFileSync('git', args, { cwd, encoding: 'utf8' }).trim();
}

async function writeRepoFile(cwd, repositoryPath, contents) {
  const target = join(cwd, ...repositoryPath.split('/'));
  await mkdir(dirname(target), { recursive: true });
  await writeFile(target, contents);
}

function platform(id, osVersion, arch) {
  return {
    id,
    osVersion,
    arch,
    command: 'node --version',
    exitCode: 0,
    runUrl: 'https://github.com/example/yinmi/actions/runs/123',
  };
}

function platformsFor(gateId) {
  if (gateId === 'gd-contract-pagination') {
    return [platform('gd-live-service', 'live service', 'x86_64')];
  }
  if (gateId === 'signature-webview') {
    return [
      platform(
        'windows-10-webview2-111-x64',
        'Windows 10 22H2 / WebView2 111.0.1661.62',
        'x86_64',
      ),
      platform(
        'windows-11-webview2-current-x64',
        'Windows 11 24H2 / WebView2 current',
        'x86_64',
      ),
      platform('macos-13.3-intel', 'macOS 13.3', 'x86_64'),
      platform('macos-current-arm', 'macOS 15', 'aarch64'),
    ];
  }
  if (gateId === 'atomic-commit') {
    return [
      platform('windows-ntfs-x64', 'Windows 11 / NTFS', 'x86_64'),
      platform('macos-apfs-intel', 'macOS 13.3 / APFS', 'x86_64'),
      platform('macos-apfs-arm', 'macOS 15 / APFS', 'aarch64'),
    ];
  }
  return [
    platform('windows-x64', 'Windows Server 2025', 'x86_64'),
    platform('macos-intel', 'macOS 15 Intel', 'x86_64'),
    platform('macos-arm', 'macOS 15', 'aarch64'),
  ];
}

function checksFor(gateId, testedCommit) {
  switch (gateId) {
    case 'toolchain-ci':
      return {
        event: 'push',
        headSha: testedCommit,
        quality: {
          conclusion: 'success',
          headSha: testedCommit,
          runUrl: 'https://github.com/example/yinmi/actions/runs/101',
        },
        'platform-windows': {
          conclusion: 'success',
          headSha: testedCommit,
          runUrl: 'https://github.com/example/yinmi/actions/runs/102',
        },
        'platform-macos': {
          conclusion: 'success',
          headSha: testedCommit,
          runUrl: 'https://github.com/example/yinmi/actions/runs/102',
        },
      };
    case 'gd-contract-pagination':
      return {
        bodyFixtures: ['search', 'url', 'picture', 'lyric', 'page', 'error'],
        strictMixedRecordParser: true,
        rejects429: true,
        rejectsOtherNon2xx: true,
        rejectsOversizeBody: true,
        liveCases: ['search', 'url', 'metadata'],
        pageLimit: 50,
      };
    case 'signature-webview':
      return {
        runtimeModes: Object.fromEntries(
          platformsFor(gateId).map(({ id }) => [id, `recorded:${id}`]),
        ),
        filterModes: Object.fromEntries(
          platformsFor(gateId).map(({ id }) => [id, 'official-only']),
        ),
        ipcBridgeAbsent: true,
        nestedResourceCanaries: 0,
        officialOnlyOrigins: true,
        timeoutCheck: true,
        retryCheck: true,
      };
    case 'network-policy':
      return {
        allAddressSet: true,
        redirect: true,
        peerPin: true,
        bodyLimit: true,
        proxyDisabled: true,
      };
    case 'atomic-commit':
      return {
        exactlyOneWinner: true,
        overwriteCount: 0,
        leftoverCount: 0,
        cancelLinearized: true,
      };
    case 'media-containers':
      return {
        mp3RoundTrip: true,
        flacRoundTrip: true,
        negativeFamiliesRejected: ['mp2', 'truncated-id3', 'truncated-flac'],
      };
    case 'updater-exit-barrier':
      return {
        realDropFutureObserved: true,
        realBoundedWaitOnlyObserved: true,
        earlyExitObserved: false,
        earlyInstallObserved: false,
        productionTimeoutMs: 30_000,
        feedbackIntervalMs: 250,
      };
    default:
      throw new Error(`unsupported test gate: ${gateId}`);
  }
}

async function createRepository(gateId = 'toolchain-ci') {
  const cwd = await mkdtemp(join(tmpdir(), 'yinmi-evidence-'));
  git(cwd, 'init', '--initial-branch=main');
  git(cwd, 'config', 'user.name', 'Evidence Test');
  git(cwd, 'config', 'user.email', 'evidence-test@example.invalid');
  git(cwd, 'config', 'core.autocrlf', 'false');

  for (const repositoryPath of EXPECTED_SCOPES[gateId]) {
    if (repositoryPath !== 'docs/feasibility/evidence-scopes.json') {
      await writeRepoFile(cwd, repositoryPath, `fixture:${repositoryPath}\n`);
    }
  }
  await writeRepoFile(
    cwd,
    'docs/feasibility/evidence-scopes.json',
    `${JSON.stringify(EXPECTED_SCOPES, null, 2)}\n`,
  );
  git(cwd, 'add', '.');
  git(cwd, 'commit', '-m', 'test: create scoped harness');
  const testedCommit = git(cwd, 'rev-parse', 'HEAD');

  const markdownPath = `docs/feasibility/${gateId}.md`;
  await writeRepoFile(cwd, markdownPath, `# ${gateId}\n\nConclusion: pass\n`);
  const decisionPath = DECISION_PATHS[gateId];
  if (decisionPath) {
    await writeRepoFile(cwd, decisionPath, `# Decision for ${gateId}\n`);
  }

  const raw = {
    schemaVersion: 1,
    gateId,
    status: 'pass',
    designCommit: DESIGN_COMMIT,
    testedCommit,
    testedAt: '2026-07-15T12:00:00Z',
    decisions: decisionPath ? [decisionPath] : [],
    platforms: platformsFor(gateId),
    checks: checksFor(gateId, testedCommit),
  };

  return {
    cwd,
    raw,
    markdownPath,
    outputPath: `docs/feasibility/${gateId}.json`,
    decisionPath,
  };
}

async function buildFixture(gateId = 'toolchain-ci') {
  const fixture = await createRepository(gateId);
  const evidence = await buildEvidence(fixture.raw, {
    cwd: fixture.cwd,
    markdownPath: fixture.markdownPath,
    outputPath: fixture.outputPath,
  });
  return { ...fixture, evidence };
}

test('scope manifest enumerates every exact planned gate path', async () => {
  const manifest = JSON.parse(
    await readFile('docs/feasibility/evidence-scopes.json', 'utf8'),
  );
  assert.deepEqual(manifest, EXPECTED_SCOPES);
});

test('digestScope sorts paths and hashes the canonical byte stream', async () => {
  const cwd = await mkdtemp(join(tmpdir(), 'yinmi-digest-'));
  await writeRepoFile(cwd, 'z.bin', Buffer.from([0, 1, 2]));
  await writeRepoFile(cwd, 'a.txt', 'å');

  const expected = createHash('sha256')
    .update('a.txt\0')
    .update('2\0')
    .update(Buffer.from('å'))
    .update('z.bin\0')
    .update('3\0')
    .update(Buffer.from([0, 1, 2]))
    .digest('hex');

  assert.equal(await digestScope(['z.bin', 'a.txt'], { cwd }), expected);
  assert.equal(await digestScope(['a.txt', 'z.bin'], { cwd }), expected);
});

test('buildEvidence produces the exact common envelope', async () => {
  const { cwd, evidence, outputPath } = await buildFixture();
  assert.deepEqual(Object.keys(evidence), [
    'schemaVersion',
    'gateId',
    'status',
    'designCommit',
    'testedCommit',
    'testedAt',
    'scopeFiles',
    'scopeSha256',
    'markdownPath',
    'markdownSha256',
    'decisions',
    'platforms',
    'checks',
  ]);
  assert.deepEqual(evidence.scopeFiles, EXPECTED_SCOPES['toolchain-ci']);
  assert.equal(await validateEvidence(evidence, { cwd }), true);
  assert.deepEqual(
    JSON.parse(await readFile(join(cwd, outputPath), 'utf8')),
    evidence,
  );
});

test('check rejects one-byte scoped-file tampering', async () => {
  const { cwd, evidence } = await buildFixture();
  const scopedPath = join(cwd, 'scripts', 'verify-ci.mjs');
  const bytes = await readFile(scopedPath);
  bytes[0] ^= 1;
  await writeFile(scopedPath, bytes);
  await assert.rejects(
    validateEvidence(evidence, { cwd }),
    /scope|hash|changed/i,
  );
});

for (const mutation of ['omitted', 'extra', 'substituted']) {
  test(`check rejects a manifest with an ${mutation} scope path`, async () => {
    const { cwd, evidence } = await buildFixture();
    const manifestPath = join(
      cwd,
      'docs',
      'feasibility',
      'evidence-scopes.json',
    );
    const manifest = JSON.parse(await readFile(manifestPath, 'utf8'));
    const scope = manifest['toolchain-ci'];
    if (mutation === 'omitted') scope.splice(1, 1);
    if (mutation === 'extra') scope.push('unexpected/file.txt');
    if (mutation === 'substituted') scope[1] = 'substituted/file.txt';
    scope.sort();
    await writeFile(manifestPath, `${JSON.stringify(manifest, null, 2)}\n`);
    await assert.rejects(
      validateEvidence(evidence, { cwd }),
      /scope|manifest|hash/i,
    );
  });
}

test('build rejects caller-supplied scopeFiles', async () => {
  const fixture = await createRepository();
  fixture.raw.scopeFiles = ['scripts/verify-ci.mjs'];
  await assert.rejects(
    buildEvidence(fixture.raw, {
      cwd: fixture.cwd,
      markdownPath: fixture.markdownPath,
    }),
    /scopeFiles.*derived|must not supply scopeFiles/i,
  );
});

test('build rejects empty Markdown', async () => {
  const fixture = await createRepository();
  await writeRepoFile(fixture.cwd, fixture.markdownPath, '   \n');
  await assert.rejects(
    buildEvidence(fixture.raw, {
      cwd: fixture.cwd,
      markdownPath: fixture.markdownPath,
    }),
    /Markdown.*empty|nonempty Markdown/i,
  );
});

test('check rejects one-byte ADR tampering', async () => {
  const { cwd, evidence, decisionPath } = await buildFixture('atomic-commit');
  const adrPath = join(cwd, ...decisionPath.split('/'));
  const bytes = await readFile(adrPath);
  bytes[0] ^= 1;
  await writeFile(adrPath, bytes);
  await assert.rejects(
    validateEvidence(evidence, { cwd }),
    /decision|ADR|hash/i,
  );
});

test('build rejects a missing required ADR', async () => {
  const fixture = await createRepository('network-policy');
  fixture.raw.decisions = [];
  await assert.rejects(
    buildEvidence(fixture.raw, {
      cwd: fixture.cwd,
      markdownPath: fixture.markdownPath,
    }),
    /decision|ADR/i,
  );
});

test('build rejects the wrong design commit and tested commit', async (t) => {
  await t.test('wrong design commit', async () => {
    const fixture = await createRepository();
    fixture.raw.designCommit = '0'.repeat(40);
    await assert.rejects(
      buildEvidence(fixture.raw, {
        cwd: fixture.cwd,
        markdownPath: fixture.markdownPath,
      }),
      /designCommit/i,
    );
  });
  await t.test('tested commit is not HEAD', async () => {
    const fixture = await createRepository();
    fixture.raw.testedCommit = '0'.repeat(40);
    await assert.rejects(
      buildEvidence(fixture.raw, {
        cwd: fixture.cwd,
        markdownPath: fixture.markdownPath,
      }),
      /testedCommit.*HEAD/i,
    );
  });
});

test('check rejects a non-ancestor tested commit', async () => {
  const { cwd, evidence } = await buildFixture();
  const tree = git(cwd, 'write-tree');
  const unrelated = git(cwd, 'commit-tree', tree, '-m', 'unrelated commit');
  evidence.testedCommit = unrelated;
  evidence.checks.headSha = unrelated;
  for (const name of ['quality', 'platform-windows', 'platform-macos']) {
    evidence.checks[name].headSha = unrelated;
  }
  await assert.rejects(
    validateEvidence(evidence, { cwd }),
    /ancestor|testedCommit/i,
  );
});

test('build rejects a dirty scoped file', async () => {
  const fixture = await createRepository();
  await writeRepoFile(fixture.cwd, 'scripts/verify-ci.mjs', 'dirty\n');
  await assert.rejects(
    buildEvidence(fixture.raw, {
      cwd: fixture.cwd,
      markdownPath: fixture.markdownPath,
    }),
    /dirty|changed|testedCommit/i,
  );
});

test('build rejects absolute and duplicate scope paths', async (t) => {
  await t.test('absolute path', async () => {
    const fixture = await createRepository();
    const manifestPath = join(
      fixture.cwd,
      'docs',
      'feasibility',
      'evidence-scopes.json',
    );
    const manifest = JSON.parse(await readFile(manifestPath, 'utf8'));
    manifest['toolchain-ci'].push('C:/Users/example/private.txt');
    await writeFile(manifestPath, `${JSON.stringify(manifest, null, 2)}\n`);
    await assert.rejects(
      buildEvidence(fixture.raw, {
        cwd: fixture.cwd,
        markdownPath: fixture.markdownPath,
      }),
      /absolute|repository-relative/i,
    );
  });
  await t.test('duplicate path', async () => {
    const fixture = await createRepository();
    const manifestPath = join(
      fixture.cwd,
      'docs',
      'feasibility',
      'evidence-scopes.json',
    );
    const manifest = JSON.parse(await readFile(manifestPath, 'utf8'));
    manifest['toolchain-ci'].push(manifest['toolchain-ci'][0]);
    await writeFile(manifestPath, `${JSON.stringify(manifest, null, 2)}\n`);
    await assert.rejects(
      buildEvidence(fixture.raw, {
        cwd: fixture.cwd,
        markdownPath: fixture.markdownPath,
      }),
      /duplicate/i,
    );
  });
});

test('pass evidence rejects a nonzero command exit', async () => {
  const fixture = await createRepository();
  fixture.raw.platforms[0].exitCode = 1;
  await assert.rejects(
    buildEvidence(fixture.raw, {
      cwd: fixture.cwd,
      markdownPath: fixture.markdownPath,
    }),
    /exitCode.*0|command.*failed/i,
  );
});

test('build rejects duplicate and missing platform IDs', async (t) => {
  await t.test('duplicate platform ID', async () => {
    const fixture = await createRepository();
    fixture.raw.platforms[1].id = fixture.raw.platforms[0].id;
    await assert.rejects(
      buildEvidence(fixture.raw, {
        cwd: fixture.cwd,
        markdownPath: fixture.markdownPath,
      }),
      /duplicate platform id/i,
    );
  });
  await t.test('missing platform ID', async () => {
    const fixture = await createRepository();
    delete fixture.raw.platforms[0].id;
    await assert.rejects(
      buildEvidence(fixture.raw, {
        cwd: fixture.cwd,
        markdownPath: fixture.markdownPath,
      }),
      /platform.*id/i,
    );
  });
});

test('build rejects absolute paths and private-key material anywhere in raw input', async (t) => {
  await t.test('absolute local path', async () => {
    const fixture = await createRepository();
    fixture.raw.platforms[0].command =
      'node C:\\Users\\example\\private\\probe.mjs';
    await assert.rejects(
      buildEvidence(fixture.raw, {
        cwd: fixture.cwd,
        markdownPath: fixture.markdownPath,
      }),
      /absolute path|local path/i,
    );
  });
  await t.test('private key text', async () => {
    const fixture = await createRepository();
    fixture.raw.checks.note =
      '-----BEGIN OPENSSH PRIVATE KEY----- secret material';
    await assert.rejects(
      buildEvidence(fixture.raw, {
        cwd: fixture.cwd,
        markdownPath: fixture.markdownPath,
      }),
      /private key|secret/i,
    );
  });
});

const INVALID_GATE_CHECK = {
  'gd-contract-pagination': (raw) => {
    raw.checks.pageLimit = 51;
  },
  'signature-webview': (raw) => {
    raw.checks.ipcBridgeAbsent = false;
  },
  'network-policy': (raw) => {
    raw.checks.peerPin = false;
  },
  'atomic-commit': (raw) => {
    raw.checks.overwriteCount = 1;
  },
  'media-containers': (raw) => {
    raw.checks.flacRoundTrip = false;
  },
  'updater-exit-barrier': (raw) => {
    raw.checks.earlyExitObserved = true;
  },
};

for (const [gateId, invalidate] of Object.entries(INVALID_GATE_CHECK)) {
  test(`${gateId} validates its required machine fields`, async () => {
    const fixture = await createRepository(gateId);
    invalidate(fixture.raw);
    await assert.rejects(
      buildEvidence(fixture.raw, {
        cwd: fixture.cwd,
        markdownPath: fixture.markdownPath,
      }),
      /checks|requires|must/i,
    );
  });
}
