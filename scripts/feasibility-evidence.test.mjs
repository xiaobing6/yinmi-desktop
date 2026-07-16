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

const SCHEMA_VERSION = 2;
const DESIGN_COMMIT = '782b30d8eb1075cce708ddef878cd236d2fa7dc2';
const COMMON_SCOPE = [
  'docs/feasibility/evidence-scopes.json',
  'docs/feasibility/evidence.schema.json',
  'scripts/feasibility-evidence.mjs',
];
const MEDIA_NEGATIVE_FAMILIES = [
  'mp2',
  'aac',
  'mp4',
  'ogg',
  'opus',
  'wav',
  'truncated',
];
const SIGNATURE_PLATFORM_IDS = [
  'windows-10-webview2-111-x64',
  'windows-11-x64',
  'macos-13-intel',
  'macos-current-arm64',
];
const RESOURCE_VECTORS = [
  'document',
  'iframe',
  'script',
  'style',
  'image',
  'media',
  'fetch',
  'xhr',
  'worker',
  'service_worker',
  'websocket',
  'sse',
  'beacon',
  'redirect',
  'popup',
  'download',
  'top_level_data',
  'top_level_blob',
  'top_level_file',
  'top_level_custom_protocol',
];
const RESOURCE_REQUEST_VECTORS = new Set(RESOURCE_VECTORS.slice(0, 14));
const SIGNATURE_TRUE_CHECKS = [
  'rawWryHost',
  'tauriGlobalsAbsent',
  'applicationInitializationScriptsAbsent',
  'applicationIpcHandlerAbsent',
  'inertWryShimPresent',
  'hiddenIpcCanaryDeltaZero',
  'hiddenIpcProducedNoResponse',
  'appStateUnchanged',
  'capabilityMatchAbsent',
  'policyInstalledBeforeFirstNetworkNavigation',
  'officialFinishedBeforePolling',
  'officialOnlyOrigins',
  'storageNonPersistent',
  'timeoutCheck',
  'retryCheck',
  'policyFaultInvalidatesInstance',
  'lateCallbackIsolated',
  'destroyConfirmedBeforeRetry',
  'resourcePolicyCleanupAcknowledged',
  'policyTombstonesEmptyBeforeExit',
  'lifecycleNoMonotonicGrowth',
  'noOrphanHostWindows',
  'visibleWindowLeakAbsent',
  'unexpectedActivationAbsent',
  'ordinaryExitCleanupAcknowledged',
];
const SIGNATURE_FALSE_CHECKS = [
  'usesTauriManagedWebView',
  'newInstanceStorageRecovered',
  'restartStorageRecovered',
];
const SEARCH_DEFAULTS_AND_BOUNDS = {
  defaultInternalCode: 'netease_music',
  defaultDisplayName: '网易云音乐',
  defaultWireValue: 'netease',
  defaultCount: 20,
  minimumCount: 1,
  maximumCount: 1000,
  boundaryTestsPassed: true,
  singleCount1000RequestedCount: 1000,
  singleCount1000ApiRequests: 1,
};
const SEARCH_CONTRACT_TEST_RESULT = {
  defaultInternalCode: 'netease_music',
  defaultDisplayName: '网易云音乐',
  defaultWireValue: 'netease',
  defaultCount: 20,
  minimumCount: 1,
  maximumCount: 1000,
  boundaryTestsPassed: true,
};
const SINGLE_COUNT_1000_LIVE_CASE = {
  requestedCount: 1000,
  apiRequests: 1,
};

const withCommon = (paths) => [...new Set([...COMMON_SCOPE, ...paths])].sort();

const TASK_4_SCOPE = [
  'package.json',
  'pnpm-lock.yaml',
  'scripts/verify-signature-host.mjs',
  'scripts/verify-signature-host.test.mjs',
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
  'src-tauri/src/feasibility/signature_host.rs',
  'src-tauri/src/feasibility/signature_probe.rs',
  'src-tauri/src/feasibility/signature_webview.rs',
  'src-tauri/src/feasibility/webview_resource_policy.rs',
  'src-tauri/src/feasibility/webview_resource_policy/macos.rs',
  'src-tauri/src/feasibility/webview_resource_policy/windows.rs',
  'src-tauri/src/lib.rs',
  'src-tauri/tauri.conf.json',
  'src-tauri/tauri.feasibility.conf.json',
  'vite.config.ts',
];

const TASK_3_GD_SCOPE = [
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
    ...TASK_3_GD_SCOPE,
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
  'signature-webview': withCommon([
    ...TASK_4_SCOPE,
    ...TASK_8_SCOPE,
    ...TASK_3_GD_SCOPE,
  ]),
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
      platform('windows-10-webview2-111-x64', '10.0.19045', 'x86_64'),
      platform('windows-11-x64', '10.0.26100', 'x86_64'),
      platform('macos-13-intel', '13.3', 'x86_64'),
      platform('macos-current-arm64', '15.5', 'aarch64'),
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

function expectedBarrier(platformId, vector) {
  if (RESOURCE_REQUEST_VECTORS.has(vector)) {
    return platformId.startsWith('windows-')
      ? 'webview2-web-resource-requested'
      : 'wk-content-rule-list';
  }
  if (vector === 'popup') return 'new-window-handler';
  if (vector === 'download') return 'download-handler';
  return 'navigation-handler';
}

function resourceVectorResult(platformId, vector) {
  const isWindows = platformId.startsWith('windows-');
  const resourceRequest = RESOURCE_REQUEST_VECTORS.has(vector);
  return {
    runtimeAttempted: true,
    availabilityOutcome: 'available',
    deterministicBarrierSeamCovered: true,
    expectedBarrier: expectedBarrier(platformId, vector),
    enforcedBarrier: expectedBarrier(platformId, vector),
    barrierEvidenceMode: resourceRequest
      ? isWindows
        ? 'native-callback'
        : 'paired-counterfactual'
      : 'handler-callback',
    counterfactualServerHits: resourceRequest && !isWindows ? 1 : null,
    allowedRedirectHopHits: vector === 'redirect' ? 2 : 0,
    serverHits: 0,
  };
}

function signaturePlatformRow(platformId) {
  const isWindows = platformId.startsWith('windows-');
  const matrix = {
    'windows-10-webview2-111-x64': {
      hostPlatform: 'win32',
      hostArch: 'x64',
      osVersion: '10.0.19045',
      binaryTargetOs: 'windows',
      binaryTargetArch: 'x86_64',
      translatedProcess: null,
      webviewRuntimeVersion: '111.0.1661.62',
      resourcePolicyMode: 'webview2-22-all-source-kinds',
      strongSourceKindsInterfaceAvailable: true,
    },
    'windows-11-x64': {
      hostPlatform: 'win32',
      hostArch: 'x64',
      osVersion: '10.0.26100',
      binaryTargetOs: 'windows',
      binaryTargetArch: 'x86_64',
      translatedProcess: null,
      webviewRuntimeVersion: '138.0.3351.121',
      resourcePolicyMode: 'webview2-22-all-source-kinds',
      strongSourceKindsInterfaceAvailable: true,
    },
    'macos-13-intel': {
      hostPlatform: 'darwin',
      hostArch: 'x64',
      osVersion: '13.3',
      binaryTargetOs: 'macos',
      binaryTargetArch: 'x86_64',
      translatedProcess: false,
      webviewRuntimeVersion: '616.1.17',
      resourcePolicyMode: 'wk-content-rule-list-exact-origin',
      strongSourceKindsInterfaceAvailable: null,
    },
    'macos-current-arm64': {
      hostPlatform: 'darwin',
      hostArch: 'arm64',
      osVersion: '15.5',
      binaryTargetOs: 'macos',
      binaryTargetArch: 'aarch64',
      translatedProcess: false,
      webviewRuntimeVersion: '620.4.2',
      resourcePolicyMode: 'wk-content-rule-list-exact-origin',
      strongSourceKindsInterfaceAvailable: null,
    },
  }[platformId];
  return {
    ...matrix,
    runtimeMode: 'native-host-raw-wry-0.55.1',
    ...Object.fromEntries(SIGNATURE_TRUE_CHECKS.map((key) => [key, true])),
    ...Object.fromEntries(SIGNATURE_FALSE_CHECKS.map((key) => [key, false])),
    crossOriginCanaryServerHits: 0,
    blockedCanaryAttempts: isWindows ? 0 : null,
    resourceVectorResults: Object.fromEntries(
      RESOURCE_VECTORS.map((vector) => [
        vector,
        resourceVectorResult(platformId, vector),
      ]),
    ),
  };
}

function signatureChecks() {
  const byPlatform = Object.fromEntries(
    SIGNATURE_PLATFORM_IDS.map((id) => [id, signaturePlatformRow(id)]),
  );
  return {
    runtimeModes: Object.fromEntries(
      SIGNATURE_PLATFORM_IDS.map((id) => [id, byPlatform[id].runtimeMode]),
    ),
    resourcePolicyModes: Object.fromEntries(
      SIGNATURE_PLATFORM_IDS.map((id) => [
        id,
        byPlatform[id].resourcePolicyMode,
      ]),
    ),
    webviewRuntimeVersions: Object.fromEntries(
      SIGNATURE_PLATFORM_IDS.map((id) => [
        id,
        byPlatform[id].webviewRuntimeVersion,
      ]),
    ),
    resourceVectorsCovered: [...RESOURCE_VECTORS],
    ...Object.fromEntries(SIGNATURE_TRUE_CHECKS.map((key) => [key, true])),
    ...Object.fromEntries(SIGNATURE_FALSE_CHECKS.map((key) => [key, false])),
    crossOriginCanaryServerHits: 0,
    byPlatform,
  };
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
        searchContractTestResult: { ...SEARCH_CONTRACT_TEST_RESULT },
        singleCount1000LiveCase: { ...SINGLE_COUNT_1000_LIVE_CASE },
      };
    case 'signature-webview':
      return signatureChecks();
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
        negativeFamiliesRejected: [...MEDIA_NEGATIVE_FAMILIES],
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
    schemaVersion: SCHEMA_VERSION,
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

async function assertMutationRejected(
  fixture,
  mutate,
  pattern = /checks|must|requires/i,
) {
  const raw = structuredClone(fixture.raw);
  mutate(raw);
  await assert.rejects(
    buildEvidence(raw, {
      cwd: fixture.cwd,
      markdownPath: fixture.markdownPath,
    }),
    pattern,
  );
}

test('scope manifest enumerates every exact planned gate path', async () => {
  const manifest = JSON.parse(
    await readFile('docs/feasibility/evidence-scopes.json', 'utf8'),
  );
  assert.deepEqual(manifest, EXPECTED_SCOPES);
});

test('schema declares the v2 identity and closed GD/signature contracts', async () => {
  const schema = JSON.parse(
    await readFile('docs/feasibility/evidence.schema.json', 'utf8'),
  );
  assert.equal(schema.additionalProperties, false);
  assert.equal(schema.properties.schemaVersion.const, SCHEMA_VERSION);
  assert.equal(schema.properties.designCommit.const, DESIGN_COMMIT);
  for (const definition of [
    'gdChecks',
    'searchDefaultsAndBounds',
    'platformRuntimeModes',
    'platformResourcePolicyModes',
    'platformRuntimeVersions',
    'signatureChecks',
    'signatureByPlatform',
    'signaturePlatformRow',
    'resourceVectorResult',
    'resourceVectorResults',
  ]) {
    assert.equal(
      schema.$defs[definition].additionalProperties,
      false,
      `${definition} must reject extra fields`,
    );
  }
  assert.deepEqual(
    Object.fromEntries(
      schema.allOf.map((branch) => [
        branch.if.properties.gateId.const,
        branch.then.properties.checks.$ref,
      ]),
    ),
    {
      'gd-contract-pagination': '#/$defs/gdChecks',
      'signature-webview': '#/$defs/signatureChecks',
    },
  );
  assert.equal(
    JSON.stringify(schema).includes('ipcBridgeAbsent'),
    false,
    'the v2 schema must not preserve the legacy ipcBridgeAbsent key',
  );
  const exactVersionPattern = new RegExp(schema.$defs.exactVersion.pattern);
  for (const placeholder of [
    'current',
    'CURRENT',
    ' recorded:anything ',
    'unknown',
    ' unavailable ',
  ]) {
    assert.equal(
      exactVersionPattern.test(placeholder),
      false,
      `exactVersion must reject ${JSON.stringify(placeholder)}`,
    );
  }
  assert.equal(exactVersionPattern.test('620.4.2'), true);
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
  assert.equal(evidence.schemaVersion, SCHEMA_VERSION);
  assert.equal(evidence.designCommit, DESIGN_COMMIT);
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

for (const gateId of ['gd-contract-pagination', 'signature-webview']) {
  for (const mutation of ['omitted', 'extra', 'substituted']) {
    test(`${gateId} rejects an ${mutation} final scope path`, async () => {
      const { cwd, evidence } = await buildFixture(gateId);
      const manifestPath = join(
        cwd,
        'docs',
        'feasibility',
        'evidence-scopes.json',
      );
      const manifest = JSON.parse(await readFile(manifestPath, 'utf8'));
      const scope = manifest[gateId];
      const verifierIndex = scope.indexOf('scripts/verify-signature-host.mjs');
      assert.notEqual(verifierIndex, -1);
      if (mutation === 'omitted') scope.splice(verifierIndex, 1);
      if (mutation === 'extra') scope.push('unexpected/signature-source.rs');
      if (mutation === 'substituted') {
        scope[verifierIndex] = 'scripts/substituted-signature-host.mjs';
      }
      scope.sort();
      await writeFile(manifestPath, `${JSON.stringify(manifest, null, 2)}\n`);
      await assert.rejects(
        validateEvidence(evidence, { cwd }),
        /scope|manifest|hash/i,
      );
    });
  }
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

test('build rejects the wrong schema, design commit, and tested commit', async (t) => {
  for (const wrongVersion of [1, '2']) {
    await t.test(`schemaVersion ${JSON.stringify(wrongVersion)}`, async () => {
      const fixture = await createRepository();
      fixture.raw.schemaVersion = wrongVersion;
      await assert.rejects(
        buildEvidence(fixture.raw, {
          cwd: fixture.cwd,
          markdownPath: fixture.markdownPath,
        }),
        /schemaVersion.*2/i,
      );
    });
  }
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

test('build rejects an absolute local path hidden in an object key', async () => {
  const fixture = await createRepository();
  fixture.raw.checks['C:\\Users\\example\\private\\probe.mjs'] = true;
  await assert.rejects(
    buildEvidence(fixture.raw, {
      cwd: fixture.cwd,
      markdownPath: fixture.markdownPath,
    }),
    /absolute path|local path/i,
  );
});

test('build rejects private-key material hidden in an object key', async () => {
  const fixture = await createRepository();
  fixture.raw.checks['-----BEGIN OPENSSH PRIVATE KEY-----'] = true;
  await assert.rejects(
    buildEvidence(fixture.raw, {
      cwd: fixture.cwd,
      markdownPath: fixture.markdownPath,
    }),
    /private key|secret/i,
  );
});

test('media-containers rejects truncated aliases in place of the planned family', async () => {
  const fixture = await createRepository('media-containers');
  fixture.raw.checks.negativeFamiliesRejected = [
    'mp2',
    'aac',
    'mp4',
    'ogg',
    'opus',
    'wav',
    'truncated-id3',
    'truncated-flac',
  ];
  await assert.rejects(
    buildEvidence(fixture.raw, {
      cwd: fixture.cwd,
      markdownPath: fixture.markdownPath,
    }),
    /negativeFamiliesRejected|negative famil|reject/i,
  );
});

test('media-containers accepts the exact planned negative-family set', async () => {
  const fixture = await createRepository('media-containers');
  const evidence = await buildEvidence(fixture.raw, {
    cwd: fixture.cwd,
    markdownPath: fixture.markdownPath,
  });
  assert.deepEqual(
    evidence.checks.negativeFamiliesRejected,
    MEDIA_NEGATIVE_FAMILIES,
  );
});

test('media-containers rejects a string masquerading as negative families', async () => {
  const fixture = await createRepository('media-containers');
  fixture.raw.checks.negativeFamiliesRejected =
    'mp2,truncated-id3,truncated-flac';
  await assert.rejects(
    buildEvidence(fixture.raw, {
      cwd: fixture.cwd,
      markdownPath: fixture.markdownPath,
    }),
    /negativeFamiliesRejected|array|unique strings/i,
  );
});

test('media-containers rejects duplicate negative families', async () => {
  const fixture = await createRepository('media-containers');
  fixture.raw.checks.negativeFamiliesRejected = [
    'mp2',
    'truncated-id3',
    'truncated-flac',
    'mp2',
  ];
  await assert.rejects(
    buildEvidence(fixture.raw, {
      cwd: fixture.cwd,
      markdownPath: fixture.markdownPath,
    }),
    /negativeFamiliesRejected|duplicate|unique strings/i,
  );
});

test('gd-contract-pagination accepts the exact search defaults and bounds', async () => {
  const { cwd, raw, evidence } = await buildFixture('gd-contract-pagination');
  assert.equal(Object.hasOwn(raw.checks, 'searchDefaultsAndBounds'), false);
  assert.deepEqual(
    evidence.checks.searchDefaultsAndBounds,
    SEARCH_DEFAULTS_AND_BOUNDS,
  );
  assert.equal(await validateEvidence(evidence, { cwd }), true);
});

test('gd-contract-pagination rejects incomplete or extended search defaults', async (t) => {
  const fixture = await createRepository('gd-contract-pagination');
  await t.test('missing typed-contract result', async () => {
    await assertMutationRejected(fixture, (raw) => {
      delete raw.checks.searchContractTestResult;
    });
  });
  await t.test('missing typed-contract result key', async () => {
    await assertMutationRejected(fixture, (raw) => {
      delete raw.checks.searchContractTestResult.defaultInternalCode;
    });
  });
  await t.test('extra typed-contract result key', async () => {
    await assertMutationRejected(fixture, (raw) => {
      raw.checks.searchContractTestResult.unexpected = true;
    });
  });
  await t.test('missing count-1000 live case', async () => {
    await assertMutationRejected(fixture, (raw) => {
      delete raw.checks.singleCount1000LiveCase;
    });
  });
  await t.test('extra count-1000 live-case key', async () => {
    await assertMutationRejected(fixture, (raw) => {
      raw.checks.singleCount1000LiveCase.unexpected = true;
    });
  });
  await t.test('raw input cannot override the derived object', async () => {
    await assertMutationRejected(fixture, (raw) => {
      raw.checks.searchDefaultsAndBounds = { ...SEARCH_DEFAULTS_AND_BOUNDS };
    });
  });
  await t.test('extra checks key', async () => {
    await assertMutationRejected(fixture, (raw) => {
      raw.checks.unexpected = true;
    });
  });
});

test('gd-contract-pagination rejects every default/count/live-request mismatch', async (t) => {
  const fixture = await createRepository('gd-contract-pagination');
  const mismatches = {
    defaultInternalCode: 'netease',
    defaultDisplayName: 'Netease',
    defaultWireValue: 'netease_music',
    defaultCount: 19,
    minimumCount: 0,
    maximumCount: 1001,
    boundaryTestsPassed: false,
    singleCount1000RequestedCount: 999,
    singleCount1000ApiRequests: 2,
  };
  for (const [field, value] of Object.entries(mismatches)) {
    await t.test(field, async () => {
      await assertMutationRejected(fixture, (raw) => {
        if (Object.hasOwn(SEARCH_CONTRACT_TEST_RESULT, field)) {
          raw.checks.searchContractTestResult[field] = value;
        } else if (field === 'singleCount1000RequestedCount') {
          raw.checks.singleCount1000LiveCase.requestedCount = value;
        } else {
          raw.checks.singleCount1000LiveCase.apiRequests = value;
        }
      });
    });
  }
  await t.test('wrong typed-contract result type', async () => {
    await assertMutationRejected(fixture, (raw) => {
      raw.checks.searchContractTestResult = 'passed';
    });
  });
  await t.test('wrong typed-contract field type', async () => {
    await assertMutationRejected(fixture, (raw) => {
      raw.checks.searchContractTestResult.defaultCount = '20';
    });
  });
  await t.test('wrong count-1000 live-case type', async () => {
    await assertMutationRejected(fixture, (raw) => {
      raw.checks.singleCount1000LiveCase = 'one request';
    });
  });
  await t.test('wrong count-1000 live field type', async () => {
    await assertMutationRejected(fixture, (raw) => {
      raw.checks.singleCount1000LiveCase.apiRequests = '1';
    });
  });
});

test('gd-contract-pagination companion accepts only the derived final object', async (t) => {
  const { cwd, evidence } = await buildFixture('gd-contract-pagination');
  const cases = [
    [
      'missing derived object',
      (candidate) => delete candidate.checks.searchDefaultsAndBounds,
    ],
    [
      'missing derived field',
      (candidate) =>
        delete candidate.checks.searchDefaultsAndBounds.defaultInternalCode,
    ],
    [
      'extra derived field',
      (candidate) => {
        candidate.checks.searchDefaultsAndBounds.unexpected = true;
      },
    ],
    [
      'wrong derived field type',
      (candidate) => {
        candidate.checks.searchDefaultsAndBounds.defaultCount = '20';
      },
    ],
    [
      'raw-only typed-contract result',
      (candidate) => {
        candidate.checks.searchContractTestResult = {
          ...SEARCH_CONTRACT_TEST_RESULT,
        };
      },
    ],
    [
      'raw-only live case',
      (candidate) => {
        candidate.checks.singleCount1000LiveCase = {
          ...SINGLE_COUNT_1000_LIVE_CASE,
        };
      },
    ],
  ];
  for (const [name, mutate] of cases) {
    await t.test(name, async () => {
      const candidate = structuredClone(evidence);
      mutate(candidate);
      await assert.rejects(
        validateEvidence(candidate, { cwd }),
        /checks|must/i,
      );
    });
  }
});

test('signature-webview accepts the exact canonical Task 10 platform IDs', async () => {
  const { cwd, evidence } = await buildFixture('signature-webview');
  assert.deepEqual(
    evidence.platforms.map(({ id }) => id).sort(),
    [...SIGNATURE_PLATFORM_IDS].sort(),
  );
  assert.deepEqual(
    Object.keys(evidence.checks.byPlatform).sort(),
    [...SIGNATURE_PLATFORM_IDS].sort(),
  );
  assert.deepEqual(evidence.checks.resourceVectorsCovered, RESOURCE_VECTORS);
  assert.equal(await validateEvidence(evidence, { cwd }), true);
});

test('signature-webview rejects legacy platform ID aliases', async () => {
  const fixture = await createRepository('signature-webview');
  fixture.raw.platforms[1].id = 'windows-11-webview2-current-x64';
  fixture.raw.platforms[2].id = 'macos-13.3-intel';
  fixture.raw.platforms[3].id = 'macos-current-arm';
  await assert.rejects(
    buildEvidence(fixture.raw, {
      cwd: fixture.cwd,
      markdownPath: fixture.markdownPath,
    }),
    /signature-webview platforms|required.*platform|platform.*match/i,
  );
});

test('signature-webview rejects legacy and wrongly-valued top-level checks', async (t) => {
  const fixture = await createRepository('signature-webview');
  await t.test('legacy ipcBridgeAbsent key', async () => {
    await assertMutationRejected(fixture, (raw) => {
      raw.checks.ipcBridgeAbsent = true;
    });
  });
  await t.test('extra key', async () => {
    await assertMutationRejected(fixture, (raw) => {
      raw.checks.unexpected = true;
    });
  });
  await t.test('missing key', async () => {
    await assertMutationRejected(fixture, (raw) => {
      delete raw.checks.rawWryHost;
    });
  });
  for (const key of SIGNATURE_TRUE_CHECKS) {
    await t.test(`${key} must be true`, async () => {
      await assertMutationRejected(fixture, (raw) => {
        raw.checks[key] = false;
      });
    });
  }
  for (const key of SIGNATURE_FALSE_CHECKS) {
    await t.test(`${key} must be false`, async () => {
      await assertMutationRejected(fixture, (raw) => {
        raw.checks[key] = true;
      });
    });
  }
  await t.test('true check rejects a string', async () => {
    await assertMutationRejected(fixture, (raw) => {
      raw.checks.rawWryHost = 'true';
    });
  });
  await t.test('false check rejects zero', async () => {
    await assertMutationRejected(fixture, (raw) => {
      raw.checks.usesTauriManagedWebView = 0;
    });
  });
  await t.test('top-level canary total must be numeric zero', async () => {
    await assertMutationRejected(fixture, (raw) => {
      raw.checks.crossOriginCanaryServerHits = '0';
    });
  });
});

test('signature-webview requires exact four-key derived maps', async (t) => {
  const fixture = await createRepository('signature-webview');
  for (const field of [
    'runtimeModes',
    'resourcePolicyModes',
    'webviewRuntimeVersions',
  ]) {
    await t.test(`${field} missing platform`, async () => {
      await assertMutationRejected(fixture, (raw) => {
        delete raw.checks[field]['macos-current-arm64'];
      });
    });
    await t.test(`${field} extra platform`, async () => {
      await assertMutationRejected(fixture, (raw) => {
        raw.checks[field].unexpected = 'value';
      });
    });
  }
  await t.test('runtime mode is fixed', async () => {
    await assertMutationRejected(fixture, (raw) => {
      raw.checks.runtimeModes['windows-11-x64'] = 'tauri-managed';
    });
  });
  await t.test('resource policy mode belongs to the fixed enum', async () => {
    await assertMutationRejected(fixture, (raw) => {
      raw.checks.resourcePolicyModes['windows-11-x64'] = 'official-only';
    });
  });
  for (const value of ['', 'CURRENT', ' recorded:anything ', ' unavailable ']) {
    await t.test(
      `runtime version rejects ${JSON.stringify(value)}`,
      async () => {
        await assertMutationRejected(fixture, (raw) => {
          raw.checks.webviewRuntimeVersions['windows-11-x64'] = value;
          raw.checks.byPlatform['windows-11-x64'].webviewRuntimeVersion = value;
        });
      },
    );
  }
});

test('signature-webview enforces the host/child platform matrix and modes', async (t) => {
  const fixture = await createRepository('signature-webview');
  const row = (raw, id) => raw.checks.byPlatform[id];
  const cases = [
    [
      'missing platform row',
      (raw) => delete raw.checks.byPlatform['macos-current-arm64'],
    ],
    [
      'extra platform row',
      (raw) => {
        raw.checks.byPlatform.unexpected = structuredClone(
          raw.checks.byPlatform['macos-current-arm64'],
        );
      },
    ],
    [
      'missing row key',
      (raw) => delete row(raw, 'windows-11-x64').hostPlatform,
    ],
    [
      'extra row key',
      (raw) => {
        row(raw, 'windows-11-x64').unexpected = true;
      },
    ],
    [
      'wrong Windows host OS label',
      (raw) => {
        row(raw, 'windows-11-x64').hostPlatform = 'windows';
      },
    ],
    [
      'wrong Windows host architecture label',
      (raw) => {
        row(raw, 'windows-11-x64').hostArch = 'x86_64';
      },
    ],
    [
      'wrong macOS child architecture',
      (raw) => {
        row(raw, 'macos-current-arm64').binaryTargetArch = 'arm64';
      },
    ],
    [
      'wrong translated-process nullability',
      (raw) => {
        row(raw, 'macos-13-intel').translatedProcess = null;
      },
    ],
    [
      'swapped Windows/macOS mode',
      (raw) => {
        row(raw, 'macos-13-intel').resourcePolicyMode =
          'webview2-22-all-source-kinds';
        raw.checks.resourcePolicyModes['macos-13-intel'] =
          'webview2-22-all-source-kinds';
      },
    ],
    [
      'Windows 11 legacy mode',
      (raw) => {
        row(raw, 'windows-11-x64').resourcePolicyMode =
          'webview2-legacy-all-contexts-candidate';
        raw.checks.resourcePolicyModes['windows-11-x64'] =
          'webview2-legacy-all-contexts-candidate';
      },
    ],
    [
      'Windows 10 legacy mode with strong interface',
      (raw) => {
        row(raw, 'windows-10-webview2-111-x64').resourcePolicyMode =
          'webview2-legacy-all-contexts-candidate';
        raw.checks.resourcePolicyModes['windows-10-webview2-111-x64'] =
          'webview2-legacy-all-contexts-candidate';
      },
    ],
    [
      'Windows 10 v22 mode without strong interface',
      (raw) => {
        row(
          raw,
          'windows-10-webview2-111-x64',
        ).strongSourceKindsInterfaceAvailable = false;
      },
    ],
    [
      'Windows 10 wrong native OS build',
      (raw) => {
        row(raw, 'windows-10-webview2-111-x64').osVersion = '10.0.19044';
        raw.platforms[0].osVersion = '10.0.19044';
      },
    ],
    [
      'Windows 10 wrong WebView2 runtime',
      (raw) => {
        row(raw, 'windows-10-webview2-111-x64').webviewRuntimeVersion =
          '110.0.0.0';
        raw.checks.webviewRuntimeVersions['windows-10-webview2-111-x64'] =
          '110.0.0.0';
      },
    ],
    [
      'Windows 11 build below 22000',
      (raw) => {
        row(raw, 'windows-11-x64').osVersion = '10.0.19045';
        raw.platforms[1].osVersion = '10.0.19045';
      },
    ],
    [
      'macOS 13 wrong product version',
      (raw) => {
        row(raw, 'macos-13-intel').osVersion = '13.2.1';
        raw.platforms[2].osVersion = '13.2.1';
      },
    ],
    [
      'macOS current version below 13.3',
      (raw) => {
        row(raw, 'macos-current-arm64').osVersion = '13.2';
        raw.platforms[3].osVersion = '13.2';
      },
    ],
  ];
  for (const [name, mutate] of cases) {
    await t.test(name, async () => {
      await assertMutationRejected(fixture, mutate);
    });
  }
});

test('signature-webview requires exact row predicates and derived values', async (t) => {
  const fixture = await createRepository('signature-webview');
  const cases = [
    [
      'row true predicate',
      (raw) => {
        raw.checks.byPlatform['windows-11-x64'].rawWryHost = false;
      },
    ],
    [
      'row false predicate',
      (raw) => {
        raw.checks.byPlatform['windows-11-x64'].usesTauriManagedWebView = true;
      },
    ],
    [
      'row zero predicate type',
      (raw) => {
        raw.checks.byPlatform['windows-11-x64'].crossOriginCanaryServerHits =
          '0';
      },
    ],
    [
      'Windows blocked-canary counter is nonnegative',
      (raw) => {
        raw.checks.byPlatform['windows-11-x64'].blockedCanaryAttempts = -1;
      },
    ],
    [
      'macOS blocked-canary counter is null',
      (raw) => {
        raw.checks.byPlatform['macos-13-intel'].blockedCanaryAttempts = 0;
      },
    ],
    [
      'runtime map is derived from rows',
      (raw) => {
        raw.checks.runtimeModes['windows-11-x64'] =
          'native-host-raw-wry-0.55.1-other';
      },
    ],
    [
      'policy map is derived from rows',
      (raw) => {
        raw.checks.resourcePolicyModes['windows-11-x64'] =
          'wk-content-rule-list-exact-origin';
      },
    ],
    [
      'runtime-version map is derived from rows',
      (raw) => {
        raw.checks.webviewRuntimeVersions['windows-11-x64'] = 'different';
      },
    ],
    [
      'outer platform OS version matches row',
      (raw) => {
        raw.platforms[1].osVersion = '10.0.22631';
      },
    ],
    [
      'outer platform architecture matches child target',
      (raw) => {
        raw.platforms[3].arch = 'x86_64';
      },
    ],
  ];
  for (const [name, mutate] of cases) {
    await t.test(name, async () => {
      await assertMutationRejected(fixture, mutate);
    });
  }
});

test('signature-webview accepts only the structurally-proven service-worker absent outcome', async (t) => {
  const fixture = await createRepository('signature-webview');
  const result =
    fixture.raw.checks.byPlatform['macos-13-intel'].resourceVectorResults
      .service_worker;
  result.availabilityOutcome = 'service-worker-api-absent';
  result.barrierEvidenceMode = 'deterministic-seam-only';
  result.counterfactualServerHits = null;
  const evidence = await buildEvidence(fixture.raw, {
    cwd: fixture.cwd,
    markdownPath: fixture.markdownPath,
  });
  assert.equal(
    evidence.checks.byPlatform['macos-13-intel'].resourceVectorResults
      .service_worker.availabilityOutcome,
    'service-worker-api-absent',
  );

  const resultFor = (raw) =>
    raw.checks.byPlatform['macos-13-intel'].resourceVectorResults
      .service_worker;
  const invalidCases = [
    [
      'runtime was not attempted',
      (raw) => {
        resultFor(raw).runtimeAttempted = false;
      },
    ],
    [
      'deterministic seam was not covered',
      (raw) => {
        resultFor(raw).deterministicBarrierSeamCovered = false;
      },
    ],
    [
      'barrier mismatch',
      (raw) => {
        resultFor(raw).enforcedBarrier = 'navigation-handler';
      },
    ],
    [
      'runtime interception claim',
      (raw) => {
        resultFor(raw).barrierEvidenceMode = 'paired-counterfactual';
      },
    ],
    [
      'counterfactual claim',
      (raw) => {
        resultFor(raw).counterfactualServerHits = 1;
      },
    ],
    [
      'protected server hit',
      (raw) => {
        resultFor(raw).serverHits = 1;
      },
    ],
    [
      'invented runtime-expression evidence field',
      (raw) => {
        resultFor(raw).featureDetectionExpression =
          '!("serviceWorker" in navigator)';
      },
    ],
  ];
  for (const [name, mutate] of invalidCases) {
    await t.test(name, async () => {
      const raw = structuredClone(fixture.raw);
      const absent = resultFor(raw);
      absent.availabilityOutcome = 'service-worker-api-absent';
      absent.barrierEvidenceMode = 'deterministic-seam-only';
      absent.counterfactualServerHits = null;
      mutate(raw);
      await assert.rejects(
        buildEvidence(raw, {
          cwd: fixture.cwd,
          markdownPath: fixture.markdownPath,
        }),
        /resourceVectorResults|service.worker|must|exactly/i,
      );
    });
  }
});

test('signature-webview requires the exact unique resource vector set', async (t) => {
  const fixture = await createRepository('signature-webview');
  const results = (raw, id = 'windows-11-x64') =>
    raw.checks.byPlatform[id].resourceVectorResults;
  const cases = [
    [
      'covered set missing vector',
      (raw) => raw.checks.resourceVectorsCovered.pop(),
    ],
    [
      'covered set extra vector',
      (raw) => raw.checks.resourceVectorsCovered.push('plugin'),
    ],
    [
      'covered set duplicate vector',
      (raw) => raw.checks.resourceVectorsCovered.push('document'),
    ],
    [
      'covered set wrong type',
      (raw) => {
        raw.checks.resourceVectorsCovered = 'document';
      },
    ],
    ['platform vector missing', (raw) => delete results(raw).document],
    [
      'platform vector extra',
      (raw) => {
        results(raw).plugin = structuredClone(results(raw).document);
      },
    ],
    [
      'platform vector substituted',
      (raw) => {
        results(raw).plugin = results(raw).document;
        delete results(raw).document;
      },
    ],
    [
      'rows disagree on vector keys',
      (raw) => delete results(raw, 'macos-13-intel').iframe,
    ],
  ];
  for (const [name, mutate] of cases) {
    await t.test(name, async () => {
      await assertMutationRejected(fixture, mutate);
    });
  }
});

test('signature-webview enforces exact per-vector result objects', async (t) => {
  const fixture = await createRepository('signature-webview');
  const result = (raw, id = 'windows-11-x64', vector = 'document') =>
    raw.checks.byPlatform[id].resourceVectorResults[vector];
  const cases = [
    ['missing result key', (raw) => delete result(raw).runtimeAttempted],
    [
      'extra result key',
      (raw) => {
        result(raw).unexpected = true;
      },
    ],
    [
      'runtime was not attempted',
      (raw) => {
        result(raw).runtimeAttempted = false;
      },
    ],
    [
      'wrong runtime-attempted type',
      (raw) => {
        result(raw).runtimeAttempted = 'true';
      },
    ],
    [
      'deterministic seam missing',
      (raw) => {
        result(raw).deterministicBarrierSeamCovered = false;
      },
    ],
    [
      'non-service-worker unavailable',
      (raw) => {
        result(raw).availabilityOutcome = 'service-worker-api-absent';
      },
    ],
    [
      'probe error mislabeled unavailable',
      (raw) => {
        result(raw).availabilityOutcome = 'probe-error';
      },
    ],
    [
      'wrong expected barrier',
      (raw) => {
        result(raw).expectedBarrier = 'navigation-handler';
        result(raw).enforcedBarrier = 'navigation-handler';
      },
    ],
    [
      'enforced barrier differs',
      (raw) => {
        result(raw).enforcedBarrier = 'navigation-handler';
      },
    ],
    [
      'wrong Windows evidence mode',
      (raw) => {
        result(raw).barrierEvidenceMode = 'paired-counterfactual';
      },
    ],
    [
      'missing macOS counterfactual',
      (raw) => {
        result(raw, 'macos-13-intel').counterfactualServerHits = null;
      },
    ],
    [
      'nonpositive macOS counterfactual',
      (raw) => {
        result(raw, 'macos-13-intel').counterfactualServerHits = 0;
      },
    ],
    [
      'counterfactual on Windows',
      (raw) => {
        result(raw).counterfactualServerHits = 1;
      },
    ],
    [
      'wrong redirect hop count',
      (raw) => {
        result(raw, 'windows-11-x64', 'redirect').allowedRedirectHopHits = 1;
      },
    ],
    [
      'non-redirect hop count',
      (raw) => {
        result(raw).allowedRedirectHopHits = 1;
      },
    ],
    [
      'protected server hit',
      (raw) => {
        result(raw).serverHits = 1;
      },
    ],
    [
      'row canary total disagrees with vectors',
      (raw) => {
        raw.checks.byPlatform['windows-11-x64'].crossOriginCanaryServerHits = 1;
      },
    ],
    [
      'top-level canary total is nonzero',
      (raw) => {
        raw.checks.crossOriginCanaryServerHits = 1;
      },
    ],
  ];
  for (const [name, mutate] of cases) {
    await t.test(name, async () => {
      await assertMutationRejected(fixture, mutate);
    });
  }
});

const INVALID_GATE_CHECK = {
  'gd-contract-pagination': (raw) => {
    raw.checks.pageLimit = 51;
  },
  'signature-webview': (raw) => {
    raw.checks.rawWryHost = false;
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
