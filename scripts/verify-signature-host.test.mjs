import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdtemp, mkdir, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import test from 'node:test';
import { fileURLToPath } from 'node:url';

import {
  SIGNATURE_HOST_SOURCE_PATHS,
  inspectSignatureHostSources,
  verifySignatureHost,
} from './verify-signature-host.mjs';

const EXPECTED_SOURCE_PATHS = [
  'src-tauri/src/feasibility/signature_host.rs',
  'src-tauri/src/feasibility/signature_probe.rs',
  'src-tauri/src/feasibility/signature_webview.rs',
  'src-tauri/src/feasibility/webview_resource_policy.rs',
  'src-tauri/src/feasibility/webview_resource_policy/macos.rs',
  'src-tauri/src/feasibility/webview_resource_policy/windows.rs',
];
const VERIFIER_PATH = fileURLToPath(
  new URL('./verify-signature-host.mjs', import.meta.url),
);

function validSources() {
  return new Map([
    [
      'src-tauri/src/feasibility/signature_host.rs',
      `
        use tauri::window::WindowBuilder;
        use wry::WebViewBuilder;
        let builder = WebViewBuilder::new().with_id(SIGNATURE_WEBVIEW_ID);
        builder.with_on_page_load_handler(page_loaded).build_as_child(&host)?;
        app.run_on_main_thread(move || create_raw_child())?;
        let label = SIGNATURE_HOST_WINDOW_LABEL;
      `,
    ],
    [
      'src-tauri/src/feasibility/signature_probe.rs',
      'fn probe() { with_on_page_load_handler(record_finished); }',
    ],
    [
      'src-tauri/src/feasibility/signature_webview.rs',
      `
        enum Slot { Pending, Ready, Destroying }
        fn event(event: WindowEvent) {
          if matches!(event, WindowEvent::Destroyed) { maybe_complete_teardown(); }
        }
        static LATE_POLICY_TOMBSTONES: OnceLock<()> = OnceLock::new();
      `,
    ],
    [
      'src-tauri/src/feasibility/webview_resource_policy.rs',
      'pub enum ResourcePolicy { Platform }',
    ],
    [
      'src-tauri/src/feasibility/webview_resource_policy/macos.rs',
      `
        let store = WKWebsiteDataStore::nonPersistentDataStore();
        let builder = builder.with_webview_configuration(configuration);
        store.removeContentRuleListForIdentifier_completionHandler(id, completion);
        app.run_on_main_thread(move || complete_store_removal())?;
      `,
    ],
    [
      'src-tauri/src/feasibility/webview_resource_policy/windows.rs',
      `
        webview.AddWebResourceRequestedFilterWithRequestSourceKinds(filter)?;
        webview.RemoveWebResourceRequestedFilter(filter)?;
      `,
    ],
  ]);
}

async function writeSourceSet(cwd, sources = validSources()) {
  for (const [repositoryPath, source] of sources) {
    const target = join(cwd, ...repositoryPath.split('/'));
    await mkdir(dirname(target), { recursive: true });
    await writeFile(target, source);
  }
}

test('source verifier exposes the exact Task 4 signature source set', () => {
  assert.deepEqual(SIGNATURE_HOST_SOURCE_PATHS, EXPECTED_SOURCE_PATHS);
  assert.deepEqual(inspectSignatureHostSources(validSources()), []);
});

test('source verifier rejects every forbidden managed/IPC/unsafe construct', async (t) => {
  const forbidden = [
    'tauri::WebviewWindowBuilder::new()',
    'let managed: tauri::WebviewWindow = value;',
    'use tauri::{WebviewWindowBuilder as ManagedBuilder};',
    'use tauri::{WebviewWindow as ManagedWindow};',
    'managed . with_webview (move |_| {});',
    'builder . with_ipc_handler (handler);',
    'builder . with_initialization_script (script);',
    'builder . with_initialization_script_for_main_only (script);',
    'builder . with_custom_protocol (scheme, handler);',
    'builder . with_asynchronous_custom_protocol (scheme, handler);',
    'Managed::with_webview(managed, move |_| {});',
    'wry::WebViewBuilder::with_ipc_handler(builder, handler);',
    'wry::WebViewBuilder::with_initialization_script(builder, script);',
    'wry::WebViewBuilder::with_initialization_script_for_main_only(builder, script);',
    'wry::WebViewBuilder::with_custom_protocol(builder, scheme, handler);',
    'wry::WebViewBuilder::with_asynchronous_custom_protocol(builder, scheme, handler);',
    'builder.with_webview::<_>(move |_| {});',
    'builder.with_ipc_handler::<_>(handler);',
    'builder.with_initialization_script::<_>(script);',
    'builder.with_initialization_script_for_main_only::<_>(script);',
    'builder.with_custom_protocol::<_>(scheme, handler);',
    'builder.with_asynchronous_custom_protocol::<_>(scheme, handler);',
    'builder.r#with_webview(move |_| {});',
    'builder.r#with_ipc_handler(handler);',
    'builder.r#with_initialization_script(script);',
    'builder.r#with_initialization_script_for_main_only(script);',
    'builder.r#with_custom_protocol(scheme, handler);',
    'builder.r#with_asynchronous_custom_protocol(scheme, handler);',
    'unsafe impl   Send for Guard {}',
    'unsafe impl\nSync for Guard {}',
    'unsafe impl<T> Send for Guard<T> {}',
    'unsafe impl<T: Foo<{1}>> Send for Guard<T> {}',
    'unsafe impl core::marker::Sync for Guard {}',
    'unsafe impl r#Send for Guard {}',
    'unsafe impl core::marker::r#Sync for Guard {}',
  ];
  for (const snippet of forbidden) {
    await t.test(snippet.split(/[ (]/, 1)[0], () => {
      const sources = validSources();
      const path = 'src-tauri/src/feasibility/signature_webview.rs';
      sources.set(path, `${sources.get(path)}\n${snippet}\n`);
      const diagnostics = inspectSignatureHostSources(sources);
      assert.equal(
        diagnostics.some(
          (diagnostic) =>
            diagnostic.includes(path) && diagnostic.includes('forbidden'),
        ),
        true,
        diagnostics.join('\n'),
      );
    });
  }
});

test('source verifier ignores forbidden-looking comments and strings', () => {
  const sources = validSources();
  const path = 'src-tauri/src/feasibility/signature_webview.rs';
  sources.set(
    path,
    `${sources.get(path)}
      // tauri::WebviewWindowBuilder::new()
      /* builder.with_ipc_handler(handler); unsafe impl Send for Guard {} */
      const NOTE: &str = "tauri::{WebviewWindow as Managed}";
      const RAW_NOTE: &str = r#"builder.with_custom_protocol(handler)"#;
    `,
  );
  assert.deepEqual(inspectSignatureHostSources(sources), []);
});

test('non-BMP literals preserve source offsets for later code', () => {
  const hostPath = 'src-tauri/src/feasibility/signature_host.rs';
  const harmless = validSources();
  harmless.set(
    hostPath,
    `${harmless.get(hostPath).replace('SIGNATURE_HOST_WINDOW_LABEL', 'HOST_LABEL_REMOVED')}
      const NOTE: &str = "😀😀😀😀😀😀😀😀😀😀";SIGNATURE_HOST_WINDOW_LABEL;
    `,
  );
  assert.deepEqual(inspectSignatureHostSources(harmless), []);

  const forbidden = validSources();
  const path = 'src-tauri/src/feasibility/signature_webview.rs';
  forbidden.set(
    path,
    `${forbidden.get(path)}
      const NOTE: &str = "😀😀😀😀😀😀😀😀😀😀";builder.with_ipc_handler(handler);
    `,
  );
  assert.equal(
    inspectSignatureHostSources(forbidden).some(
      (diagnostic) =>
        diagnostic.includes(path) && diagnostic.includes('with_ipc_handler'),
    ),
    true,
  );
});

test('source verifier rejects missing, extra, and substituted source paths', async (t) => {
  for (const repositoryPath of EXPECTED_SOURCE_PATHS) {
    await t.test(`missing ${repositoryPath}`, () => {
      const sources = validSources();
      sources.delete(repositoryPath);
      assert.equal(
        inspectSignatureHostSources(sources).some((diagnostic) =>
          diagnostic.includes(`missing source file: ${repositoryPath}`),
        ),
        true,
      );
    });
  }
  await t.test('extra path', () => {
    const sources = validSources();
    sources.set('src-tauri/src/feasibility/unexpected.rs', 'raw host');
    assert.equal(
      inspectSignatureHostSources(sources).some((diagnostic) =>
        diagnostic.includes('unexpected source file'),
      ),
      true,
    );
  });
  await t.test('substituted path', () => {
    const sources = validSources();
    sources.delete('src-tauri/src/feasibility/signature_probe.rs');
    sources.set('src-tauri/src/feasibility/signature_eval.rs', 'probe');
    const diagnostics = inspectSignatureHostSources(sources);
    assert.equal(
      diagnostics.some((entry) => entry.includes('missing source')),
      true,
    );
    assert.equal(
      diagnostics.some((entry) => entry.includes('unexpected source')),
      true,
    );
  });
});

test('source verifier requires every raw-host/lifecycle/policy marker', async (t) => {
  const required = [
    [
      'src-tauri/src/feasibility/signature_host.rs',
      'tauri::window::WindowBuilder',
    ],
    ['src-tauri/src/feasibility/signature_host.rs', 'wry::WebViewBuilder'],
    [
      'src-tauri/src/feasibility/signature_host.rs',
      'with_id(SIGNATURE_WEBVIEW_ID)',
    ],
    ['src-tauri/src/feasibility/signature_host.rs', 'build_as_child'],
    ['src-tauri/src/feasibility/signature_host.rs', 'run_on_main_thread'],
    [
      'src-tauri/src/feasibility/signature_host.rs',
      'SIGNATURE_HOST_WINDOW_LABEL',
    ],
    ['src-tauri/src/feasibility/signature_webview.rs', 'Pending'],
    ['src-tauri/src/feasibility/signature_webview.rs', 'Destroying'],
    [
      'src-tauri/src/feasibility/signature_webview.rs',
      'WindowEvent::Destroyed',
    ],
    [
      'src-tauri/src/feasibility/signature_webview.rs',
      'maybe_complete_teardown',
    ],
    [
      'src-tauri/src/feasibility/signature_webview.rs',
      'LATE_POLICY_TOMBSTONES',
    ],
    [
      'src-tauri/src/feasibility/webview_resource_policy/windows.rs',
      'AddWebResourceRequestedFilterWithRequestSourceKinds',
    ],
    [
      'src-tauri/src/feasibility/webview_resource_policy/windows.rs',
      'RemoveWebResourceRequestedFilter',
    ],
    [
      'src-tauri/src/feasibility/webview_resource_policy/macos.rs',
      'WKWebsiteDataStore::nonPersistentDataStore',
    ],
    [
      'src-tauri/src/feasibility/webview_resource_policy/macos.rs',
      'with_webview_configuration',
    ],
    [
      'src-tauri/src/feasibility/webview_resource_policy/macos.rs',
      'removeContentRuleListForIdentifier_completionHandler',
    ],
    [
      'src-tauri/src/feasibility/webview_resource_policy/macos.rs',
      'run_on_main_thread',
    ],
  ];
  for (const [repositoryPath, marker] of required) {
    await t.test(marker, () => {
      const sources = validSources();
      sources.set(
        repositoryPath,
        sources.get(repositoryPath).replace(marker, ''),
      );
      const diagnostics = inspectSignatureHostSources(sources);
      assert.equal(
        diagnostics.some(
          (diagnostic) =>
            diagnostic.includes(repositoryPath) && diagnostic.includes(marker),
        ),
        true,
        diagnostics.join('\n'),
      );
    });
  }
});

test('with_on_page_load_handler is required across the fixed source set', () => {
  const hostPath = 'src-tauri/src/feasibility/signature_host.rs';
  const probePath = 'src-tauri/src/feasibility/signature_probe.rs';
  const relocated = validSources();
  relocated.set(
    probePath,
    relocated.get(probePath).replace('with_on_page_load_handler', 'probe'),
  );
  assert.deepEqual(inspectSignatureHostSources(relocated), []);

  const missing = validSources();
  for (const path of [hostPath, probePath]) {
    missing.set(
      path,
      missing.get(path).replace('with_on_page_load_handler', 'page_handler'),
    );
  }
  assert.equal(
    inspectSignatureHostSources(missing).some((diagnostic) =>
      diagnostic.includes('with_on_page_load_handler'),
    ),
    true,
  );
});

test('comments and strings cannot satisfy required markers', async (t) => {
  const required = [
    [
      'src-tauri/src/feasibility/signature_host.rs',
      'tauri::window::WindowBuilder',
    ],
    ['src-tauri/src/feasibility/signature_host.rs', 'wry::WebViewBuilder'],
    [
      'src-tauri/src/feasibility/signature_host.rs',
      'with_id(SIGNATURE_WEBVIEW_ID)',
    ],
    ['src-tauri/src/feasibility/signature_host.rs', 'build_as_child'],
    ['src-tauri/src/feasibility/signature_host.rs', 'run_on_main_thread'],
    [
      'src-tauri/src/feasibility/signature_host.rs',
      'SIGNATURE_HOST_WINDOW_LABEL',
    ],
    ['src-tauri/src/feasibility/signature_webview.rs', 'Pending'],
    ['src-tauri/src/feasibility/signature_webview.rs', 'Destroying'],
    [
      'src-tauri/src/feasibility/signature_webview.rs',
      'WindowEvent::Destroyed',
    ],
    [
      'src-tauri/src/feasibility/signature_webview.rs',
      'maybe_complete_teardown',
    ],
    [
      'src-tauri/src/feasibility/signature_webview.rs',
      'LATE_POLICY_TOMBSTONES',
    ],
    [
      'src-tauri/src/feasibility/webview_resource_policy/windows.rs',
      'AddWebResourceRequestedFilterWithRequestSourceKinds',
    ],
    [
      'src-tauri/src/feasibility/webview_resource_policy/windows.rs',
      'RemoveWebResourceRequestedFilter',
    ],
    [
      'src-tauri/src/feasibility/webview_resource_policy/macos.rs',
      'WKWebsiteDataStore::nonPersistentDataStore',
    ],
    [
      'src-tauri/src/feasibility/webview_resource_policy/macos.rs',
      'with_webview_configuration',
    ],
    [
      'src-tauri/src/feasibility/webview_resource_policy/macos.rs',
      'removeContentRuleListForIdentifier_completionHandler',
    ],
    [
      'src-tauri/src/feasibility/webview_resource_policy/macos.rs',
      'run_on_main_thread',
    ],
  ];
  for (const [repositoryPath, marker] of required) {
    await t.test(marker, () => {
      const sources = validSources();
      sources.set(
        repositoryPath,
        `${sources.get(repositoryPath).replace(marker, '')}
          // ${marker}
          const PLACEHOLDER: &str = ${JSON.stringify(marker)};
        `,
      );
      const diagnostics = inspectSignatureHostSources(sources);
      assert.equal(
        diagnostics.some(
          (diagnostic) =>
            diagnostic.includes(repositoryPath) && diagnostic.includes(marker),
        ),
        true,
        diagnostics.join('\n'),
      );
    });
  }
});

test('comments and strings cannot satisfy the source-set page-load marker', () => {
  const hostPath = 'src-tauri/src/feasibility/signature_host.rs';
  const probePath = 'src-tauri/src/feasibility/signature_probe.rs';
  const sources = validSources();
  for (const path of [hostPath, probePath]) {
    sources.set(
      path,
      sources.get(path).replace('with_on_page_load_handler', 'page_handler'),
    );
  }
  sources.set(
    probePath,
    `${sources.get(probePath)}
      // with_on_page_load_handler
      const PLACEHOLDER: &str = "with_on_page_load_handler";
    `,
  );
  assert.equal(
    inspectSignatureHostSources(sources).some((diagnostic) =>
      diagnostic.includes('with_on_page_load_handler'),
    ),
    true,
  );
});

test('repository verification reads only the fixed signature source set', async () => {
  const cwd = await mkdtemp(join(tmpdir(), 'yinmi-signature-source-'));
  await writeSourceSet(cwd);
  const unrelated = join(cwd, 'src-tauri', 'src', 'lib.rs');
  await mkdir(dirname(unrelated), { recursive: true });
  await writeFile(unrelated, 'tauri::WebviewWindowBuilder::new();\n');
  assert.deepEqual(await verifySignatureHost({ cwd }), {
    filesChecked: EXPECTED_SOURCE_PATHS.length,
    sourcePaths: EXPECTED_SOURCE_PATHS,
  });
});

test('repository verification aggregates forbidden and missing-file diagnostics', async () => {
  const cwd = await mkdtemp(join(tmpdir(), 'yinmi-signature-source-red-'));
  const sources = validSources();
  sources.delete('src-tauri/src/feasibility/signature_host.rs');
  const policyPath =
    'src-tauri/src/feasibility/webview_resource_policy/windows.rs';
  sources.set(
    policyPath,
    `${sources.get(policyPath)}\nlet _: tauri::WebviewWindow = managed;`,
  );
  await writeSourceSet(cwd, sources);
  await assert.rejects(verifySignatureHost({ cwd }), (error) => {
    assert.match(error.message, /forbidden.*tauri::WebviewWindow/i);
    assert.match(error.message, /missing source file:.*signature_host\.rs/i);
    assert.ok(
      error.message.indexOf('forbidden') <
        error.message.indexOf('missing source'),
    );
    return true;
  });
});

test('CLI exits zero for valid sources and one for a forbidden source', async () => {
  const cwd = await mkdtemp(join(tmpdir(), 'yinmi-signature-cli-'));
  const sources = validSources();
  await writeSourceSet(cwd, sources);
  const green = spawnSync(process.execPath, [VERIFIER_PATH], {
    cwd,
    encoding: 'utf8',
  });
  assert.equal(green.status, 0, green.stderr);
  assert.match(
    green.stdout,
    /Signature host source verification: PASS \(6 files\)/,
  );

  const hostPath = 'src-tauri/src/feasibility/signature_host.rs';
  sources.set(
    hostPath,
    `${sources.get(hostPath)}\nbuilder.with_ipc_handler(handler);`,
  );
  await writeSourceSet(cwd, sources);
  const red = spawnSync(process.execPath, [VERIFIER_PATH], {
    cwd,
    encoding: 'utf8',
  });
  assert.equal(red.status, 1);
  assert.match(red.stderr, /forbidden.*with_ipc_handler/i);
});
