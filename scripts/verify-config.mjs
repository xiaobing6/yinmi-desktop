import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';

const packageJson = JSON.parse(await readFile('package.json', 'utf8'));
const tauri = JSON.parse(await readFile('src-tauri/tauri.conf.json', 'utf8'));
const feasibilityTauri = JSON.parse(
  await readFile('src-tauri/tauri.feasibility.conf.json', 'utf8'),
);
const feasibilityCapability = JSON.parse(
  await readFile('src-tauri/capabilities/feasibility-main.json', 'utf8'),
);
const feasibilityPermission = await readFile(
  'src-tauri/permissions/feasibility.toml',
  'utf8',
);
const buildScript = await readFile('src-tauri/build.rs', 'utf8');
const nodeVersion = (await readFile('.node-version', 'utf8')).trim();

const feasibilityCommands = [
  'feasibility_signature_initialize',
  'feasibility_signature_sign',
  'feasibility_signature_destroy',
  'feasibility_signature_isolation',
  'feasibility_run_gd_probe',
  'feasibility_ipc_canary',
];

assert.equal(packageJson.name, 'yinmi');
assert.equal(packageJson.version, '0.1.0');
assert.equal(packageJson.packageManager, 'pnpm@11.7.0');
assert.equal(nodeVersion, '24');
assert.equal(tauri.productName, '音觅');
assert.equal(tauri.version, packageJson.version);
assert.equal(tauri.identifier, 'io.github.xiaobing6.yinmi');
assert.deepEqual(tauri.build.frontendDist, '../dist');
assert.equal(tauri.app.windows[0].label, 'main');
assert.equal(tauri.app.windows[0].minWidth, 800);
assert.equal(tauri.app.windows[0].minHeight, 480);
assert.equal(tauri.bundle.macOS.minimumSystemVersion, '13.3');
assert.equal(tauri.bundle.windows.webviewInstallMode.type, 'embedBootstrapper');
assert.deepEqual(tauri.app.security.capabilities, ['main-capability']);
for (const forbidden of [
  'feasibility',
  'gd-signature-host-feasibility',
  'gd-signature-raw-wry',
  'YINMI_FEASIBILITY_',
]) {
  assert.equal(JSON.stringify(tauri).includes(forbidden), false);
}

assert.equal(
  feasibilityTauri.build.beforeDevCommand,
  'pnpm dev --mode feasibility',
);
assert.equal(
  feasibilityTauri.build.beforeBuildCommand,
  'pnpm build --mode feasibility',
);
assert.deepEqual(feasibilityTauri.app.security.capabilities, [
  'feasibility-main',
]);
assert.deepEqual(feasibilityCapability.windows, ['main']);
assert.equal(feasibilityCapability.local, true);
assert.deepEqual(feasibilityCapability.permissions, [
  'core:default',
  'feasibility',
]);
for (const forbiddenKey of ['remote', 'urls']) {
  assert.equal(forbiddenKey in feasibilityCapability, false);
}
for (const forbiddenValue of [
  '*',
  'gd-signature-host-feasibility',
  'gd-signature-raw-wry',
]) {
  assert.equal(
    JSON.stringify(feasibilityCapability).includes(forbiddenValue),
    false,
  );
  assert.equal(
    JSON.stringify(feasibilityTauri).includes(forbiddenValue),
    false,
  );
}
for (const command of feasibilityCommands) {
  assert.match(feasibilityPermission, new RegExp(`"${command}"`));
  assert.match(buildScript, new RegExp(`"${command}"`));
}
assert.equal(
  [...feasibilityPermission.matchAll(/"(feasibility_[a-z_]+)"/g)].length,
  feasibilityCommands.length,
);
assert.match(buildScript, /cfg\(feature = "feasibility"\)/);
assert.match(
  buildScript,
  /cfg\(not\(feature = "feasibility"\)\)[\s\S]*permissions\/default\/\*\*\/\*/,
);
assert.deepEqual(
  [
    ...buildScript.matchAll(
      /capabilities_path_pattern\("([^"]+)"\)/g,
    ),
  ].map((match) => match[1]),
  ['capabilities/*main.json', 'capabilities/main.json'],
);
console.log('configuration contract: PASS');
