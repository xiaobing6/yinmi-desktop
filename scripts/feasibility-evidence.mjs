import { execFile } from 'node:child_process';
import { createHash, randomUUID } from 'node:crypto';
import {
  access,
  mkdir,
  readFile,
  rename,
  rm,
  writeFile,
} from 'node:fs/promises';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { promisify } from 'node:util';

const execFileAsync = promisify(execFile);

const SCHEMA_VERSION = 2;
const DESIGN_COMMIT = '782b30d8eb1075cce708ddef878cd236d2fa7dc2';
const MANIFEST_PATH = 'docs/feasibility/evidence-scopes.json';
const COMMON_SCOPE = [
  'docs/feasibility/evidence-scopes.json',
  'docs/feasibility/evidence.schema.json',
  'scripts/feasibility-evidence.mjs',
];
const COMMON_GATES = [
  'atomic-commit',
  'gd-contract-pagination',
  'media-containers',
  'network-policy',
  'signature-webview',
  'toolchain-ci',
  'updater-exit-barrier',
];
const ALL_GATES = [...COMMON_GATES, 'result-list-performance'].sort();
const DECISION_PATHS = {
  'atomic-commit': 'docs/decisions/0004-atomic-no-clobber.md',
  'gd-contract-pagination': 'docs/decisions/0001-gd-pagination.md',
  'media-containers': 'docs/decisions/0005-media-container-allowlist.md',
  'network-policy': 'docs/decisions/0003-network-ssrf-policy.md',
  'signature-webview': 'docs/decisions/0002-signature-webview.md',
  'updater-exit-barrier': 'docs/decisions/0006-updater-exit-barrier.md',
};
const ENVELOPE_KEYS = [
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
];
const RAW_KEYS = [
  'schemaVersion',
  'gateId',
  'status',
  'designCommit',
  'testedCommit',
  'testedAt',
  'decisions',
  'platforms',
  'checks',
];
const PLATFORM_KEYS = [
  'id',
  'osVersion',
  'arch',
  'command',
  'exitCode',
  'runUrl',
];
const SHA_PATTERN = /^[0-9a-f]{40}$/;
const SHA256_PATTERN = /^[0-9a-f]{64}$/;
const UTC_PATTERN = /^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(?:\.\d{1,9})?Z$/;
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
const RESOURCE_RESULT_KEYS = [
  'runtimeAttempted',
  'availabilityOutcome',
  'deterministicBarrierSeamCovered',
  'expectedBarrier',
  'enforcedBarrier',
  'barrierEvidenceMode',
  'counterfactualServerHits',
  'allowedRedirectHopHits',
  'serverHits',
];
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
const SIGNATURE_CHECK_KEYS = [
  'runtimeModes',
  'resourcePolicyModes',
  'webviewRuntimeVersions',
  'resourceVectorsCovered',
  ...SIGNATURE_TRUE_CHECKS,
  ...SIGNATURE_FALSE_CHECKS,
  'crossOriginCanaryServerHits',
  'byPlatform',
];
const SIGNATURE_PLATFORM_ROW_KEYS = [
  'hostPlatform',
  'hostArch',
  'osVersion',
  'binaryTargetOs',
  'binaryTargetArch',
  'translatedProcess',
  'webviewRuntimeVersion',
  'runtimeMode',
  'resourcePolicyMode',
  'strongSourceKindsInterfaceAvailable',
  ...SIGNATURE_TRUE_CHECKS,
  ...SIGNATURE_FALSE_CHECKS,
  'crossOriginCanaryServerHits',
  'blockedCanaryAttempts',
  'resourceVectorResults',
];
const RESOURCE_POLICY_MODES = [
  'webview2-22-all-source-kinds',
  'webview2-legacy-all-contexts-candidate',
  'wk-content-rule-list-exact-origin',
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
const GD_COMMON_CHECK_KEYS = [
  'bodyFixtures',
  'strictMixedRecordParser',
  'rejects429',
  'rejectsOtherNon2xx',
  'rejectsOversizeBody',
  'liveCases',
  'pageLimit',
];

function fail(message) {
  throw new Error(message);
}

function isRecord(value) {
  return value !== null && typeof value === 'object' && !Array.isArray(value);
}

function assertExactKeys(value, expected, label) {
  if (!isRecord(value)) fail(`${label} must be an object`);
  const actual = Object.keys(value).sort();
  const wanted = [...expected].sort();
  if (
    actual.length !== wanted.length ||
    actual.some((key, index) => key !== wanted[index])
  ) {
    fail(`${label} must contain exactly: ${wanted.join(', ')}`);
  }
}

function assertNonemptyString(value, label) {
  if (typeof value !== 'string' || value.trim() === '') {
    fail(`${label} must be a nonempty string`);
  }
}

function assertRepositoryPath(value, label) {
  assertNonemptyString(value, label);
  if (
    value.includes('\\') ||
    value.startsWith('/') ||
    /^[A-Za-z]:\//.test(value) ||
    value.startsWith('//')
  ) {
    fail(`${label} must be a repository-relative path`);
  }
  const segments = value.split('/');
  if (
    segments.some((segment) => !segment || segment === '.' || segment === '..')
  ) {
    fail(`${label} must be a normalized repository-relative path`);
  }
}

function assertPathList(value, label, { sorted = true } = {}) {
  if (!Array.isArray(value)) fail(`${label} must be an array`);
  const seen = new Set();
  for (const [index, repositoryPath] of value.entries()) {
    assertRepositoryPath(repositoryPath, `${label}[${index}]`);
    if (seen.has(repositoryPath))
      fail(`${label} contains duplicate path ${repositoryPath}`);
    seen.add(repositoryPath);
  }
  if (sorted) {
    const canonical = [...value].sort();
    if (
      canonical.some((repositoryPath, index) => repositoryPath !== value[index])
    ) {
      fail(`${label} must be sorted`);
    }
  }
}

function assertSameStrings(actual, expected, label) {
  if (
    actual.length !== expected.length ||
    actual.some((value, index) => value !== expected[index])
  ) {
    fail(`${label} does not match the scope manifest`);
  }
}

function assertSafeString(value, label) {
  if (/-----BEGIN(?: [A-Z0-9]+)? PRIVATE KEY-----/i.test(value)) {
    fail(`${label} contains private key material`);
  }
  const withoutUrls = value.replace(/https?:\/\/[^\s)\]>'"]+/gi, '');
  if (
    /[A-Za-z]:[\\/]/.test(withoutUrls) ||
    /(^|[\s('"=])\\\\[^\s]/.test(withoutUrls) ||
    /(^|[\s('"=])\/(?!\/)[^\s]/.test(withoutUrls)
  ) {
    fail(`${label} contains an absolute local path`);
  }
}

function assertSafeValue(value, label = 'evidence', seen = new Set()) {
  if (typeof value === 'string') {
    assertSafeString(value, label);
    return;
  }
  if (value === null || typeof value !== 'object') return;
  if (seen.has(value)) fail(`${label} must not contain cycles`);
  seen.add(value);
  if (Array.isArray(value)) {
    value.forEach((item, index) =>
      assertSafeValue(item, `${label}[${index}]`, seen),
    );
  } else {
    for (const [key, item] of Object.entries(value)) {
      assertSafeString(key, `${label} object key`);
      assertSafeValue(item, `${label}.${key}`, seen);
    }
  }
  seen.delete(value);
}

function repositoryFile(cwd, repositoryPath) {
  assertRepositoryPath(repositoryPath, 'repository path');
  return resolve(cwd, ...repositoryPath.split('/'));
}

function sha256(bytes) {
  return createHash('sha256').update(bytes).digest('hex');
}

async function readRequiredFile(cwd, repositoryPath, label) {
  try {
    return await readFile(repositoryFile(cwd, repositoryPath));
  } catch (error) {
    if (error?.code === 'ENOENT')
      fail(`${label} is missing: ${repositoryPath}`);
    throw error;
  }
}

export async function digestScope(scopeFiles, { cwd = process.cwd() } = {}) {
  assertPathList(scopeFiles, 'scopeFiles', { sorted: false });
  const hash = createHash('sha256');
  for (const repositoryPath of [...scopeFiles].sort()) {
    const bytes = await readRequiredFile(cwd, repositoryPath, 'scope file');
    hash.update(repositoryPath, 'utf8');
    hash.update('\0', 'ascii');
    hash.update(String(bytes.byteLength), 'ascii');
    hash.update('\0', 'ascii');
    hash.update(bytes);
  }
  return hash.digest('hex');
}

async function gitText(cwd, args) {
  try {
    const { stdout } = await execFileAsync('git', args, {
      cwd,
      encoding: 'utf8',
      windowsHide: true,
    });
    return stdout.trim();
  } catch (error) {
    const detail = String(error?.stderr ?? error?.message ?? '').trim();
    fail(`git ${args.join(' ')} failed${detail ? `: ${detail}` : ''}`);
  }
}

async function gitSucceeds(cwd, args) {
  try {
    await execFileAsync('git', args, { cwd, windowsHide: true });
    return true;
  } catch (error) {
    if (error?.code === 1) return false;
    const detail = String(error?.stderr ?? error?.message ?? '').trim();
    fail(`git ${args.join(' ')} failed${detail ? `: ${detail}` : ''}`);
  }
}

async function loadManifest(cwd) {
  const bytes = await readRequiredFile(cwd, MANIFEST_PATH, 'scope manifest');
  let manifest;
  try {
    manifest = JSON.parse(bytes.toString('utf8'));
  } catch (error) {
    fail(`scope manifest is not valid JSON: ${error.message}`);
  }
  assertExactKeys(manifest, ALL_GATES, 'scope manifest');
  for (const gateId of ALL_GATES) {
    const scope = manifest[gateId];
    assertPathList(scope, `scope manifest entry ${gateId}`);
    for (const commonPath of COMMON_SCOPE) {
      if (!scope.includes(commonPath)) {
        fail(
          `scope manifest entry ${gateId} is missing common path ${commonPath}`,
        );
      }
    }
  }
  return manifest;
}

async function assertScopeMatchesCommit(cwd, testedCommit, scopeFiles) {
  const tracked = new Set(
    (await gitText(cwd, ['ls-tree', '-r', '--name-only', testedCommit]))
      .split(/\r?\n/)
      .filter(Boolean),
  );
  for (const repositoryPath of scopeFiles) {
    if (!tracked.has(repositoryPath)) {
      fail(`scope file is not tracked at testedCommit: ${repositoryPath}`);
    }
  }
  const unchanged = await gitSucceeds(cwd, [
    'diff',
    '--quiet',
    testedCommit,
    '--',
    ...scopeFiles,
  ]);
  if (!unchanged) fail('a scoped file is dirty or changed from testedCommit');
}

function assertCoreFields(value) {
  if (value.schemaVersion !== SCHEMA_VERSION) {
    fail(`schemaVersion must equal ${SCHEMA_VERSION}`);
  }
  if (!COMMON_GATES.includes(value.gateId)) {
    fail(`unsupported common evidence gateId: ${value.gateId}`);
  }
  if (!['pass', 'design-change-required', 'blocked'].includes(value.status)) {
    fail('status must be pass, design-change-required, or blocked');
  }
  if (value.designCommit !== DESIGN_COMMIT) {
    fail(`designCommit must equal ${DESIGN_COMMIT}`);
  }
  if (
    typeof value.testedCommit !== 'string' ||
    !SHA_PATTERN.test(value.testedCommit)
  ) {
    fail('testedCommit must be 40 lowercase hex characters');
  }
  if (
    typeof value.testedAt !== 'string' ||
    !UTC_PATTERN.test(value.testedAt) ||
    Number.isNaN(Date.parse(value.testedAt))
  ) {
    fail('testedAt must be an RFC 3339 UTC timestamp');
  }
}

function normalizeRawDecisions(gateId, decisions) {
  if (!Array.isArray(decisions)) fail('decisions must be an array');
  const paths = decisions.map((entry, index) => {
    if (typeof entry === 'string') return entry;
    assertExactKeys(entry, ['path'], `decisions[${index}]`);
    return entry.path;
  });
  assertPathList(paths, 'decisions');
  const required = DECISION_PATHS[gateId] ? [DECISION_PATHS[gateId]] : [];
  assertSameStrings(paths, required, `${gateId} decision/ADR paths`);
  return paths;
}

function validateDecisionEnvelope(gateId, decisions) {
  if (!Array.isArray(decisions)) fail('decisions must be an array');
  const paths = decisions.map((decision, index) => {
    assertExactKeys(decision, ['path', 'sha256'], `decisions[${index}]`);
    assertRepositoryPath(decision.path, `decisions[${index}].path`);
    if (
      typeof decision.sha256 !== 'string' ||
      !SHA256_PATTERN.test(decision.sha256)
    ) {
      fail(`decisions[${index}].sha256 must be 64 lowercase hex characters`);
    }
    return decision.path;
  });
  assertPathList(paths, 'decision paths');
  const required = DECISION_PATHS[gateId] ? [DECISION_PATHS[gateId]] : [];
  assertSameStrings(paths, required, `${gateId} decision/ADR paths`);
}

function assertHttpsUrl(value, label, { nullable = false } = {}) {
  if (nullable && value === null) return;
  assertNonemptyString(value, label);
  try {
    const parsed = new URL(value);
    if (parsed.protocol !== 'https:' || !parsed.hostname)
      throw new Error('not HTTPS');
  } catch {
    fail(`${label} must be a real HTTPS URL`);
  }
}

function validatePlatforms(platforms, status) {
  if (!Array.isArray(platforms) || platforms.length === 0) {
    fail('platforms must contain at least one platform row');
  }
  const ids = new Set();
  for (const [index, platform] of platforms.entries()) {
    assertExactKeys(platform, PLATFORM_KEYS, `platforms[${index}]`);
    assertNonemptyString(platform.id, `platforms[${index}].id`);
    if (ids.has(platform.id)) fail(`duplicate platform id: ${platform.id}`);
    ids.add(platform.id);
    assertNonemptyString(platform.osVersion, `platforms[${index}].osVersion`);
    assertNonemptyString(platform.arch, `platforms[${index}].arch`);
    assertNonemptyString(platform.command, `platforms[${index}].command`);
    if (!Number.isInteger(platform.exitCode)) {
      fail(`platforms[${index}].exitCode must be an integer`);
    }
    if (platform.runUrl !== null) {
      assertHttpsUrl(platform.runUrl, `platforms[${index}].runUrl`);
    }
    if (status === 'pass' && platform.exitCode !== 0) {
      fail(`pass evidence requires command exitCode 0 for ${platform.id}`);
    }
  }
}

function hasPlatform(platforms, idPart, arch) {
  return platforms.some(
    (platform) =>
      platform.id.toLowerCase().includes(idPart) && platform.arch === arch,
  );
}

function requireDesktopTriplet(platforms, label = 'gate') {
  if (!hasPlatform(platforms, 'windows', 'x86_64')) {
    fail(`${label} requires a Windows x64 platform row`);
  }
  if (!hasPlatform(platforms, 'macos', 'x86_64')) {
    fail(`${label} requires a macOS Intel platform row`);
  }
  if (!hasPlatform(platforms, 'macos', 'aarch64')) {
    fail(`${label} requires a macOS ARM platform row`);
  }
}

function assertTrueChecks(checks, keys, gateId) {
  for (const key of keys) {
    if (checks[key] !== true) fail(`${gateId} checks.${key} must be true`);
  }
}

function assertFalseChecks(checks, keys, gateId) {
  for (const key of keys) {
    if (checks[key] !== false) fail(`${gateId} checks.${key} must be false`);
  }
}

function assertUniqueStringArray(value, count, label) {
  if (
    !Array.isArray(value) ||
    value.length !== count ||
    value.some((entry) => typeof entry !== 'string' || entry.trim() === '') ||
    new Set(value).size !== value.length
  ) {
    fail(`${label} must contain exactly ${count} unique nonempty strings`);
  }
}

function assertExactStringSet(value, expected, label) {
  assertUniqueStringArray(value, expected.length, label);
  const actual = [...value].sort();
  const wanted = [...expected].sort();
  if (actual.some((entry, index) => entry !== wanted[index])) {
    fail(`${label} must equal the fixed set: ${wanted.join(', ')}`);
  }
}

function assertExactValueObject(value, expected, label) {
  assertExactKeys(value, Object.keys(expected), label);
  for (const [key, wanted] of Object.entries(expected)) {
    if (value[key] !== wanted) {
      fail(`${label}.${key} must equal ${JSON.stringify(wanted)}`);
    }
  }
}

function assertFrozenVersion(value, label) {
  assertNonemptyString(value, label);
  if (
    value !== value.trim() ||
    /^(?:current|unknown|unavailable|recorded(?::.*)?)$/i.test(value)
  ) {
    fail(`${label} must be an exact frozen version`);
  }
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

function validateResourceVectorResult(platformId, vector, result) {
  const label = `checks.byPlatform.${platformId}.resourceVectorResults.${vector}`;
  assertExactKeys(result, RESOURCE_RESULT_KEYS, label);
  if (result.runtimeAttempted !== true) {
    fail(`${label}.runtimeAttempted must be true`);
  }
  if (result.deterministicBarrierSeamCovered !== true) {
    fail(`${label}.deterministicBarrierSeamCovered must be true`);
  }

  const absentServiceWorker =
    vector === 'service_worker' &&
    result.availabilityOutcome === 'service-worker-api-absent';
  if (result.availabilityOutcome !== 'available' && !absentServiceWorker) {
    fail(
      `${label}.availabilityOutcome must be available except for the fixed service-worker API absence`,
    );
  }

  const barrier = expectedBarrier(platformId, vector);
  if (result.expectedBarrier !== barrier) {
    fail(`${label}.expectedBarrier must equal ${barrier}`);
  }
  if (result.enforcedBarrier !== barrier) {
    fail(`${label}.enforcedBarrier must equal its expected barrier`);
  }
  if (result.serverHits !== 0) {
    fail(`${label}.serverHits must be exactly 0`);
  }
  const redirectHops = vector === 'redirect' ? 2 : 0;
  if (result.allowedRedirectHopHits !== redirectHops) {
    fail(`${label}.allowedRedirectHopHits must equal ${redirectHops}`);
  }

  if (absentServiceWorker) {
    if (result.barrierEvidenceMode !== 'deterministic-seam-only') {
      fail(`${label}.barrierEvidenceMode must be deterministic-seam-only`);
    }
    if (result.counterfactualServerHits !== null) {
      fail(
        `${label}.counterfactualServerHits must be null when the API is absent`,
      );
    }
    return result.serverHits;
  }

  if (RESOURCE_REQUEST_VECTORS.has(vector)) {
    if (platformId.startsWith('windows-')) {
      if (result.barrierEvidenceMode !== 'native-callback') {
        fail(`${label}.barrierEvidenceMode must be native-callback`);
      }
      if (result.counterfactualServerHits !== null) {
        fail(`${label}.counterfactualServerHits must be null on Windows`);
      }
    } else {
      if (result.barrierEvidenceMode !== 'paired-counterfactual') {
        fail(`${label}.barrierEvidenceMode must be paired-counterfactual`);
      }
      if (
        !Number.isInteger(result.counterfactualServerHits) ||
        result.counterfactualServerHits <= 0
      ) {
        fail(`${label}.counterfactualServerHits must be a positive integer`);
      }
    }
  } else {
    if (result.barrierEvidenceMode !== 'handler-callback') {
      fail(`${label}.barrierEvidenceMode must be handler-callback`);
    }
    if (result.counterfactualServerHits !== null) {
      fail(
        `${label}.counterfactualServerHits must be null for handler vectors`,
      );
    }
  }
  return result.serverHits;
}

function assertVersionAtLeast13_3(value, label) {
  const match = /^(\d+)\.(\d+)(?:\.(\d+))?$/.exec(value);
  if (!match) fail(`${label} must be an exact macOS product version`);
  const major = Number(match[1]);
  const minor = Number(match[2]);
  if (major < 13 || (major === 13 && minor < 3)) {
    fail(`${label} must be at least 13.3`);
  }
}

function validateSignaturePlatformRow(platformId, row, platform) {
  const label = `checks.byPlatform.${platformId}`;
  assertExactKeys(row, SIGNATURE_PLATFORM_ROW_KEYS, label);
  const isWindows = platformId.startsWith('windows-');
  const matrix = {
    'windows-10-webview2-111-x64': {
      hostPlatform: 'win32',
      hostArch: 'x64',
      binaryTargetOs: 'windows',
      binaryTargetArch: 'x86_64',
      translatedProcess: null,
    },
    'windows-11-x64': {
      hostPlatform: 'win32',
      hostArch: 'x64',
      binaryTargetOs: 'windows',
      binaryTargetArch: 'x86_64',
      translatedProcess: null,
    },
    'macos-13-intel': {
      hostPlatform: 'darwin',
      hostArch: 'x64',
      binaryTargetOs: 'macos',
      binaryTargetArch: 'x86_64',
      translatedProcess: false,
    },
    'macos-current-arm64': {
      hostPlatform: 'darwin',
      hostArch: 'arm64',
      binaryTargetOs: 'macos',
      binaryTargetArch: 'aarch64',
      translatedProcess: false,
    },
  }[platformId];
  for (const [field, expected] of Object.entries(matrix)) {
    if (row[field] !== expected) {
      fail(`${label}.${field} must equal ${JSON.stringify(expected)}`);
    }
  }
  if (platform.osVersion !== row.osVersion) {
    fail(`${label}.osVersion must match the runner platform row`);
  }
  if (platform.arch !== row.binaryTargetArch) {
    fail(`${label}.binaryTargetArch must match the runner platform row`);
  }

  if (platformId === 'windows-10-webview2-111-x64') {
    if (row.osVersion !== '10.0.19045') {
      fail(`${label}.osVersion must equal 10.0.19045`);
    }
    if (!/^111\.0\.1661\.\d+$/.test(row.webviewRuntimeVersion)) {
      fail(`${label}.webviewRuntimeVersion must match 111.0.1661.x`);
    }
    if (typeof row.strongSourceKindsInterfaceAvailable !== 'boolean') {
      fail(`${label}.strongSourceKindsInterfaceAvailable must be boolean`);
    }
    const expectedMode = row.strongSourceKindsInterfaceAvailable
      ? 'webview2-22-all-source-kinds'
      : 'webview2-legacy-all-contexts-candidate';
    if (row.resourcePolicyMode !== expectedMode) {
      fail(`${label}.resourcePolicyMode must match interface availability`);
    }
  } else if (platformId === 'windows-11-x64') {
    const buildMatch = /^10\.0\.(\d+)$/.exec(row.osVersion);
    if (!buildMatch || Number(buildMatch[1]) < 22000) {
      fail(`${label}.osVersion must be a frozen Windows 11 build >= 22000`);
    }
    if (row.strongSourceKindsInterfaceAvailable !== true) {
      fail(`${label}.strongSourceKindsInterfaceAvailable must be true`);
    }
    if (row.resourcePolicyMode !== 'webview2-22-all-source-kinds') {
      fail(`${label}.resourcePolicyMode must use the WebView2 v22 interface`);
    }
  } else {
    if (platformId === 'macos-13-intel') {
      if (!/^13\.3(?:\.\d+)?$/.test(row.osVersion)) {
        fail(`${label}.osVersion must match 13.3 or 13.3.x`);
      }
    } else {
      assertVersionAtLeast13_3(row.osVersion, `${label}.osVersion`);
    }
    if (row.strongSourceKindsInterfaceAvailable !== null) {
      fail(`${label}.strongSourceKindsInterfaceAvailable must be null`);
    }
    if (row.resourcePolicyMode !== 'wk-content-rule-list-exact-origin') {
      fail(`${label}.resourcePolicyMode must use the WK exact-origin policy`);
    }
  }
  if (!RESOURCE_POLICY_MODES.includes(row.resourcePolicyMode)) {
    fail(`${label}.resourcePolicyMode is not a fixed platform mode`);
  }
  if (row.runtimeMode !== 'native-host-raw-wry-0.55.1') {
    fail(`${label}.runtimeMode must equal native-host-raw-wry-0.55.1`);
  }
  assertFrozenVersion(
    row.webviewRuntimeVersion,
    `${label}.webviewRuntimeVersion`,
  );
  assertTrueChecks(row, SIGNATURE_TRUE_CHECKS, label);
  assertFalseChecks(row, SIGNATURE_FALSE_CHECKS, label);
  if (isWindows) {
    if (
      !Number.isInteger(row.blockedCanaryAttempts) ||
      row.blockedCanaryAttempts < 0
    ) {
      fail(`${label}.blockedCanaryAttempts must be a nonnegative integer`);
    }
  } else if (row.blockedCanaryAttempts !== null) {
    fail(`${label}.blockedCanaryAttempts must be null on macOS`);
  }

  assertExactKeys(
    row.resourceVectorResults,
    RESOURCE_VECTORS,
    `${label}.resourceVectorResults`,
  );
  let protectedHits = 0;
  for (const vector of RESOURCE_VECTORS) {
    protectedHits += validateResourceVectorResult(
      platformId,
      vector,
      row.resourceVectorResults[vector],
    );
  }
  if (row.crossOriginCanaryServerHits !== protectedHits) {
    fail(
      `${label}.crossOriginCanaryServerHits must equal protected vector hits`,
    );
  }
  if (row.crossOriginCanaryServerHits !== 0) {
    fail(`${label}.crossOriginCanaryServerHits must be exactly 0`);
  }
  return protectedHits;
}

function validateGdCommonChecks(checks) {
  assertUniqueStringArray(checks.bodyFixtures, 6, 'checks.bodyFixtures');
  assertUniqueStringArray(checks.liveCases, 3, 'checks.liveCases');
  assertTrueChecks(
    checks,
    [
      'strictMixedRecordParser',
      'rejects429',
      'rejectsOtherNon2xx',
      'rejectsOversizeBody',
    ],
    'gd-contract-pagination',
  );
  if (
    !Number.isInteger(checks.pageLimit) ||
    checks.pageLimit < 1 ||
    checks.pageLimit > 50
  ) {
    fail('gd-contract-pagination checks.pageLimit must be an integer <= 50');
  }
}

function validateGdContractChecks(checks) {
  assertExactKeys(
    checks,
    [...GD_COMMON_CHECK_KEYS, 'searchDefaultsAndBounds'],
    'gd-contract-pagination checks',
  );
  validateGdCommonChecks(checks);
  assertExactValueObject(
    checks.searchDefaultsAndBounds,
    SEARCH_DEFAULTS_AND_BOUNDS,
    'gd-contract-pagination checks.searchDefaultsAndBounds',
  );
}

function deriveGdContractChecks(rawChecks) {
  assertExactKeys(
    rawChecks,
    [
      ...GD_COMMON_CHECK_KEYS,
      'searchContractTestResult',
      'singleCount1000LiveCase',
    ],
    'gd-contract-pagination raw checks',
  );
  validateGdCommonChecks(rawChecks);
  assertExactValueObject(
    rawChecks.searchContractTestResult,
    SEARCH_CONTRACT_TEST_RESULT,
    'gd-contract-pagination raw checks.searchContractTestResult',
  );
  assertExactValueObject(
    rawChecks.singleCount1000LiveCase,
    SINGLE_COUNT_1000_LIVE_CASE,
    'gd-contract-pagination raw checks.singleCount1000LiveCase',
  );
  return {
    bodyFixtures: [...rawChecks.bodyFixtures],
    strictMixedRecordParser: rawChecks.strictMixedRecordParser,
    rejects429: rawChecks.rejects429,
    rejectsOtherNon2xx: rawChecks.rejectsOtherNon2xx,
    rejectsOversizeBody: rawChecks.rejectsOversizeBody,
    liveCases: [...rawChecks.liveCases],
    pageLimit: rawChecks.pageLimit,
    searchDefaultsAndBounds: {
      ...rawChecks.searchContractTestResult,
      singleCount1000RequestedCount:
        rawChecks.singleCount1000LiveCase.requestedCount,
      singleCount1000ApiRequests: rawChecks.singleCount1000LiveCase.apiRequests,
    },
  };
}

function validateSignatureChecks(checks, platforms) {
  const gateId = 'signature-webview';
  assertExactKeys(checks, SIGNATURE_CHECK_KEYS, `${gateId} checks`);
  const ids = platforms.map(({ id }) => id).sort();
  assertSameStrings(
    ids,
    [...SIGNATURE_PLATFORM_IDS].sort(),
    `${gateId} platforms`,
  );
  assertTrueChecks(checks, SIGNATURE_TRUE_CHECKS, gateId);
  assertFalseChecks(checks, SIGNATURE_FALSE_CHECKS, gateId);
  assertExactStringSet(
    checks.resourceVectorsCovered,
    RESOURCE_VECTORS,
    'checks.resourceVectorsCovered',
  );
  assertExactKeys(
    checks.byPlatform,
    SIGNATURE_PLATFORM_IDS,
    'checks.byPlatform',
  );
  for (const field of [
    'runtimeModes',
    'resourcePolicyModes',
    'webviewRuntimeVersions',
  ]) {
    assertExactKeys(checks[field], SIGNATURE_PLATFORM_IDS, `checks.${field}`);
  }

  const platformById = new Map(
    platforms.map((platform) => [platform.id, platform]),
  );
  let protectedHits = 0;
  for (const platformId of SIGNATURE_PLATFORM_IDS) {
    const row = checks.byPlatform[platformId];
    protectedHits += validateSignaturePlatformRow(
      platformId,
      row,
      platformById.get(platformId),
    );
    if (checks.runtimeModes[platformId] !== row.runtimeMode) {
      fail(
        `checks.runtimeModes.${platformId} must be derived from checks.byPlatform`,
      );
    }
    if (checks.resourcePolicyModes[platformId] !== row.resourcePolicyMode) {
      fail(
        `checks.resourcePolicyModes.${platformId} must be derived from checks.byPlatform`,
      );
    }
    if (
      checks.webviewRuntimeVersions[platformId] !== row.webviewRuntimeVersion
    ) {
      fail(
        `checks.webviewRuntimeVersions.${platformId} must be derived from checks.byPlatform`,
      );
    }
    if (checks.runtimeModes[platformId] !== 'native-host-raw-wry-0.55.1') {
      fail(`checks.runtimeModes.${platformId} must use raw WRY 0.55.1`);
    }
    if (
      !RESOURCE_POLICY_MODES.includes(checks.resourcePolicyModes[platformId])
    ) {
      fail(
        `checks.resourcePolicyModes.${platformId} is not a fixed platform mode`,
      );
    }
    assertFrozenVersion(
      checks.webviewRuntimeVersions[platformId],
      `checks.webviewRuntimeVersions.${platformId}`,
    );
  }
  if (checks.crossOriginCanaryServerHits !== protectedHits) {
    fail(
      'signature-webview checks.crossOriginCanaryServerHits must equal row sums',
    );
  }
  if (checks.crossOriginCanaryServerHits !== 0) {
    fail(
      'signature-webview checks.crossOriginCanaryServerHits must be exactly 0',
    );
  }
}

function validateToolchainChecks(checks, platforms, testedCommit) {
  requireDesktopTriplet(platforms, 'toolchain-ci');
  if (checks.event !== 'push') fail('toolchain-ci checks.event must be push');
  if (checks.headSha !== testedCommit) {
    fail('toolchain-ci checks.headSha must equal testedCommit');
  }
  for (const name of ['quality', 'platform-windows', 'platform-macos']) {
    const check = checks[name];
    assertExactKeys(
      check,
      ['conclusion', 'headSha', 'runUrl'],
      `checks.${name}`,
    );
    if (check.conclusion !== 'success') fail(`${name} must conclude success`);
    if (check.headSha !== testedCommit)
      fail(`${name} headSha must equal testedCommit`);
    assertHttpsUrl(check.runUrl, `${name} runUrl`);
  }
  for (const platform of platforms) {
    assertHttpsUrl(platform.runUrl, `${platform.id} runUrl`);
  }
}

function validateGatePass(gateId, platforms, checks, testedCommit) {
  if (!isRecord(checks)) fail('checks must be an object');
  switch (gateId) {
    case 'toolchain-ci':
      validateToolchainChecks(checks, platforms, testedCommit);
      break;
    case 'gd-contract-pagination':
      validateGdContractChecks(checks);
      break;
    case 'signature-webview':
      validateSignatureChecks(checks, platforms);
      break;
    case 'network-policy':
      requireDesktopTriplet(platforms, gateId);
      assertTrueChecks(
        checks,
        ['allAddressSet', 'redirect', 'peerPin', 'bodyLimit', 'proxyDisabled'],
        gateId,
      );
      break;
    case 'atomic-commit':
      if (
        !hasPlatform(platforms, 'ntfs', 'x86_64') ||
        !hasPlatform(platforms, 'apfs', 'x86_64') ||
        !hasPlatform(platforms, 'apfs', 'aarch64')
      ) {
        fail('atomic-commit requires NTFS Windows and APFS Intel/ARM rows');
      }
      assertTrueChecks(
        checks,
        ['exactlyOneWinner', 'cancelLinearized'],
        gateId,
      );
      if (checks.overwriteCount !== 0 || checks.leftoverCount !== 0) {
        fail('atomic-commit checks require zero overwrite and zero leftovers');
      }
      break;
    case 'media-containers':
      requireDesktopTriplet(platforms, gateId);
      assertTrueChecks(checks, ['mp3RoundTrip', 'flacRoundTrip'], gateId);
      {
        const expectedFamilies = [
          'mp2',
          'aac',
          'mp4',
          'ogg',
          'opus',
          'wav',
          'truncated',
        ];
        assertUniqueStringArray(
          checks.negativeFamiliesRejected,
          expectedFamilies.length,
          'media-containers checks.negativeFamiliesRejected',
        );
        assertSameStrings(
          [...checks.negativeFamiliesRejected].sort(),
          [...expectedFamilies].sort(),
          'media-containers checks.negativeFamiliesRejected',
        );
      }
      break;
    case 'updater-exit-barrier':
      requireDesktopTriplet(platforms, gateId);
      assertTrueChecks(
        checks,
        ['realDropFutureObserved', 'realBoundedWaitOnlyObserved'],
        gateId,
      );
      if (
        checks.earlyExitObserved !== false ||
        checks.earlyInstallObserved !== false
      ) {
        fail('updater-exit-barrier checks must show no early exit/install');
      }
      for (const field of ['productionTimeoutMs', 'feedbackIntervalMs']) {
        if (!Number.isFinite(checks[field]) || checks[field] <= 0) {
          fail(
            `updater-exit-barrier checks.${field} must be a positive number`,
          );
        }
      }
      break;
  }
}

async function readEvidenceText(cwd, repositoryPath, label) {
  const bytes = await readRequiredFile(cwd, repositoryPath, label);
  const text = bytes.toString('utf8');
  if (text.trim() === '') fail(`${label} must be nonempty`);
  assertSafeString(text, label);
  return { bytes, text };
}

export async function validateEvidence(evidence, { cwd = process.cwd() } = {}) {
  assertExactKeys(evidence, ENVELOPE_KEYS, 'evidence envelope');
  assertSafeValue(evidence);
  assertCoreFields(evidence);
  assertPathList(evidence.scopeFiles, 'scopeFiles');
  if (
    typeof evidence.scopeSha256 !== 'string' ||
    !SHA256_PATTERN.test(evidence.scopeSha256)
  ) {
    fail('scopeSha256 must be 64 lowercase hex characters');
  }
  const expectedMarkdown = `docs/feasibility/${evidence.gateId}.md`;
  assertRepositoryPath(evidence.markdownPath, 'markdownPath');
  if (evidence.markdownPath !== expectedMarkdown) {
    fail(`markdownPath must equal ${expectedMarkdown}`);
  }
  if (
    typeof evidence.markdownSha256 !== 'string' ||
    !SHA256_PATTERN.test(evidence.markdownSha256)
  ) {
    fail('markdownSha256 must be 64 lowercase hex characters');
  }
  validateDecisionEnvelope(evidence.gateId, evidence.decisions);
  validatePlatforms(evidence.platforms, evidence.status);
  if (!isRecord(evidence.checks)) fail('checks must be an object');
  if (evidence.status === 'pass') {
    validateGatePass(
      evidence.gateId,
      evidence.platforms,
      evidence.checks,
      evidence.testedCommit,
    );
  }

  const manifest = await loadManifest(cwd);
  assertSameStrings(
    evidence.scopeFiles,
    manifest[evidence.gateId],
    `${evidence.gateId} scopeFiles`,
  );

  const isAncestor = await gitSucceeds(cwd, [
    'merge-base',
    '--is-ancestor',
    evidence.testedCommit,
    'HEAD',
  ]);
  if (!isAncestor)
    fail('testedCommit must be an ancestor of the evidence commit');
  await assertScopeMatchesCommit(
    cwd,
    evidence.testedCommit,
    evidence.scopeFiles,
  );

  const scopeDigest = await digestScope(evidence.scopeFiles, { cwd });
  if (scopeDigest !== evidence.scopeSha256) fail('scope hash changed');

  const markdown = await readEvidenceText(
    cwd,
    evidence.markdownPath,
    'Markdown evidence',
  );
  if (sha256(markdown.bytes) !== evidence.markdownSha256) {
    fail('Markdown evidence hash changed');
  }

  for (const decision of evidence.decisions) {
    const adr = await readEvidenceText(cwd, decision.path, 'decision/ADR');
    if (sha256(adr.bytes) !== decision.sha256)
      fail('decision/ADR hash changed');
  }
  return true;
}

export async function buildEvidence(
  raw,
  { cwd = process.cwd(), markdownPath, outputPath } = {},
) {
  if (!isRecord(raw)) fail('raw input must be an object');
  for (const derived of [
    'scopeFiles',
    'scopeSha256',
    'markdownPath',
    'markdownSha256',
  ]) {
    if (Object.hasOwn(raw, derived)) {
      fail(`raw input must not supply ${derived}; it is derived by the helper`);
    }
  }
  assertSafeValue(raw, 'raw input');
  assertExactKeys(raw, RAW_KEYS, 'raw input');
  assertCoreFields(raw);
  const head = await gitText(cwd, ['rev-parse', 'HEAD']);
  if (raw.testedCommit !== head)
    fail('testedCommit must equal git HEAD during build');
  validatePlatforms(raw.platforms, raw.status);
  if (!isRecord(raw.checks)) fail('checks must be an object');
  let checks = raw.checks;
  if (raw.status === 'pass') {
    if (raw.gateId === 'gd-contract-pagination') {
      checks = deriveGdContractChecks(raw.checks);
    }
    validateGatePass(raw.gateId, raw.platforms, checks, raw.testedCommit);
  }
  const decisionPaths = normalizeRawDecisions(raw.gateId, raw.decisions);

  const manifest = await loadManifest(cwd);
  const scopeFiles = manifest[raw.gateId];
  await assertScopeMatchesCommit(cwd, raw.testedCommit, scopeFiles);

  const expectedMarkdown = `docs/feasibility/${raw.gateId}.md`;
  assertRepositoryPath(markdownPath, 'markdownPath');
  if (markdownPath !== expectedMarkdown) {
    fail(`markdownPath must equal ${expectedMarkdown}`);
  }
  const markdown = await readEvidenceText(
    cwd,
    markdownPath,
    'Markdown evidence',
  );

  const decisions = [];
  for (const decisionPath of decisionPaths) {
    const adr = await readEvidenceText(cwd, decisionPath, 'decision/ADR');
    decisions.push({ path: decisionPath, sha256: sha256(adr.bytes) });
  }

  const evidence = {
    schemaVersion: raw.schemaVersion,
    gateId: raw.gateId,
    status: raw.status,
    designCommit: raw.designCommit,
    testedCommit: raw.testedCommit,
    testedAt: raw.testedAt,
    scopeFiles: [...scopeFiles],
    scopeSha256: await digestScope(scopeFiles, { cwd }),
    markdownPath,
    markdownSha256: sha256(markdown.bytes),
    decisions,
    platforms: raw.platforms,
    checks,
  };

  await validateEvidence(evidence, { cwd });

  if (outputPath !== undefined) {
    const expectedOutput = `docs/feasibility/${raw.gateId}.json`;
    assertRepositoryPath(outputPath, 'outputPath');
    if (outputPath !== expectedOutput)
      fail(`outputPath must equal ${expectedOutput}`);
    const target = repositoryFile(cwd, outputPath);
    await mkdir(dirname(target), { recursive: true });
    const temporary = `${target}.tmp-${process.pid}-${randomUUID()}`;
    let moved = false;
    try {
      await writeFile(temporary, `${JSON.stringify(evidence, null, 2)}\n`, {
        flag: 'wx',
      });
      await rename(temporary, target);
      moved = true;
    } finally {
      if (!moved) await rm(temporary, { force: true });
    }
  }
  return evidence;
}

function parseFlags(args) {
  const flags = {};
  if (args.length % 2 !== 0)
    fail('flags must be provided as --name value pairs');
  for (let index = 0; index < args.length; index += 2) {
    const flag = args[index];
    if (!flag.startsWith('--') || flag.length === 2)
      fail(`invalid flag: ${flag}`);
    const name = flag.slice(2);
    if (Object.hasOwn(flags, name)) fail(`duplicate flag: ${flag}`);
    flags[name] = args[index + 1];
  }
  return flags;
}

async function readJson(cwd, repositoryPath, label) {
  const bytes = await readRequiredFile(cwd, repositoryPath, label);
  try {
    return JSON.parse(bytes.toString('utf8'));
  } catch (error) {
    fail(`${label} is not valid JSON: ${error.message}`);
  }
}

async function checkExisting(directory, cwd) {
  assertRepositoryPath(directory, 'evidence directory');
  let checked = 0;
  for (const gateId of COMMON_GATES) {
    const evidencePath = `${directory}/${gateId}.json`;
    try {
      await access(repositoryFile(cwd, evidencePath));
    } catch (error) {
      if (error?.code === 'ENOENT') continue;
      throw error;
    }
    const evidence = await readJson(cwd, evidencePath, `${gateId} evidence`);
    if (evidence.gateId !== gateId) {
      fail(`${evidencePath} gateId must equal ${gateId}`);
    }
    await validateEvidence(evidence, { cwd });
    checked += 1;
  }
  console.log(`Existing feasibility evidence: PASS (${checked} checked)`);
}

async function main(argv = process.argv.slice(2), cwd = process.cwd()) {
  const [command, ...args] = argv;
  if (command === 'build') {
    const flags = parseFlags(args);
    assertExactKeys(flags, ['input', 'markdown', 'output'], 'build flags');
    for (const [name, repositoryPath] of Object.entries(flags)) {
      assertRepositoryPath(repositoryPath, `--${name}`);
    }
    const raw = await readJson(cwd, flags.input, 'raw evidence input');
    await buildEvidence(raw, {
      cwd,
      markdownPath: flags.markdown,
      outputPath: flags.output,
    });
    console.log(`Evidence build: PASS (${flags.output})`);
    return;
  }
  if (command === 'check') {
    if (args.length !== 1)
      fail('usage: feasibility-evidence.mjs check <file.json>');
    assertRepositoryPath(args[0], 'evidence path');
    const evidence = await readJson(cwd, args[0], 'evidence');
    await validateEvidence(evidence, { cwd });
    console.log(`Evidence check: PASS (${args[0]})`);
    return;
  }
  if (command === 'check-existing') {
    if (args.length !== 1) {
      fail('usage: feasibility-evidence.mjs check-existing <directory>');
    }
    await checkExisting(args[0], cwd);
    return;
  }
  fail(
    'usage: feasibility-evidence.mjs build --input <raw.json> --markdown <file.md> --output <file.json> | check <file.json> | check-existing <directory>',
  );
}

if (
  process.argv[1] &&
  resolve(process.argv[1]).toLowerCase() ===
    fileURLToPath(import.meta.url).toLowerCase()
) {
  try {
    await main();
  } catch (error) {
    console.error(error?.stack ?? error);
    process.exitCode = 1;
  }
}
