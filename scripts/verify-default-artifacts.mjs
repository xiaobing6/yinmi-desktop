import assert from 'node:assert/strict';
import { readdir, readFile, stat } from 'node:fs/promises';
import { basename, join, resolve, sep } from 'node:path';

const SENTINELS = [
  'FeasibilityPanel',
  'GdProbe',
  'gd-signature-host-feasibility',
  'gd-signature-raw-wry',
  'feasibility_',
  'YINMI_FEASIBILITY_',
];

async function filesBelow(root) {
  const files = [];
  for (const entry of await readdir(root, { withFileTypes: true })) {
    const path = join(root, entry.name);
    if (entry.isDirectory()) files.push(...(await filesBelow(path)));
    else if (entry.isFile()) files.push(path);
  }
  return files;
}

function assertNoSentinels(buffer, label) {
  const ascii = buffer.toString('utf8');
  const utf16 = buffer.toString('utf16le');
  for (const sentinel of SENTINELS) {
    assert.equal(
      ascii.includes(sentinel) || utf16.includes(sentinel),
      false,
      `${label} contains default-forbidden sentinel ${sentinel}`,
    );
  }
}

const root = process.cwd();
const configuredTarget = process.env.CARGO_TARGET_DIR;
const targetRoot = resolve(root, configuredTarget ?? 'src-tauri/target');
const targetSegments = targetRoot.toLowerCase().split(sep);
assert.equal(
  targetSegments.includes('feasibility'),
  false,
  'default artifact verification refuses the feasibility target',
);

const frontendRoot = resolve(root, 'dist');
assert.equal((await stat(frontendRoot)).isDirectory(), true);
const frontendFiles = await filesBelow(frontendRoot);
assert.ok(frontendFiles.length > 0, 'default frontend artifact is empty');
for (const file of frontendFiles) {
  assertNoSentinels(await readFile(file), `frontend ${file}`);
}

const executableName = process.platform === 'win32' ? 'yinmi.exe' : 'yinmi';
const executablePath = join(targetRoot, 'debug', executableName);
assert.equal(
  (await stat(executablePath)).isFile(),
  true,
  `default Tauri executable is missing at ${executablePath}`,
);
assert.equal(basename(executablePath), executableName);
assertNoSentinels(await readFile(executablePath), `Tauri ${executablePath}`);

console.log('default artifact isolation: PASS');
