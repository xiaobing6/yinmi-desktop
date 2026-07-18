import { readFileSync, writeFileSync } from 'node:fs';

const EXPECTED_REPOSITORY = 'xiaobing6/yinmi-desktop';
const PRODUCTION_ENDPOINT =
  'https://github.com/xiaobing6/yinmi-desktop/releases/latest/download/latest.json';

function fail(message) {
  throw new Error(`[release-guard] ${message}`);
}

function readJson(path) {
  try {
    return JSON.parse(readFileSync(path, 'utf8'));
  } catch (error) {
    fail(`Cannot read valid JSON from ${path}: ${error.message}`);
  }
}

function parseArguments(argv) {
  const [mode, ...rest] = argv;
  if (!mode) {
    fail('Expected a mode: config or manifest.');
  }

  const options = {};
  for (let index = 0; index < rest.length; index += 2) {
    const flag = rest[index];
    const value = rest[index + 1];
    if (!flag?.startsWith('--') || value === undefined) {
      fail(`Invalid argument near ${flag ?? '<end>'}.`);
    }
    options[flag.slice(2)] = value;
  }

  return { mode, options };
}

function requireOption(options, name) {
  const value = options[name];
  if (!value) {
    fail(`Missing --${name}.`);
  }
  return value;
}

function validateRepository(repository) {
  if (repository !== EXPECTED_REPOSITORY) {
    fail(
      `Production releases are restricted to ${EXPECTED_REPOSITORY}; got ${repository}.`,
    );
  }
}

function versionFromTag(tag) {
  if (!/^v\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?$/.test(tag)) {
    fail(`Release tag must be a SemVer tag beginning with v; got ${tag}.`);
  }
  return tag.slice(1);
}

function validateConfig(options) {
  const tag = requireOption(options, 'tag');
  const repository = requireOption(options, 'repository');
  validateRepository(repository);

  const expectedVersion = versionFromTag(tag);
  const packageJson = readJson('package.json');
  const tauriConfig = readJson('src-tauri/tauri.conf.json');

  if (packageJson.version !== expectedVersion) {
    fail(
      `package.json version ${packageJson.version} does not match tag ${tag}.`,
    );
  }
  if (tauriConfig.version !== expectedVersion) {
    fail(
      `src-tauri/tauri.conf.json version ${tauriConfig.version} does not match tag ${tag}.`,
    );
  }

  const endpoints = tauriConfig.plugins?.updater?.endpoints;
  if (!Array.isArray(endpoints) || endpoints.length === 0) {
    fail('The production updater endpoint is not configured.');
  }
  if (
    endpoints.length !== 1 ||
    endpoints.some((endpoint) => endpoint !== PRODUCTION_ENDPOINT)
  ) {
    fail(
      `Updater endpoints must contain only ${PRODUCTION_ENDPOINT}; test or alternate endpoints are not publishable.`,
    );
  }

  console.log(`Release configuration is valid for ${tag}.`);
}

function requirePlatform(platforms, key) {
  const entry = platforms[key];
  if (!entry || typeof entry !== 'object') {
    fail(`latest.json is missing platforms.${key}.`);
  }
  if (typeof entry.url !== 'string' || entry.url.length === 0) {
    fail(`platforms.${key}.url is missing.`);
  }
  if (
    typeof entry.signature !== 'string' ||
    entry.signature.trim().length === 0
  ) {
    fail(`platforms.${key}.signature is missing.`);
  }
  return entry;
}

function validateEntryAsset(key, entry, assets, expectedSuffixes) {
  const asset = assets.find(
    (candidate) =>
      candidate.url === entry.url ||
      candidate.browser_download_url === entry.url,
  );
  if (!asset) {
    fail(`platforms.${key}.url does not reference an asset in this release.`);
  }
  if (!expectedSuffixes.some((suffix) => asset.name.endsWith(suffix))) {
    fail(
      `platforms.${key} references ${asset.name}, expected one of ${expectedSuffixes.join(', ')}.`,
    );
  }
  if (!assets.some((candidate) => candidate.name === `${asset.name}.sig`)) {
    fail(`The detached signature asset ${asset.name}.sig is missing.`);
  }
}

function validateManifest(options) {
  const tag = requireOption(options, 'tag');
  const repository = requireOption(options, 'repository');
  const manifestPath = requireOption(options, 'manifest');
  const releasePath = requireOption(options, 'release');
  validateRepository(repository);

  const expectedVersion = versionFromTag(tag);
  const manifest = readJson(manifestPath);
  const releaseInventory = readJson(releasePath);
  const releases = Array.isArray(releaseInventory)
    ? releaseInventory.flatMap((page) => (Array.isArray(page) ? page : [page]))
    : [releaseInventory];
  const release = releases.find((candidate) => candidate.tag_name === tag);

  if (!release) {
    fail(`Release inventory does not contain ${tag}.`);
  }

  if (release.tag_name !== tag) {
    fail(`Release inventory is for ${release.tag_name}, expected ${tag}.`);
  }
  if (manifest.version !== expectedVersion) {
    fail(
      `latest.json version ${manifest.version} does not match release tag ${tag}.`,
    );
  }
  if (!manifest.platforms || typeof manifest.platforms !== 'object') {
    fail('latest.json has no platforms object.');
  }
  if (!Array.isArray(release.assets)) {
    fail('Release inventory has no assets array.');
  }

  const windows = requirePlatform(manifest.platforms, 'windows-x86_64');
  const universalSource = requirePlatform(
    manifest.platforms,
    'darwin-universal',
  );

  // Tauri Action emits darwin-universal. The app uses the documented custom target
  // macos-universal, so copy the already signed entry without changing its signature.
  manifest.platforms['macos-universal'] = {
    signature: universalSource.signature,
    url: universalSource.url,
  };
  const macos = requirePlatform(manifest.platforms, 'macos-universal');

  validateEntryAsset('windows-x86_64', windows, release.assets, [
    '.nsis.zip',
    '.exe',
  ]);
  validateEntryAsset('macos-universal', macos, release.assets, ['.app.tar.gz']);
  if (
    !release.assets.some(
      (asset) =>
        asset.name.endsWith('.dmg') &&
        asset.name.toLowerCase().includes('universal'),
    )
  ) {
    fail('The universal macOS DMG installer asset is missing.');
  }

  for (const [key, entry] of Object.entries(manifest.platforms)) {
    if (typeof entry?.url !== 'string') {
      fail(`platforms.${key}.url is invalid.`);
    }
    const isReleaseAsset = release.assets.some(
      (asset) =>
        asset.url === entry.url || asset.browser_download_url === entry.url,
    );
    if (!isReleaseAsset) {
      fail(
        `platforms.${key}.url is outside this production release (possible test endpoint leak).`,
      );
    }
  }

  const sortedPlatforms = Object.fromEntries(
    Object.entries(manifest.platforms).sort(([left], [right]) =>
      left.localeCompare(right),
    ),
  );
  writeFileSync(
    manifestPath,
    `${JSON.stringify({ ...manifest, platforms: sortedPlatforms }, null, 2)}\n`,
  );
  console.log(
    'latest.json contains verified windows-x86_64 and macos-universal updater assets.',
  );
}

const { mode, options } = parseArguments(process.argv.slice(2));
if (mode === 'config') {
  validateConfig(options);
} else if (mode === 'manifest') {
  validateManifest(options);
} else {
  fail(`Unknown mode ${mode}.`);
}
