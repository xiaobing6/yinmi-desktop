export const SIGNATURE_PLATFORM_MATRIX = Object.freeze({
  'windows-10-webview2-111-x64': Object.freeze({
    hostPlatform: 'win32',
    hostArch: 'x64',
    binaryTargetOs: 'windows',
    binaryTargetArch: 'x86_64',
    translatedProcess: null,
  }),
  'windows-11-x64': Object.freeze({
    hostPlatform: 'win32',
    hostArch: 'x64',
    binaryTargetOs: 'windows',
    binaryTargetArch: 'x86_64',
    translatedProcess: null,
  }),
  'macos-13-intel': Object.freeze({
    hostPlatform: 'darwin',
    hostArch: 'x64',
    binaryTargetOs: 'macos',
    binaryTargetArch: 'x86_64',
    translatedProcess: false,
  }),
  'macos-current-arm64': Object.freeze({
    hostPlatform: 'darwin',
    hostArch: 'arm64',
    binaryTargetOs: 'macos',
    binaryTargetArch: 'aarch64',
    translatedProcess: false,
  }),
});

export const SIGNATURE_RESOURCE_VECTORS = Object.freeze([
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
]);

const SIGNATURE_PLATFORM_IDS = Object.freeze(
  Object.keys(SIGNATURE_PLATFORM_MATRIX),
);
const RESOURCE_REQUEST_VECTORS = new Set(
  SIGNATURE_RESOURCE_VECTORS.slice(0, 14),
);
const FIXED_SCENARIO_IDS = Object.freeze([
  'policy-registration-fault',
  'initialization-finished-delay-past-20s',
  'sign-callback-delay-past-5s',
  'destroy-during-pending-policy',
  'late-callback-after-new-generation',
  'main-close-state-machine-seam',
]);
const FIXED_SCENARIO_KEYS = Object.freeze([
  'id',
  'generation',
  'operationId',
  'orderedActorEvents',
  'terminalState',
]);
const FIXED_ACTOR_EVENTS = new Set([
  'scenario-started',
  'policy-registration-fault-observed',
  'initialization-timeout-observed',
  'sign-timeout-observed',
  'pending-policy-observed',
  'destroy-requested',
  'generation-invalidated',
  'native-destroyed',
  'manager-host-absent',
  'policy-cleanup-acknowledged',
  'policy-tombstones-empty',
  'teardown-complete',
  'retry-ready',
  'retry-destroyed',
  'new-generation-ready',
  'late-callback-isolated',
  'new-generation-sign-succeeded',
  'would-exit-blocked',
  'would-exit-released',
]);
const RESOURCE_RESULT_KEYS = Object.freeze([
  'runtimeAttempted',
  'availabilityOutcome',
  'deterministicBarrierSeamCovered',
  'expectedBarrier',
  'enforcedBarrier',
  'barrierEvidenceMode',
  'counterfactualServerHits',
  'allowedRedirectHopHits',
  'serverHits',
]);
const SIGNATURE_BASE_TRUE_CHECKS = Object.freeze([
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
]);
const SIGNATURE_FALSE_CHECKS = Object.freeze([
  'usesTauriManagedWebView',
  'newInstanceStorageRecovered',
  'restartStorageRecovered',
]);
const FINAL_TRUE_CHECKS = Object.freeze([
  ...SIGNATURE_BASE_TRUE_CHECKS,
  'ordinaryExitCleanupAcknowledged',
]);
const PROBE_FALSE_CHECKS = Object.freeze([
  'ordinaryExitCleanupAcknowledged',
  ...SIGNATURE_FALSE_CHECKS,
]);
const PROBE_CHECK_KEYS = Object.freeze([
  ...SIGNATURE_BASE_TRUE_CHECKS,
  ...PROBE_FALSE_CHECKS,
  'crossOriginCanaryServerHits',
  'blockedCanaryAttempts',
  'resourceVectorResults',
]);
const SIGNATURE_CHECK_KEYS = Object.freeze([
  'runtimeModes',
  'resourcePolicyModes',
  'webviewRuntimeVersions',
  'resourceVectorsCovered',
  ...FINAL_TRUE_CHECKS,
  ...SIGNATURE_FALSE_CHECKS,
  'crossOriginCanaryServerHits',
  'byPlatform',
]);
const SIGNATURE_PLATFORM_ROW_KEYS = Object.freeze([
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
  ...FINAL_TRUE_CHECKS,
  ...SIGNATURE_FALSE_CHECKS,
  'crossOriginCanaryServerHits',
  'blockedCanaryAttempts',
  'resourceVectorResults',
]);
const PLATFORM_FIELDS = Object.freeze([
  'hostPlatform',
  'hostArch',
  'osVersion',
  'binaryTargetOs',
  'binaryTargetArch',
  'translatedProcess',
]);
const ISOLATION_COUNTER_KEYS = Object.freeze([
  'blockedNavigations',
  'blockedNewWindows',
  'blockedDownloads',
  'blockedResourceRequests',
  'resourceCanaryHits',
  'policyFaults',
]);
const RESOURCE_POLICY_MODES = new Set([
  'webview2-22-all-source-kinds',
  'webview2-legacy-all-contexts-candidate',
  'wk-content-rule-list-exact-origin',
]);
const DEPRECATED_WINDOWS_POLICY_MODES = new Set([
  'webview2-web-resource-requested-source-kinds',
  'webview2-web-resource-requested-legacy',
]);

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
  const actual = Object.keys(value);
  const missing = expectedKeys.filter((key) => !actual.includes(key));
  const extra = actual.filter((key) => !expectedKeys.includes(key));
  if (missing.length > 0 || extra.length > 0) {
    fail(`${label} must contain the exact keys from the canonical contract`);
  }
}

function assertSafeInteger(value, label, { positive = false } = {}) {
  if (!Number.isSafeInteger(value) || (positive ? value < 1 : value < 0)) {
    fail(
      `${label} must be a ${positive ? 'positive' : 'nonnegative'} safe integer`,
    );
  }
}

function assertCheckProfile(value, trueChecks, falseChecks, label) {
  for (const key of trueChecks) {
    if (value[key] !== true) fail(`${label}.${key} must equal true`);
  }
  for (const key of falseChecks) {
    if (value[key] !== false) fail(`${label}.${key} must equal false`);
  }
}

function assertExactStringSet(value, expected, label) {
  if (
    !Array.isArray(value) ||
    value.length !== expected.length ||
    value.some((item) => typeof item !== 'string') ||
    new Set(value).size !== expected.length
  ) {
    fail(`${label} must contain the exact unique string set`);
  }
  const actual = [...value].sort();
  const wanted = [...expected].sort();
  if (actual.some((item, index) => item !== wanted[index])) {
    fail(`${label} must equal the fixed signature set`);
  }
}

function assertFrozenVersion(value, label) {
  if (
    typeof value !== 'string' ||
    value.length === 0 ||
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
  const label = `resource vector ${vector}`;
  assertExactKeys(result, RESOURCE_RESULT_KEYS, label);
  if (result.runtimeAttempted !== true) {
    fail(`${label} runtimeAttempted must equal true`);
  }
  if (result.deterministicBarrierSeamCovered !== true) {
    fail(`${label} deterministicBarrierSeamCovered must equal true`);
  }
  const absentServiceWorker =
    vector === 'service_worker' &&
    result.availabilityOutcome === 'service-worker-api-absent';
  if (result.availabilityOutcome !== 'available' && !absentServiceWorker) {
    fail(
      `${label} availabilityOutcome must be available except for an absent service-worker API`,
    );
  }

  const barrier = expectedBarrier(platformId, vector);
  if (result.expectedBarrier !== barrier) {
    fail(`${label} expectedBarrier must equal ${barrier}`);
  }
  if (result.enforcedBarrier !== barrier) {
    fail(`${label} enforcedBarrier must equal ${barrier}`);
  }
  assertSafeInteger(
    result.allowedRedirectHopHits,
    `${label} allowedRedirectHopHits`,
  );
  const redirectHops = vector === 'redirect' ? 2 : 0;
  if (result.allowedRedirectHopHits !== redirectHops) {
    fail(`${label} allowedRedirectHopHits must equal ${redirectHops}`);
  }
  assertSafeInteger(result.serverHits, `${label} serverHits`);
  if (result.serverHits !== 0) fail(`${label} serverHits must equal 0`);

  if (absentServiceWorker) {
    if (result.barrierEvidenceMode !== 'deterministic-seam-only') {
      fail(`${label} barrierEvidenceMode must equal deterministic-seam-only`);
    }
    if (result.counterfactualServerHits !== null) {
      fail(
        `${label} counterfactualServerHits must be explicitly null when the API is absent`,
      );
    }
    return result.serverHits;
  }

  if (RESOURCE_REQUEST_VECTORS.has(vector)) {
    if (platformId.startsWith('windows-')) {
      if (result.barrierEvidenceMode !== 'native-callback') {
        fail(
          `${label} barrierEvidenceMode must equal native-callback on Windows`,
        );
      }
      if (result.counterfactualServerHits !== null) {
        fail(
          `${label} counterfactualServerHits must be explicitly null on Windows`,
        );
      }
    } else {
      if (result.barrierEvidenceMode !== 'paired-counterfactual') {
        fail(
          `${label} barrierEvidenceMode must equal paired-counterfactual on macOS`,
        );
      }
      assertSafeInteger(
        result.counterfactualServerHits,
        `${label} counterfactualServerHits`,
        { positive: true },
      );
    }
  } else {
    if (result.barrierEvidenceMode !== 'handler-callback') {
      fail(`${label} barrierEvidenceMode must equal handler-callback`);
    }
    if (result.counterfactualServerHits !== null) {
      fail(
        `${label} counterfactualServerHits must be explicitly null for handler vectors`,
      );
    }
  }
  return result.serverHits;
}

function validatePlatformIdentity(platformId, row, platform) {
  const matrix = SIGNATURE_PLATFORM_MATRIX[platformId];
  if (!matrix) fail('signature platform id is not canonical');
  for (const [field, expected] of Object.entries(matrix)) {
    if (row[field] !== expected) {
      if (field === 'translatedProcess' && expected === null) {
        fail(
          `${platformId} translatedProcess must be explicitly null on Windows`,
        );
      }
      fail(`${platformId} ${field} must equal ${JSON.stringify(expected)}`);
    }
  }
  if (platform) {
    if (platform.osVersion !== row.osVersion) {
      fail(`${platformId} osVersion must match the runner platform row`);
    }
    if (platform.arch !== row.binaryTargetArch) {
      fail(`${platformId} binaryTargetArch must match the runner platform row`);
    }
  }

  if (platformId === 'windows-10-webview2-111-x64') {
    if (row.osVersion !== '10.0.19045') {
      fail(`${platformId} osVersion must equal 10.0.19045`);
    }
  } else if (platformId === 'windows-11-x64') {
    const build = /^10\.0\.(\d+)$/.exec(row.osVersion);
    if (!build || Number(build[1]) < 22_000) {
      fail(`${platformId} osVersion must be a Windows 11 build at least 22000`);
    }
  } else if (platformId === 'macos-13-intel') {
    if (!/^13\.3(?:\.\d+)?$/.test(row.osVersion)) {
      fail(`${platformId} osVersion must equal 13.3 or 13.3.x`);
    }
  } else {
    const match = /^(\d+)\.(\d+)(?:\.\d+)?$/.exec(row.osVersion);
    if (
      !match ||
      Number(match[1]) < 13 ||
      (Number(match[1]) === 13 && Number(match[2]) < 3)
    ) {
      fail(`${platformId} osVersion must be at least 13.3`);
    }
  }

  assertFrozenVersion(
    row.webviewRuntimeVersion,
    `${platformId} webviewRuntimeVersion`,
  );
  if (
    platformId === 'windows-10-webview2-111-x64' &&
    !/^111\.0\.1661\.\d+$/.test(row.webviewRuntimeVersion)
  ) {
    fail(`${platformId} webviewRuntimeVersion must match 111.0.1661.x`);
  }
  if (row.runtimeMode !== 'native-host-raw-wry-0.55.1') {
    fail(`${platformId} runtimeMode must equal native-host-raw-wry-0.55.1`);
  }
  if (DEPRECATED_WINDOWS_POLICY_MODES.has(row.resourcePolicyMode)) {
    fail(`${platformId} resourcePolicyMode is a deprecated Windows wire alias`);
  }

  if (platformId === 'windows-10-webview2-111-x64') {
    if (typeof row.strongSourceKindsInterfaceAvailable !== 'boolean') {
      fail(
        `${platformId} strongSourceKindsInterfaceAvailable must be boolean on Windows`,
      );
    }
    const expectedMode = row.strongSourceKindsInterfaceAvailable
      ? 'webview2-22-all-source-kinds'
      : 'webview2-legacy-all-contexts-candidate';
    if (row.resourcePolicyMode !== expectedMode) {
      fail(
        `${platformId} resourcePolicyMode must match strongSourceKindsInterfaceAvailable`,
      );
    }
  } else if (platformId === 'windows-11-x64') {
    if (row.strongSourceKindsInterfaceAvailable !== true) {
      fail(`${platformId} strongSourceKindsInterfaceAvailable must equal true`);
    }
    if (row.resourcePolicyMode !== 'webview2-22-all-source-kinds') {
      fail(
        `${platformId} resourcePolicyMode must equal webview2-22-all-source-kinds`,
      );
    }
  } else {
    if (row.strongSourceKindsInterfaceAvailable !== null) {
      fail(
        `${platformId} strongSourceKindsInterfaceAvailable must be explicitly null on macOS`,
      );
    }
    if (row.resourcePolicyMode !== 'wk-content-rule-list-exact-origin') {
      fail(
        `${platformId} resourcePolicyMode must equal wk-content-rule-list-exact-origin`,
      );
    }
  }
  if (!RESOURCE_POLICY_MODES.has(row.resourcePolicyMode)) {
    fail(`${platformId} resourcePolicyMode is not canonical`);
  }
}

function validatePlatformRowCore(platformId, row, platform, stage) {
  validatePlatformIdentity(platformId, row, platform);
  assertCheckProfile(
    row,
    stage === 'probe' ? SIGNATURE_BASE_TRUE_CHECKS : FINAL_TRUE_CHECKS,
    stage === 'probe' ? PROBE_FALSE_CHECKS : SIGNATURE_FALSE_CHECKS,
    `${platformId} checks`,
  );
  assertSafeInteger(
    row.crossOriginCanaryServerHits,
    `${platformId} crossOriginCanaryServerHits`,
  );
  if (row.crossOriginCanaryServerHits !== 0) {
    fail(`${platformId} crossOriginCanaryServerHits must equal 0`);
  }
  if (platformId.startsWith('windows-')) {
    if (
      !Number.isSafeInteger(row.blockedCanaryAttempts) ||
      row.blockedCanaryAttempts < 0
    ) {
      fail(
        `${platformId} blockedCanaryAttempts must be a nonnegative safe integer on Windows`,
      );
    }
  } else if (row.blockedCanaryAttempts !== null) {
    fail(
      `${platformId} blockedCanaryAttempts must be explicitly null on macOS`,
    );
  }

  assertExactKeys(
    row.resourceVectorResults,
    SIGNATURE_RESOURCE_VECTORS,
    `${platformId} resourceVectorResults`,
  );
  let protectedHits = 0;
  for (const vector of SIGNATURE_RESOURCE_VECTORS) {
    protectedHits += validateResourceVectorResult(
      platformId,
      vector,
      row.resourceVectorResults[vector],
    );
  }
  if (row.crossOriginCanaryServerHits !== protectedHits) {
    fail(
      `${platformId} crossOriginCanaryServerHits must equal protected vector hits`,
    );
  }
  return protectedHits;
}

function eventIndex(report, event) {
  return report.orderedActorEvents.indexOf(event);
}

function eventCount(report, event) {
  return report.orderedActorEvents.filter((candidate) => candidate === event)
    .length;
}

function deriveFixedScenarioChecks(reports, reportedChecks) {
  if (!Array.isArray(reports) || reports.length !== FIXED_SCENARIO_IDS.length) {
    fail('fixed scenarios must contain the exact six reports');
  }
  const generations = new Set();
  reports.forEach((report, index) => {
    assertExactKeys(report, FIXED_SCENARIO_KEYS, 'fixed scenario report');
    if (report.id !== FIXED_SCENARIO_IDS[index]) {
      fail('fixed scenarios IDs and order must match the canonical matrix');
    }
    assertSafeInteger(
      report.generation,
      `fixed scenario ${report.id} generation`,
      {
        positive: true,
      },
    );
    assertSafeInteger(
      report.operationId,
      `fixed scenario ${report.id} operationId`,
      {
        positive: true,
      },
    );
    if (!generations.add(report.generation)) {
      fail('fixed scenarios generations must be unique');
    }
    if (
      !Array.isArray(report.orderedActorEvents) ||
      report.orderedActorEvents.length < 2 ||
      report.orderedActorEvents.length > 32 ||
      report.orderedActorEvents.some(
        (event) => typeof event !== 'string' || !FIXED_ACTOR_EVENTS.has(event),
      )
    ) {
      fail(`fixed scenarios ${report.id} orderedActorEvents are not canonical`);
    }
    if (report.terminalState !== 'destroy-confirmed') {
      fail(
        `fixed scenarios ${report.id} terminalState must equal destroy-confirmed`,
      );
    }
  });

  const firstFive = reports.slice(0, 5);
  const destroyConfirmedBeforeRetry = firstFive.every((report) => {
    const teardown = eventIndex(report, 'teardown-complete');
    const retry = eventIndex(report, 'retry-ready');
    return teardown >= 0 && retry >= 0 && teardown < retry;
  });
  const resourcePolicyCleanupAcknowledged = firstFive.every((report) => {
    const teardown = eventIndex(report, 'teardown-complete');
    return (
      teardown >= 0 &&
      eventCount(report, 'teardown-complete') === 1 &&
      [
        'native-destroyed',
        'manager-host-absent',
        'policy-cleanup-acknowledged',
        'policy-tombstones-empty',
      ].every(
        (event) =>
          eventCount(report, event) === 1 &&
          eventIndex(report, event) < teardown,
      )
    );
  });
  const timeoutCheck =
    eventIndex(reports[1], 'initialization-timeout-observed') >= 0 &&
    eventIndex(reports[2], 'sign-timeout-observed') >= 0;
  const retryCheck = firstFive.every(
    (report) => eventIndex(report, 'retry-destroyed') >= 0,
  );
  const policyFaultInvalidatesInstance =
    eventIndex(reports[0], 'policy-registration-fault-observed') >= 0;
  const newGeneration = eventIndex(reports[4], 'new-generation-ready');
  const lateCallback = eventIndex(reports[4], 'late-callback-isolated');
  const lateCallbackIsolated =
    newGeneration >= 0 && lateCallback >= 0 && newGeneration < lateCallback;
  const blockedExit = eventIndex(reports[5], 'would-exit-blocked');
  const releasedExit = eventIndex(reports[5], 'would-exit-released');
  const policyTombstonesEmptyBeforeExit =
    firstFive.every(
      (report) => eventIndex(report, 'policy-tombstones-empty') >= 0,
    ) &&
    blockedExit >= 0 &&
    releasedExit >= 0 &&
    blockedExit < releasedExit;
  const derived = {
    timeoutCheck,
    retryCheck,
    policyFaultInvalidatesInstance,
    lateCallbackIsolated,
    destroyConfirmedBeforeRetry,
    resourcePolicyCleanupAcknowledged,
    policyTombstonesEmptyBeforeExit,
  };
  for (const [key, value] of Object.entries(derived)) {
    if (reportedChecks[key] !== value) {
      fail(`fixed scenarios ${key} disagrees with report checks`);
    }
    if (value !== true) fail(`fixed scenarios did not derive ${key}=true`);
  }
}

function probeRow(report) {
  return {
    hostPlatform: report.hostPlatform,
    hostArch: report.hostArch,
    osVersion: report.osVersion,
    binaryTargetOs: report.binaryTargetOs,
    binaryTargetArch: report.binaryTargetArch,
    translatedProcess: report.translatedProcess,
    webviewRuntimeVersion: report.webviewRuntimeVersion,
    runtimeMode: report.runtimeMode,
    resourcePolicyMode: report.resourcePolicyMode,
    strongSourceKindsInterfaceAvailable:
      report.strongSourceKindsInterfaceAvailable,
    ...report.checks,
  };
}

export function validateSignatureProbeSemantics(report, correlation) {
  assertPlainObject(report, 'isolation report');
  assertSafeInteger(report.generation, 'isolation report generation', {
    positive: true,
  });
  assertSafeInteger(report.operationId, 'isolation report operationId', {
    positive: true,
  });
  assertExactKeys(
    report.counters,
    ISOLATION_COUNTER_KEYS,
    'isolation counters',
  );
  for (const key of ISOLATION_COUNTER_KEYS) {
    assertSafeInteger(report.counters[key], `isolation counters ${key}`);
  }
  for (const field of PLATFORM_FIELDS) {
    if (report[field] !== correlation[field]) {
      if (field === 'translatedProcess' && correlation[field] === null) {
        fail(
          'isolation report translatedProcess must be explicitly null on Windows',
        );
      }
      fail(`isolation report ${field} must match the runner correlation`);
    }
  }
  if (report.platformId !== correlation.platformId) {
    fail('isolation report platformId must match the runner correlation');
  }
  assertExactKeys(report.checks, PROBE_CHECK_KEYS, 'isolation report checks');
  deriveFixedScenarioChecks(report.fixedScenarios, report.checks);
  const protectedHits = validatePlatformRowCore(
    correlation.platformId,
    probeRow(report),
    {
      id: correlation.platformId,
      osVersion: correlation.osVersion,
      arch: correlation.binaryTargetArch,
    },
    'probe',
  );
  if (
    report.platformId.startsWith('windows-') &&
    report.checks.blockedCanaryAttempts !== report.counters.resourceCanaryHits
  ) {
    fail(
      'isolation report blockedCanaryAttempts must equal counters.resourceCanaryHits on Windows',
    );
  }
  return protectedHits;
}

export function validateSignatureEvidenceChecks(checks, platforms) {
  assertExactKeys(checks, SIGNATURE_CHECK_KEYS, 'signature-webview checks');
  const ids = platforms.map(({ id }) => id).sort();
  const wantedIds = [...SIGNATURE_PLATFORM_IDS].sort();
  if (
    ids.length !== wantedIds.length ||
    ids.some((id, index) => id !== wantedIds[index])
  ) {
    fail(
      'signature-webview platforms must equal the canonical four-platform set',
    );
  }
  assertCheckProfile(
    checks,
    FINAL_TRUE_CHECKS,
    SIGNATURE_FALSE_CHECKS,
    'signature-webview checks',
  );
  assertExactStringSet(
    checks.resourceVectorsCovered,
    SIGNATURE_RESOURCE_VECTORS,
    'signature-webview resourceVectorsCovered',
  );
  assertExactKeys(
    checks.byPlatform,
    SIGNATURE_PLATFORM_IDS,
    'signature-webview checks.byPlatform',
  );
  for (const key of [
    'runtimeModes',
    'resourcePolicyModes',
    'webviewRuntimeVersions',
  ]) {
    assertExactKeys(
      checks[key],
      SIGNATURE_PLATFORM_IDS,
      `signature-webview ${key}`,
    );
  }

  const platformById = new Map(
    platforms.map((platform) => [platform.id, platform]),
  );
  let protectedHits = 0;
  for (const platformId of SIGNATURE_PLATFORM_IDS) {
    const row = checks.byPlatform[platformId];
    assertExactKeys(
      row,
      SIGNATURE_PLATFORM_ROW_KEYS,
      `${platformId} evidence row`,
    );
    protectedHits += validatePlatformRowCore(
      platformId,
      row,
      platformById.get(platformId),
      'evidence',
    );
    if (checks.runtimeModes[platformId] !== row.runtimeMode) {
      fail(`runtimeModes.${platformId} must be derived from its platform row`);
    }
    if (checks.resourcePolicyModes[platformId] !== row.resourcePolicyMode) {
      fail(
        `resourcePolicyModes.${platformId} must be derived from its platform row`,
      );
    }
    if (
      checks.webviewRuntimeVersions[platformId] !== row.webviewRuntimeVersion
    ) {
      fail(
        `webviewRuntimeVersions.${platformId} must be derived from its platform row`,
      );
    }
  }
  assertSafeInteger(
    checks.crossOriginCanaryServerHits,
    'signature-webview crossOriginCanaryServerHits',
  );
  if (checks.crossOriginCanaryServerHits !== protectedHits) {
    fail('signature-webview crossOriginCanaryServerHits must equal row sums');
  }
  if (checks.crossOriginCanaryServerHits !== 0) {
    fail('signature-webview crossOriginCanaryServerHits must equal 0');
  }
}
