import { execFileSync, spawn } from 'node:child_process';
import { createHash, randomBytes } from 'node:crypto';
import { mkdir, readFile, rm, stat, writeFile } from 'node:fs/promises';
import { createServer } from 'node:http';
import { createServer as createHttpsServer } from 'node:https';
import { homedir, release } from 'node:os';
import { dirname, isAbsolute, join, relative, resolve, sep } from 'node:path';
import { fileURLToPath } from 'node:url';

import {
  SIGNATURE_PLATFORM_MATRIX,
  SIGNATURE_RESOURCE_VECTORS,
  validateSignatureProbeSemantics,
} from './signature-wire-contract.mjs';

export const BODY_LIMIT_BYTES = 4 * 1024;
export const ISOLATION_REPORT_BODY_LIMIT_BYTES = 256 * 1024;
export const PHASE_DEADLINE_MS = 60_000;
export const TOTAL_DEADLINE_MS = 130_000;
export const ISOLATION_SUBSTEP_DEADLINES_MS = Object.freeze({
  setupAndPreflight: 60_000,
  resourceMatrix: 20 * 20_000,
  fixedScenarios: 6 * 25_000,
  lifecycleCycles: 20 * 20_000,
  lifecycleIdle: 600_000,
  reportAndCleanup: 120_000,
});
export const ISOLATION_STAGE_DEADLINE_MS = Object.values(
  ISOLATION_SUBSTEP_DEADLINES_MS,
).reduce((total, value) => total + value, 0);
export const PROCESS_INFO_PATH = '/process-info';
export const EVENT_PATH = '/events';
export const ISOLATION_REPORT_PATH = '/isolation-report';
export const CANARY_CONFIG_PATH = '/canary-config';
export const CANARY_CONFIG_KEYS = Object.freeze([
  'runId',
  'phase',
  'platformId',
  'controlOrigin',
  'allowedOrigin',
  'blockedHttpOrigin',
  'blockedHttpsOrigin',
  'blockedWsOrigin',
  'blockedWssOrigin',
  'idleDurationMs',
]);
export const CANARY_IDLE_DURATION_MS = 600_000;
export const CANARY_RESOURCE_VECTORS = SIGNATURE_RESOURCE_VECTORS;
export const CANARY_RESET_PATH = '/canary/reset';
export const CANARY_SNAPSHOT_PATH = '/canary/snapshot';
export const CANARY_SLEEP_WAKE_PATH = '/canary/sleep-wake';
export const CANARY_COMPLETE_PATH = '/canary/complete';
export const CANARY_BARRIER_PATH = '/canary/barrier';
export const CANARY_PROTECTED_SEAL_PATH = '/canary/protected-seal';
export const CANARY_PROTECTED_VERIFY_PATH = '/canary/protected-verify';
export const CANARY_COMPLETION_QUIET_MS = 5_000;

export const PROCESS_INFO_KEYS = Object.freeze([
  'runId',
  'phase',
  'binaryTargetOs',
  'binaryTargetArch',
  'translatedProcess',
]);
export const EVENT_KEYS = Object.freeze(['runId', 'phase', 'event']);
const ISOLATION_REPORT_ENVELOPE_KEYS = Object.freeze([
  'runId',
  'phase',
  'report',
]);
const ISOLATION_REPORT_KEYS = Object.freeze([
  'generation',
  'operationId',
  'platformId',
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
  'currentUrl',
  'finalUrl',
  'counters',
  'hostLabelsAfterDestroy',
  'fixedScenarios',
  'checks',
]);
const ISOLATION_RUNNER_OBSERVATION_KEYS = Object.freeze([
  'httpsPreflightHits',
  'wssPreflightHandshakes',
]);
const WRITE_PHASE = 'write-marker-and-close-main';
const VERIFY_PHASE = 'verify-marker-absent';
const EXTERNAL_EXIT_EVENT = 'process-exit-observed';

export const PHASES = Object.freeze([WRITE_PHASE, VERIFY_PHASE]);
export const PHASE_GRAMMARS = Object.freeze({
  [WRITE_PHASE]: Object.freeze([
    'process-started',
    'active-host-ready',
    'marker-written',
    'main-close-requested',
    'host-destroyed',
    'manager-host-absent',
    'policy-cleanup-acknowledged',
    'policy-tombstones-empty',
    'tls-entry-absent',
    'app-exit-invoked',
    EXTERNAL_EXIT_EVENT,
  ]),
  [VERIFY_PHASE]: Object.freeze([
    'process-started',
    'active-host-ready',
    'marker-absent',
    'main-close-requested',
    'host-destroyed',
    'manager-host-absent',
    'policy-cleanup-acknowledged',
    'policy-tombstones-empty',
    'tls-entry-absent',
    'app-exit-invoked',
    EXTERNAL_EXIT_EVENT,
  ]),
});

export const PLATFORM_MATRIX = SIGNATURE_PLATFORM_MATRIX;

const HOST_FACT_KEYS = Object.freeze(['hostPlatform', 'hostArch', 'osVersion']);
const TARGET_FACT_KEYS = Object.freeze([
  'binaryTargetOs',
  'binaryTargetArch',
  'translatedProcess',
]);
const SIGNATURE_ENV_PREFIX = 'YINMI_FEASIBILITY_SIGNATURE_';
export const CONTROLLED_VM_ENV = 'YINMI_FEASIBILITY_CONTROLLED_VM';
const OUTPUT_DIRECTORY = 'artifacts/feasibility/signature';
const USAGE =
  'usage: node scripts/run-signature-lifecycle-probe.mjs --app <path> --platform-id <id> --output <artifacts/feasibility/signature/path>';

function fail(message) {
  throw new Error(message);
}

function assertPlainObject(value, label) {
  if (
    value === null ||
    typeof value !== 'object' ||
    Array.isArray(value) ||
    Object.getPrototypeOf(value) !== Object.prototype
  ) {
    fail(`${label} must be a JSON object`);
  }
}

function assertExactKeys(value, expectedKeys, label) {
  assertPlainObject(value, label);
  const actualKeys = Object.keys(value);
  const missing = expectedKeys.filter((key) => !actualKeys.includes(key));
  const extra = actualKeys.filter((key) => !expectedKeys.includes(key));
  if (missing.length > 0 || extra.length > 0) {
    const details = [
      missing.length > 0 ? `missing ${missing.join(', ')}` : '',
      extra.length > 0 ? `extra ${extra.join(', ')}` : '',
    ]
      .filter(Boolean)
      .join('; ');
    fail(
      `${label} must contain the exact keys ${expectedKeys.join(', ')} (${details})`,
    );
  }
}

function assertRunId(runId, label = 'run id') {
  if (typeof runId !== 'string' || !/^[0-9a-f]{32}$/.test(runId)) {
    fail(`${label} must be a lowercase 128-bit hexadecimal run id`);
  }
}

const CANARY_MODES = Object.freeze([
  'counterfactual',
  'protected',
  'preflight',
  'lifecycle',
]);
const CANARY_SNAPSHOT_KEYS = Object.freeze([
  'runId',
  'mode',
  'vector',
  'directHits',
  'allowedRedirectHopHits',
  'browserPreflightHits',
  'websocketHandshakes',
  'sleepWakeObserved',
  'browserProcessBaseline',
  'browserProcessCurrent',
  'visibleWindowLeakObserved',
  'unexpectedActivationObserved',
]);

function validateCanaryCoordinate(runId, mode, vector) {
  assertRunId(runId, 'canary run id');
  if (!CANARY_MODES.includes(mode)) {
    fail(`canary mode must be one of ${CANARY_MODES.join(', ')}`);
  }
  if (
    !CANARY_RESOURCE_VECTORS.includes(vector) &&
    !(mode === 'preflight' && vector === 'preflight') &&
    !(mode === 'lifecycle' && vector === 'lifecycle')
  ) {
    fail('canary vector must be one of the frozen resource vectors');
  }
  return { runId, mode, vector };
}

function canaryKey(mode, vector) {
  return `${mode}\u0000${vector}`;
}

export class CanaryRecorder {
  #observations = new Map();
  #completion = new Map();
  #now;
  #protectedLateViolation = false;
  #protectedSealed = false;
  #runId;

  constructor({ runId, now = Date.now }) {
    assertRunId(runId, 'canary recorder run id');
    if (typeof now !== 'function')
      fail('canary recorder clock must be a function');
    this.#runId = runId;
    this.#now = now;
  }

  #coordinate(runId, mode, vector) {
    validateCanaryCoordinate(runId, mode, vector);
    if (runId !== this.#runId) {
      fail('canary run id must match the recorder nonce');
    }
    return canaryKey(mode, vector);
  }

  #assertRunId(runId) {
    assertRunId(runId, 'canary recorder run id');
    if (runId !== this.#runId)
      fail('canary run id must match the recorder nonce');
  }

  reset(
    { runId, mode, vector },
    {
      browserProcessCount = 0,
      visibleWindowLeakObserved = false,
      unexpectedActivationObserved = false,
    } = {},
  ) {
    if (!Number.isInteger(browserProcessCount) || browserProcessCount < 0) {
      fail('browser process count must be a nonnegative integer');
    }
    if (
      typeof visibleWindowLeakObserved !== 'boolean' ||
      typeof unexpectedActivationObserved !== 'boolean'
    ) {
      fail('platform window observations must be boolean');
    }
    const key = this.#coordinate(runId, mode, vector);
    if (mode === 'protected' && this.#protectedSealed) {
      fail('protected canary recorder is already sealed');
    }
    if (this.#observations.has(key)) {
      fail('canary coordinate reset is one-shot and cannot be replayed');
    }
    this.#observations.set(key, {
      runId,
      mode,
      vector,
      directHits: 0,
      allowedRedirectHopHits: 0,
      browserPreflightHits: 0,
      websocketHandshakes: 0,
      sleepWakeObserved: false,
      browserProcessBaseline: browserProcessCount,
      browserProcessCurrent: browserProcessCount,
      visibleWindowLeakObserved,
      unexpectedActivationObserved,
    });
    this.#completion.set(key, {
      completedAtMs: null,
      lastActivityAtMs: this.#now(),
      barrierComplete: false,
    });
  }

  hit({ runId, mode, vector }, kind) {
    const key = this.#coordinate(runId, mode, vector);
    if (mode === 'protected' && this.#protectedSealed) {
      this.#protectedLateViolation = true;
      fail('late protected canary hit arrived after the zero-hit seal');
    }
    const observation = this.#observations.get(key);
    if (!observation) {
      fail('canary coordinate must be reset before accepting hits');
    }
    if (kind === 'blocked-origin') observation.directHits += 1;
    else if (kind === 'allowed-redirect-hop') {
      observation.allowedRedirectHopHits += 1;
    } else if (kind === 'browser-preflight') {
      observation.browserPreflightHits += 1;
    } else if (kind === 'websocket-handshake') {
      observation.directHits += 1;
      observation.websocketHandshakes += 1;
    } else {
      fail('unknown canary hit kind');
    }
    const completion = this.#completion.get(key);
    completion.lastActivityAtMs = this.#now();
    completion.barrierComplete = false;
  }

  complete({ runId, mode, vector }) {
    const key = this.#coordinate(runId, mode, vector);
    if (!this.#observations.has(key)) {
      fail('canary coordinate must be reset before completion');
    }
    if (mode === 'protected' && this.#protectedSealed) {
      fail('protected canary recorder is already sealed');
    }
    const completion = this.#completion.get(key);
    if (completion.completedAtMs !== null) {
      fail('canary coordinate completion may be acknowledged once');
    }
    const now = this.#now();
    completion.completedAtMs = now;
    completion.lastActivityAtMs = Math.max(completion.lastActivityAtMs, now);
    return { accepted: 'complete' };
  }

  completionBarrier({ runId, mode, vector }) {
    const key = this.#coordinate(runId, mode, vector);
    const completion = this.#completion.get(key);
    if (!completion || completion.completedAtMs === null) {
      fail('canary completion barrier requires explicit trigger completion');
    }
    const quietSince = Math.max(
      completion.completedAtMs,
      completion.lastActivityAtMs,
    );
    const retryAfterMs = Math.max(
      0,
      CANARY_COMPLETION_QUIET_MS - (this.#now() - quietSince),
    );
    if (retryAfterMs > 0) return { status: 'pending', retryAfterMs };
    completion.barrierComplete = true;
    return {
      status: 'complete',
      retryAfterMs: 0,
      snapshot: this.snapshot({ runId, mode, vector }),
    };
  }

  sealProtected({ runId = this.#runId } = {}) {
    this.#assertRunId(runId);
    if (this.#protectedSealed)
      fail('protected canary recorder is already sealed');
    for (const vector of CANARY_RESOURCE_VECTORS) {
      const key = canaryKey('protected', vector);
      const observation = this.#observations.get(key);
      const completion = this.#completion.get(key);
      if (!observation || !completion?.barrierComplete) {
        fail(`protected canary ${vector} has no completed silence barrier`);
      }
      if (observation.directHits !== 0) {
        fail(`protected canary ${vector} recorded a blocked-origin hit`);
      }
    }
    this.#protectedSealed = true;
    return { accepted: 'protected-sealed' };
  }

  verifyProtectedSeal({ runId = this.#runId } = {}) {
    this.#assertRunId(runId);
    if (!this.#protectedSealed) fail('protected canary recorder is not sealed');
    if (this.#protectedLateViolation) {
      fail('protected canary seal recorded a sticky late-hit violation');
    }
    for (const vector of CANARY_RESOURCE_VECTORS) {
      const observation = this.#observations.get(
        canaryKey('protected', vector),
      );
      if (!observation || observation.directHits !== 0) {
        fail(`protected canary ${vector} zero-hit seal failed verification`);
      }
    }
    return true;
  }

  observeSleepWake({ runId, mode, vector }) {
    const key = this.#coordinate(runId, mode, vector);
    const observation = this.#observations.get(key);
    if (!observation) {
      fail('canary coordinate must be reset before sleep/wake observation');
    }
    observation.sleepWakeObserved = true;
  }

  recordPlatformState(
    { runId, mode, vector },
    {
      browserProcessCount,
      visibleWindowLeakObserved = false,
      unexpectedActivationObserved = false,
    },
  ) {
    const count = browserProcessCount;
    if (!Number.isInteger(count) || count < 0) {
      fail('browser process count must be a nonnegative integer');
    }
    if (
      typeof visibleWindowLeakObserved !== 'boolean' ||
      typeof unexpectedActivationObserved !== 'boolean'
    ) {
      fail('platform window observations must be boolean');
    }
    const key = this.#coordinate(runId, mode, vector);
    const observation = this.#observations.get(key);
    if (!observation) {
      fail('canary coordinate must be reset before process observation');
    }
    observation.browserProcessCurrent = count;
    observation.visibleWindowLeakObserved ||= visibleWindowLeakObserved;
    observation.unexpectedActivationObserved ||= unexpectedActivationObserved;
  }

  snapshot({ runId, mode, vector }) {
    const key = this.#coordinate(runId, mode, vector);
    const observation = this.#observations.get(key);
    if (!observation) {
      fail('canary coordinate has no reset observation');
    }
    const snapshot = { ...observation };
    assertExactKeys(snapshot, CANARY_SNAPSHOT_KEYS, 'canary snapshot');
    return snapshot;
  }
}

function parseCanaryUrl(path) {
  let url;
  try {
    url = new URL(path, 'http://127.0.0.1');
  } catch {
    fail('canary path must be a valid relative URL');
  }
  const parameters = Object.fromEntries(url.searchParams);
  if (url.searchParams.size !== Object.keys(parameters).length) {
    fail('canary query parameters may appear exactly once');
  }
  return { url, parameters };
}

export function routeCanaryControl(request, recorder, platformState = {}) {
  if (request.method === 'POST' && request.path === CANARY_RESET_PATH) {
    const coordinate = decodeExactJsonBody(
      request.body,
      ['runId', 'mode', 'vector'],
      'canary reset',
    );
    if (
      coordinate.mode === 'lifecycle' &&
      platformState.browserProcessCount === undefined
    ) {
      fail('lifecycle reset requires an observed browser process count');
    }
    recorder.reset(coordinate, platformState);
    return { accepted: 'reset' };
  }
  if (request.method === 'POST' && request.path === CANARY_SLEEP_WAKE_PATH) {
    const coordinate = decodeExactJsonBody(
      request.body,
      ['runId', 'mode', 'vector'],
      'canary sleep-wake',
    );
    if (coordinate.mode !== 'lifecycle' || coordinate.vector !== 'lifecycle') {
      fail('sleep/wake acknowledgement requires the lifecycle coordinate');
    }
    recorder.observeSleepWake(coordinate);
    return { accepted: 'sleep-wake' };
  }
  if (request.method === 'POST' && request.path === CANARY_COMPLETE_PATH) {
    const coordinate = decodeExactJsonBody(
      request.body,
      ['runId', 'mode', 'vector'],
      'canary completion',
    );
    return recorder.complete(coordinate);
  }
  if (
    request.method === 'POST' &&
    request.path === CANARY_PROTECTED_SEAL_PATH
  ) {
    const correlation = decodeExactJsonBody(
      request.body,
      ['runId'],
      'protected canary seal',
    );
    return recorder.sealProtected(correlation);
  }
  if (request.method === 'GET') {
    const { url, parameters } = parseCanaryUrl(request.path);
    if (url.pathname === CANARY_PROTECTED_VERIFY_PATH) {
      assertExactKeys(parameters, ['runId'], 'protected canary verification');
      if (Buffer.byteLength(request.body ?? '', 'utf8') !== 0) {
        fail('protected canary verification body must be empty');
      }
      return {
        verified: recorder.verifyProtectedSeal(parameters),
      };
    }
    if (
      url.pathname !== CANARY_SNAPSHOT_PATH &&
      url.pathname !== CANARY_BARRIER_PATH
    ) {
      fail('unknown canary control endpoint path');
    }
    assertExactKeys(
      parameters,
      ['runId', 'mode', 'vector'],
      'canary observation query',
    );
    if (Buffer.byteLength(request.body ?? '', 'utf8') !== 0) {
      fail('canary observation request body must be empty');
    }
    if (
      parameters.mode === 'lifecycle' &&
      platformState.browserProcessCount === undefined
    ) {
      fail('lifecycle snapshot requires an observed browser process count');
    }
    recorder.recordPlatformState(parameters, {
      browserProcessCount: platformState.browserProcessCount ?? 0,
      visibleWindowLeakObserved:
        platformState.visibleWindowLeakObserved ?? false,
      unexpectedActivationObserved:
        platformState.unexpectedActivationObserved ?? false,
    });
    if (url.pathname === CANARY_BARRIER_PATH) {
      return recorder.completionBarrier(parameters);
    }
    return recorder.snapshot(parameters);
  }
  fail('unknown canary control endpoint method or path');
}

export function routeCanaryHit(path, kind, recorder) {
  const { url, parameters } = canaryCoordinateFromPath(path);
  const correlated =
    (kind === 'allowed-redirect-hop' &&
      parameters.vector === 'redirect' &&
      ['/redirect/one', '/redirect/two'].includes(url.pathname)) ||
    (kind === 'blocked-origin' &&
      [`/blocked/${parameters.vector}`, `/sse/${parameters.vector}`].includes(
        url.pathname,
      )) ||
    (kind === 'websocket-handshake' &&
      url.pathname === `/ws/${parameters.vector}`) ||
    (kind === 'browser-preflight' &&
      parameters.mode === 'preflight' &&
      parameters.vector === 'preflight' &&
      url.pathname === '/preflight');
  if (!correlated) {
    fail('canary hit path, kind, and vector correlation failed');
  }
  recorder.hit(
    {
      runId: parameters.runId,
      mode: parameters.mode,
      vector: parameters.vector,
    },
    kind,
  );
}

function canaryQuery(parameters) {
  const query = new URLSearchParams({
    runId: parameters.runId,
    mode: parameters.mode,
    vector: parameters.vector,
  });
  return query.toString();
}

function canaryCoordinateFromPath(path) {
  const { url, parameters } = parseCanaryUrl(path);
  assertExactKeys(
    parameters,
    ['runId', 'mode', 'vector'],
    'canary route query',
  );
  validateCanaryCoordinate(
    parameters.runId,
    parameters.mode,
    parameters.vector,
  );
  return { url, parameters };
}

export function buildCanaryRouteResponse({
  surface,
  path,
  blockedHttpsOrigin,
  recorder,
}) {
  if (!['allowed-https', 'blocked-http', 'blocked-https'].includes(surface)) {
    fail('unknown controlled canary surface');
  }
  if (surface === 'allowed-https' && path === '/') {
    return {
      status: 200,
      headers: {
        'content-type': 'text/html; charset=utf-8',
        'cache-control': 'no-store',
      },
      body: '<!doctype html><meta charset="utf-8"><title>yinmi canary</title><script>globalThis.crc32 = (value) => String(value);</script>',
    };
  }
  const { url, parameters } = canaryCoordinateFromPath(path);
  const query = canaryQuery(parameters);

  if (surface === 'allowed-https' && url.pathname === '/redirect/one') {
    if (parameters.vector !== 'redirect') {
      fail('first redirect path requires the redirect vector');
    }
    recorder.hit(parameters, 'allowed-redirect-hop');
    return {
      status: 302,
      headers: {
        location: `/redirect/two?${query}`,
        'cache-control': 'no-store',
      },
      body: '',
    };
  }
  if (surface === 'allowed-https' && url.pathname === '/redirect/two') {
    if (parameters.vector !== 'redirect') {
      fail('second redirect path requires the redirect vector');
    }
    recorder.hit(parameters, 'allowed-redirect-hop');
    return {
      status: 302,
      headers: {
        location: `${blockedHttpsOrigin}/blocked/redirect?${query}`,
        'cache-control': 'no-store',
      },
      body: '',
    };
  }
  if (surface === 'allowed-https' && url.pathname === '/sw.js') {
    if (parameters.vector !== 'service_worker') {
      fail('service-worker route requires the service_worker vector');
    }
    const blockedUrl = `${blockedHttpsOrigin}/blocked/service_worker?${query}`;
    return {
      status: 200,
      headers: {
        'content-type': 'application/javascript',
        'cache-control': 'no-store',
        'service-worker-allowed': '/',
      },
      body: `self.addEventListener("install", (event) => { event.waitUntil(fetch(${JSON.stringify(blockedUrl)}, { cache: "no-store" }).catch(() => undefined).then(() => self.skipWaiting())); });`,
    };
  }
  if (
    surface !== 'allowed-https' &&
    url.pathname === '/preflight' &&
    parameters.mode === 'preflight' &&
    parameters.vector === 'preflight'
  ) {
    recorder.hit(parameters, 'browser-preflight');
    return {
      status: 204,
      headers: { 'cache-control': 'no-store' },
      body: '',
    };
  }
  if (surface !== 'allowed-https' && url.pathname.startsWith('/sse/')) {
    if (url.pathname !== `/sse/${parameters.vector}`) {
      fail('SSE path must match the correlated vector');
    }
    recorder.hit(parameters, 'blocked-origin');
    return {
      status: 200,
      headers: {
        'content-type': 'text/event-stream',
        'cache-control': 'no-store',
        connection: 'close',
      },
      body: 'data: yinmi-canary\n\n',
    };
  }
  if (surface !== 'allowed-https' && url.pathname.startsWith('/blocked/')) {
    if (url.pathname !== `/blocked/${parameters.vector}`) {
      fail('blocked path must match the correlated vector');
    }
    recorder.hit(parameters, 'blocked-origin');
    return {
      status: 200,
      headers: {
        'content-type': 'application/octet-stream',
        'cache-control': 'no-store',
        'access-control-allow-origin': '*',
      },
      body: 'yinmi-canary',
    };
  }
  fail('unknown controlled canary route');
}

export async function withCanaryTrust(material, adapter, operation) {
  if (
    !adapter ||
    typeof adapter.install !== 'function' ||
    typeof adapter.remove !== 'function'
  ) {
    fail('canary trust adapter must provide install and remove');
  }
  if (typeof operation !== 'function') {
    fail('canary trust operation must be a function');
  }
  const receipt = await adapter.install(material);
  try {
    return await operation(receipt);
  } finally {
    await adapter.remove(receipt);
  }
}

export async function runControlledCanaryHarness({ runId }, dependencies) {
  assertRunId(runId, 'controlled canary run id');
  const {
    createCertificate,
    trustAdapter,
    startServers,
    browserPreflight,
    startPlatformMonitor,
    operation,
  } = dependencies ?? {};
  for (const [name, value] of Object.entries({
    createCertificate,
    startServers,
    browserPreflight,
    startPlatformMonitor,
    operation,
  })) {
    if (typeof value !== 'function') {
      fail(`controlled canary dependency ${name} must be a function`);
    }
  }

  const material = await createCertificate({ runId });
  let servers;
  try {
    servers = await startServers({ runId, material });
    if (!servers?.fatal || typeof servers.fatal.then !== 'function') {
      fail('controlled canary servers must expose a fatal promise');
    }
    const fatal = Promise.resolve(servers.fatal).then(
      () => {
        throw new Error(
          'controlled canary fatal channel resolved unexpectedly',
        );
      },
      (error) => Promise.reject(error),
    );
    fatal.catch(() => {});
    const raceFatal = (stage) =>
      Promise.race([fatal, Promise.resolve().then(stage)]);
    return await withCanaryTrust(material, trustAdapter, async () => {
      const monitor = await raceFatal(() => startPlatformMonitor(servers));
      if (!monitor || typeof monitor.stop !== 'function') {
        fail('controlled canary platform monitor must provide stop');
      }
      try {
        const preflight = await raceFatal(() => browserPreflight(servers));
        assertExactKeys(
          preflight,
          ['httpsReachable', 'wssReachable', 'certificateTrusted'],
          'browser preflight result',
        );
        if (
          preflight.httpsReachable !== true ||
          preflight.wssReachable !== true ||
          preflight.certificateTrusted !== true
        ) {
          fail(
            'controlled canary browser preflight did not prove TLS and WSS reachability',
          );
        }
        return await raceFatal(() => operation(servers));
      } finally {
        await monitor.stop();
      }
    });
  } finally {
    try {
      await servers?.close?.();
    } finally {
      await material.cleanup?.();
    }
  }
}

function normalizedFingerprint(value, label) {
  if (
    typeof value !== 'string' ||
    !/^[0-9a-f]{2}(?::[0-9a-f]{2})+$/i.test(value)
  ) {
    fail(`${label} must be a colon-delimited certificate fingerprint`);
  }
  return value.replaceAll(':', '').toUpperCase();
}

export function controlledVmTrustCommands({
  platform,
  caPath,
  sha1Fingerprint,
  sha256Fingerprint,
  loginKeychain,
}) {
  if (typeof caPath !== 'string' || caPath.length === 0) {
    fail('controlled-VM CA path is required');
  }
  const sha1 = normalizedFingerprint(sha1Fingerprint, 'SHA-1 fingerprint');
  normalizedFingerprint(sha256Fingerprint, 'SHA-256 fingerprint');
  if (platform === 'win32') {
    return {
      install: {
        file: 'certutil.exe',
        args: ['-user', '-addstore', 'Root', caPath],
      },
      remove: {
        file: 'certutil.exe',
        args: ['-user', '-delstore', 'Root', sha1],
      },
    };
  }
  if (platform === 'darwin') {
    if (typeof loginKeychain !== 'string' || loginKeychain.length === 0) {
      fail('controlled-VM macOS login keychain path is required');
    }
    return {
      install: {
        file: '/usr/bin/security',
        args: [
          'add-trusted-cert',
          '-d',
          '-r',
          'trustRoot',
          '-k',
          loginKeychain,
          caPath,
        ],
      },
      remove: {
        file: '/usr/bin/security',
        args: ['delete-certificate', '-Z', sha1, loginKeychain],
      },
    };
  }
  fail('controlled canary trust supports only Windows and macOS');
}

export function createControlledVmTrustAdapter({
  controlledVm,
  platform = process.platform,
  loginKeychain,
  execFile,
} = {}) {
  if (controlledVm !== true) {
    fail('system trust changes require an explicit controlled VM opt-in');
  }
  if (typeof execFile !== 'function') {
    fail('controlled-VM trust adapter requires an injected command executor');
  }
  return {
    async install(material) {
      const plan = controlledVmTrustCommands({
        platform,
        caPath: material.caPath,
        sha1Fingerprint: material.sha1Fingerprint,
        sha256Fingerprint: material.sha256Fingerprint,
        loginKeychain,
      });
      await execFile(plan.install.file, plan.install.args);
      return { plan, runId: material.runId };
    },
    async remove(receipt) {
      if (!receipt?.plan?.remove) {
        fail('controlled-VM trust removal receipt is missing');
      }
      await execFile(receipt.plan.remove.file, receipt.plan.remove.args);
    },
  };
}

function parseOpenSslFingerprint(output, algorithm) {
  const normalized = String(output).trim();
  const match = new RegExp(
    `^${algorithm} Fingerprint=([0-9A-F]{2}(?::[0-9A-F]{2})+)$`,
    'i',
  ).exec(normalized);
  if (!match) {
    fail(`OpenSSL did not return an exact ${algorithm} fingerprint`);
  }
  return match[1].toUpperCase();
}

export async function createPerRunCanaryCertificate(
  { runId, rootDirectory },
  dependencies = {},
) {
  assertRunId(runId, 'certificate run id');
  const deps = {
    mkdir,
    readFile,
    rm,
    writeFile,
    execFile: async (file, args) =>
      execFileSync(file, args, { encoding: 'utf8', windowsHide: true }),
    ...dependencies,
  };
  const root = resolve(rootDirectory);
  const runDirectory = resolve(root, runId);
  if (!outputIsBelowDirectory(runDirectory, root)) {
    fail('certificate directory must remain below the ignored canary root');
  }
  const configPath = join(runDirectory, 'openssl.cnf');
  const caKeyPath = join(runDirectory, 'ca.key');
  const caPath = join(runDirectory, 'ca.pem');
  const serverKeyPath = join(runDirectory, 'server.key');
  const serverRequestPath = join(runDirectory, 'server.csr');
  const serverCertPath = join(runDirectory, 'server.pem');
  const opensslConfig = `[req]
distinguished_name = req_distinguished_name
prompt = no
req_extensions = v3_req

[req_distinguished_name]
CN = yinmi-controlled-canary-${runId}

[v3_req]
basicConstraints = critical,CA:FALSE
keyUsage = critical,digitalSignature,keyEncipherment
extendedKeyUsage = serverAuth
subjectAltName = @alt_names

[alt_names]
IP.1 = 127.0.0.1
`;
  await deps.mkdir(runDirectory, { recursive: true });
  try {
    await deps.writeFile(configPath, opensslConfig, {
      encoding: 'utf8',
      mode: 0o600,
    });
    const commands = [
      [
        'genpkey',
        '-algorithm',
        'RSA',
        '-pkeyopt',
        'rsa_keygen_bits:2048',
        '-out',
        caKeyPath,
      ],
      [
        'req',
        '-x509',
        '-new',
        '-key',
        caKeyPath,
        '-sha256',
        '-days',
        '1',
        '-subj',
        `/CN=yinmi-controlled-canary-root-${runId}`,
        '-set_serial',
        `0x${runId}`,
        '-out',
        caPath,
      ],
      [
        'genpkey',
        '-algorithm',
        'RSA',
        '-pkeyopt',
        'rsa_keygen_bits:2048',
        '-out',
        serverKeyPath,
      ],
      [
        'req',
        '-new',
        '-key',
        serverKeyPath,
        '-out',
        serverRequestPath,
        '-config',
        configPath,
      ],
      [
        'x509',
        '-req',
        '-in',
        serverRequestPath,
        '-CA',
        caPath,
        '-CAkey',
        caKeyPath,
        '-set_serial',
        `0x${runId.slice(0, 30)}01`,
        '-days',
        '1',
        '-sha256',
        '-extfile',
        configPath,
        '-extensions',
        'v3_req',
        '-out',
        serverCertPath,
      ],
    ];
    for (const args of commands) {
      await deps.execFile('openssl', args);
    }
    const sha1Output = await deps.execFile('openssl', [
      'x509',
      '-in',
      caPath,
      '-noout',
      '-fingerprint',
      '-sha1',
    ]);
    const sha256Output = await deps.execFile('openssl', [
      'x509',
      '-in',
      caPath,
      '-noout',
      '-fingerprint',
      '-sha256',
    ]);
    return {
      runId,
      caPath,
      key: await deps.readFile(serverKeyPath),
      cert: await deps.readFile(serverCertPath),
      sha1Fingerprint: parseOpenSslFingerprint(sha1Output, 'sha1'),
      sha256Fingerprint: parseOpenSslFingerprint(sha256Output, 'sha256'),
      cleanup: () => deps.rm(runDirectory, { recursive: true, force: true }),
    };
  } catch (error) {
    await deps.rm(runDirectory, { recursive: true, force: true });
    throw error;
  }
}

function assertKnownPhase(phase, label = 'phase') {
  if (!PHASES.includes(phase)) {
    fail(`${label} must be one of ${PHASES.join(', ')}`);
  }
}

function parseVersion(value, label) {
  if (typeof value !== 'string') {
    fail(`${label} must be a version string`);
  }
  const match = /^(\d+)\.(\d+)(?:\.(\d+))?$/.exec(value);
  if (!match) {
    fail(`${label} must be an exact numeric product version`);
  }
  return match.slice(1).map((part) => (part === undefined ? 0 : Number(part)));
}

function validateOsVersion(platformId, osVersion) {
  if (platformId === 'windows-10-webview2-111-x64') {
    if (osVersion !== '10.0.19045') {
      fail(`${platformId} osVersion must equal 10.0.19045`);
    }
    return;
  }

  if (platformId === 'windows-11-x64') {
    const match = /^10\.0\.(\d+)$/.exec(osVersion);
    if (!match || Number(match[1]) < 22_000) {
      fail(`${platformId} osVersion must be a Windows 11 build at least 22000`);
    }
    return;
  }

  if (platformId === 'macos-13-intel') {
    if (!/^13\.3(?:\.\d+)?$/.test(osVersion)) {
      fail(`${platformId} osVersion must equal 13.3 or 13.3.x`);
    }
    return;
  }

  const [major, minor] = parseVersion(osVersion, `${platformId} osVersion`);
  if (major < 13 || (major === 13 && minor < 3)) {
    fail(`${platformId} osVersion must be at least 13.3`);
  }
}

function expectedMatrix(platformId) {
  const matrix = PLATFORM_MATRIX[platformId];
  if (!matrix) {
    fail(
      `platform id must be one of ${Object.keys(PLATFORM_MATRIX).join(', ')}`,
    );
  }
  return matrix;
}

export function validateHostFacts(platformId, hostFacts) {
  const matrix = expectedMatrix(platformId);
  assertExactKeys(hostFacts, HOST_FACT_KEYS, 'host facts');
  for (const field of ['hostPlatform', 'hostArch']) {
    if (hostFacts[field] !== matrix[field]) {
      fail(
        `${platformId} ${field} must equal ${JSON.stringify(matrix[field])}`,
      );
    }
  }
  validateOsVersion(platformId, hostFacts.osVersion);
  return { ...hostFacts };
}

function validateTargetFacts(platformId, targetFacts, previousTargetFacts) {
  const matrix = expectedMatrix(platformId);
  if (previousTargetFacts) {
    for (const field of TARGET_FACT_KEYS) {
      if (targetFacts[field] !== previousTargetFacts[field]) {
        fail(`process-info ${field} must agree with the first phase`);
      }
    }
  }
  if (
    platformId.startsWith('macos-') &&
    targetFacts.translatedProcess === true
  ) {
    fail(
      'translatedProcess must be false; Rosetta-translated probes are invalid',
    );
  }
  for (const field of TARGET_FACT_KEYS) {
    if (targetFacts[field] !== matrix[field]) {
      fail(
        `${platformId} platform correlation requires ${field}=${JSON.stringify(matrix[field])}`,
      );
    }
  }
  return { ...targetFacts };
}

export function createRunId(randomSource = randomBytes) {
  const bytes = randomSource(16);
  if (!(bytes instanceof Uint8Array) || bytes.byteLength !== 16) {
    fail('run-id random source must return exactly 16 bytes');
  }
  return Buffer.from(bytes).toString('hex');
}

function decodeExactJsonBodyWithLimit(
  body,
  expectedKeys,
  label,
  byteLimit,
  limitLabel,
) {
  let bytes;
  if (typeof body === 'string') {
    bytes = Buffer.from(body, 'utf8');
  } else if (body instanceof Uint8Array) {
    bytes = Buffer.from(body);
  } else {
    fail(`${label} body must be UTF-8 bytes or a string`);
  }
  if (bytes.byteLength > byteLimit) {
    fail(`${label} body exceeds the ${limitLabel} limit`);
  }

  let value;
  try {
    value = JSON.parse(bytes.toString('utf8'));
  } catch {
    fail(`${label} body must be valid JSON`);
  }
  assertExactKeys(value, expectedKeys, `${label} body`);
  return value;
}

export function decodeExactJsonBody(body, expectedKeys, label) {
  return decodeExactJsonBodyWithLimit(
    body,
    expectedKeys,
    label,
    BODY_LIMIT_BYTES,
    '4096-byte (4 KiB)',
  );
}

function validateIsolationReportPayload(report, correlation) {
  assertExactKeys(report, ISOLATION_REPORT_KEYS, 'isolation report payload');
  validateSignatureProbeSemantics(report, correlation);
  if (
    report.currentUrl !== 'https://music.gdstudio.xyz/' ||
    report.finalUrl !== 'https://music.gdstudio.xyz/'
  ) {
    fail('isolation report URLs must be the exact official page');
  }
  if (
    !Array.isArray(report.hostLabelsAfterDestroy) ||
    report.hostLabelsAfterDestroy.length !== 0
  ) {
    fail('isolation report must have no host labels after destroy');
  }
}

export class LifecycleRecorder {
  #activePhase = null;
  #hostFacts;
  #nextOrdinal = 1;
  #nextPhaseIndex = 0;
  #platformId;
  #runId;
  #targetFacts = null;
  #traces = [];

  constructor({ runId, platformId, hostFacts }) {
    assertRunId(runId);
    this.#runId = runId;
    this.#platformId = platformId;
    this.#hostFacts = validateHostFacts(platformId, hostFacts);
  }

  beginPhase(phase) {
    if (this.#activePhase) {
      fail(`phase ${this.#activePhase.phase} is already active`);
    }
    if (this.#nextPhaseIndex >= PHASES.length) {
      fail('two-phase lifecycle is complete; an extra phase is forbidden');
    }
    const expected = PHASES[this.#nextPhaseIndex];
    if (phase !== expected) {
      fail(`expected phase ${expected}, received ${String(phase)}`);
    }
    this.#activePhase = {
      phase,
      processInfo: null,
      canaryConfigIssued: false,
      nextEventIndex: 0,
      events: [],
    };
    this.#traces.push(this.#activePhase);
  }

  #requireActivePhase(phase, label) {
    if (!this.#activePhase) {
      fail(`${label} has no active phase and is stale or replayed`);
    }
    if (phase !== this.#activePhase.phase) {
      fail(`${label} phase must match active phase ${this.#activePhase.phase}`);
    }
    return this.#activePhase;
  }

  #validateEnvelope(body, expectedKeys, label) {
    assertExactKeys(body, expectedKeys, label);
    if (body.runId !== this.#runId) {
      fail(`${label} run id must match the recorder nonce`);
    }
    assertKnownPhase(body.phase, `${label} phase`);
    return this.#requireActivePhase(body.phase, label);
  }

  acceptProcessInfo(body) {
    const active = this.#validateEnvelope(
      body,
      PROCESS_INFO_KEYS,
      'process-info',
    );
    if (active.processInfo) {
      fail(
        `process-info may be submitted exactly once for phase ${body.phase}`,
      );
    }
    const targetFacts = Object.fromEntries(
      TARGET_FACT_KEYS.map((key) => [key, body[key]]),
    );
    active.processInfo = validateTargetFacts(
      this.#platformId,
      targetFacts,
      this.#targetFacts,
    );
    this.#targetFacts ??= { ...active.processInfo };
    return {
      accepted: 'process-info',
      runId: this.#runId,
      kind: 'lifecycle',
      phase: active.phase,
    };
  }

  acceptEvent(body) {
    const active = this.#validateEnvelope(body, EVENT_KEYS, 'event');
    if (!active.processInfo) {
      fail('process-info must be accepted first for the active phase');
    }
    if (body.event === EXTERNAL_EXIT_EVENT) {
      fail(
        'process-exit-observed is external-only and forbidden in child events',
      );
    }
    if (typeof body.event !== 'string') {
      fail('event name must be a string');
    }
    const childGrammar = PHASE_GRAMMARS[active.phase].slice(0, -1);
    if (active.nextEventIndex >= childGrammar.length) {
      fail(
        `phase ${active.phase} accepts no more child events; ${EXTERNAL_EXIT_EVENT} must be external`,
      );
    }
    const expected = childGrammar[active.nextEventIndex];
    if (body.event !== expected) {
      fail(`phase ${active.phase} expected event ${expected}`);
    }
    active.events.push({ ordinal: this.#nextOrdinal, event: body.event });
    this.#nextOrdinal += 1;
    active.nextEventIndex += 1;
  }

  acceptCanaryConfigRequest(body, config) {
    const active = this.#validateEnvelope(
      body,
      ['runId', 'phase'],
      'canary-config',
    );
    if (!active.processInfo) {
      fail('process-info must be acknowledged before canary config');
    }
    if (active.canaryConfigIssued) {
      fail('canary config may be issued once; replay is forbidden');
    }
    validateCanaryConfig(config, {
      runId: this.#runId,
      phase: active.phase,
      platformId: this.#platformId,
    });
    active.canaryConfigIssued = true;
    return { ...config };
  }

  observeProcessExit(phase, exit) {
    const active = this.#requireActivePhase(phase, 'process exit');
    if (!active.processInfo) {
      fail('process-info is missing before process exit');
    }
    if (exit?.forced === true) {
      fail('a forced process exit is not a clean child exit');
    }
    if (exit?.signal !== null) {
      fail(
        `child process exit must not have a signal (${String(exit?.signal)})`,
      );
    }
    if (exit?.code !== 0) {
      fail(
        `child process exit code must be zero (received ${String(exit?.code)})`,
      );
    }
    const childGrammar = PHASE_GRAMMARS[active.phase].slice(0, -1);
    if (active.nextEventIndex !== childGrammar.length) {
      fail(
        `phase ${active.phase} is missing event ${childGrammar[active.nextEventIndex]}`,
      );
    }
    active.events.push({
      ordinal: this.#nextOrdinal,
      event: EXTERNAL_EXIT_EVENT,
    });
    this.#nextOrdinal += 1;
    this.#activePhase = null;
    this.#nextPhaseIndex += 1;
  }

  assertComplete() {
    if (this.#activePhase) {
      fail(`phase ${this.#activePhase.phase} is incomplete`);
    }
    if (this.#nextPhaseIndex !== PHASES.length) {
      fail(`lifecycle is missing phase ${PHASES[this.#nextPhaseIndex]}`);
    }
  }

  hostFacts() {
    return { ...this.#hostFacts };
  }

  targetFacts() {
    return {
      ...Object.fromEntries(
        TARGET_FACT_KEYS.map((key) => [
          key,
          (this.#targetFacts ?? PLATFORM_MATRIX[this.#platformId])[key],
        ]),
      ),
    };
  }

  traces() {
    return this.#traces.map(({ phase, events }) => ({
      phase,
      events: events.map((item) => ({ ...item })),
    }));
  }
}

export class IsolationRecorder {
  #canaryConfigIssued = false;
  #hostFacts;
  #platformId;
  #processExited = false;
  #processInfo = null;
  #report = null;
  #runId;
  #sealed = false;

  constructor({ runId, platformId, hostFacts }) {
    assertRunId(runId);
    this.#runId = runId;
    this.#platformId = platformId;
    this.#hostFacts = validateHostFacts(platformId, hostFacts);
  }

  #validateEnvelope(body, expectedKeys, label) {
    assertExactKeys(body, expectedKeys, label);
    if (body.runId !== this.#runId) {
      fail(`${label} run id must match the isolation recorder nonce`);
    }
    if (body.phase !== WRITE_PHASE) {
      fail(`${label} phase must equal ${WRITE_PHASE}`);
    }
  }

  acceptProcessInfo(body) {
    this.#validateEnvelope(body, PROCESS_INFO_KEYS, 'process-info');
    if (this.#processInfo) {
      fail(
        'isolation process-info may be submitted exactly once; replay is forbidden',
      );
    }
    this.#processInfo = validateTargetFacts(
      this.#platformId,
      Object.fromEntries(TARGET_FACT_KEYS.map((key) => [key, body[key]])),
      null,
    );
    return {
      accepted: 'process-info',
      runId: this.#runId,
      kind: 'isolation',
      phase: WRITE_PHASE,
    };
  }

  acceptEvent() {
    fail('isolation pre-stage event submissions are forbidden');
  }

  acceptCanaryConfigRequest(body, config) {
    this.#validateEnvelope(body, ['runId', 'phase'], 'canary-config');
    if (!this.#processInfo) {
      fail('process-info must be acknowledged before isolation canary config');
    }
    if (this.#canaryConfigIssued) {
      fail('isolation canary config may be issued once; replay is forbidden');
    }
    validateCanaryConfig(config, {
      runId: this.#runId,
      phase: WRITE_PHASE,
      platformId: this.#platformId,
    });
    this.#canaryConfigIssued = true;
    return { ...config };
  }

  acceptIsolationReport(body) {
    this.#validateEnvelope(
      body,
      ISOLATION_REPORT_ENVELOPE_KEYS,
      'isolation report',
    );
    if (!this.#processInfo) {
      fail('process-info must be acknowledged before the isolation report');
    }
    if (this.#report) {
      fail(
        'isolation report may be submitted exactly once; replay is forbidden',
      );
    }
    const report = body.report;
    validateIsolationReportPayload(report, {
      platformId: this.#platformId,
      ...this.#hostFacts,
      ...this.#processInfo,
    });
    this.#report = JSON.parse(JSON.stringify(report));
    return {
      accepted: 'isolation-report',
      runId: this.#runId,
      kind: 'isolation',
      phase: WRITE_PHASE,
    };
  }

  observeProcessExit(exit) {
    if (!this.#processInfo || !this.#report) {
      fail('isolation process exit requires process-info and one report first');
    }
    if (this.#processExited) {
      fail('isolation process exit may be observed once');
    }
    if (exit?.forced === true || exit?.signal !== null || exit?.code !== 0) {
      fail(
        'isolation child requires a zero-code, non-signaled, non-forced exit',
      );
    }
    this.#processExited = true;
  }

  seal(runnerObservation) {
    if (!this.#processExited || !this.#report) {
      fail(
        'isolation recorder cannot seal before report and clean process exit',
      );
    }
    if (this.#sealed) {
      fail('isolation recorder seal may be consumed once; replay is forbidden');
    }
    assertExactKeys(
      runnerObservation,
      ISOLATION_RUNNER_OBSERVATION_KEYS,
      'isolation runner observation',
    );
    if (
      runnerObservation.httpsPreflightHits !== 1 ||
      runnerObservation.wssPreflightHandshakes !== 1
    ) {
      fail(
        'runner must observe exactly one Rust HTTPS and WSS browser preflight',
      );
    }
    this.#sealed = true;
    return {
      report: JSON.parse(JSON.stringify(this.#report)),
      runnerObservation: { ...runnerObservation },
    };
  }

  hostFacts() {
    return { ...this.#hostFacts };
  }

  targetFacts() {
    return {
      ...Object.fromEntries(
        TARGET_FACT_KEYS.map((key) => [
          key,
          (this.#processInfo ?? PLATFORM_MATRIX[this.#platformId])[key],
        ]),
      ),
    };
  }

  traces() {
    return [];
  }
}

function validateCanaryOrigin(raw, scheme, label) {
  let url;
  try {
    url = new URL(raw);
  } catch {
    fail(`${label} must be a valid URL origin`);
  }
  if (
    url.protocol !== `${scheme}:` ||
    url.hostname !== '127.0.0.1' ||
    url.port === '' ||
    url.pathname !== '/' ||
    url.search !== '' ||
    url.hash !== '' ||
    url.username !== '' ||
    url.password !== ''
  ) {
    fail(
      `${label} must be an exact OS-assigned IPv4 loopback ${scheme} origin`,
    );
  }
  return url;
}

export function validateCanaryConfig(config, expected) {
  assertExactKeys(config, CANARY_CONFIG_KEYS, 'canary config');
  if (
    config.runId !== expected.runId ||
    config.phase !== expected.phase ||
    config.platformId !== expected.platformId
  ) {
    fail('canary config correlation must match run id, phase, and platform id');
  }
  assertRunId(config.runId, 'canary config run id');
  assertKnownPhase(config.phase, 'canary config phase');
  expectedMatrix(config.platformId);
  if (config.idleDurationMs !== CANARY_IDLE_DURATION_MS) {
    fail('canary config idle duration must be exactly ten minutes');
  }
  const control = validateCanaryOrigin(
    config.controlOrigin,
    'http',
    'canary control origin',
  );
  const allowed = validateCanaryOrigin(
    config.allowedOrigin,
    'https',
    'canary allowed origin',
  );
  const blockedHttp = validateCanaryOrigin(
    config.blockedHttpOrigin,
    'http',
    'canary blocked HTTP origin',
  );
  const blockedHttps = validateCanaryOrigin(
    config.blockedHttpsOrigin,
    'https',
    'canary blocked HTTPS origin',
  );
  const blockedWs = validateCanaryOrigin(
    config.blockedWsOrigin,
    'ws',
    'canary blocked WS origin',
  );
  const blockedWss = validateCanaryOrigin(
    config.blockedWssOrigin,
    'wss',
    'canary blocked WSS origin',
  );
  if (
    control.port !== blockedHttp.port ||
    control.port !== blockedWs.port ||
    blockedHttps.port !== blockedWss.port ||
    allowed.port === blockedHttps.port
  ) {
    fail('canary config origin port correlation failed');
  }
  return { ...config };
}

export function routeSubmission(request, recorder, canaryConfig) {
  if (request.method !== 'POST') {
    fail('recorder accepts only the POST method');
  }
  if (request.path === PROCESS_INFO_PATH) {
    return recorder.acceptProcessInfo(
      decodeExactJsonBody(request.body, PROCESS_INFO_KEYS, 'process-info'),
    );
  }
  if (request.path === EVENT_PATH) {
    recorder.acceptEvent(
      decodeExactJsonBody(request.body, EVENT_KEYS, 'event'),
    );
    return { accepted: 'event' };
  }
  if (request.path === CANARY_CONFIG_PATH) {
    if (canaryConfig === undefined) {
      fail('canary config is unavailable on this recorder');
    }
    return recorder.acceptCanaryConfigRequest(
      decodeExactJsonBody(request.body, ['runId', 'phase'], 'canary-config'),
      canaryConfig,
    );
  }
  if (request.path === ISOLATION_REPORT_PATH) {
    if (typeof recorder.acceptIsolationReport !== 'function') {
      fail('isolation reports are unavailable on this recorder');
    }
    return recorder.acceptIsolationReport(
      decodeExactJsonBodyWithLimit(
        request.body,
        ISOLATION_REPORT_ENVELOPE_KEYS,
        'isolation report',
        ISOLATION_REPORT_BODY_LIMIT_BYTES,
        '262144-byte (256 KiB)',
      ),
    );
  }
  fail('unknown recorder endpoint path');
}

function outputIsBelowDirectory(outputPath, directoryPath) {
  const pathFromDirectory = relative(directoryPath, outputPath);
  return (
    pathFromDirectory.length > 0 &&
    !pathFromDirectory.startsWith(`..${sep}`) &&
    pathFromDirectory !== '..' &&
    !isAbsolute(pathFromDirectory)
  );
}

export function parseArguments(argv, { cwd = process.cwd() } = {}) {
  if (!Array.isArray(argv) || argv.length !== 6) {
    fail(USAGE);
  }
  const acceptedFlags = new Set(['--app', '--platform-id', '--output']);
  const values = new Map();
  for (let index = 0; index < argv.length; index += 2) {
    const flag = argv[index];
    const value = argv[index + 1];
    if (!acceptedFlags.has(flag)) {
      fail(`unknown argument ${String(flag)}; ${USAGE}`);
    }
    if (values.has(flag)) {
      fail(`argument ${flag} may be supplied exactly once; ${USAGE}`);
    }
    if (
      typeof value !== 'string' ||
      value.length === 0 ||
      value.startsWith('--')
    ) {
      fail(`argument ${flag} requires exactly one value; ${USAGE}`);
    }
    values.set(flag, value);
  }
  for (const flag of acceptedFlags) {
    if (!values.has(flag)) fail(`missing argument ${flag}; ${USAGE}`);
  }

  const platformId = values.get('--platform-id');
  expectedMatrix(platformId);
  const appPath = resolve(cwd, values.get('--app'));
  const outputPath = resolve(cwd, values.get('--output'));
  const outputDirectory = resolve(cwd, OUTPUT_DIRECTORY);
  if (!outputIsBelowDirectory(outputPath, outputDirectory)) {
    fail(`output must be below ${OUTPUT_DIRECTORY}`);
  }
  return { appPath, platformId, outputPath };
}

export function buildChildEnvironment(
  baseEnvironment,
  { phase, endpoint, runId },
) {
  assertKnownPhase(phase);
  assertRunId(runId);
  let endpointUrl;
  try {
    endpointUrl = new URL(endpoint);
  } catch {
    fail('trace endpoint must be a valid loopback HTTP URL');
  }
  if (
    endpointUrl.protocol !== 'http:' ||
    endpointUrl.hostname !== '127.0.0.1' ||
    endpointUrl.pathname !== '/' ||
    endpointUrl.search !== '' ||
    endpointUrl.hash !== '' ||
    endpointUrl.port === ''
  ) {
    fail('trace endpoint must be an origin on OS-assigned IPv4 loopback');
  }

  const environment = Object.fromEntries(
    Object.entries(baseEnvironment).filter(
      ([key]) =>
        !key.toUpperCase().startsWith(SIGNATURE_ENV_PREFIX) &&
        key.toUpperCase() !== CONTROLLED_VM_ENV,
    ),
  );
  return {
    ...environment,
    YINMI_FEASIBILITY_SIGNATURE_AUTORUN: phase,
    YINMI_FEASIBILITY_SIGNATURE_TRACE_ENDPOINT: endpoint,
    YINMI_FEASIBILITY_SIGNATURE_RUN_ID: runId,
  };
}

export function waitForCleanChildExit(child) {
  return new Promise((resolveExit, rejectExit) => {
    let settled = false;
    const cleanup = () => {
      child.removeListener('error', onError);
      child.removeListener('exit', onExit);
    };
    const rejectOnce = (error) => {
      if (settled) return;
      settled = true;
      cleanup();
      rejectExit(error);
    };
    const onError = (error) => {
      rejectOnce(new Error(`child process failed to launch: ${error.message}`));
    };
    const onExit = (code, signal) => {
      if (settled) return;
      if (signal !== null) {
        rejectOnce(new Error(`child process exited from signal ${signal}`));
        return;
      }
      if (code !== 0) {
        rejectOnce(
          new Error(
            `child process must exit with code zero; received code ${code}`,
          ),
        );
        return;
      }
      settled = true;
      cleanup();
      resolveExit({ code, signal });
    };

    child.once('error', onError);
    child.once('exit', onExit);
    if (child.exitCode !== undefined && child.exitCode !== null) {
      queueMicrotask(() => onExit(child.exitCode, child.signalCode ?? null));
    }
  });
}

export async function withDeadline(
  work,
  milliseconds,
  label,
  timerApi = {
    setTimeout: globalThis.setTimeout,
    clearTimeout: globalThis.clearTimeout,
  },
) {
  let timeoutHandle;
  const timeout = new Promise((_, rejectTimeout) => {
    timeoutHandle = timerApi.setTimeout(
      () =>
        rejectTimeout(
          new Error(`${label} timed out after ${milliseconds} milliseconds`),
        ),
      milliseconds,
    );
  });
  const operation = Promise.resolve().then(() =>
    typeof work === 'function' ? work() : work,
  );
  try {
    return await Promise.race([operation, timeout]);
  } finally {
    timerApi.clearTimeout(timeoutHandle);
  }
}

function redactMessage(message, secrets) {
  let sanitized = String(message);
  const orderedSecrets = [...new Set(secrets.filter((secret) => secret))].sort(
    (left, right) => String(right).length - String(left).length,
  );
  for (const secret of orderedSecrets) {
    sanitized = sanitized.split(String(secret)).join('[redacted]');
  }
  sanitized = sanitized.replace(/\b[0-9a-f]{32}\b/gi, '[redacted]');
  const withoutUrls = sanitized.replace(/https?:\/\/[^\s)\]>'"]+/gi, '');
  const containsAbsoluteLocalPath =
    /[A-Za-z]:[\\/]/.test(withoutUrls) ||
    /(^|[\s('"=])\\\\[^\s]/.test(withoutUrls) ||
    /(^|[\s('"=])\/(?!\/)[^\s]/.test(withoutUrls);
  if (
    containsAbsoluteLocalPath ||
    /-----BEGIN(?: [A-Z0-9]+)? PRIVATE KEY-----/i.test(sanitized) ||
    /(?:secret|sentinel)/i.test(sanitized)
  ) {
    return '[redacted]';
  }
  return sanitized;
}

function redactStructuredValue(value, secrets) {
  if (typeof value === 'string') return redactMessage(value, secrets);
  if (Array.isArray(value)) {
    return value.map((item) => redactStructuredValue(item, secrets));
  }
  if (value && typeof value === 'object') {
    return Object.fromEntries(
      Object.entries(value).map(([key, item]) => [
        key,
        redactStructuredValue(item, secrets),
      ]),
    );
  }
  return value;
}

export function buildSanitizedOutput({
  status,
  platformId,
  hostFacts,
  recorder,
  isolation,
  failure,
  forcedKill = false,
  secrets = [],
}) {
  const targetFacts = recorder.targetFacts();
  const output = {
    status,
    platformId,
    hostPlatform: hostFacts.hostPlatform,
    hostArch: hostFacts.hostArch,
    osVersion: hostFacts.osVersion,
    binaryTargetOs: targetFacts.binaryTargetOs,
    binaryTargetArch: targetFacts.binaryTargetArch,
    translatedProcess: targetFacts.translatedProcess,
    traces: recorder.traces(),
  };
  if (isolation !== undefined) {
    output.isolation = redactStructuredValue(isolation, secrets);
  }
  if (failure !== undefined && failure !== null) {
    output.failure = {
      message: redactMessage(failure.message ?? failure, secrets),
      forcedKill: Boolean(forcedKill),
    };
  }
  return output;
}

function mergeIsolationWithLifecycle(isolation, recorder) {
  const traces = recorder.traces();
  const requiredCleanupEvents = [
    'host-destroyed',
    'manager-host-absent',
    'policy-cleanup-acknowledged',
    'policy-tombstones-empty',
    'tls-entry-absent',
    'process-exit-observed',
  ];
  const ordinaryExitCleanupAcknowledged =
    traces.length === PHASES.length &&
    traces.every(({ events }) => {
      const names = new Set(events.map(({ event }) => event));
      return requiredCleanupEvents.every((name) => names.has(name));
    });
  const restartTrace = traces.find(({ phase }) => phase === VERIFY_PHASE);
  const restartStorageRecovered = !restartTrace?.events.some(
    ({ event }) => event === 'marker-absent',
  );
  if (
    ordinaryExitCleanupAcknowledged !== true ||
    restartStorageRecovered !== false
  ) {
    fail(
      'lifecycle traces did not prove external exit cleanup and restart isolation',
    );
  }
  return {
    report: {
      ...isolation.report,
      checks: {
        ...isolation.report.checks,
        ordinaryExitCleanupAcknowledged,
        restartStorageRecovered,
      },
    },
    runnerObservation: { ...isolation.runnerObservation },
  };
}

export function collectHostFacts({
  hostPlatform = process.platform,
  hostArch = process.arch,
  osRelease = release,
  execFile = execFileSync,
} = {}) {
  let osVersion;
  if (hostPlatform === 'win32') {
    osVersion = osRelease();
  } else if (hostPlatform === 'darwin') {
    osVersion = execFile('/usr/bin/sw_vers', ['-productVersion'], {
      encoding: 'utf8',
    });
  } else {
    fail(`signature lifecycle probes do not support host ${hostPlatform}`);
  }
  return {
    hostPlatform,
    hostArch,
    osVersion: String(osVersion).trim(),
  };
}

async function readRequestBody(
  request,
  byteLimit = BODY_LIMIT_BYTES,
  limitLabel = '4096-byte (4 KiB)',
) {
  const chunks = [];
  let byteLength = 0;
  let overflow = false;
  for await (const chunk of request) {
    const bytes = Buffer.from(chunk);
    byteLength += bytes.byteLength;
    if (byteLength > byteLimit) {
      overflow = true;
    } else if (!overflow) {
      chunks.push(bytes);
    }
  }
  if (overflow) {
    fail(`request body exceeds the ${limitLabel} limit`);
  }
  return Buffer.concat(chunks, byteLength);
}

function listenOnLoopback(server, port = 0) {
  return new Promise((resolveListen, rejectListen) => {
    const onError = (error) => {
      server.removeListener('listening', onListening);
      rejectListen(error);
    };
    const onListening = () => {
      server.removeListener('error', onError);
      resolveListen();
    };
    server.once('error', onError);
    server.once('listening', onListening);
    server.listen(port, '127.0.0.1');
  });
}

function closeServer(server) {
  return new Promise((resolveClose, rejectClose) => {
    if (!server.listening) {
      resolveClose();
      return;
    }
    server.close((error) => {
      if (error) rejectClose(error);
      else resolveClose();
    });
    server.closeAllConnections?.();
  });
}

function writeCanaryResponse(response, specification) {
  response.writeHead(specification.status, specification.headers);
  response.end(specification.body);
}

function websocketFrame(payload) {
  const bytes = Buffer.from(payload, 'utf8');
  if (bytes.byteLength > 125) {
    fail(
      'controlled canary WebSocket frame exceeds the fixed short-frame limit',
    );
  }
  return Buffer.concat([Buffer.from([0x81, bytes.byteLength]), bytes]);
}

function handleCanaryWebSocketUpgrade(request, socket, recorder, fatalChannel) {
  try {
    if (String(request.headers.upgrade).toLowerCase() !== 'websocket') {
      fail('canary upgrade must request WebSocket');
    }
    const key = request.headers['sec-websocket-key'];
    if (
      typeof key !== 'string' ||
      Buffer.from(key, 'base64').byteLength !== 16
    ) {
      fail('canary WebSocket key must encode exactly 16 bytes');
    }
    routeCanaryHit(request.url, 'websocket-handshake', recorder);
    const accept = createHash('sha1')
      .update(`${key}258EAFA5-E914-47DA-95CA-C5AB0DC85B11`, 'ascii')
      .digest('base64');
    socket.write(
      [
        'HTTP/1.1 101 Switching Protocols',
        'Upgrade: websocket',
        'Connection: Upgrade',
        `Sec-WebSocket-Accept: ${accept}`,
        '',
        '',
      ].join('\r\n'),
    );
    socket.write(websocketFrame('yinmi-canary'));
    socket.end(Buffer.from([0x88, 0x00]));
  } catch (error) {
    fatalChannel.reject(error);
    socket.destroy();
  }
}

function canaryFatalChannel() {
  let rejectFatal;
  let failed = false;
  const fatal = new Promise((_, reject) => {
    rejectFatal = reject;
  });
  fatal.catch(() => {});
  return {
    fatal,
    reject(error) {
      if (failed) return;
      failed = true;
      rejectFatal(error);
    },
  };
}

function serverPort(server, label) {
  const address = server.address();
  if (!address || typeof address === 'string') {
    fail(`${label} did not bind an IPv4 loopback port`);
  }
  return address.port;
}

export async function startControlledCanaryServers(
  { runId, material },
  {
    createHttpServer = createServer,
    createHttpsServer: makeHttpsServer = createHttpsServer,
    observePlatformState,
  } = {},
) {
  assertRunId(runId, 'controlled canary server run id');
  const recorder = new CanaryRecorder({ runId });
  const fatalChannel = canaryFatalChannel();
  let blockedHttpsOrigin;

  const controlServer = createHttpServer(async (request, response) => {
    try {
      if (request.url.startsWith('/canary/')) {
        const body = await readRequestBody(request);
        const platformState =
          typeof observePlatformState === 'function'
            ? await observePlatformState()
            : {};
        if (typeof observePlatformState === 'function') {
          assertExactKeys(
            platformState,
            [
              'browserProcessCount',
              'visibleWindowLeakObserved',
              'unexpectedActivationObserved',
            ],
            'controlled canary platform observation',
          );
        }
        const result = routeCanaryControl(
          {
            method: request.method,
            path: request.url,
            body,
          },
          recorder,
          platformState,
        );
        if (request.method === 'POST') response.writeHead(204).end();
        else {
          response
            .writeHead(200, {
              'content-type': 'application/json',
              'cache-control': 'no-store',
            })
            .end(JSON.stringify(result));
        }
        return;
      }
      writeCanaryResponse(
        response,
        buildCanaryRouteResponse({
          surface: 'blocked-http',
          path: request.url,
          blockedHttpsOrigin,
          recorder,
        }),
      );
    } catch (error) {
      fatalChannel.reject(error);
      if (!response.headersSent) response.writeHead(400);
      response.end();
    }
  });

  const allowedServer = makeHttpsServer(
    { key: material.key, cert: material.cert },
    (request, response) => {
      try {
        writeCanaryResponse(
          response,
          buildCanaryRouteResponse({
            surface: 'allowed-https',
            path: request.url,
            blockedHttpsOrigin,
            recorder,
          }),
        );
      } catch (error) {
        fatalChannel.reject(error);
        if (!response.headersSent) response.writeHead(400);
        response.end();
      }
    },
  );

  const blockedServer = makeHttpsServer(
    { key: material.key, cert: material.cert },
    (request, response) => {
      try {
        writeCanaryResponse(
          response,
          buildCanaryRouteResponse({
            surface: 'blocked-https',
            path: request.url,
            blockedHttpsOrigin,
            recorder,
          }),
        );
      } catch (error) {
        fatalChannel.reject(error);
        if (!response.headersSent) response.writeHead(400);
        response.end();
      }
    },
  );
  controlServer.on('upgrade', (request, socket) =>
    handleCanaryWebSocketUpgrade(request, socket, recorder, fatalChannel),
  );
  blockedServer.on('upgrade', (request, socket) =>
    handleCanaryWebSocketUpgrade(request, socket, recorder, fatalChannel),
  );
  for (const server of [controlServer, allowedServer, blockedServer]) {
    server.on('clientError', (error, socket) => {
      fatalChannel.reject(error);
      socket.destroy();
    });
  }

  const opened = [];
  try {
    await listenOnLoopback(controlServer);
    opened.push(controlServer);
    await listenOnLoopback(allowedServer);
    opened.push(allowedServer);
    await listenOnLoopback(blockedServer);
    opened.push(blockedServer);
  } catch (error) {
    await Promise.all(opened.map((server) => closeServer(server)));
    throw error;
  }
  const controlPort = serverPort(controlServer, 'canary control server');
  const allowedPort = serverPort(allowedServer, 'allowed HTTPS canary server');
  const blockedPort = serverPort(blockedServer, 'blocked HTTPS canary server');
  blockedHttpsOrigin = `https://127.0.0.1:${blockedPort}`;
  return {
    recorder,
    fatal: fatalChannel.fatal,
    controlOrigin: `http://127.0.0.1:${controlPort}`,
    allowedOrigin: `https://127.0.0.1:${allowedPort}/`,
    blockedHttpOrigin: `http://127.0.0.1:${controlPort}`,
    blockedHttpsOrigin,
    blockedWsOrigin: `ws://127.0.0.1:${controlPort}`,
    blockedWssOrigin: `wss://127.0.0.1:${blockedPort}`,
    markSleepWake: () =>
      recorder.observeSleepWake({
        runId,
        mode: 'lifecycle',
        vector: 'lifecycle',
      }),
    close: async () => {
      await Promise.all(
        [controlServer, allowedServer, blockedServer].map((server) =>
          closeServer(server),
        ),
      );
    },
  };
}

function exactPlatformSample(sample, label = 'controlled platform sample') {
  assertExactKeys(
    sample,
    ['browserProcessCount', 'visibleWindowCount', 'childForeground'],
    label,
  );
  for (const key of ['browserProcessCount', 'visibleWindowCount']) {
    if (!Number.isSafeInteger(sample[key]) || sample[key] < 0) {
      fail(`${label} ${key} must be a nonnegative safe integer`);
    }
  }
  if (typeof sample.childForeground !== 'boolean') {
    fail(`${label} childForeground must be boolean`);
  }
  return { ...sample };
}

async function executeControlledCommand(file, args) {
  return execFileSync(file, args, {
    encoding: 'utf8',
    windowsHide: true,
  });
}

export async function sampleControlledPlatformState(
  { platform, childPid },
  { execFile = executeControlledCommand } = {},
) {
  const pid = childPid ?? 0;
  if (!Number.isSafeInteger(pid) || pid < 0) {
    fail('controlled platform child PID must be a nonnegative safe integer');
  }
  if (platform === 'win32') {
    const script = [
      '$ErrorActionPreference = "Stop"',
      '$browserCount = @(Get-Process -Name msedgewebview2 -ErrorAction SilentlyContinue).Count',
      `$targetPid = ${pid}`,
      '$target = if ($targetPid -gt 0) { Get-Process -Id $targetPid -ErrorAction SilentlyContinue } else { $null }',
      '$visibleWindowCount = if ($null -ne $target -and $target.MainWindowHandle -ne 0) { 1 } else { 0 }',
      'Add-Type -TypeDefinition \'using System; using System.Runtime.InteropServices; public static class YinmiForegroundWindow { [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow(); [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr handle, out uint processId); }\'',
      '$foregroundPid = [uint32]0',
      '[void][YinmiForegroundWindow]::GetWindowThreadProcessId([YinmiForegroundWindow]::GetForegroundWindow(), [ref]$foregroundPid)',
      '$childForeground = $targetPid -gt 0 -and $foregroundPid -eq $targetPid',
      '[ordered]@{ browserProcessCount = [int]$browserCount; visibleWindowCount = [int]$visibleWindowCount; childForeground = [bool]$childForeground } | ConvertTo-Json -Compress',
    ].join('; ');
    const value = JSON.parse(
      String(
        await execFile('powershell.exe', [
          '-NoLogo',
          '-NoProfile',
          '-NonInteractive',
          '-Command',
          script,
        ]),
      ),
    );
    return exactPlatformSample(value);
  }
  if (platform === 'darwin') {
    const processList = String(
      await execFile('/bin/ps', ['-axo', 'pid=,comm=']),
    );
    const browserProcessCount = processList
      .split(/\r?\n/u)
      .filter((line) => /(?:^|\/)com\.apple\.WebKit\./u.test(line)).length;
    let visibleWindowCount = 0;
    let childForeground = false;
    if (pid > 0) {
      const appleScript = [
        'tell application "System Events"',
        `set matches to every application process whose unix id is ${pid}`,
        'if (count of matches) is 0 then return "0,false"',
        'set targetProcess to item 1 of matches',
        'set visibleCount to count of (windows of targetProcess whose visible is true)',
        'set isFrontmost to frontmost of targetProcess',
        'return (visibleCount as text) & "," & (isFrontmost as text)',
        'end tell',
      ];
      const raw = String(
        await execFile(
          '/usr/bin/osascript',
          appleScript.flatMap((line) => ['-e', line]),
        ),
      ).trim();
      const match = /^(\d+),(true|false)$/u.exec(raw);
      if (!match) fail('controlled macOS window sample was malformed');
      visibleWindowCount = Number(match[1]);
      childForeground = match[2] === 'true';
    }
    return exactPlatformSample({
      browserProcessCount,
      visibleWindowCount,
      childForeground,
    });
  }
  fail('controlled platform monitor supports only Windows and macOS');
}

export async function startControlledPlatformMonitor(
  { platform },
  { sample = sampleControlledPlatformState } = {},
) {
  if (!['win32', 'darwin'].includes(platform)) {
    fail('controlled platform monitor supports only Windows and macOS');
  }
  const initial = exactPlatformSample(
    await sample({ platform, childPid: null }),
  );
  let childPid = null;
  let expectedUi = null;
  let finalSample = null;
  let stopped = false;
  let visibleWindowLeakObserved = false;
  let unexpectedActivationObserved = false;
  const fatal = new Promise(() => {});

  const observe = async () => {
    if (stopped) fail('controlled platform monitor is stopped');
    const current = exactPlatformSample(await sample({ platform, childPid }));
    if (expectedUi) {
      visibleWindowLeakObserved ||=
        current.visibleWindowCount > expectedUi.visibleWindowCount;
      unexpectedActivationObserved ||=
        !expectedUi.childForeground && current.childForeground;
    }
    return {
      browserProcessCount: current.browserProcessCount,
      visibleWindowLeakObserved,
      unexpectedActivationObserved,
    };
  };

  return {
    fatal,
    registerChildProcess(pid) {
      if (!Number.isSafeInteger(pid) || pid < 1 || childPid !== null) {
        fail('controlled platform monitor accepts one positive child PID');
      }
      childPid = pid;
    },
    async processInfoAcknowledged() {
      if (childPid === null || expectedUi !== null) {
        fail('controlled platform process-info baseline is out of order');
      }
      expectedUi = exactPlatformSample(await sample({ platform, childPid }));
    },
    observe,
    async stop() {
      if (stopped) return;
      finalSample = exactPlatformSample(await sample({ platform, childPid }));
      stopped = true;
    },
    async verifyBaseline() {
      if (!stopped || !finalSample) {
        fail('controlled platform baseline requires a stopped monitor');
      }
      if (
        finalSample.browserProcessCount !== initial.browserProcessCount ||
        finalSample.visibleWindowCount !== 0 ||
        finalSample.childForeground ||
        visibleWindowLeakObserved ||
        unexpectedActivationObserved
      ) {
        fail('controlled platform state did not return to its clean baseline');
      }
    },
  };
}

function normalizedCanaryOrigin(raw) {
  return `${new URL(raw).origin}/`;
}

function buildControlledCanaryConfig({ runId, platformId, servers }) {
  return validateCanaryConfig(
    {
      runId,
      phase: WRITE_PHASE,
      platformId,
      controlOrigin: normalizedCanaryOrigin(servers.controlOrigin),
      allowedOrigin: normalizedCanaryOrigin(servers.allowedOrigin),
      blockedHttpOrigin: normalizedCanaryOrigin(servers.blockedHttpOrigin),
      blockedHttpsOrigin: normalizedCanaryOrigin(servers.blockedHttpsOrigin),
      blockedWsOrigin: normalizedCanaryOrigin(servers.blockedWsOrigin),
      blockedWssOrigin: normalizedCanaryOrigin(servers.blockedWssOrigin),
      idleDurationMs: CANARY_IDLE_DURATION_MS,
    },
    { runId, phase: WRITE_PHASE, platformId },
  );
}

function defaultControlledTrustAdapter({
  controlledVm,
  platform,
  environment,
}) {
  const home = environment.HOME || environment.USERPROFILE || homedir();
  return createControlledVmTrustAdapter({
    controlledVm,
    platform,
    loginKeychain:
      platform === 'darwin'
        ? join(home, 'Library', 'Keychains', 'login.keychain-db')
        : undefined,
    execFile: executeControlledCommand,
  });
}

function rejectingFatal(source, label) {
  if (!source || typeof source.then !== 'function') {
    fail(`${label} must expose a fatal promise`);
  }
  return Promise.resolve(source).then(
    () => {
      throw new Error(`${label} fatal channel resolved unexpectedly`);
    },
    (error) => Promise.reject(error),
  );
}

export async function startControlledIsolationRecorderServer(
  recorder,
  {
    runId,
    platformId,
    certificateRootDirectory,
    controlledVm = false,
    environment = process.env,
  },
  dependencies = {},
) {
  assertRunId(runId, 'controlled isolation run id');
  if (controlledVm !== true) {
    fail(
      `controlled isolation requires explicit runner opt-in ${CONTROLLED_VM_ENV}=1`,
    );
  }
  const matrix = expectedMatrix(platformId);
  if (
    typeof certificateRootDirectory !== 'string' ||
    certificateRootDirectory.length === 0
  ) {
    fail('controlled isolation certificate root directory is required');
  }
  const deps = {
    createCertificate: ({ runId: certificateRunId, rootDirectory }) =>
      createPerRunCanaryCertificate({
        runId: certificateRunId,
        rootDirectory,
      }),
    createTrustAdapter: ({
      controlledVm: adapterControlledVm,
      platform,
      environment: adapterEnvironment,
    }) =>
      defaultControlledTrustAdapter({
        controlledVm: adapterControlledVm,
        platform,
        environment: adapterEnvironment,
      }),
    startCanaryServers: startControlledCanaryServers,
    startPlatformMonitor: ({ platform }) =>
      startControlledPlatformMonitor({ platform }),
    startTraceServer: startRecorderServer,
    ...dependencies,
  };
  for (const name of [
    'createCertificate',
    'createTrustAdapter',
    'startCanaryServers',
    'startPlatformMonitor',
    'startTraceServer',
  ]) {
    if (typeof deps[name] !== 'function') {
      fail(`controlled isolation dependency ${name} must be a function`);
    }
  }

  let material;
  let monitor;
  let canaryServers;
  let trustAdapter;
  let trustReceipt;
  let trustInstalled = false;
  let traceServer;
  let closed = false;
  let cleanupComplete = false;

  const closeResources = async () => {
    if (closed) return;
    closed = true;
    let firstError;
    for (const cleanup of [
      () => traceServer?.close?.(),
      () => monitor?.stop?.(),
      () =>
        !trustInstalled ? undefined : trustAdapter?.remove?.(trustReceipt),
      () => canaryServers?.close?.(),
      () => material?.cleanup?.(),
    ]) {
      try {
        await cleanup();
      } catch (error) {
        firstError ??= error;
      }
    }
    cleanupComplete = firstError === undefined;
    if (firstError) throw firstError;
  };

  try {
    material = await deps.createCertificate({
      runId,
      rootDirectory: certificateRootDirectory,
    });
    monitor = await deps.startPlatformMonitor({
      runId,
      platformId,
      platform: matrix.hostPlatform,
    });
    for (const method of [
      'registerChildProcess',
      'processInfoAcknowledged',
      'observe',
      'stop',
      'verifyBaseline',
    ]) {
      if (typeof monitor?.[method] !== 'function') {
        fail(`controlled platform monitor must provide ${method}`);
      }
    }
    canaryServers = await deps.startCanaryServers(
      { runId, material },
      { observePlatformState: () => monitor.observe() },
    );
    if (!canaryServers?.recorder) {
      fail('controlled canary servers must expose their recorder');
    }
    trustAdapter = deps.createTrustAdapter({
      runId,
      platformId,
      platform: matrix.hostPlatform,
      controlledVm,
      environment,
    });
    if (
      !trustAdapter ||
      typeof trustAdapter.install !== 'function' ||
      typeof trustAdapter.remove !== 'function'
    ) {
      fail(
        'controlled isolation trust adapter must provide install and remove',
      );
    }
    trustReceipt = await trustAdapter.install(material);
    trustInstalled = true;
    const canaryConfig = buildControlledCanaryConfig({
      runId,
      platformId,
      servers: canaryServers,
    });
    traceServer = await deps.startTraceServer(recorder, {
      canaryConfig,
      onProcessInfoAccepted: () => monitor.processInfoAcknowledged(),
    });
    const fatalSources = [
      rejectingFatal(traceServer?.fatal, 'trace recorder'),
      rejectingFatal(canaryServers.fatal, 'controlled canary server'),
    ];
    if (monitor.fatal?.then) {
      fatalSources.push(
        rejectingFatal(monitor.fatal, 'controlled platform monitor'),
      );
    }
    const fatal = Promise.race(fatalSources);
    fatal.catch(() => {});
    return {
      endpoint: traceServer.endpoint,
      fatal,
      registerChildProcess(child) {
        monitor.registerChildProcess(child?.pid);
      },
      async isolationObservation() {
        const snapshot = canaryServers.recorder.snapshot({
          runId,
          mode: 'preflight',
          vector: 'preflight',
        });
        const observation = {
          httpsPreflightHits: snapshot.browserPreflightHits,
          wssPreflightHandshakes: snapshot.websocketHandshakes,
        };
        assertExactKeys(
          observation,
          ISOLATION_RUNNER_OBSERVATION_KEYS,
          'runner isolation observation',
        );
        return observation;
      },
      close: closeResources,
      async verifyBaseline() {
        if (!closed || !cleanupComplete) {
          fail('controlled isolation baseline requires successful close');
        }
        await monitor.verifyBaseline();
      },
    };
  } catch (error) {
    try {
      await closeResources();
    } catch {
      // Preserve the setup failure; cleanup was still attempted in full.
    }
    throw error;
  }
}

export function startProductionProbeServer(recorder, options) {
  if (options?.kind === 'isolation') {
    return startControlledIsolationRecorderServer(recorder, options);
  }
  if (options?.kind === 'lifecycle') {
    return startRecorderServer(recorder);
  }
  fail('production recorder kind must be isolation or lifecycle');
}

export async function startRecorderServer(
  recorder,
  { createHttpServer = createServer, canaryConfig, onProcessInfoAccepted } = {},
) {
  let rejectFatal;
  let failed = false;
  const fatal = new Promise((_, reject) => {
    rejectFatal = reject;
  });
  fatal.catch(() => {});
  const recordFatal = (error) => {
    if (failed) return;
    failed = true;
    rejectFatal(error);
  };

  const server = createHttpServer(async (request, response) => {
    try {
      const isolationReport = request.url === ISOLATION_REPORT_PATH;
      const body = await readRequestBody(
        request,
        isolationReport ? ISOLATION_REPORT_BODY_LIMIT_BYTES : BODY_LIMIT_BYTES,
        isolationReport ? '262144-byte (256 KiB)' : '4096-byte (4 KiB)',
      );
      const result = routeSubmission(
        { method: request.method, path: request.url, body },
        recorder,
        canaryConfig,
      );
      if (
        request.url === PROCESS_INFO_PATH &&
        typeof onProcessInfoAccepted === 'function'
      ) {
        await onProcessInfoAccepted(result);
      }
      if (
        request.url === CANARY_CONFIG_PATH ||
        request.url === PROCESS_INFO_PATH ||
        request.url === ISOLATION_REPORT_PATH
      ) {
        response
          .writeHead(200, {
            'content-type': 'application/json',
            'cache-control': 'no-store',
          })
          .end(JSON.stringify(result));
      } else {
        response.writeHead(204).end();
      }
    } catch (error) {
      recordFatal(error);
      if (!response.headersSent) response.writeHead(400);
      response.end();
    }
  });
  server.on('clientError', (error, socket) => {
    recordFatal(error);
    socket.destroy();
  });
  await listenOnLoopback(server);
  const address = server.address();
  if (!address || typeof address === 'string') {
    await closeServer(server);
    fail('recorder did not bind an OS-assigned loopback port');
  }
  return {
    endpoint: `http://127.0.0.1:${address.port}`,
    fatal,
    close: () => closeServer(server),
  };
}

async function writeOutputArtifact(outputPath, output, dependencies) {
  await dependencies.mkdir(dirname(outputPath), { recursive: true });
  await dependencies.writeFile(
    outputPath,
    `${JSON.stringify(output, null, 2)}\n`,
    'utf8',
  );
}

function forceKillChild(child) {
  if (!child) return false;
  if (child.exitCode !== null && child.exitCode !== undefined) return false;
  if (child.signalCode !== null && child.signalCode !== undefined) return false;
  try {
    return child.kill() !== false;
  } catch {
    return false;
  }
}

export async function runLifecycleProbe(
  { appPath, platformId, outputPath },
  dependencies = {},
) {
  const deps = {
    environment: process.env,
    hostFacts: undefined,
    mkdir,
    randomSource: randomBytes,
    spawnChild: spawn,
    startServer: startProductionProbeServer,
    stat,
    timerApi: {
      setTimeout: globalThis.setTimeout,
      clearTimeout: globalThis.clearTimeout,
    },
    verifyIsolationBaseline: async () => {},
    writeFile,
    ...dependencies,
  };
  expectedMatrix(platformId);
  const hostFacts = validateHostFacts(
    platformId,
    deps.hostFacts ?? collectHostFacts(deps),
  );
  const appMetadata = await deps.stat(appPath);
  if (!appMetadata.isFile()) {
    fail('the exact --app path must identify an existing file');
  }

  const isolationRunId = createRunId(deps.randomSource);
  let diagnosticRecorder = new IsolationRecorder({
    runId: isolationRunId,
    platformId,
    hostFacts,
  });
  let recorder = null;
  let isolation = null;
  const secrets = [isolationRunId, appPath];
  let activeChild = null;
  let activeServer = null;
  const closeActiveServer = async () => {
    if (!activeServer) return;
    const closing = activeServer;
    activeServer = null;
    await closing.close();
  };

  const spawnFeatureChild = (phase, endpoint, runId) => {
    activeChild = deps.spawnChild(appPath, [], {
      env: buildChildEnvironment(deps.environment, { phase, endpoint, runId }),
      shell: false,
      stdio: 'inherit',
      windowsHide: true,
    });
    return activeChild;
  };

  const runOnePhase = async (phase, lifecycleServer, lifecycleRunId) => {
    recorder.beginPhase(phase);
    spawnFeatureChild(phase, lifecycleServer.endpoint, lifecycleRunId);
    const exit = await Promise.race([
      waitForCleanChildExit(activeChild),
      lifecycleServer.fatal,
    ]);
    recorder.observeProcessExit(phase, { ...exit, forced: false });
    activeChild = null;
  };

  try {
    activeServer = await deps.startServer(diagnosticRecorder, {
      kind: 'isolation',
      runId: isolationRunId,
      platformId,
      environment: deps.environment,
      certificateRootDirectory: join(dirname(outputPath), '.canary-certs'),
      controlledVm: deps.environment[CONTROLLED_VM_ENV] === '1',
    });
    secrets.push(activeServer.endpoint);
    const isolationServer = activeServer;
    await withDeadline(
      async () => {
        const child = spawnFeatureChild(
          WRITE_PHASE,
          isolationServer.endpoint,
          isolationRunId,
        );
        if (typeof isolationServer.registerChildProcess === 'function') {
          isolationServer.registerChildProcess(child);
        }
        const exit = await Promise.race([
          waitForCleanChildExit(activeChild),
          isolationServer.fatal,
        ]);
        diagnosticRecorder.observeProcessExit({ ...exit, forced: false });
        activeChild = null;
      },
      ISOLATION_STAGE_DEADLINE_MS,
      'isolated raw WRY pre-stage',
      deps.timerApi,
    );
    if (typeof isolationServer.isolationObservation !== 'function') {
      fail(
        'isolation server must expose runner-observed HTTPS/WSS preflight evidence',
      );
    }
    isolation = diagnosticRecorder.seal(
      await isolationServer.isolationObservation(),
    );
    await closeActiveServer();
    if (typeof isolationServer.verifyBaseline !== 'function') {
      fail('isolation server must verify its post-cleanup platform baseline');
    }
    await isolationServer.verifyBaseline();
    await deps.verifyIsolationBaseline({
      isolation,
      platformId,
    });

    const lifecycleRunId = createRunId(deps.randomSource);
    if (lifecycleRunId === isolationRunId) {
      fail('isolation and lifecycle recorders require distinct fresh nonces');
    }
    secrets.push(lifecycleRunId);
    recorder = new LifecycleRecorder({
      runId: lifecycleRunId,
      platformId,
      hostFacts,
    });
    diagnosticRecorder = recorder;
    activeServer = await deps.startServer(recorder, {
      kind: 'lifecycle',
      runId: lifecycleRunId,
      platformId,
    });
    secrets.push(activeServer.endpoint);
    const lifecycleServer = activeServer;
    await withDeadline(
      () =>
        Promise.race([
          (async () => {
            for (const phase of PHASES) {
              await withDeadline(
                () => runOnePhase(phase, lifecycleServer, lifecycleRunId),
                PHASE_DEADLINE_MS,
                `phase ${phase}`,
                deps.timerApi,
              );
            }
          })(),
          lifecycleServer.fatal,
        ]),
      TOTAL_DEADLINE_MS,
      'two-phase lifecycle',
      deps.timerApi,
    );
    recorder.assertComplete();
    isolation = mergeIsolationWithLifecycle(isolation, recorder);
    await closeActiveServer();
    const output = buildSanitizedOutput({
      status: 'pass',
      platformId,
      hostFacts,
      recorder,
      isolation,
      secrets,
    });
    await writeOutputArtifact(outputPath, output, deps);
    return output;
  } catch (error) {
    const forcedKill = forceKillChild(activeChild);
    try {
      await closeActiveServer();
    } catch {
      // Preserve the original probe failure in the sanitized diagnostic.
    }
    const output = buildSanitizedOutput({
      status: 'failure',
      platformId,
      hostFacts,
      recorder: diagnosticRecorder,
      isolation,
      failure: error,
      forcedKill,
      secrets,
    });
    try {
      await writeOutputArtifact(outputPath, output, deps);
    } catch (writeError) {
      const sanitizedWriteError = redactMessage(writeError.message, secrets);
      throw new Error(
        `signature lifecycle probe failed and diagnostic write failed: ${sanitizedWriteError}`,
        { cause: writeError },
      );
    }
    throw new Error(
      `signature lifecycle probe failed: ${output.failure.message}`,
      { cause: error },
    );
  } finally {
    if (activeServer) {
      try {
        await closeActiveServer();
      } catch {
        // The primary result or failure has already been recorded.
      }
    }
  }
}

async function main() {
  const options = parseArguments(process.argv.slice(2));
  await runLifecycleProbe(options);
}

const entryPath = process.argv[1] ? resolve(process.argv[1]) : '';
if (entryPath === fileURLToPath(import.meta.url)) {
  main().catch((error) => {
    console.error(error.message);
    process.exitCode = 1;
  });
}
