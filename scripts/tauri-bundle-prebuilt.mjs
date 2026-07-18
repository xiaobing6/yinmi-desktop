import { spawnSync } from 'node:child_process';

const [command, ...args] = process.argv.slice(2);

if (!command) {
  console.error('Expected a Tauri CLI command.');
  process.exit(2);
}

const tauriCommand = command === 'build' ? 'bundle' : command;
const pnpm = process.platform === 'win32' ? 'pnpm.cmd' : 'pnpm';
const result = spawnSync(pnpm, ['tauri', tauriCommand, ...args], {
  shell: process.platform === 'win32',
  stdio: 'inherit',
});

if (result.error) {
  console.error(result.error.message);
  process.exit(1);
}

process.exit(result.status ?? 1);
