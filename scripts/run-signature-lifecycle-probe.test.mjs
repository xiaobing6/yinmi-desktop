import assert from 'node:assert/strict';
import { EventEmitter } from 'node:events';
import { readFile } from 'node:fs/promises';
import { resolve } from 'node:path';
import test from 'node:test';

import {
  BODY_LIMIT_BYTES,
  CANARY_COMPLETION_QUIET_MS,
  CANARY_IDLE_DURATION_MS,
  CANARY_CONFIG_KEYS,
  CANARY_CONFIG_PATH,
  CANARY_RESOURCE_VECTORS,
  CONTROLLED_VM_ENV,
  CanaryRecorder,
  EVENT_KEYS,
  EVENT_PATH,
  ISOLATION_REPORT_BODY_LIMIT_BYTES,
  ISOLATION_REPORT_PATH,
  ISOLATION_STAGE_DEADLINE_MS,
  IsolationRecorder,
  LifecycleRecorder,
  PHASE_DEADLINE_MS,
  PHASE_GRAMMARS,
  PHASES,
  PLATFORM_MATRIX,
  PROCESS_INFO_KEYS,
  PROCESS_INFO_PATH,
  TOTAL_DEADLINE_MS,
  buildChildEnvironment,
  buildCanaryRouteResponse,
  buildSanitizedOutput,
  collectHostFacts,
  createRunId,
  decodeExactJsonBody,
  parseArguments,
  routeSubmission,
  routeCanaryControl,
  routeCanaryHit,
  runLifecycleProbe,
  runControlledCanaryHarness,
  startControlledIsolationRecorderServer,
  startControlledCanaryServers,
  startControlledPlatformMonitor,
  controlledVmTrustCommands,
  createControlledVmTrustAdapter,
  createPerRunCanaryCertificate,
  validateHostFacts,
  waitForCleanChildExit,
  withCanaryTrust,
  withDeadline,
} from './run-signature-lifecycle-probe.mjs';

const RUN_ID = '0123456789abcdef0123456789abcdef';
const STALE_RUN_ID = 'fedcba9876543210fedcba9876543210';
const [WRITE_PHASE, VERIFY_PHASE] = [
  'write-marker-and-close-main',
  'verify-marker-absent',
];
const RUST_CANONICAL_WINDOWS_10_REPORT = JSON.parse(
  await readFile(
    new URL(
      '../src-tauri/tests/fixtures/signature/windows-10-webview2-111-x64-isolation-report.json',
      import.meta.url,
    ),
    'utf8',
  ),
);

const WRITE_GRAMMAR = [
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
  'process-exit-observed',
];
const VERIFY_GRAMMAR = [
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
  'process-exit-observed',
];

const WINDOWS_HOST = {
  hostPlatform: 'win32',
  hostArch: 'x64',
  osVersion: '10.0.19045',
};

function processInfo(phase, overrides = {}) {
  return {
    runId: RUN_ID,
    phase,
    binaryTargetOs: 'windows',
    binaryTargetArch: 'x86_64',
    translatedProcess: null,
    ...overrides,
  };
}

function event(phase, name, overrides = {}) {
  return { runId: RUN_ID, phase, event: name, ...overrides };
}

function isolationReport(overrides = {}) {
  return {
    ...structuredClone(RUST_CANONICAL_WINDOWS_10_REPORT),
    ...overrides,
  };
}

function windowsRecorder() {
  return new LifecycleRecorder({
    runId: RUN_ID,
    platformId: 'windows-10-webview2-111-x64',
    hostFacts: WINDOWS_HOST,
  });
}

function isolationWindowsRecorder(runId = RUN_ID) {
  return new IsolationRecorder({
    runId,
    platformId: 'windows-10-webview2-111-x64',
    hostFacts: WINDOWS_HOST,
  });
}

function acceptWindowsIsolationReport(report = isolationReport()) {
  const recorder = isolationWindowsRecorder();
  recorder.acceptProcessInfo(processInfo(WRITE_PHASE));
  return recorder.acceptIsolationReport({
    runId: RUN_ID,
    phase: WRITE_PHASE,
    report,
  });
}

function assertWindowsIsolationReportRejected(mutate, expected, label) {
  const report = isolationReport();
  mutate(report);
  assert.throws(() => acceptWindowsIsolationReport(report), expected, label);
}

function canaryConfig(runId = RUN_ID) {
  return {
    runId,
    phase: WRITE_PHASE,
    platformId: 'windows-10-webview2-111-x64',
    controlOrigin: 'http://127.0.0.1:50000/',
    allowedOrigin: 'https://127.0.0.1:50001/',
    blockedHttpOrigin: 'http://127.0.0.1:50000/',
    blockedHttpsOrigin: 'https://127.0.0.1:50002/',
    blockedWsOrigin: 'ws://127.0.0.1:50000/',
    blockedWssOrigin: 'wss://127.0.0.1:50002/',
    idleDurationMs: 600_000,
  };
}

function startWindowsPhase(recorder, phase) {
  recorder.beginPhase(phase);
  recorder.acceptProcessInfo(processInfo(phase));
}

function submitChildGrammar(recorder, phase, grammar = PHASE_GRAMMARS[phase]) {
  for (const name of grammar.slice(0, -1)) {
    recorder.acceptEvent(event(phase, name));
  }
}

function completeWindowsPhase(recorder, phase) {
  startWindowsPhase(recorder, phase);
  submitChildGrammar(recorder, phase);
  recorder.observeProcessExit(phase, {
    code: 0,
    signal: null,
    forced: false,
  });
}

test('freezes the exact two-phase lifecycle grammar and global ordinals', () => {
  assert.deepEqual(PHASES, [WRITE_PHASE, VERIFY_PHASE]);
  assert.deepEqual(PHASE_GRAMMARS[WRITE_PHASE], WRITE_GRAMMAR);
  assert.deepEqual(PHASE_GRAMMARS[VERIFY_PHASE], VERIFY_GRAMMAR);

  const recorder = windowsRecorder();
  completeWindowsPhase(recorder, WRITE_PHASE);
  completeWindowsPhase(recorder, VERIFY_PHASE);
  recorder.assertComplete();

  const traces = recorder.traces();
  assert.deepEqual(
    traces.map(({ phase }) => phase),
    PHASES,
  );
  assert.deepEqual(
    traces.flatMap(({ events }) => events.map(({ event: name }) => name)),
    [...WRITE_GRAMMAR, ...VERIFY_GRAMMAR],
  );
  assert.deepEqual(
    traces.flatMap(({ events }) => events.map(({ ordinal }) => ordinal)),
    Array.from({ length: 22 }, (_, index) => index + 1),
  );
});

test('generates a fresh 128-bit lowercase hexadecimal nonce', () => {
  let calls = 0;
  const runIdOne = createRunId((size) => {
    assert.equal(size, 16);
    calls += 1;
    return Buffer.alloc(size, 0xab);
  });
  const runIdTwo = createRunId((size) => {
    assert.equal(size, 16);
    calls += 1;
    return Buffer.alloc(size, 0xcd);
  });

  assert.equal(runIdOne, 'abababababababababababababababab');
  assert.equal(runIdTwo, 'cdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcd');
  assert.match(runIdOne, /^[0-9a-f]{32}$/);
  assert.notEqual(runIdOne, runIdTwo);
  assert.equal(calls, 2);
  assert.throws(() => createRunId(() => Buffer.alloc(15)), /exactly 16 bytes/i);
});

test('exact-matches the nonce and rejects stale, duplicate and replayed messages', () => {
  assert.throws(
    () =>
      new LifecycleRecorder({
        runId: RUN_ID.toUpperCase(),
        platformId: 'windows-10-webview2-111-x64',
        hostFacts: WINDOWS_HOST,
      }),
    /lowercase.*128-bit|run id/i,
  );

  const recorder = windowsRecorder();
  recorder.beginPhase(WRITE_PHASE);
  assert.throws(
    () =>
      recorder.acceptProcessInfo(
        processInfo(WRITE_PHASE, { runId: STALE_RUN_ID }),
      ),
    /run id.*match/i,
  );
  recorder.acceptProcessInfo(processInfo(WRITE_PHASE));
  assert.throws(
    () => recorder.acceptProcessInfo(processInfo(WRITE_PHASE)),
    /process-info.*once|duplicate/i,
  );
  recorder.acceptEvent(event(WRITE_PHASE, 'process-started'));
  assert.throws(
    () => recorder.acceptEvent(event(WRITE_PHASE, 'process-started')),
    /expected.*active-host-ready|duplicate|out of order/i,
  );
  assert.throws(
    () =>
      recorder.acceptEvent(
        event(WRITE_PHASE, 'active-host-ready', { runId: STALE_RUN_ID }),
      ),
    /run id.*match/i,
  );
});

test('requires process-info before events and exactly once per phase', () => {
  const recorder = windowsRecorder();
  recorder.beginPhase(WRITE_PHASE);
  assert.throws(
    () => recorder.acceptEvent(event(WRITE_PHASE, 'process-started')),
    /process-info.*first/i,
  );
  assert.throws(
    () =>
      recorder.observeProcessExit(WRITE_PHASE, {
        code: 0,
        signal: null,
        forced: false,
      }),
    /process-info|missing.*event/i,
  );
  assert.throws(() => recorder.assertComplete(), /missing|incomplete/i);
});

test('rejects missing, duplicate, extra and out-of-order phases', () => {
  const early = windowsRecorder();
  assert.throws(
    () => early.beginPhase(VERIFY_PHASE),
    /expected.*write-marker/i,
  );
  early.beginPhase(WRITE_PHASE);
  assert.throws(
    () => early.beginPhase(WRITE_PHASE),
    /already active|duplicate/i,
  );

  const recorder = windowsRecorder();
  completeWindowsPhase(recorder, WRITE_PHASE);
  assert.throws(
    () => recorder.assertComplete(),
    /verify-marker-absent|missing/i,
  );
  completeWindowsPhase(recorder, VERIFY_PHASE);
  assert.throws(() => recorder.beginPhase(VERIFY_PHASE), /extra|complete/i);
  assert.throws(
    () => recorder.beginPhase('third-phase'),
    /unknown|extra|complete/i,
  );
});

test('rejects missing, duplicate, extra and out-of-order events', () => {
  const skipped = windowsRecorder();
  startWindowsPhase(skipped, WRITE_PHASE);
  assert.throws(
    () => skipped.acceptEvent(event(WRITE_PHASE, 'active-host-ready')),
    /expected.*process-started/i,
  );

  const missingCleanup = windowsRecorder();
  startWindowsPhase(missingCleanup, WRITE_PHASE);
  for (const name of WRITE_GRAMMAR.slice(0, 6)) {
    missingCleanup.acceptEvent(event(WRITE_PHASE, name));
  }
  assert.throws(
    () =>
      missingCleanup.observeProcessExit(WRITE_PHASE, {
        code: 0,
        signal: null,
        forced: false,
      }),
    /policy-cleanup-acknowledged|missing.*event/i,
  );

  const missingTombstones = windowsRecorder();
  startWindowsPhase(missingTombstones, WRITE_PHASE);
  for (const name of WRITE_GRAMMAR.slice(0, 7)) {
    missingTombstones.acceptEvent(event(WRITE_PHASE, name));
  }
  assert.throws(
    () =>
      missingTombstones.observeProcessExit(WRITE_PHASE, {
        code: 0,
        signal: null,
        forced: false,
      }),
    /policy-tombstones-empty|missing.*event/i,
  );

  const extra = windowsRecorder();
  startWindowsPhase(extra, WRITE_PHASE);
  submitChildGrammar(extra, WRITE_PHASE);
  assert.throws(
    () => extra.acceptEvent(event(WRITE_PHASE, 'tls-entry-absent')),
    /no more child events|expected.*process-exit-observed|extra/i,
  );
});

test('forbids child-submitted process exit and appends it only after clean exit', () => {
  const childSubmitted = windowsRecorder();
  startWindowsPhase(childSubmitted, WRITE_PHASE);
  submitChildGrammar(childSubmitted, WRITE_PHASE, WRITE_GRAMMAR.slice(0, -1));
  assert.throws(
    () =>
      childSubmitted.acceptEvent(event(WRITE_PHASE, 'process-exit-observed')),
    /external|child.*forbidden/i,
  );

  for (const exit of [
    { code: 1, signal: null, forced: false },
    { code: 0, signal: 'SIGTERM', forced: false },
    { code: 0, signal: null, forced: true },
  ]) {
    const recorder = windowsRecorder();
    startWindowsPhase(recorder, WRITE_PHASE);
    submitChildGrammar(recorder, WRITE_PHASE);
    assert.throws(
      () => recorder.observeProcessExit(WRITE_PHASE, exit),
      /zero|signal|forced|clean/i,
    );
    assert.equal(
      recorder
        .traces()[0]
        .events.some(({ event: name }) => name === 'process-exit-observed'),
      false,
    );
  }

  const clean = windowsRecorder();
  startWindowsPhase(clean, WRITE_PHASE);
  submitChildGrammar(clean, WRITE_PHASE);
  clean.observeProcessExit(WRITE_PHASE, {
    code: 0,
    signal: null,
    forced: false,
  });
  assert.deepEqual(clean.traces()[0].events.at(-1), {
    ordinal: 11,
    event: 'process-exit-observed',
  });
});

test('waits for a zero-code non-signaled child exit', async () => {
  const clean = new EventEmitter();
  const cleanExit = waitForCleanChildExit(clean);
  clean.emit('exit', 0, null);
  assert.deepEqual(await cleanExit, { code: 0, signal: null });

  const nonzero = new EventEmitter();
  const nonzeroExit = waitForCleanChildExit(nonzero);
  nonzero.emit('exit', 7, null);
  await assert.rejects(nonzeroExit, /code 7|zero/i);

  const signaled = new EventEmitter();
  const signaledExit = waitForCleanChildExit(signaled);
  signaled.emit('exit', null, 'SIGTERM');
  await assert.rejects(signaledExit, /SIGTERM|signal/i);
});

test('enforces exact JSON keys and the inclusive 4 KiB body limit', () => {
  assert.deepEqual(PROCESS_INFO_KEYS, [
    'runId',
    'phase',
    'binaryTargetOs',
    'binaryTargetArch',
    'translatedProcess',
  ]);
  assert.deepEqual(EVENT_KEYS, ['runId', 'phase', 'event']);
  assert.equal(BODY_LIMIT_BYTES, 4 * 1024);

  const base = JSON.stringify(processInfo(WRITE_PHASE));
  const exactLimit = `${base}${' '.repeat(BODY_LIMIT_BYTES - Buffer.byteLength(base))}`;
  assert.deepEqual(
    decodeExactJsonBody(exactLimit, PROCESS_INFO_KEYS, 'process-info'),
    processInfo(WRITE_PHASE),
  );
  assert.throws(
    () =>
      decodeExactJsonBody(`${exactLimit} `, PROCESS_INFO_KEYS, 'process-info'),
    /4096|4 KiB|too large/i,
  );
  assert.throws(
    () =>
      decodeExactJsonBody(
        JSON.stringify({ ...processInfo(WRITE_PHASE), ordinal: 1 }),
        PROCESS_INFO_KEYS,
        'process-info',
      ),
    /exact keys|extra.*ordinal/i,
  );
  const { translatedProcess, ...missing } = processInfo(WRITE_PHASE);
  void translatedProcess;
  assert.throws(
    () =>
      decodeExactJsonBody(
        JSON.stringify(missing),
        PROCESS_INFO_KEYS,
        'process-info',
      ),
    /exact keys|missing.*translatedProcess/i,
  );
  assert.throws(
    () =>
      decodeExactJsonBody(
        JSON.stringify({ ...event(WRITE_PHASE, 'process-started'), time: 1 }),
        EVENT_KEYS,
        'event',
      ),
    /exact keys|extra.*time/i,
  );
});

test('routes only exact POST process-info and event endpoints', () => {
  assert.equal(PROCESS_INFO_PATH, '/process-info');
  assert.equal(EVENT_PATH, '/events');
  const recorder = windowsRecorder();
  recorder.beginPhase(WRITE_PHASE);

  assert.deepEqual(
    routeSubmission(
      {
        method: 'POST',
        path: PROCESS_INFO_PATH,
        body: JSON.stringify(processInfo(WRITE_PHASE)),
      },
      recorder,
    ),
    {
      accepted: 'process-info',
      runId: RUN_ID,
      kind: 'lifecycle',
      phase: WRITE_PHASE,
    },
  );
  assert.deepEqual(
    routeSubmission(
      {
        method: 'POST',
        path: EVENT_PATH,
        body: JSON.stringify(event(WRITE_PHASE, 'process-started')),
      },
      recorder,
    ),
    { accepted: 'event' },
  );
  assert.throws(
    () =>
      routeSubmission(
        {
          method: 'GET',
          path: PROCESS_INFO_PATH,
          body: JSON.stringify(processInfo(WRITE_PHASE)),
        },
        recorder,
      ),
    /POST|method/i,
  );
  assert.throws(
    () =>
      routeSubmission(
        { method: 'POST', path: '/events/', body: '{}' },
        recorder,
      ),
    /unknown.*path|endpoint/i,
  );
});

test('isolated pre-stage exact-matches ACK kind, nonce, one-shot config/report and cap', () => {
  assert.equal(ISOLATION_REPORT_PATH, '/isolation-report');
  assert.equal(ISOLATION_REPORT_BODY_LIMIT_BYTES, 256 * 1024);
  const recorder = isolationWindowsRecorder();
  const processRequest = {
    method: 'POST',
    path: PROCESS_INFO_PATH,
    body: JSON.stringify(processInfo(WRITE_PHASE)),
  };
  assert.deepEqual(routeSubmission(processRequest, recorder, canaryConfig()), {
    accepted: 'process-info',
    runId: RUN_ID,
    kind: 'isolation',
    phase: WRITE_PHASE,
  });
  assert.throws(
    () => routeSubmission(processRequest, recorder, canaryConfig()),
    /once|duplicate|replay/i,
  );
  assert.throws(
    () =>
      routeSubmission(
        {
          method: 'POST',
          path: EVENT_PATH,
          body: JSON.stringify(event(WRITE_PHASE, 'process-started')),
        },
        recorder,
        canaryConfig(),
      ),
    /isolation.*event|event.*forbidden/i,
  );

  const configRequest = {
    method: 'POST',
    path: CANARY_CONFIG_PATH,
    body: JSON.stringify({ runId: RUN_ID, phase: WRITE_PHASE }),
  };
  assert.deepEqual(
    routeSubmission(configRequest, recorder, canaryConfig()),
    canaryConfig(),
  );
  assert.throws(
    () => routeSubmission(configRequest, recorder, canaryConfig()),
    /once|replay/i,
  );

  const reportBody = JSON.stringify({
    runId: RUN_ID,
    phase: WRITE_PHASE,
    report: isolationReport(),
  });
  assert.deepEqual(
    routeSubmission(
      { method: 'POST', path: ISOLATION_REPORT_PATH, body: reportBody },
      recorder,
      canaryConfig(),
    ),
    {
      accepted: 'isolation-report',
      runId: RUN_ID,
      kind: 'isolation',
      phase: WRITE_PHASE,
    },
  );
  assert.throws(
    () =>
      routeSubmission(
        { method: 'POST', path: ISOLATION_REPORT_PATH, body: reportBody },
        recorder,
        canaryConfig(),
      ),
    /once|duplicate|replay/i,
  );

  const oversized = JSON.stringify({
    runId: STALE_RUN_ID,
    phase: WRITE_PHASE,
    report: { padding: 'x'.repeat(ISOLATION_REPORT_BODY_LIMIT_BYTES) },
  });
  assert.throws(
    () =>
      routeSubmission(
        { method: 'POST', path: ISOLATION_REPORT_PATH, body: oversized },
        isolationWindowsRecorder(STALE_RUN_ID),
        canaryConfig(STALE_RUN_ID),
      ),
    /256 KiB|262144|too large/i,
  );

  recorder.observeProcessExit({ code: 0, signal: null, forced: false });
  assert.deepEqual(
    recorder.seal({ httpsPreflightHits: 1, wssPreflightHandshakes: 1 }),
    {
      report: isolationReport(),
      runnerObservation: {
        httpsPreflightHits: 1,
        wssPreflightHandshakes: 1,
      },
    },
  );

  for (const mutate of [
    (report) => {
      report.checks.sentinelSecret = 'must-never-enter-output';
    },
    (report) => {
      report.checks.resourceVectorResults.fetch.extra = true;
    },
    (report) => {
      report.fixedScenarios[0].extra = 'sentinel';
    },
    (report) => {
      report.counters.extra = 1;
    },
  ]) {
    const rejecting = isolationWindowsRecorder();
    rejecting.acceptProcessInfo(processInfo(WRITE_PHASE));
    const report = isolationReport();
    mutate(report);
    assert.throws(
      () =>
        rejecting.acceptIsolationReport({
          runId: RUN_ID,
          phase: WRITE_PHASE,
          report,
        }),
      /exact keys|extra|sentinel/i,
    );
  }
});

test('isolated pre-stage accepts the Rust-canonical Windows 10 raw report', () => {
  assert.deepEqual(acceptWindowsIsolationReport(), {
    accepted: 'isolation-report',
    runId: RUN_ID,
    kind: 'isolation',
    phase: WRITE_PHASE,
  });
});

test('isolated pre-stage rejects deprecated Windows resource-policy wire aliases', () => {
  const cases = [
    ['webview2-web-resource-requested-source-kinds', true],
    ['webview2-web-resource-requested-legacy', false],
  ];
  for (const [mode, strong] of cases) {
    assertWindowsIsolationReportRejected(
      (report) => {
        report.resourcePolicyMode = mode;
        report.strongSourceKindsInterfaceAvailable = strong;
      },
      /resourcePolicyMode.*deprecated Windows wire alias/i,
      mode,
    );
  }
});

test('isolated pre-stage pins Windows 10 runtime, mode, interface, and optional counter', () => {
  const cases = [
    [
      'wrong WebView2 111 runtime',
      (report) => {
        report.webviewRuntimeVersion = '110.0.0.0';
      },
      /webviewRuntimeVersion.*111\.0\.1661/i,
    ],
    [
      'placeholder runtime',
      (report) => {
        report.webviewRuntimeVersion = 'current';
      },
      /webviewRuntimeVersion.*exact frozen version/i,
    ],
    [
      'legacy mode with strong interface',
      (report) => {
        report.resourcePolicyMode = 'webview2-legacy-all-contexts-candidate';
      },
      /resourcePolicyMode.*strongSourceKindsInterfaceAvailable/i,
    ],
    [
      'v22 mode without strong interface',
      (report) => {
        report.strongSourceKindsInterfaceAvailable = false;
      },
      /resourcePolicyMode.*strongSourceKindsInterfaceAvailable/i,
    ],
    [
      'Windows optional blocked counter is null',
      (report) => {
        report.checks.blockedCanaryAttempts = null;
      },
      /blockedCanaryAttempts.*nonnegative safe integer on Windows/i,
    ],
  ];
  for (const [label, mutate, expected] of cases) {
    assertWindowsIsolationReportRejected(mutate, expected, label);
  }
});

test('isolated pre-stage correlates the Windows blocked counter with the raw snapshot', () => {
  assertWindowsIsolationReportRejected((report) => {
    report.checks.blockedCanaryAttempts =
      report.counters.resourceCanaryHits + 1;
  }, /blockedCanaryAttempts.*counters\.resourceCanaryHits/i);
});

test('isolated pre-stage pins every vector barrier, evidence mode, counterfactual, redirect, and hit relation', () => {
  const row = (report, vector) => report.checks.resourceVectorResults[vector];
  const cases = [
    [
      'Windows resource barrier',
      (report) => {
        row(report, 'document').expectedBarrier = 'wk-content-rule-list';
        row(report, 'document').enforcedBarrier = 'wk-content-rule-list';
      },
      /document.*expectedBarrier.*webview2-web-resource-requested/i,
    ],
    [
      'Windows resource evidence mode',
      (report) => {
        row(report, 'document').barrierEvidenceMode = 'paired-counterfactual';
      },
      /document.*barrierEvidenceMode.*native-callback/i,
    ],
    [
      'Windows resource counterfactual must be null',
      (report) => {
        row(report, 'document').counterfactualServerHits = 1;
      },
      /document.*counterfactualServerHits.*null on Windows/i,
    ],
    [
      'popup barrier',
      (report) => {
        row(report, 'popup').expectedBarrier = 'download-handler';
        row(report, 'popup').enforcedBarrier = 'download-handler';
      },
      /popup.*expectedBarrier.*new-window-handler/i,
    ],
    [
      'download barrier',
      (report) => {
        row(report, 'download').expectedBarrier = 'navigation-handler';
        row(report, 'download').enforcedBarrier = 'navigation-handler';
      },
      /download.*expectedBarrier.*download-handler/i,
    ],
    [
      'top-level barrier',
      (report) => {
        row(report, 'top_level_data').expectedBarrier = 'new-window-handler';
        row(report, 'top_level_data').enforcedBarrier = 'new-window-handler';
      },
      /top_level_data.*expectedBarrier.*navigation-handler/i,
    ],
    [
      'handler evidence mode',
      (report) => {
        row(report, 'popup').barrierEvidenceMode = 'native-callback';
      },
      /popup.*barrierEvidenceMode.*handler-callback/i,
    ],
    [
      'handler counterfactual must be null',
      (report) => {
        row(report, 'popup').counterfactualServerHits = 1;
      },
      /popup.*counterfactualServerHits.*null for handler vectors/i,
    ],
    [
      'redirect hop count',
      (report) => {
        row(report, 'redirect').allowedRedirectHopHits = 1;
      },
      /redirect.*allowedRedirectHopHits.*2/i,
    ],
    [
      'protected server hit',
      (report) => {
        row(report, 'document').serverHits = 1;
      },
      /document.*serverHits.*0/i,
    ],
    [
      'absent service-worker evidence mode',
      (report) => {
        const serviceWorker = row(report, 'service_worker');
        serviceWorker.availabilityOutcome = 'service-worker-api-absent';
      },
      /service_worker.*barrierEvidenceMode.*deterministic-seam-only/i,
    ],
  ];
  for (const [label, mutate, expected] of cases) {
    assertWindowsIsolationReportRejected(mutate, expected, label);
  }
});

test('isolated pre-stage derives every mandatory fixed-scenario check from Rust actor events', () => {
  const events = (report, index) =>
    report.fixedScenarios[index].orderedActorEvents;
  const remove = (values, eventName) => {
    values.splice(values.indexOf(eventName), 1);
  };
  const reversePair = (values, first, second) => {
    const firstIndex = values.indexOf(first);
    const secondIndex = values.indexOf(second);
    [values[firstIndex], values[secondIndex]] = [
      values[secondIndex],
      values[firstIndex],
    ];
  };
  const cases = [
    [
      'policy fault trace',
      (report) =>
        remove(events(report, 0), 'policy-registration-fault-observed'),
      /fixed scenarios.*policyFaultInvalidatesInstance/i,
    ],
    [
      'initialization timeout trace',
      (report) => remove(events(report, 1), 'initialization-timeout-observed'),
      /fixed scenarios.*timeoutCheck/i,
    ],
    [
      'sign timeout trace',
      (report) => remove(events(report, 2), 'sign-timeout-observed'),
      /fixed scenarios.*timeoutCheck/i,
    ],
    [
      'destroy before retry relation',
      (report) =>
        reversePair(events(report, 0), 'teardown-complete', 'retry-ready'),
      /fixed scenarios.*destroyConfirmedBeforeRetry/i,
    ],
    [
      'unique cleanup evidence',
      (report) => events(report, 0).push('native-destroyed'),
      /fixed scenarios.*resourcePolicyCleanupAcknowledged/i,
    ],
    [
      'retry completion trace',
      (report) => remove(events(report, 0), 'retry-destroyed'),
      /fixed scenarios.*retryCheck/i,
    ],
    [
      'late callback generation relation',
      (report) =>
        reversePair(
          events(report, 4),
          'new-generation-ready',
          'late-callback-isolated',
        ),
      /fixed scenarios.*lateCallbackIsolated/i,
    ],
    [
      'main-close release relation',
      (report) =>
        reversePair(
          events(report, 5),
          'would-exit-blocked',
          'would-exit-released',
        ),
      /fixed scenarios.*policyTombstonesEmptyBeforeExit/i,
    ],
    [
      'derived checks cannot lie',
      (report) => {
        report.checks.timeoutCheck = false;
      },
      /fixed scenarios.*timeoutCheck.*disagrees/i,
    ],
  ];
  for (const [label, mutate, expected] of cases) {
    assertWindowsIsolationReportRejected(mutate, expected, label);
  }
});

test('isolated pre-stage rejects every unsafe Rust u64 wire value and non-null Option omission', () => {
  const unsafe = Number.MAX_SAFE_INTEGER + 1;
  const unsafeCases = [
    ['report generation', (report) => (report.generation = unsafe)],
    ['report operation', (report) => (report.operationId = unsafe)],
    ...[
      'blockedNavigations',
      'blockedNewWindows',
      'blockedDownloads',
      'blockedResourceRequests',
      'resourceCanaryHits',
      'policyFaults',
    ].map((key) => [
      `counter ${key}`,
      (report) => (report.counters[key] = unsafe),
    ]),
    ...RUST_CANONICAL_WINDOWS_10_REPORT.fixedScenarios.flatMap((_, index) => [
      [
        `fixed scenario ${index} generation`,
        (report) => (report.fixedScenarios[index].generation = unsafe),
      ],
      [
        `fixed scenario ${index} operation`,
        (report) => (report.fixedScenarios[index].operationId = unsafe),
      ],
    ]),
    [
      'row protected hit total',
      (report) => (report.checks.crossOriginCanaryServerHits = unsafe),
    ],
    [
      'optional blocked counter',
      (report) => (report.checks.blockedCanaryAttempts = unsafe),
    ],
    [
      'redirect hops',
      (report) =>
        (report.checks.resourceVectorResults.redirect.allowedRedirectHopHits =
          unsafe),
    ],
    [
      'vector server hits',
      (report) =>
        (report.checks.resourceVectorResults.document.serverHits = unsafe),
    ],
  ];
  for (const [label, mutate] of unsafeCases) {
    assertWindowsIsolationReportRejected(mutate, /safe integer/i, label);
  }

  const omittedOptions = [
    [
      'translatedProcess',
      (report) => (report.translatedProcess = undefined),
      /translatedProcess.*explicitly null on Windows/i,
    ],
    [
      'strong-source interface',
      (report) => (report.strongSourceKindsInterfaceAvailable = undefined),
      /strongSourceKindsInterfaceAvailable.*boolean on Windows/i,
    ],
    [
      'blocked counter',
      (report) => (report.checks.blockedCanaryAttempts = undefined),
      /blockedCanaryAttempts.*nonnegative safe integer on Windows/i,
    ],
    [
      'counterfactual',
      (report) =>
        (report.checks.resourceVectorResults.document.counterfactualServerHits =
          undefined),
      /document.*counterfactualServerHits.*explicitly null on Windows/i,
    ],
  ];
  for (const [label, mutate, expected] of omittedOptions) {
    assertWindowsIsolationReportRejected(mutate, expected, label);
  }
});

test('validates all four host, child architecture and translation correlations', () => {
  const cases = [
    [
      'windows-10-webview2-111-x64',
      WINDOWS_HOST,
      {
        binaryTargetOs: 'windows',
        binaryTargetArch: 'x86_64',
        translatedProcess: null,
      },
    ],
    [
      'windows-11-x64',
      {
        hostPlatform: 'win32',
        hostArch: 'x64',
        osVersion: '10.0.26100',
      },
      {
        binaryTargetOs: 'windows',
        binaryTargetArch: 'x86_64',
        translatedProcess: null,
      },
    ],
    [
      'macos-13-intel',
      { hostPlatform: 'darwin', hostArch: 'x64', osVersion: '13.3.1' },
      {
        binaryTargetOs: 'macos',
        binaryTargetArch: 'x86_64',
        translatedProcess: false,
      },
    ],
    [
      'macos-current-arm64',
      { hostPlatform: 'darwin', hostArch: 'arm64', osVersion: '15.5' },
      {
        binaryTargetOs: 'macos',
        binaryTargetArch: 'aarch64',
        translatedProcess: false,
      },
    ],
  ];

  assert.deepEqual(
    Object.keys(PLATFORM_MATRIX),
    cases.map(([id]) => id),
  );
  for (const [platformId, hostFacts, target] of cases) {
    assert.deepEqual(validateHostFacts(platformId, hostFacts), hostFacts);
    const recorder = new LifecycleRecorder({
      runId: RUN_ID,
      platformId,
      hostFacts,
    });
    recorder.beginPhase(WRITE_PHASE);
    assert.doesNotThrow(() =>
      recorder.acceptProcessInfo({
        runId: RUN_ID,
        phase: WRITE_PHASE,
        ...target,
      }),
    );
  }
});

test('derives Windows and macOS versions from the required host authorities', () => {
  let releaseCalls = 0;
  assert.deepEqual(
    collectHostFacts({
      hostPlatform: 'win32',
      hostArch: 'x64',
      osRelease: () => {
        releaseCalls += 1;
        return '10.0.26100';
      },
      execFile: () => {
        throw new Error('Windows must not call sw_vers');
      },
    }),
    {
      hostPlatform: 'win32',
      hostArch: 'x64',
      osVersion: '10.0.26100',
    },
  );
  assert.equal(releaseCalls, 1);

  let swVersCall;
  assert.deepEqual(
    collectHostFacts({
      hostPlatform: 'darwin',
      hostArch: 'arm64',
      osRelease: () => {
        throw new Error('macOS must not use os.release() as product version');
      },
      execFile: (...args) => {
        swVersCall = args;
        return '15.5\n';
      },
    }),
    {
      hostPlatform: 'darwin',
      hostArch: 'arm64',
      osVersion: '15.5',
    },
  );
  assert.deepEqual(swVersCall, [
    '/usr/bin/sw_vers',
    ['-productVersion'],
    { encoding: 'utf8' },
  ]);
  assert.throws(
    () => collectHostFacts({ hostPlatform: 'linux', hostArch: 'x64' }),
    /do not support host linux/i,
  );
});

test('fails closed on wrong host/child combinations and Rosetta translation', () => {
  assert.throws(
    () =>
      validateHostFacts('windows-10-webview2-111-x64', {
        ...WINDOWS_HOST,
        hostArch: 'arm64',
      }),
    /hostArch|x64/i,
  );
  assert.throws(
    () =>
      validateHostFacts('windows-10-webview2-111-x64', {
        ...WINDOWS_HOST,
        osVersion: '10.0.22631',
      }),
    /19045|osVersion/i,
  );
  assert.throws(
    () =>
      validateHostFacts('windows-11-x64', {
        ...WINDOWS_HOST,
        osVersion: '10.0.19045',
      }),
    /22000|Windows 11|osVersion/i,
  );
  assert.throws(
    () =>
      validateHostFacts('macos-13-intel', {
        hostPlatform: 'darwin',
        hostArch: 'x64',
        osVersion: '13.4',
      }),
    /13\.3|osVersion/i,
  );

  const badTargetCases = [
    { binaryTargetOs: 'macos' },
    { binaryTargetArch: 'aarch64' },
    { translatedProcess: false },
  ];
  for (const overrides of badTargetCases) {
    const recorder = windowsRecorder();
    recorder.beginPhase(WRITE_PHASE);
    assert.throws(
      () => recorder.acceptProcessInfo(processInfo(WRITE_PHASE, overrides)),
      /binaryTarget|translatedProcess|platform correlation/i,
    );
  }

  const translated = new LifecycleRecorder({
    runId: RUN_ID,
    platformId: 'macos-13-intel',
    hostFacts: {
      hostPlatform: 'darwin',
      hostArch: 'x64',
      osVersion: '13.3',
    },
  });
  translated.beginPhase(WRITE_PHASE);
  assert.throws(
    () =>
      translated.acceptProcessInfo({
        runId: RUN_ID,
        phase: WRITE_PHASE,
        binaryTargetOs: 'macos',
        binaryTargetArch: 'x86_64',
        translatedProcess: true,
      }),
    /Rosetta|translatedProcess|translated/i,
  );
});

test('requires both process-info reports to agree', () => {
  const recorder = windowsRecorder();
  completeWindowsPhase(recorder, WRITE_PHASE);
  recorder.beginPhase(VERIFY_PHASE);
  assert.throws(
    () =>
      recorder.acceptProcessInfo(
        processInfo(VERIFY_PHASE, { binaryTargetArch: 'aarch64' }),
      ),
    /binaryTargetArch|agree|platform correlation/i,
  );
});

test('accepts only the exact CLI and output directory contract', () => {
  const cwd = resolve('C:/yinmi-repository');
  const valid = parseArguments(
    [
      '--app',
      'src-tauri/target/feasibility/debug/yinmi.exe',
      '--platform-id',
      'windows-10-webview2-111-x64',
      '--output',
      'artifacts/feasibility/signature/windows-10-lifecycle.json',
    ],
    { cwd },
  );
  assert.deepEqual(valid, {
    appPath: resolve(cwd, 'src-tauri/target/feasibility/debug/yinmi.exe'),
    platformId: 'windows-10-webview2-111-x64',
    outputPath: resolve(
      cwd,
      'artifacts/feasibility/signature/windows-10-lifecycle.json',
    ),
  });

  const invalidArgv = [
    [],
    ['--app', 'app', '--platform-id', 'windows-11-x64'],
    [
      '--app',
      'app',
      '--app',
      'other',
      '--platform-id',
      'windows-11-x64',
      '--output',
      'artifacts/feasibility/signature/out.json',
    ],
    [
      '--app',
      'app',
      '--platform-id',
      'windows-x64',
      '--output',
      'artifacts/feasibility/signature/out.json',
    ],
    [
      '--app',
      'app',
      '--platform-id',
      'windows-11-x64',
      '--output',
      'artifacts/feasibility/out.json',
    ],
    [
      '--app',
      'app',
      '--platform-id',
      'windows-11-x64',
      '--output',
      'artifacts/feasibility/signature-other/out.json',
    ],
    [
      '--app',
      'app',
      '--platform-id',
      'windows-11-x64',
      '--output',
      'artifacts/feasibility/signature/out.json',
      '--verbose',
    ],
  ];
  for (const argv of invalidArgv) {
    assert.throws(
      () => parseArguments(argv, { cwd }),
      /usage|argument|output|platform/i,
    );
  }
});

test('injects exactly the three signature autorun variables', () => {
  const environment = buildChildEnvironment(
    {
      PATH: 'kept',
      YINMI_FEASIBILITY_SIGNATURE_AUTORUN: 'stale-mode',
      YINMI_FEASIBILITY_SIGNATURE_TRACE_ENDPOINT: 'http://stale.invalid',
      YINMI_FEASIBILITY_SIGNATURE_RUN_ID: STALE_RUN_ID,
      YINMI_FEASIBILITY_SIGNATURE_UNRECOGNIZED: 'must-be-removed',
      yinmi_feasibility_signature_case_shadow: 'must-also-be-removed',
      [CONTROLLED_VM_ENV]: '1',
    },
    {
      phase: WRITE_PHASE,
      endpoint: 'http://127.0.0.1:43210',
      runId: RUN_ID,
    },
  );
  assert.equal(environment.PATH, 'kept');
  assert.equal(CONTROLLED_VM_ENV in environment, false);
  assert.equal('yinmi_feasibility_signature_case_shadow' in environment, false);
  assert.deepEqual(
    Object.fromEntries(
      Object.entries(environment).filter(([key]) =>
        key.startsWith('YINMI_FEASIBILITY_SIGNATURE_'),
      ),
    ),
    {
      YINMI_FEASIBILITY_SIGNATURE_AUTORUN: WRITE_PHASE,
      YINMI_FEASIBILITY_SIGNATURE_TRACE_ENDPOINT: 'http://127.0.0.1:43210',
      YINMI_FEASIBILITY_SIGNATURE_RUN_ID: RUN_ID,
    },
  );
});

test('uses exact 60-second phase and 130-second total deadline seams', async () => {
  assert.equal(PHASE_DEADLINE_MS, 60_000);
  assert.equal(TOTAL_DEADLINE_MS, 130_000);

  for (const [label, milliseconds] of [
    ['phase write-marker-and-close-main', PHASE_DEADLINE_MS],
    ['two-phase lifecycle', TOTAL_DEADLINE_MS],
  ]) {
    let scheduled;
    let cleared;
    const pending = withDeadline(new Promise(() => {}), milliseconds, label, {
      setTimeout(callback, delay) {
        scheduled = { callback, delay };
        return 91;
      },
      clearTimeout(handle) {
        cleared = handle;
      },
    });
    assert.equal(scheduled.delay, milliseconds);
    scheduled.callback();
    await assert.rejects(pending, new RegExp(`${milliseconds}|timed out`, 'i'));
    assert.equal(cleared, 91);
  }
});

test('isolation autorun awaits one nonce-bound IPC readiness barrier without sleeping', async () => {
  const source = await readFile(
    resolve('src-tauri/src/feasibility/signature_probe.rs'),
    'utf8',
  );
  const autorunStart = source.indexOf('async fn run_lifecycle_autorun');
  const autorunEnd = source.indexOf('pub fn start_lifecycle_autorun');
  assert.ok(autorunStart >= 0 && autorunEnd > autorunStart);
  const autorun = source.slice(autorunStart, autorunEnd);
  const processInfo = autorun.indexOf('post_process_info');
  const readiness = autorun.indexOf('await_ipc_canary_readiness');
  const isolation = autorun.indexOf('run_isolation_probe');
  const report = autorun.indexOf('post_isolation_report');
  assert.ok(
    processInfo >= 0 &&
      readiness > processInfo &&
      isolation > readiness &&
      report > isolation,
    'process-info ACK must precede readiness, isolation and report submission',
  );
  assert.doesNotMatch(autorun.slice(processInfo, isolation), /(?:sleep|interval)\s*\(/);
  assert.match(
    source,
    /IPC_CANARY_READY_DEADLINE[^;]*Duration::from_secs\(15\)/s,
  );
  for (const state of [
    'Inactive',
    'Armed',
    'BaselineIssued',
    'ReadyAccepted',
    'Sealed',
    'Failed',
  ]) {
    assert.match(source, new RegExp(`\\b${state}\\b`));
  }

  const start = source.slice(autorunEnd, source.indexOf('#[derive', autorunEnd));
  assert.ok(
    start.indexOf('.arm(') >= 0 &&
      start.indexOf('tauri::async_runtime::spawn') > start.indexOf('.arm('),
    'the readiness gate must arm synchronously before the autorun task is spawned',
  );
});

test('orchestrates isolation then two lifecycle children with fresh nonces and merged preflight', async () => {
  const endpoints = ['http://127.0.0.1:43210', 'http://127.0.0.1:43211'];
  const appPath = 'C:\\private\\yinmi.exe';
  const outputPath = 'C:\\repo\\artifacts\\feasibility\\signature\\out.json';
  const never = new Promise(() => {});
  const launches = [];
  const scheduledDeadlines = [];
  const writes = [];
  const recorders = [];
  const serverContexts = [];
  const order = [];
  let closeCount = 0;
  let randomIndex = 0;

  const output = await runLifecycleProbe(
    {
      appPath,
      platformId: 'windows-10-webview2-111-x64',
      outputPath,
    },
    {
      environment: { PATH: 'kept', [CONTROLLED_VM_ENV]: '1' },
      hostFacts: WINDOWS_HOST,
      randomSource: () =>
        Buffer.from([RUN_ID, STALE_RUN_ID][randomIndex++], 'hex'),
      stat: async () => ({ isFile: () => true }),
      startServer: async (value, context) => {
        const serverIndex = recorders.length;
        recorders.push(value);
        serverContexts.push(context);
        order.push(
          serverIndex === 0
            ? 'isolation-server-started'
            : 'lifecycle-server-started',
        );
        return {
          endpoint: endpoints[serverIndex],
          fatal: never,
          isolationObservation:
            serverIndex === 0
              ? () => ({ httpsPreflightHits: 1, wssPreflightHandshakes: 1 })
              : undefined,
          verifyBaseline:
            serverIndex === 0
              ? async () => order.push('server-baseline-verified')
              : undefined,
          close: async () => {
            closeCount += 1;
            order.push(
              serverIndex === 0
                ? 'isolation-server-closed'
                : 'lifecycle-server-closed',
            );
          },
        };
      },
      verifyIsolationBaseline: async () =>
        order.push('isolation-baseline-verified'),
      spawnChild: (command, args, options) => {
        const child = new EventEmitter();
        child.exitCode = null;
        child.signalCode = null;
        child.kill = () => {
          throw new Error('clean fake child must not be killed');
        };
        launches.push({ command, args, options });
        order.push(
          `spawn:${launches.length}:${options.env.YINMI_FEASIBILITY_SIGNATURE_AUTORUN}`,
        );
        queueMicrotask(() => {
          const phase = options.env.YINMI_FEASIBILITY_SIGNATURE_AUTORUN;
          const runId = options.env.YINMI_FEASIBILITY_SIGNATURE_RUN_ID;
          const recorder = recorders[recorders.length - 1];
          const ack = recorder.acceptProcessInfo(processInfo(phase, { runId }));
          if (recorder instanceof IsolationRecorder) {
            assert.deepEqual(ack, {
              accepted: 'process-info',
              runId: RUN_ID,
              kind: 'isolation',
              phase: WRITE_PHASE,
            });
            recorder.acceptIsolationReport({
              runId,
              phase,
              report: isolationReport(),
            });
          } else {
            assert.equal(ack.kind, 'lifecycle');
            for (const name of PHASE_GRAMMARS[phase].slice(0, -1)) {
              recorder.acceptEvent(event(phase, name, { runId }));
            }
          }
          child.exitCode = 0;
          child.emit('exit', 0, null);
        });
        return child;
      },
      timerApi: {
        setTimeout(callback, delay) {
          const handle = { callback, delay, cleared: false };
          scheduledDeadlines.push(handle);
          return handle;
        },
        clearTimeout(handle) {
          handle.cleared = true;
        },
      },
      mkdir: async () => {},
      writeFile: async (...args) => {
        writes.push(args);
      },
    },
  );

  assert.equal(output.status, 'pass');
  assert.equal(closeCount, 2);
  assert.equal(serverContexts[0].controlledVm, true);
  assert.equal(serverContexts[1].kind, 'lifecycle');
  assert.equal(launches.length, 3);
  assert.deepEqual(
    launches.map(({ command, args }) => ({ command, args })),
    [
      { command: appPath, args: [] },
      { command: appPath, args: [] },
      { command: appPath, args: [] },
    ],
  );
  assert.deepEqual(
    launches.map(
      ({ options }) => options.env.YINMI_FEASIBILITY_SIGNATURE_AUTORUN,
    ),
    [WRITE_PHASE, ...PHASES],
  );
  assert.deepEqual(
    launches.map(
      ({ options }) => options.env.YINMI_FEASIBILITY_SIGNATURE_RUN_ID,
    ),
    [RUN_ID, STALE_RUN_ID, STALE_RUN_ID],
  );
  assert.deepEqual(
    launches.map(
      ({ options }) => options.env.YINMI_FEASIBILITY_SIGNATURE_TRACE_ENDPOINT,
    ),
    [endpoints[0], endpoints[1], endpoints[1]],
  );
  for (const { options } of launches) {
    assert.equal(options.shell, false);
    assert.equal(options.windowsHide, true);
    assert.equal(CONTROLLED_VM_ENV in options.env, false);
  }
  assert.deepEqual(
    scheduledDeadlines.map(({ delay }) => delay),
    [
      ISOLATION_STAGE_DEADLINE_MS,
      TOTAL_DEADLINE_MS,
      PHASE_DEADLINE_MS,
      PHASE_DEADLINE_MS,
    ],
  );
  assert.equal(
    scheduledDeadlines.every(({ cleared }) => cleared),
    true,
  );
  assert.equal(writes.length, 1);
  assert.equal(writes[0][0], outputPath);
  assert.deepEqual(JSON.parse(writes[0][1]), output);
  assert.equal(writes[0][2], 'utf8');
  assert.deepEqual(order, [
    'isolation-server-started',
    `spawn:1:${WRITE_PHASE}`,
    'isolation-server-closed',
    'server-baseline-verified',
    'isolation-baseline-verified',
    'lifecycle-server-started',
    `spawn:2:${WRITE_PHASE}`,
    `spawn:3:${VERIFY_PHASE}`,
    'lifecycle-server-closed',
  ]);
  assert.deepEqual(output.isolation.runnerObservation, {
    httpsPreflightHits: 1,
    wssPreflightHandshakes: 1,
  });
  assert.equal(
    output.isolation.report.checks.ordinaryExitCleanupAcknowledged,
    true,
  );
  assert.equal(output.isolation.report.checks.restartStorageRecovered, false);
});

test('isolation deadline failure force-kills only for cleanup and saves a diagnostic', async () => {
  const endpoint = 'http://127.0.0.1:43210';
  const appPath = 'C:\\private\\yinmi.exe';
  const never = new Promise(() => {});
  const scheduledDeadlines = [];
  const writes = [];
  let killed = false;

  const probe = runLifecycleProbe(
    {
      appPath,
      platformId: 'windows-10-webview2-111-x64',
      outputPath: 'C:\\repo\\artifacts\\feasibility\\signature\\timeout.json',
    },
    {
      environment: {},
      hostFacts: WINDOWS_HOST,
      randomSource: () => Buffer.from(RUN_ID, 'hex'),
      stat: async () => ({ isFile: () => true }),
      startServer: async () => ({
        endpoint,
        fatal: never,
        close: async () => {},
      }),
      spawnChild: () => {
        const child = new EventEmitter();
        child.exitCode = null;
        child.signalCode = null;
        child.kill = () => {
          killed = true;
          child.signalCode = 'SIGTERM';
          queueMicrotask(() => child.emit('exit', null, 'SIGTERM'));
          return true;
        };
        return child;
      },
      timerApi: {
        setTimeout(callback, delay) {
          const handle = { callback, delay };
          scheduledDeadlines.push(handle);
          return handle;
        },
        clearTimeout() {},
      },
      mkdir: async () => {},
      writeFile: async (...args) => {
        writes.push(args);
      },
    },
  );

  for (
    let attempt = 0;
    attempt < 10 && scheduledDeadlines.length < 1;
    attempt += 1
  ) {
    await Promise.resolve();
  }
  assert.deepEqual(
    scheduledDeadlines.map(({ delay }) => delay),
    [ISOLATION_STAGE_DEADLINE_MS],
  );
  scheduledDeadlines[0].callback();
  await assert.rejects(
    probe,
    new RegExp(
      `timed out after ${ISOLATION_STAGE_DEADLINE_MS} milliseconds`,
      'i',
    ),
  );
  assert.equal(killed, true);
  assert.equal(writes.length, 1);
  const diagnostic = JSON.parse(writes[0][1]);
  assert.equal(diagnostic.status, 'failure');
  assert.equal(diagnostic.failure.forcedKill, true);
  const serialized = JSON.stringify(diagnostic);
  assert.equal(serialized.includes(RUN_ID), false);
  assert.equal(serialized.includes(endpoint), false);
  assert.equal(serialized.includes(appPath), false);
});

test('sanitized output retains correlation and traces but redacts all secrets', () => {
  const absoluteAppPath = 'C:\\private\\yinmi-marker-supersecret\\yinmi.exe';
  const endpoint = 'http://127.0.0.1:49152';
  const markerValue = 'yinmi-marker-supersecret';
  const recorder = windowsRecorder();
  completeWindowsPhase(recorder, WRITE_PHASE);
  completeWindowsPhase(recorder, VERIFY_PHASE);

  const output = buildSanitizedOutput({
    status: 'failure',
    platformId: 'windows-10-webview2-111-x64',
    hostFacts: WINDOWS_HOST,
    recorder,
    failure: new Error(
      `probe ${RUN_ID} at ${endpoint} for ${absoluteAppPath} marker ${markerValue}`,
    ),
    forcedKill: true,
    secrets: [RUN_ID, endpoint, absoluteAppPath, markerValue],
  });
  assert.deepEqual(Object.keys(output), [
    'status',
    'platformId',
    'hostPlatform',
    'hostArch',
    'osVersion',
    'binaryTargetOs',
    'binaryTargetArch',
    'translatedProcess',
    'traces',
    'failure',
  ]);
  assert.deepEqual(output.failure, {
    message: 'probe [redacted] at [redacted] for [redacted] marker [redacted]',
    forcedKill: true,
  });
  assert.deepEqual(
    output.traces.map(({ phase }) => phase),
    PHASES,
  );
  const serialized = JSON.stringify(output);
  for (const secret of [RUN_ID, endpoint, absoluteAppPath, markerValue]) {
    assert.equal(serialized.includes(secret), false);
  }

  const invalidEventRecorder = windowsRecorder();
  startWindowsPhase(invalidEventRecorder, WRITE_PHASE);
  let protocolFailure;
  try {
    invalidEventRecorder.acceptEvent(event(WRITE_PHASE, markerValue));
  } catch (error) {
    protocolFailure = error;
  }
  assert.ok(protocolFailure instanceof Error);
  const protocolOutput = buildSanitizedOutput({
    status: 'failure',
    platformId: 'windows-10-webview2-111-x64',
    hostFacts: WINDOWS_HOST,
    recorder: invalidEventRecorder,
    failure: protocolFailure,
    forcedKill: true,
    secrets: [RUN_ID, endpoint, absoluteAppPath],
  });
  assert.equal(JSON.stringify(protocolOutput).includes(markerValue), false);
});

test('sanitized output redacts sensitive sentinels, absolute paths, and nonces from allowed field values', () => {
  const sentinel = 'allowed-field-secret-sentinel';
  const absolutePath = 'C:\\private\\allowed-field\\payload.json';
  const nonce = STALE_RUN_ID;
  const report = isolationReport();
  report.webviewRuntimeVersion = absolutePath;
  report.fixedScenarios[0].orderedActorEvents.push(sentinel);
  report.fixedScenarios[1].orderedActorEvents.push(nonce);
  const recorder = windowsRecorder();
  completeWindowsPhase(recorder, WRITE_PHASE);
  completeWindowsPhase(recorder, VERIFY_PHASE);

  const output = buildSanitizedOutput({
    status: 'pass',
    platformId: 'windows-10-webview2-111-x64',
    hostFacts: WINDOWS_HOST,
    recorder,
    isolation: {
      report,
      runnerObservation: {
        httpsPreflightHits: 1,
        wssPreflightHandshakes: 1,
      },
    },
    secrets: [],
  });
  const serialized = JSON.stringify(output);
  for (const sensitive of [sentinel, absolutePath, nonce]) {
    assert.equal(serialized.includes(sensitive), false, sensitive);
  }
});

test('controlled canary recorder freezes routes and derives paired observations', () => {
  assert.equal(CANARY_IDLE_DURATION_MS, 600_000);
  assert.deepEqual(CANARY_RESOURCE_VECTORS, [
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
  const recorder = new CanaryRecorder({ runId: RUN_ID });
  routeCanaryControl(
    {
      method: 'POST',
      path: '/canary/reset',
      body: JSON.stringify({
        runId: RUN_ID,
        mode: 'counterfactual',
        vector: 'redirect',
      }),
    },
    recorder,
  );
  routeCanaryHit(
    `/redirect/one?runId=${RUN_ID}&mode=counterfactual&vector=redirect`,
    'allowed-redirect-hop',
    recorder,
  );
  routeCanaryHit(
    `/redirect/two?runId=${RUN_ID}&mode=counterfactual&vector=redirect`,
    'allowed-redirect-hop',
    recorder,
  );
  routeCanaryHit(
    `/blocked/redirect?runId=${RUN_ID}&mode=counterfactual&vector=redirect`,
    'blocked-origin',
    recorder,
  );
  const snapshot = routeCanaryControl(
    {
      method: 'GET',
      path: `/canary/snapshot?runId=${RUN_ID}&mode=counterfactual&vector=redirect`,
      body: '',
    },
    recorder,
  );
  assert.deepEqual(snapshot, {
    runId: RUN_ID,
    mode: 'counterfactual',
    vector: 'redirect',
    directHits: 1,
    allowedRedirectHopHits: 2,
    browserPreflightHits: 0,
    websocketHandshakes: 0,
    sleepWakeObserved: false,
    browserProcessBaseline: 0,
    browserProcessCurrent: 0,
    visibleWindowLeakObserved: false,
    unexpectedActivationObserved: false,
  });
  assert.throws(
    () =>
      routeCanaryHit(
        `/blocked/fetch?runId=${STALE_RUN_ID}&mode=protected&vector=fetch`,
        'blocked-origin',
        recorder,
      ),
    /run id/i,
  );

  const protectedFetch = {
    runId: RUN_ID,
    mode: 'protected',
    vector: 'fetch',
  };
  recorder.reset(protectedFetch);
  recorder.hit(protectedFetch, 'blocked-origin');
  assert.throws(() => recorder.reset(protectedFetch), /one-shot.*replay/i);
  assert.equal(recorder.snapshot(protectedFetch).directHits, 1);
  assert.throws(
    () =>
      routeCanaryHit(
        `/blocked/fetch?runId=${RUN_ID}&mode=protected&vector=fetch&extra=1`,
        'blocked-origin',
        recorder,
      ),
    /exact keys|extra/i,
  );
  assert.throws(
    () =>
      routeCanaryHit(
        `/blocked/image?runId=${RUN_ID}&mode=protected&vector=fetch`,
        'blocked-origin',
        recorder,
      ),
    /correlation/i,
  );
});

test('protected recorder owns completion silence and rejects every late hit after sealing', () => {
  let now = 1_000;
  const recorder = new CanaryRecorder({ runId: RUN_ID, now: () => now });
  for (const vector of CANARY_RESOURCE_VECTORS) {
    const coordinate = { runId: RUN_ID, mode: 'protected', vector };
    recorder.reset(coordinate);
    recorder.complete(coordinate);
  }
  const fetchCoordinate = {
    runId: RUN_ID,
    mode: 'protected',
    vector: 'fetch',
  };
  assert.deepEqual(recorder.completionBarrier(fetchCoordinate), {
    status: 'pending',
    retryAfterMs: CANARY_COMPLETION_QUIET_MS,
  });
  recorder.hit(fetchCoordinate, 'blocked-origin');
  now += CANARY_COMPLETION_QUIET_MS;
  for (const vector of CANARY_RESOURCE_VECTORS) {
    const barrier = recorder.completionBarrier({
      runId: RUN_ID,
      mode: 'protected',
      vector,
    });
    if (vector === 'fetch') assert.equal(barrier.snapshot.directHits, 1);
  }
  assert.throws(() => recorder.sealProtected(), /protected.*hit/i);

  const clean = new CanaryRecorder({ runId: RUN_ID, now: () => now });
  for (const vector of CANARY_RESOURCE_VECTORS) {
    const coordinate = { runId: RUN_ID, mode: 'protected', vector };
    clean.reset(coordinate);
    clean.complete(coordinate);
  }
  now += CANARY_COMPLETION_QUIET_MS;
  for (const vector of CANARY_RESOURCE_VECTORS) {
    assert.equal(
      clean.completionBarrier({
        runId: RUN_ID,
        mode: 'protected',
        vector,
      }).status,
      'complete',
    );
  }
  clean.sealProtected();
  assert.throws(
    () => clean.hit(fetchCoordinate, 'blocked-origin'),
    /late protected canary hit/i,
  );
  assert.throws(
    () => clean.verifyProtectedSeal(),
    /sticky late-hit violation/i,
  );
});

test('lifecycle canary snapshots require injected process/window observations and runner sleep-wake', () => {
  const recorder = new CanaryRecorder({ runId: RUN_ID });
  const reset = {
    method: 'POST',
    path: '/canary/reset',
    body: JSON.stringify({
      runId: RUN_ID,
      mode: 'lifecycle',
      vector: 'lifecycle',
    }),
  };
  assert.throws(
    () => routeCanaryControl(reset, recorder),
    /observed browser process count/i,
  );
  routeCanaryControl(reset, recorder, {
    browserProcessCount: 5,
    visibleWindowLeakObserved: false,
    unexpectedActivationObserved: false,
  });
  routeCanaryControl(
    {
      method: 'POST',
      path: '/canary/sleep-wake',
      body: JSON.stringify({
        runId: RUN_ID,
        mode: 'lifecycle',
        vector: 'lifecycle',
      }),
    },
    recorder,
  );
  const snapshot = routeCanaryControl(
    {
      method: 'GET',
      path: `/canary/snapshot?runId=${RUN_ID}&mode=lifecycle&vector=lifecycle`,
      body: '',
    },
    recorder,
    {
      browserProcessCount: 4,
      visibleWindowLeakObserved: false,
      unexpectedActivationObserved: false,
    },
  );
  assert.equal(snapshot.browserProcessBaseline, 5);
  assert.equal(snapshot.browserProcessCurrent, 4);
  assert.equal(snapshot.sleepWakeObserved, true);
  assert.equal(snapshot.visibleWindowLeakObserved, false);
  assert.equal(snapshot.unexpectedActivationObserved, false);
});

test('canary trust lifecycle always removes the per-run root through injected adapters', async () => {
  const events = [];
  const adapter = {
    async install(material) {
      events.push(`install:${material.runId}`);
      return { fingerprint: 'sha256:test-only' };
    },
    async remove(receipt) {
      events.push(`remove:${receipt.fingerprint}`);
    },
  };
  await assert.rejects(
    withCanaryTrust(
      { runId: RUN_ID, caPath: 'ignored/ca.pem' },
      adapter,
      async () => {
        events.push('browser-preflight');
        throw new Error('preflight failed');
      },
    ),
    /preflight failed/,
  );
  assert.deepEqual(events, [
    `install:${RUN_ID}`,
    'browser-preflight',
    'remove:sha256:test-only',
  ]);
});

test('controlled canary route matrix serves HTTPS, SSE, redirects, service worker, and preflight', () => {
  const recorder = new CanaryRecorder({ runId: RUN_ID });
  for (const coordinate of [
    { runId: RUN_ID, mode: 'protected', vector: 'redirect' },
    { runId: RUN_ID, mode: 'counterfactual', vector: 'service_worker' },
    { runId: RUN_ID, mode: 'protected', vector: 'sse' },
    { runId: RUN_ID, mode: 'preflight', vector: 'preflight' },
  ]) {
    recorder.reset(coordinate);
  }
  const query = `runId=${RUN_ID}&mode=protected&vector=redirect`;
  const one = buildCanaryRouteResponse({
    surface: 'allowed-https',
    path: `/redirect/one?${query}`,
    blockedHttpsOrigin: 'https://127.0.0.1:54321',
    recorder,
  });
  const two = buildCanaryRouteResponse({
    surface: 'allowed-https',
    path: `/redirect/two?${query}`,
    blockedHttpsOrigin: 'https://127.0.0.1:54321',
    recorder,
  });
  assert.equal(one.status, 302);
  assert.match(one.headers.location, /^\/redirect\/two\?/);
  assert.equal(two.status, 302);
  assert.match(
    two.headers.location,
    /^https:\/\/127\.0\.0\.1:54321\/blocked\/redirect\?/,
  );

  const sw = buildCanaryRouteResponse({
    surface: 'allowed-https',
    path: `/sw.js?runId=${RUN_ID}&mode=counterfactual&vector=service_worker`,
    blockedHttpsOrigin: 'https://127.0.0.1:54321',
    recorder,
  });
  assert.equal(sw.status, 200);
  assert.equal(sw.headers['content-type'], 'application/javascript');
  assert.match(sw.body, /addEventListener\("install"/);
  assert.match(
    sw.body,
    /https:\/\/127\.0\.0\.1:54321\/blocked\/service_worker/,
  );
  assert.match(sw.body, /waitUntil/);
  assert.match(sw.body, /fetch\([\s\S]*\.catch\(\(\) => undefined\)/);

  const sse = buildCanaryRouteResponse({
    surface: 'blocked-https',
    path: `/sse/sse?runId=${RUN_ID}&mode=protected&vector=sse`,
    blockedHttpsOrigin: 'https://127.0.0.1:54321',
    recorder,
  });
  assert.equal(sse.headers['content-type'], 'text/event-stream');
  assert.match(sse.body, /^data: yinmi-canary/m);

  const preflight = buildCanaryRouteResponse({
    surface: 'blocked-https',
    path: `/preflight?runId=${RUN_ID}&mode=preflight&vector=preflight`,
    blockedHttpsOrigin: 'https://127.0.0.1:54321',
    recorder,
  });
  assert.equal(preflight.status, 204);
  assert.equal(
    recorder.snapshot({
      runId: RUN_ID,
      mode: 'preflight',
      vector: 'preflight',
    }).browserPreflightHits,
    1,
  );
});

test('controlled harness owns certificate, trust, browser preflight, and cleanup ordering', async () => {
  const events = [];
  const result = await runControlledCanaryHarness(
    { runId: RUN_ID },
    {
      createCertificate: async () => {
        events.push('certificate-created');
        return {
          runId: RUN_ID,
          caPath: 'ignored/ca.pem',
          key: Buffer.from('key'),
          cert: Buffer.from('cert'),
          cleanup: async () => events.push('certificate-removed'),
        };
      },
      trustAdapter: {
        async install() {
          events.push('trust-installed');
          return { fingerprint: 'test' };
        },
        async remove() {
          events.push('trust-removed');
        },
      },
      startServers: async () => {
        events.push('servers-started');
        return {
          fatal: new Promise(() => {}),
          controlOrigin: 'http://127.0.0.1:49152',
          allowedOrigin: 'https://127.0.0.1:49155/',
          blockedHttpOrigin: 'http://127.0.0.1:49153',
          blockedHttpsOrigin: 'https://127.0.0.1:49154',
          blockedWsOrigin: 'ws://127.0.0.1:49153',
          blockedWssOrigin: 'wss://127.0.0.1:49154',
          close: async () => events.push('servers-closed'),
        };
      },
      browserPreflight: async (descriptor) => {
        events.push(`preflight:${descriptor.allowedOrigin}`);
        return {
          httpsReachable: true,
          wssReachable: true,
          certificateTrusted: true,
        };
      },
      startPlatformMonitor: async () => {
        events.push('platform-monitor-started');
        return {
          stop: async () => events.push('platform-monitor-stopped'),
        };
      },
      operation: async (descriptor) => {
        events.push('operation');
        return descriptor.controlOrigin;
      },
    },
  );
  assert.equal(result, 'http://127.0.0.1:49152');
  assert.deepEqual(events, [
    'certificate-created',
    'servers-started',
    'trust-installed',
    'platform-monitor-started',
    'preflight:https://127.0.0.1:49155/',
    'operation',
    'platform-monitor-stopped',
    'trust-removed',
    'servers-closed',
    'certificate-removed',
  ]);
});

test('production isolation recorder lease wires controlled TLS config and Rust-observed preflight', async () => {
  const events = [];
  const never = new Promise(() => {});
  let observedPlatformState;
  let issuedConfig;
  let acknowledgeProcessInfo;
  const recorder = isolationWindowsRecorder();
  const server = await startControlledIsolationRecorderServer(
    recorder,
    {
      runId: RUN_ID,
      platformId: 'windows-10-webview2-111-x64',
      certificateRootDirectory: 'ignored/canary-certs',
      controlledVm: true,
    },
    {
      createCertificate: async ({ runId, rootDirectory }) => {
        assert.equal(runId, RUN_ID);
        assert.equal(rootDirectory, 'ignored/canary-certs');
        events.push('certificate-created');
        return {
          runId,
          caPath: 'ignored/canary-certs/ca.pem',
          key: Buffer.from('key'),
          cert: Buffer.from('cert'),
          cleanup: async () => events.push('certificate-removed'),
        };
      },
      createTrustAdapter: () => ({
        async install() {
          events.push('trust-installed');
          return { installed: true };
        },
        async remove() {
          events.push('trust-removed');
        },
      }),
      startPlatformMonitor: async () => ({
        fatal: never,
        registerChildProcess(pid) {
          assert.equal(pid, 731);
          events.push('child-registered');
        },
        async processInfoAcknowledged() {
          events.push('process-info-baseline');
        },
        async observe() {
          observedPlatformState = {
            browserProcessCount: 4,
            visibleWindowLeakObserved: false,
            unexpectedActivationObserved: false,
          };
          return observedPlatformState;
        },
        async stop() {
          events.push('monitor-stopped');
        },
        async verifyBaseline() {
          events.push('baseline-verified');
        },
      }),
      startCanaryServers: async (
        { runId, material },
        { observePlatformState },
      ) => {
        assert.equal(runId, RUN_ID);
        assert.equal(material.cert.toString(), 'cert');
        assert.deepEqual(await observePlatformState(), observedPlatformState);
        events.push('canary-servers-started');
        return {
          fatal: never,
          recorder: {
            snapshot(coordinate) {
              assert.deepEqual(coordinate, {
                runId: RUN_ID,
                mode: 'preflight',
                vector: 'preflight',
              });
              return {
                browserPreflightHits: 1,
                websocketHandshakes: 1,
              };
            },
          },
          controlOrigin: 'http://127.0.0.1:50000',
          allowedOrigin: 'https://127.0.0.1:50001/',
          blockedHttpOrigin: 'http://127.0.0.1:50000',
          blockedHttpsOrigin: 'https://127.0.0.1:50002',
          blockedWsOrigin: 'ws://127.0.0.1:50000',
          blockedWssOrigin: 'wss://127.0.0.1:50002',
          close: async () => events.push('canary-servers-closed'),
        };
      },
      startTraceServer: async (value, options) => {
        assert.equal(value, recorder);
        issuedConfig = options.canaryConfig;
        acknowledgeProcessInfo = options.onProcessInfoAccepted;
        events.push('trace-server-started');
        return {
          endpoint: 'http://127.0.0.1:50003',
          fatal: never,
          close: async () => events.push('trace-server-closed'),
        };
      },
    },
  );

  assert.deepEqual(issuedConfig, canaryConfig());
  server.registerChildProcess({ pid: 731 });
  await acknowledgeProcessInfo();
  assert.deepEqual(await server.isolationObservation(), {
    httpsPreflightHits: 1,
    wssPreflightHandshakes: 1,
  });
  await assert.rejects(server.verifyBaseline(), /close/i);
  await server.close();
  await server.verifyBaseline();
  assert.deepEqual(events, [
    'certificate-created',
    'canary-servers-started',
    'trust-installed',
    'trace-server-started',
    'child-registered',
    'process-info-baseline',
    'trace-server-closed',
    'monitor-stopped',
    'trust-removed',
    'canary-servers-closed',
    'certificate-removed',
    'baseline-verified',
  ]);
});

test('production isolation recorder refuses trust setup without explicit runner opt-in', async () => {
  let certificateCreated = false;
  await assert.rejects(
    startControlledIsolationRecorderServer(
      isolationWindowsRecorder(),
      {
        runId: RUN_ID,
        platformId: 'windows-10-webview2-111-x64',
        certificateRootDirectory: 'ignored/canary-certs',
      },
      {
        createCertificate: async () => {
          certificateCreated = true;
        },
      },
    ),
    new RegExp(CONTROLLED_VM_ENV),
  );
  assert.equal(certificateCreated, false);
});

test('controlled platform monitor derives lifecycle observations and verifies the final baseline', async () => {
  const samples = [
    {
      browserProcessCount: 2,
      visibleWindowCount: 0,
      childForeground: false,
    },
    {
      browserProcessCount: 4,
      visibleWindowCount: 1,
      childForeground: true,
    },
    {
      browserProcessCount: 4,
      visibleWindowCount: 1,
      childForeground: true,
    },
    {
      browserProcessCount: 2,
      visibleWindowCount: 0,
      childForeground: false,
    },
  ];
  const coordinates = [];
  const monitor = await startControlledPlatformMonitor(
    { platform: 'win32' },
    {
      sample: async (coordinate) => {
        coordinates.push(coordinate);
        return samples.shift();
      },
    },
  );
  monitor.registerChildProcess(731);
  await monitor.processInfoAcknowledged();
  assert.deepEqual(await monitor.observe(), {
    browserProcessCount: 4,
    visibleWindowLeakObserved: false,
    unexpectedActivationObserved: false,
  });
  await monitor.stop();
  await monitor.verifyBaseline();
  assert.deepEqual(coordinates, [
    { platform: 'win32', childPid: null },
    { platform: 'win32', childPid: 731 },
    { platform: 'win32', childPid: 731 },
    { platform: 'win32', childPid: 731 },
  ]);
  assert.equal(samples.length, 0);
});

test('controlled harness fails closed when a sealed canary reports a late server hit', async () => {
  let rejectFatal;
  const fatal = new Promise((_, reject) => {
    rejectFatal = reject;
  });
  fatal.catch(() => {});
  const cleanup = [];
  await assert.rejects(
    runControlledCanaryHarness(
      { runId: RUN_ID },
      {
        createCertificate: async () => ({
          runId: RUN_ID,
          caPath: 'ignored/ca.pem',
          cleanup: async () => cleanup.push('material'),
        }),
        trustAdapter: {
          install: async () => ({ installed: true }),
          remove: async () => cleanup.push('trust'),
        },
        startServers: async () => ({
          fatal,
          close: async () => cleanup.push('servers'),
        }),
        startPlatformMonitor: async () => ({
          stop: async () => cleanup.push('monitor'),
        }),
        browserPreflight: async () => ({
          httpsReachable: true,
          wssReachable: true,
          certificateTrusted: true,
        }),
        operation: async () => {
          rejectFatal(new Error('late protected hit after seal'));
          await new Promise((resolve) => setTimeout(resolve, 1));
          return 'false-success';
        },
      },
    ),
    /late protected hit after seal/,
  );
  assert.deepEqual(cleanup, ['monitor', 'trust', 'servers', 'material']);
});

test('every controlled canary listener uses an OS-assigned loopback port', async () => {
  let nextPort = 50_000;
  const servers = [];
  class FakeServer extends EventEmitter {
    constructor(handler, kind) {
      super();
      this.handler = handler;
      this.kind = kind;
      this.listening = false;
      this.boundPort = null;
      servers.push(this);
    }

    listen(port, host) {
      assert.equal(host, '127.0.0.1');
      this.requestedPort = port;
      this.boundPort = port === 0 ? nextPort++ : port;
      this.listening = true;
      queueMicrotask(() => this.emit('listening'));
    }

    address() {
      return { address: '127.0.0.1', family: 'IPv4', port: this.boundPort };
    }

    close(callback) {
      this.listening = false;
      callback?.();
    }

    closeAllConnections() {}
  }

  const descriptor = await startControlledCanaryServers(
    {
      runId: RUN_ID,
      material: { key: Buffer.from('key'), cert: Buffer.from('cert') },
    },
    {
      createHttpServer: (handler) => new FakeServer(handler, 'http'),
      createHttpsServer: (_options, handler) =>
        new FakeServer(handler, 'https'),
    },
  );
  assert.equal(descriptor.controlOrigin, 'http://127.0.0.1:50000');
  assert.equal(descriptor.allowedOrigin, 'https://127.0.0.1:50001/');
  assert.equal(descriptor.blockedHttpOrigin, 'http://127.0.0.1:50000');
  assert.equal(descriptor.blockedHttpsOrigin, 'https://127.0.0.1:50002');
  assert.equal(descriptor.blockedWsOrigin, 'ws://127.0.0.1:50000');
  assert.equal(descriptor.blockedWssOrigin, 'wss://127.0.0.1:50002');
  assert.deepEqual(
    servers.map(({ kind, boundPort }) => [kind, boundPort]),
    [
      ['http', 50_000],
      ['https', 50_001],
      ['https', 50_002],
    ],
  );
  assert.ok(servers.every(({ requestedPort }) => requestedPort === 0));
  assert.equal(servers[0].listenerCount('upgrade'), 1);
  assert.equal(servers[2].listenerCount('upgrade'), 1);
  const rejectedSocket = {
    destroyed: false,
    destroy() {
      this.destroyed = true;
    },
  };
  servers[0].emit(
    'upgrade',
    {
      headers: { upgrade: 'not-websocket' },
      url: `/ws/fetch?runId=${RUN_ID}&mode=protected&vector=fetch`,
    },
    rejectedSocket,
  );
  await assert.rejects(descriptor.fatal, /must request WebSocket/i);
  assert.equal(rejectedSocket.destroyed, true);
  await descriptor.close();
  assert.ok(servers.every((server) => !server.listening));
  const source = await readFile(
    resolve('scripts/run-signature-lifecycle-probe.mjs'),
    'utf8',
  );
  assert.doesNotMatch(
    source,
    /allowedPort\s*=\s*443|listenOnLoopback\(allowedServer,\s*443\)/,
  );
});

test('controlled-VM trust adapter is opt-in and has reversible Windows/macOS command plans', async () => {
  assert.throws(
    () => createControlledVmTrustAdapter({ controlledVm: false }),
    /controlled VM/i,
  );
  assert.deepEqual(
    controlledVmTrustCommands({
      platform: 'win32',
      caPath: 'C:\\ignored\\ca.pem',
      sha1Fingerprint: 'AA:BB',
      sha256Fingerprint: 'CC:DD',
    }),
    {
      install: {
        file: 'certutil.exe',
        args: ['-user', '-addstore', 'Root', 'C:\\ignored\\ca.pem'],
      },
      remove: {
        file: 'certutil.exe',
        args: ['-user', '-delstore', 'Root', 'AABB'],
      },
    },
  );
  const macPlan = controlledVmTrustCommands({
    platform: 'darwin',
    caPath: '/ignored/ca.pem',
    sha1Fingerprint: 'AA:BB',
    sha256Fingerprint: 'CC:DD',
    loginKeychain: '/Users/test/Library/Keychains/login.keychain-db',
  });
  assert.deepEqual(macPlan.install.args.slice(0, 4), [
    'add-trusted-cert',
    '-d',
    '-r',
    'trustRoot',
  ]);
  assert.deepEqual(macPlan.remove.args, [
    'delete-certificate',
    '-Z',
    'AABB',
    '/Users/test/Library/Keychains/login.keychain-db',
  ]);

  const executed = [];
  const adapter = createControlledVmTrustAdapter({
    controlledVm: true,
    platform: 'win32',
    execFile: async (file, args) => executed.push([file, args]),
  });
  const receipt = await adapter.install({
    caPath: 'C:\\ignored\\ca.pem',
    sha1Fingerprint: 'AA:BB',
    sha256Fingerprint: 'CC:DD',
  });
  await adapter.remove(receipt);
  assert.deepEqual(
    executed.map(([file, args]) => [file, args[1]]),
    [
      ['certutil.exe', '-addstore'],
      ['certutil.exe', '-delstore'],
    ],
  );
});

test('per-run certificate material is generated below ignored artifacts with required SANs', async () => {
  const writes = [];
  const commands = [];
  const removals = [];
  const material = await createPerRunCanaryCertificate(
    {
      runId: RUN_ID,
      rootDirectory: resolve('artifacts/feasibility/signature/.canary'),
    },
    {
      mkdir: async () => {},
      writeFile: async (path, body) => writes.push([path, String(body)]),
      execFile: async (file, args) => {
        commands.push([file, args]);
        if (args.includes('-sha1')) return 'sha1 Fingerprint=AA:BB\n';
        if (args.includes('-sha256')) return 'sha256 Fingerprint=CC:DD\n';
        return '';
      },
      readFile: async (path) =>
        Buffer.from(
          path.endsWith('server.key') ? 'private-key' : 'certificate',
        ),
      rm: async (path, options) => removals.push([path, options]),
    },
  );
  assert.equal(material.runId, RUN_ID);
  assert.equal(material.sha1Fingerprint, 'AA:BB');
  assert.equal(material.sha256Fingerprint, 'CC:DD');
  assert.equal(material.key.toString(), 'private-key');
  assert.equal(material.cert.toString(), 'certificate');
  assert.match(
    material.caPath,
    /\.canary[\\/]0123456789abcdef0123456789abcdef[\\/]ca\.pem$/,
  );
  const opensslConfig = writes.find(([path]) =>
    path.endsWith('openssl.cnf'),
  )[1];
  assert.match(opensslConfig, /IP\.1\s*=\s*127\.0\.0\.1/);
  assert.doesNotMatch(
    opensslConfig,
    /music\.gdstudio\.xyz|blocked\.yinmi\.invalid/,
  );
  assert.ok(commands.every(([file]) => file === 'openssl'));
  assert.ok(commands.some(([, args]) => args.includes('-extensions')));
  await material.cleanup();
  assert.equal(removals.length, 1);
  assert.deepEqual(removals[0][1], { recursive: true, force: true });
});

test('nonce-bound canary config is available once only after process-info acknowledgement', () => {
  const recorder = windowsRecorder();
  recorder.beginPhase(WRITE_PHASE);
  const request = {
    method: 'POST',
    path: CANARY_CONFIG_PATH,
    body: JSON.stringify({ runId: RUN_ID, phase: WRITE_PHASE }),
  };
  const config = {
    runId: RUN_ID,
    phase: WRITE_PHASE,
    platformId: 'windows-10-webview2-111-x64',
    controlOrigin: 'http://127.0.0.1:50000/',
    allowedOrigin: 'https://127.0.0.1:50001/',
    blockedHttpOrigin: 'http://127.0.0.1:50000/',
    blockedHttpsOrigin: 'https://127.0.0.1:50002/',
    blockedWsOrigin: 'ws://127.0.0.1:50000/',
    blockedWssOrigin: 'wss://127.0.0.1:50002/',
    idleDurationMs: 600_000,
  };
  assert.deepEqual(Object.keys(config), CANARY_CONFIG_KEYS);
  assert.throws(
    () => routeSubmission(request, recorder, config),
    /process-info/i,
  );
  recorder.acceptProcessInfo(processInfo(WRITE_PHASE));
  assert.deepEqual(routeSubmission(request, recorder, config), config);
  assert.throws(
    () => routeSubmission(request, recorder, config),
    /once|replay/i,
  );

  const wrongNonce = windowsRecorder();
  wrongNonce.beginPhase(WRITE_PHASE);
  wrongNonce.acceptProcessInfo(processInfo(WRITE_PHASE));
  assert.throws(
    () =>
      routeSubmission(
        {
          ...request,
          body: JSON.stringify({ runId: STALE_RUN_ID, phase: WRITE_PHASE }),
        },
        wrongNonce,
        config,
      ),
    /run id/i,
  );
  assert.equal(JSON.stringify(config).includes('.pem'), false);
  assert.equal(JSON.stringify(config).includes('private'), false);
});
