import js from '@eslint/js';
import svelte from 'eslint-plugin-svelte';
import globals from 'globals';
import tseslint from 'typescript-eslint';

export default tseslint.config(
  { ignores: ['**/dist/**', 'node_modules/**', 'src-tauri/target/**'] },
  js.configs.recommended,
  ...tseslint.configs.recommended,
  ...svelte.configs.recommended,
  {
    files: ['src/**/*.{ts,svelte}', 'benchmarks/results-1000/**/*.{ts,svelte}'],
    ignores: [
      'benchmarks/results-1000/playwright.config.ts',
      'benchmarks/results-1000/playwright-reporter.ts',
      'benchmarks/results-1000/results-1000.spec.ts',
    ],
    languageOptions: { globals: globals.browser },
  },
  {
    files: [
      'scripts/**/*.mjs',
      '**/*.config.{js,ts}',
      'benchmarks/results-1000/playwright.config.ts',
      'benchmarks/results-1000/playwright-reporter.ts',
      'benchmarks/results-1000/results-1000.spec.ts',
    ],
    languageOptions: { globals: globals.nodeBuiltin },
  },
  {
    files: ['**/*.svelte'],
    languageOptions: { parserOptions: { parser: tseslint.parser } },
  },
);
