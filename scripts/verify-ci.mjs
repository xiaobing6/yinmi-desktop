import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { parse } from 'yaml';

const qualityText = await readFile('.github/workflows/quality.yml', 'utf8');
const platformText = await readFile(
  '.github/workflows/platform-smoke.yml',
  'utf8',
);
const quality = parse(qualityText);
const platform = parse(platformText);

for (const event of ['push', 'pull_request', 'merge_group']) {
  assert.ok(
    Object.hasOwn(quality.on, event),
    `quality workflow missing ${event}`,
  );
}
assert.equal(quality.name, 'quality');
assert.equal(quality.jobs.quality.name, 'quality');
assert.ok(
  quality.jobs.quality.steps.some((step) => step.run === 'pnpm quality'),
);

assert.deepEqual(platform.on.push.branches, ['master', 'phase1/**']);
assert.equal(platform.name, 'platform-smoke');
for (const event of ['pull_request', 'merge_group']) {
  assert.ok(
    Object.hasOwn(platform.on, event),
    `platform workflow missing ${event}`,
  );
}
assert.equal(platform.jobs['platform-windows'].name, 'platform-windows');
assert.equal(platform.jobs.platform_macos_intel.name, 'platform-macos-intel');
assert.equal(platform.jobs.platform_macos_arm.name, 'platform-macos-arm');
assert.equal(platform.jobs.platform_macos.name, 'platform-macos');
assert.deepEqual(platform.jobs.platform_macos.needs, [
  'platform_macos_intel',
  'platform_macos_arm',
]);
assert.equal(platform.jobs.platform_macos.if, 'always()');

for (const job of [
  quality.jobs.quality,
  platform.jobs['platform-windows'],
  platform.jobs.platform_macos_intel,
  platform.jobs.platform_macos_arm,
]) {
  const checkout = job.steps.find(
    (step) => step.uses === 'actions/checkout@v6',
  );
  assert.equal(
    checkout?.with?.['fetch-depth'],
    0,
    `${job.name} must fetch full history`,
  );
}

for (const token of [
  'macos-15-intel',
  'macos-15',
  'x86_64-pc-windows-msvc',
  'x86_64-apple-darwin',
  'aarch64-apple-darwin',
]) {
  assert.ok(platformText.includes(token), `platform workflow missing ${token}`);
}
assert.ok(
  !platformText.includes('upload-artifact'),
  'smoke workflow must not upload release artifacts',
);
console.log('CI contract: PASS');
