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

const DESIGN_COMMIT = '5893d4340a4815677da79f74223642ac855519e7';
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
  if (value.schemaVersion !== 1) fail('schemaVersion must equal 1');
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
        gateId,
      );
      if (
        !Number.isInteger(checks.pageLimit) ||
        checks.pageLimit < 1 ||
        checks.pageLimit > 50
      ) {
        fail(
          'gd-contract-pagination checks.pageLimit must be an integer <= 50',
        );
      }
      break;
    case 'signature-webview': {
      const expectedIds = [
        'windows-10-webview2-111-x64',
        'windows-11-webview2-current-x64',
        'macos-13.3-intel',
        'macos-current-arm',
      ];
      const ids = platforms.map(({ id }) => id).sort();
      assertSameStrings(
        ids,
        [...expectedIds].sort(),
        'signature-webview platforms',
      );
      const legacy = platforms.find(({ id }) => id === expectedIds[0]);
      if (!legacy.osVersion.includes('111.0.1661.')) {
        fail('signature-webview requires fixed WebView2 111.0.1661.x');
      }
      for (const field of ['runtimeModes', 'filterModes']) {
        if (!isRecord(checks[field]))
          fail(`signature-webview checks.${field} must be recorded`);
        for (const id of expectedIds) {
          assertNonemptyString(checks[field][id], `checks.${field}.${id}`);
        }
      }
      assertTrueChecks(
        checks,
        [
          'ipcBridgeAbsent',
          'officialOnlyOrigins',
          'timeoutCheck',
          'retryCheck',
        ],
        gateId,
      );
      if (checks.nestedResourceCanaries !== 0) {
        fail('signature-webview nested resource canaries must be zero');
      }
      break;
    }
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
      for (const family of ['mp2', 'truncated-id3', 'truncated-flac']) {
        if (!checks.negativeFamiliesRejected?.includes(family)) {
          fail(`media-containers checks must reject ${family}`);
        }
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
  if (raw.status === 'pass') {
    validateGatePass(raw.gateId, raw.platforms, raw.checks, raw.testedCommit);
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
    checks: raw.checks,
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
