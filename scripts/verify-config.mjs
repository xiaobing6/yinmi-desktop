import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';

const packageJson = JSON.parse(await readFile('package.json', 'utf8'));
const tauri = JSON.parse(await readFile('src-tauri/tauri.conf.json', 'utf8'));
const nodeVersion = (await readFile('.node-version', 'utf8')).trim();

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
console.log('configuration contract: PASS');
