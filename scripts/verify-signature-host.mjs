import { readFile } from 'node:fs/promises';
import { resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

export const SIGNATURE_HOST_SOURCE_PATHS = [
  'src-tauri/src/feasibility/signature_host.rs',
  'src-tauri/src/feasibility/signature_probe.rs',
  'src-tauri/src/feasibility/signature_webview.rs',
  'src-tauri/src/feasibility/webview_resource_policy.rs',
  'src-tauri/src/feasibility/webview_resource_policy/macos.rs',
  'src-tauri/src/feasibility/webview_resource_policy/windows.rs',
];

const FORBIDDEN_PATTERNS = [
  {
    label: 'tauri::WebviewWindowBuilder',
    pattern: /\bWebviewWindowBuilder\b/,
  },
  {
    label: 'tauri::WebviewWindow',
    pattern: /\bWebviewWindow\b/,
  },
  {
    label: '.with_webview(',
    pattern: /\b(?:r#)?with_webview\b/,
  },
  {
    label: '.with_ipc_handler(',
    pattern: /\b(?:r#)?with_ipc_handler\b/,
  },
  {
    label: '.with_initialization_script(',
    pattern: /\b(?:r#)?with_initialization_script\b/,
  },
  {
    label: '.with_initialization_script_for_main_only(',
    pattern: /\b(?:r#)?with_initialization_script_for_main_only\b/,
  },
  {
    label: '.with_custom_protocol(',
    pattern: /\b(?:r#)?with_custom_protocol\b/,
  },
  {
    label: '.with_asynchronous_custom_protocol(',
    pattern: /\b(?:r#)?with_asynchronous_custom_protocol\b/,
  },
  {
    label: 'unsafe impl',
    pattern: /\bunsafe\s+impl\b/,
  },
];

function masked(output, source, start, end) {
  for (let index = start; index < end; index += 1) {
    output[index] = source[index] === '\n' ? '\n' : ' ';
  }
}

function rustCodeText(source) {
  const output = source.split('');
  let index = 0;
  while (index < source.length) {
    if (source.startsWith('//', index)) {
      let end = source.indexOf('\n', index + 2);
      if (end === -1) end = source.length;
      masked(output, source, index, end);
      index = end;
      continue;
    }
    if (source.startsWith('/*', index)) {
      let depth = 1;
      let end = index + 2;
      while (end < source.length && depth > 0) {
        if (source.startsWith('/*', end)) {
          depth += 1;
          end += 2;
        } else if (source.startsWith('*/', end)) {
          depth -= 1;
          end += 2;
        } else {
          end += 1;
        }
      }
      masked(output, source, index, end);
      index = end;
      continue;
    }

    const rawString = /^(?:br|r)(#*)"/.exec(source.slice(index));
    if (rawString) {
      const terminator = `"${rawString[1]}`;
      const contentStart = index + rawString[0].length;
      const closing = source.indexOf(terminator, contentStart);
      const end = closing === -1 ? source.length : closing + terminator.length;
      masked(output, source, index, end);
      index = end;
      continue;
    }

    const stringPrefixLength = source.startsWith('b"', index)
      ? 2
      : source[index] === '"'
        ? 1
        : 0;
    if (stringPrefixLength > 0) {
      let end = index + stringPrefixLength;
      while (end < source.length) {
        if (source[end] === '\\') {
          end += 2;
        } else if (source[end] === '"') {
          end += 1;
          break;
        } else {
          end += 1;
        }
      }
      masked(output, source, index, Math.min(end, source.length));
      index = end;
      continue;
    }

    const character = /^(?:b)?'(?:\\.|[^\\'\r\n])'/.exec(source.slice(index));
    if (character) {
      const end = index + character[0].length;
      masked(output, source, index, end);
      index = end;
      continue;
    }
    index += 1;
  }
  return output.join('');
}

const REQUIRED_MARKERS = [
  {
    path: 'src-tauri/src/feasibility/signature_host.rs',
    label: 'tauri::window::WindowBuilder',
    pattern: /\btauri\s*::\s*window\s*::\s*WindowBuilder\b/,
  },
  {
    path: 'src-tauri/src/feasibility/signature_host.rs',
    label: 'wry::WebViewBuilder',
    pattern: /\bwry\s*::\s*WebViewBuilder\b/,
  },
  {
    path: 'src-tauri/src/feasibility/signature_host.rs',
    label: 'with_id(SIGNATURE_WEBVIEW_ID)',
    pattern: /\bwith_id\s*\(\s*SIGNATURE_WEBVIEW_ID\s*\)/,
  },
  {
    path: 'src-tauri/src/feasibility/signature_host.rs',
    label: 'build_as_child',
    pattern: /\bbuild_as_child\b/,
  },
  {
    path: 'src-tauri/src/feasibility/signature_host.rs',
    label: 'run_on_main_thread',
    pattern: /\brun_on_main_thread\b/,
  },
  {
    path: 'src-tauri/src/feasibility/signature_host.rs',
    label: 'SIGNATURE_HOST_WINDOW_LABEL',
    pattern: /\bSIGNATURE_HOST_WINDOW_LABEL\b/,
  },
  {
    label: 'with_on_page_load_handler',
    pattern: /\bwith_on_page_load_handler\b/,
  },
  {
    path: 'src-tauri/src/feasibility/signature_webview.rs',
    label: 'Pending',
    pattern: /\bPending\b/,
  },
  {
    path: 'src-tauri/src/feasibility/signature_webview.rs',
    label: 'Destroying',
    pattern: /\bDestroying\b/,
  },
  {
    path: 'src-tauri/src/feasibility/signature_webview.rs',
    label: 'WindowEvent::Destroyed',
    pattern: /\bWindowEvent\s*::\s*Destroyed\b/,
  },
  {
    path: 'src-tauri/src/feasibility/signature_webview.rs',
    label: 'maybe_complete_teardown',
    pattern: /\bmaybe_complete_teardown\b/,
  },
  {
    path: 'src-tauri/src/feasibility/signature_webview.rs',
    label: 'LATE_POLICY_TOMBSTONES',
    pattern: /\bLATE_POLICY_TOMBSTONES\b/,
  },
  {
    path: 'src-tauri/src/feasibility/webview_resource_policy/windows.rs',
    label: 'AddWebResourceRequestedFilterWithRequestSourceKinds',
    pattern: /\bAddWebResourceRequestedFilterWithRequestSourceKinds\b/,
  },
  {
    path: 'src-tauri/src/feasibility/webview_resource_policy/windows.rs',
    label: 'RemoveWebResourceRequestedFilter',
    pattern: /\bRemoveWebResourceRequestedFilter\b/,
  },
  {
    path: 'src-tauri/src/feasibility/webview_resource_policy/macos.rs',
    label: 'WKWebsiteDataStore::nonPersistentDataStore',
    pattern: /\bWKWebsiteDataStore\s*::\s*nonPersistentDataStore\b/,
  },
  {
    path: 'src-tauri/src/feasibility/webview_resource_policy/macos.rs',
    label: 'with_webview_configuration',
    pattern: /\bwith_webview_configuration\b/,
  },
  {
    path: 'src-tauri/src/feasibility/webview_resource_policy/macos.rs',
    label: 'removeContentRuleListForIdentifier_completionHandler',
    pattern: /\bremoveContentRuleListForIdentifier_completionHandler\b/,
  },
  {
    path: 'src-tauri/src/feasibility/webview_resource_policy/macos.rs',
    label: 'run_on_main_thread',
    pattern: /\brun_on_main_thread\b/,
  },
];

function sourceMap(sources) {
  if (sources instanceof Map) return new Map(sources);
  if (sources && typeof sources === 'object' && !Array.isArray(sources)) {
    return new Map(Object.entries(sources));
  }
  throw new TypeError('signature sources must be a Map or plain object');
}

export function inspectSignatureHostSources(sources) {
  const sourceByPath = sourceMap(sources);
  const codeByPath = new Map(
    [...sourceByPath].map(([repositoryPath, source]) => [
      repositoryPath,
      typeof source === 'string' ? rustCodeText(source) : source,
    ]),
  );
  const diagnostics = [];

  for (const repositoryPath of SIGNATURE_HOST_SOURCE_PATHS) {
    const source = codeByPath.get(repositoryPath);
    if (typeof source !== 'string') continue;
    for (const { label, pattern } of FORBIDDEN_PATTERNS) {
      if (pattern.test(source)) {
        diagnostics.push(`forbidden construct ${label} in ${repositoryPath}`);
      }
    }
  }

  for (const repositoryPath of SIGNATURE_HOST_SOURCE_PATHS) {
    if (!sourceByPath.has(repositoryPath)) {
      diagnostics.push(`missing source file: ${repositoryPath}`);
    } else if (typeof sourceByPath.get(repositoryPath) !== 'string') {
      diagnostics.push(`source file must be UTF-8 text: ${repositoryPath}`);
    }
  }
  const expected = new Set(SIGNATURE_HOST_SOURCE_PATHS);
  for (const repositoryPath of sourceByPath.keys()) {
    if (!expected.has(repositoryPath)) {
      diagnostics.push(`unexpected source file: ${repositoryPath}`);
    }
  }

  for (const { path, label, pattern } of REQUIRED_MARKERS) {
    if (path) {
      const source = codeByPath.get(path);
      if (typeof source === 'string' && !pattern.test(source)) {
        diagnostics.push(`required marker ${label} missing from ${path}`);
      }
    } else if (
      !SIGNATURE_HOST_SOURCE_PATHS.some((repositoryPath) => {
        const source = codeByPath.get(repositoryPath);
        return typeof source === 'string' && pattern.test(source);
      })
    ) {
      diagnostics.push(
        `required marker ${label} missing from fixed signature source set`,
      );
    }
  }
  return diagnostics;
}

export async function verifySignatureHost({ cwd = process.cwd() } = {}) {
  const sources = new Map();
  for (const repositoryPath of SIGNATURE_HOST_SOURCE_PATHS) {
    try {
      sources.set(
        repositoryPath,
        await readFile(resolve(cwd, ...repositoryPath.split('/')), 'utf8'),
      );
    } catch (error) {
      if (error?.code !== 'ENOENT') throw error;
    }
  }
  const diagnostics = inspectSignatureHostSources(sources);
  if (diagnostics.length > 0) {
    throw new Error(
      `Signature host source verification: FAIL\n${diagnostics
        .map((diagnostic) => `- ${diagnostic}`)
        .join('\n')}`,
    );
  }
  return {
    filesChecked: SIGNATURE_HOST_SOURCE_PATHS.length,
    sourcePaths: [...SIGNATURE_HOST_SOURCE_PATHS],
  };
}

if (
  process.argv[1] &&
  resolve(process.argv[1]).toLowerCase() ===
    fileURLToPath(import.meta.url).toLowerCase()
) {
  try {
    const result = await verifySignatureHost();
    console.log(
      `Signature host source verification: PASS (${result.filesChecked} files)`,
    );
  } catch (error) {
    console.error(error?.message ?? error);
    process.exitCode = 1;
  }
}
