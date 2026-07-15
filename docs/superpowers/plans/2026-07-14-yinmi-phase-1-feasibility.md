# 音觅第一阶段：可行性验证与工程脚手架 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 从已确认设计提交 `5893d4340a4815677da79f74223642ac855519e7` 建立可重复构建的 Tauri 2/Svelte 5 工程，并用可审计原型关闭协议分页、隐藏 WebView、SSRF/DNS、原子无覆盖提交、媒体容器、更新取消和 1000 条结果性能风险。

**Architecture:** 第一阶段只建立最小产品壳、纯函数契约模块、受 Cargo feature 与 Vite mode 双重隔离的可行性探针，以及 Windows/macOS 基础 CI。可复用的协议代码进入 `music`，未验证的平台机制全部进入 `feasibility`；原始探针输出只进入被忽略的 `artifacts/feasibility/`，仓库仅提交脱敏结论、ADR 和自动门控清单。

**Tech Stack:** Node.js 24 LTS、pnpm 11.7.0、Svelte 5.56.5、TypeScript 6.0.3、Vite 8.1.4、Tailwind CSS 4.3.2、Vitest 4.1.10、Rust 1.97.0、Tauri 2、Tokio、Reqwest/rustls、Serde、Thiserror、Lofty、GitHub Actions。

## Global Constraints

- 设计来源固定为 `docs/plans/2026-07-14-music-desktop-design.md`，基线提交为 `5893d4340a4815677da79f74223642ac855519e7`；任何改变产品行为、安全不变量、外部契约或发布结果的发现都必须先回修设计。
- 在独立 worktree 的 `phase1/<slug>` 分支中执行，不直接在 `master` 上开发；开始执行时先使用 `superpowers:using-git-worktrees`。该分支前缀也是平台 CI 获取实现分支原始提交 SHA 的固定触发契约。
- Node.js 使用 24 LTS，`packageManager` 固定 `pnpm@11.7.0` 并提交 `pnpm-lock.yaml`。
- TypeScript 固定为 `6.0.3`，与 `typescript-eslint@8.64.0` 的官方支持范围兼容；在 ESLint 工具链正式支持 TypeScript 7 前不得升至 7.x。
- Rust 使用 1.97.0，提交 `rust-toolchain.toml` 与 `Cargo.lock`；Tauri 使用主版本 2。
- 用户可见名称与日志目录名为“音觅”，工程名为 `yinmi`，Bundle Identifier 为 `io.github.xiaobing6.yinmi`，首版版本为 `0.1.0`。
- Windows 支持 Windows 10 22H2/Windows 11 x64 与 WebView2 `111.0.1661.0` 以上；macOS 支持 13.3 以上 Intel/Apple Silicon，最终产物为 Universal。
- 前端构建目标为 Chrome 111 与 Safari 16.4；首版只使用简体中文集中式文案。
- 所有音乐、媒体和更新网络请求由 Rust 发起；前端不得获得通用网络、文件系统或 Shell 权限。
- 隐藏签名 WebView 不匹配任何 capability，不配置 `remote.urls`，不得拥有 IPC、插件、事件、文件或系统能力。
- GD API 固定为 `https://music.gdstudio.xyz/api.php`；在线探针仍受全局 5 分钟 50 次请求限制，不把原始第三方响应提交到仓库。
- 搜索默认音源为网易云音乐，数量范围为 1–1000；第一阶段只探测真实分页行为，不实现正式搜索 UI。
- 签名初始化 20 秒、单次调用 5 秒、返回值 128 字节；网络与资源边界逐字沿用设计附录 B。
- 临时文件与最终提交必须在同一目录；验证结果必须证明原子且不覆盖，不能用“先检查后重命名”代替。
- 第一阶段不实现正式下载队列、播放器、账号、数据库、配置持久化、FFmpeg、正式更新安装 UI 或发布流水线。
- 每个代码任务遵循 red-green-refactor：先写失败测试并确认失败，再写最小实现，最后运行局部与相关全量检查。
- 每个任务进入最终绿色检查前先运行 `pnpm format`（如含前端/脚本）和 `cargo fmt --manifest-path src-tauri/Cargo.toml --all`（如含 Rust）；检查命令只验证，不负责修复格式。
- 每个任务单独提交；探针原始数据、测试密钥、下载媒体和本机绝对路径不得进入提交。

---

## Overall Milestone Roadmap

| 里程碑 | 交付物 | 退出条件 |
| --- | --- | --- |
| 1. 可行性验证、基础 CI 与脚手架 | 本计划的 10 个任务 | `pnpm phase1:gate` 通过，所有平台结论有证据与 ADR；否则回修设计 |
| 2. 共享契约、安全基础设施与状态快照 | DTO、稳定条件码、ts-rs、日志、限流和快照骨架 | Rust/TS 类型一致，重同步与安全边界测试通过 |
| 3. 搜索纵向链路 | 配置、签名、API、分页、归一化、搜索状态和结果界面 | 三种模式、latest-wins、1–1000 和错误行为验收通过 |
| 4. 下载纵向链路 | 不可变任务、顺序队列、媒体、存储、进度和下载界面 | 无覆盖、取消、统计、附件警告和失败重试验收通过 |
| 5. 集成打磨 | 启动页、日志抽屉、可访问性、生命周期和应用内更新 | 800×480、键盘/读屏、退出屏障和更新演练通过 |
| 6. 跨平台发布 | 完整 CI、Universal/NSIS、签名清单和发布验收 | Windows/macOS 产物、`latest.json` 与升级矩阵通过 |

只为里程碑 1 使用本详细计划。里程碑 2–6 在里程碑 1 门控通过后分别编写，不预先锁定未验证机制。

## Phase 1 Gate Semantics

每个风险项只能产生以下结论之一：

- `pass`：证据满足设计，可进入下一里程碑。
- `design-change-required`：机制无法满足设计，停止后续实施，先修改并重新审核设计。
- `blocked`：缺少指定平台、合法测试样本或外部服务条件；不得伪造 `pass`，补齐条件后重跑。

最终门控读取 `docs/feasibility/phase-1-results.json`。只有全部检查为 `pass`，且所有 ADR、平台证据和默认生产构建检查都存在时，`pnpm phase1:gate` 才能退出 0。

## Machine-Readable Evidence Contract

除性能报告使用自己的严格 schema 外，每个 Phase 1 gate 都必须提交一个 JSON companion；Markdown 只是可读解释，不能单独使 gate 通过：

```text
docs/feasibility/toolchain-ci.json
docs/feasibility/gd-contract-pagination.json
docs/feasibility/signature-webview.json
docs/feasibility/network-policy.json
docs/feasibility/atomic-commit.json
docs/feasibility/media-containers.json
docs/feasibility/updater-exit-barrier.json
docs/feasibility/result-list-performance.json
```

前七个文件使用 `docs/feasibility/evidence.schema.json` 和这个公共 envelope：

```json
{
  "schemaVersion": 1,
  "gateId": "signature-webview",
  "status": "pass",
  "designCommit": "5893d4340a4815677da79f74223642ac855519e7",
  "testedCommit": "40 lowercase hex characters",
  "testedAt": "RFC 3339 UTC timestamp",
  "scopeFiles": ["sorted/repository-relative/path"],
  "scopeSha256": "64 lowercase hex characters",
  "markdownPath": "docs/feasibility/signature-webview.md",
  "markdownSha256": "64 lowercase hex characters",
  "decisions": [
    {
      "path": "docs/decisions/0002-signature-webview.md",
      "sha256": "64 lowercase hex characters"
    }
  ],
  "platforms": [
    {
      "id": "windows-10-webview2-111-x64",
      "osVersion": "exact version",
      "arch": "x86_64",
      "command": "exact command",
      "exitCode": 0,
      "runUrl": null
    }
  ],
  "checks": {}
}
```

`testedCommit` is the clean code/harness commit used for the observation and must be an ancestor of the evidence commit. `scopeFiles` is not accepted from raw input: the helper loads the exact gate entry from `docs/feasibility/evidence-scopes.json`, requires set equality, and lists every code/config/fixture file that can change that conclusion, with no globs. Every gate scope includes the scope manifest itself plus the common helper and schema, deliberately invalidating all older companions when validation semantics or scope ownership changes. `scopeSha256` is SHA-256 over the sorted sequence `UTF8(path) + NUL + ASCII(byteLength) + NUL + rawBytes`; `scripts/feasibility-evidence.mjs` builds and later recomputes it. Any scoped change invalidates old evidence even if filenames and handwritten status remain unchanged. The helper also hashes the nonempty Markdown and every ADR, rejects absolute paths/private-key text in both object keys and values, and refuses `pass` unless every supplied command has exit code 0. Tests enumerate every planned path for each gate and prove that deleting, adding or substituting one scope entry fails; a caller cannot shrink scope through raw JSON.

Required machine fields are gate-specific:

| Gate | Required machine checks |
| --- | --- |
| `toolchain-ci` | Windows x64, macOS Intel and macOS ARM rows; `quality`, `platform-windows`, `platform-macos` exact-SHA checks all successful |
| `gd-contract-pagination` | six body fixtures, strict mixed-record parser, 429/other non-2xx/5 MiB+1 tests, all three live cases, numeric safety page limit `<=50` |
| `signature-webview` | Exact IDs `windows-10-webview2-111-x64`, `windows-11-x64`, `macos-13-intel`, `macos-current-arm64`; the Windows 10 row uses the lowest available WebView2 111.0.1661.x fixed runtime; runtime/filter mode recorded, `ipcBridgeAbsent`, nested resource canaries zero, official-only origins, timeout/retry checks |
| `network-policy` | Windows x64, macOS Intel and macOS ARM rows; all-address-set, redirect, peer-pin, body-limit and proxy-disabled checks true |
| `atomic-commit` | NTFS Windows plus APFS Intel/ARM rows; exactly one winner, zero overwrite, zero leftovers, cancel linearization true |
| `media-containers` | Windows x64, macOS Intel and macOS ARM rows; MP3/FLAC round trips true and `negativeFamiliesRejected` is exactly the unique string set `mp2,aac,mp4,ogg,opus,wav,truncated` |
| `updater-exit-barrier` | Windows x64, macOS Intel and macOS ARM rows; real drop-future and real bounded wait-only observations, no early exit/install, numeric production timeout and feedback fields |
| `result-list-performance` | its own schema; shared clean harness SHA, all budgets, Windows authority profile and macOS auxiliary rows |

Code affecting a gate is committed before platform/manual evidence is collected. Evidence Markdown/JSON and ADR are committed separately after the helper validates them. A missing platform is `blocked`; an empty, stale, dirty-tree or hand-edited result cannot become `pass`. Ordinary `pnpm quality` runs evidence-validator unit tests but deliberately does not validate previously committed observations: Tasks 3–10 share manifests and configs, so doing that would self-lock the branch before refreshed evidence could be collected. Strict current-scope validation runs immediately after each companion is built and in `pnpm phase1:gate`. Task 10 freezes the final gate-mechanics commit, reruns all eight observations against it, and rebuilds every companion before generating the aggregate.

## Planned File Structure

```text
.
├─ .github/workflows/
│  ├─ perf-results.yml
│  ├─ quality.yml
│  └─ platform-smoke.yml
├─ benchmarks/results-1000/
│  ├─ BenchmarkApp.svelte
│  ├─ ResultsTablePrototype.svelte
│  ├─ budgets.ts
│  ├─ dataset.ts
│  ├─ dataset.test.ts
│  ├─ env.d.ts
│  ├─ index.html
│  ├─ main.ts
│  ├─ metrics.ts
│  ├─ metrics.test.ts
│  ├─ playwright.config.ts
│  ├─ playwright-reporter.ts
│  ├─ report.schema.json
│  ├─ report.ts
│  ├─ report.test.ts
│  ├─ results-1000.spec.ts
│  ├─ selection.ts
│  ├─ selection.test.ts
│  ├─ tsconfig.json
│  ├─ virtual-range.ts
│  ├─ virtual-range.test.ts
│  └─ vite.config.ts
├─ docs/
│  ├─ decisions/
│  │  ├─ 0001-gd-pagination.md
│  │  ├─ 0002-signature-webview.md
│  │  ├─ 0003-network-ssrf-policy.md
│  │  ├─ 0004-atomic-no-clobber.md
│  │  ├─ 0005-media-container-allowlist.md
│  │  ├─ 0006-updater-exit-barrier.md
│  │  └─ 0007-result-list-performance.md
│  ├─ feasibility/
│  │  ├─ evidence.schema.json
│  │  ├─ evidence-scopes.json
│  │  ├─ toolchain-ci.md
│  │  ├─ toolchain-ci.json
│  │  ├─ gd-contract-pagination.md
│  │  ├─ gd-contract-pagination.json
│  │  ├─ signature-webview.md
│  │  ├─ signature-webview.json
│  │  ├─ network-policy.md
│  │  ├─ network-policy.json
│  │  ├─ atomic-commit.md
│  │  ├─ atomic-commit.json
│  │  ├─ media-containers.md
│  │  ├─ media-containers.json
│  │  ├─ updater-exit-barrier.md
│  │  ├─ updater-exit-barrier.json
│  │  ├─ result-list-performance.md
│  │  ├─ result-list-performance.json
│  │  └─ phase-1-results.json
│  └─ superpowers/plans/2026-07-14-yinmi-phase-1-feasibility.md
├─ scripts/
│  ├─ verify-config.mjs
│  ├─ verify-ci.mjs
│  ├─ verify-default-artifacts.mjs
│  ├─ feasibility-evidence.mjs
│  ├─ feasibility-evidence.test.mjs
│  ├─ check-phase1-gate.mjs
│  ├─ check-phase1-gate.test.mjs
│  ├─ generate-media-fixtures.mjs
│  ├─ slow-update-server.mjs
│  └─ perf/
│     ├─ capture-windows-baseline.ps1
│     ├─ run-browser.mjs
│     ├─ validate-report.mjs
│     └─ validate-report.test.mjs
├─ src/
│  ├─ lib/feasibility/
│  │  ├─ FeasibilityPanel.svelte
│  │  └─ GdProbe.svelte
│  ├─ test/setup.ts
│  ├─ App.svelte
│  ├─ App.test.ts
│  ├─ app.css
│  ├─ main.ts
│  └─ vite-env.d.ts
├─ src-tauri/
│  ├─ capabilities/
│  │  ├─ feasibility-main.json
│  │  └─ main.json
│  ├─ icons/
│  ├─ permissions/feasibility.toml
│  ├─ src/
│  │  ├─ bin/atomic_commit_worker.rs
│  │  ├─ feasibility/
│  │  │  ├─ atomic_commit.rs
│  │  │  ├─ gd_live.rs
│  │  │  ├─ media_probe.rs
│  │  │  ├─ network_policy.rs
│  │  │  ├─ signature_webview.rs
│  │  │  ├─ webview_resource_policy.rs
│  │  │  └─ updater_probe.rs
│  │  ├─ music/
│  │  │  ├─ contract.rs
│  │  │  └─ mod.rs
│  │  ├─ lib.rs
│  │  └─ main.rs
│  ├─ tests/
│  │  ├─ fixtures/gd/*.json
│  │  ├─ fixtures/media/*
│  │  ├─ atomic_commit.rs
│  │  ├─ gd_contract.rs
│  │  ├─ media_probe.rs
│  │  ├─ network_policy.rs
│  │  └─ updater_probe.rs
│  ├─ build.rs
│  ├─ Cargo.lock
│  ├─ Cargo.toml
│  ├─ tauri.conf.json
│  ├─ tauri.feasibility.conf.json
│  └─ tauri.perf.conf.json
├─ .editorconfig
├─ .gitignore
├─ .node-version
├─ .npmrc
├─ .prettierignore
├─ eslint.config.js
├─ index.html
├─ package.json
├─ pnpm-lock.yaml
├─ prettier.config.js
├─ README.md
├─ rust-toolchain.toml
├─ svelte.config.js
├─ tsconfig.json
└─ vite.config.ts
```

`music/contract.rs` 是固定外部契约的可复用代码。`feasibility/` 下的机制在 ADR 通过前都不是生产承诺；默认构建不得注册其命令或包含其前端入口。

### Task 1: 建立可重复的 Tauri/Svelte 工程壳

**Files:**
- Create: `.editorconfig`
- Create: `.gitignore`
- Create: `.node-version`
- Create: `.npmrc`
- Create: `.prettierignore`
- Create: `README.md`
- Create: `package.json`
- Create: `pnpm-lock.yaml`
- Create: `prettier.config.js`
- Create: `eslint.config.js`
- Create: `tsconfig.json`
- Create: `svelte.config.js`
- Create: `vite.config.ts`
- Create: `index.html`
- Create: `src/main.ts`
- Create: `src/App.svelte`
- Create: `src/App.test.ts`
- Create: `src/app.css`
- Create: `src/test/setup.ts`
- Create: `src/vite-env.d.ts`
- Create: `scripts/verify-config.mjs`
- Create: `rust-toolchain.toml`
- Create: `src-tauri/Cargo.toml`
- Create: `src-tauri/Cargo.lock`
- Create: `src-tauri/build.rs`
- Create: `src-tauri/src/main.rs`
- Create: `src-tauri/src/lib.rs`
- Create: `src-tauri/tauri.conf.json`
- Create: `src-tauri/capabilities/main.json`
- Create: `src-tauri/icons/icon-source.svg` and generated icon files

**Interfaces:**
- Consumes: design identity and platform floors from commit `5893d4340a4815677da79f74223642ac855519e7`.
- Produces: `yinmi_lib::run()`, npm scripts `format:check`, `lint`, `check`, `test`, `build`, `verify:config`, `quality`, and a default app containing no feasibility commands.

- [ ] **Step 1: Install and verify the pinned local toolchain**

On the current Windows machine, install the official prerequisites if missing:

```powershell
winget install --exact --id Microsoft.VisualStudio.2022.BuildTools --override "--wait --passive --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended"
winget install --exact --id OpenJS.NodeJS.LTS
winget install --exact --id Rustlang.Rustup
rustup toolchain install 1.97.0 --profile minimal --component rustfmt clippy
rustup default 1.97.0
if (Get-Command corepack -ErrorAction SilentlyContinue) { corepack enable; corepack prepare pnpm@11.7.0 --activate } else { npm install --global pnpm@11.7.0 }
git config --local user.name "Zhang Yingming"
git config --local user.email "912232670@qq.com"
node --version
pnpm --version
rustc --version
cargo --version
```

Expected: Node reports `v24.x`, pnpm reports `11.7.0`, both Rust commands report `1.97.0`, and only this repository receives the confirmed Git identity. On macOS validation hosts, install Xcode Command Line Tools plus `node@24`, activate pnpm 11.7.0 through Corepack, and run the same rustup commands; CI uses `setup-node` instead of mutating the runner image.

- [ ] **Step 2: Create dependency and formatting configuration**

Create `package.json` with exact plan-date frontend versions; do not add unused Skeleton, Lucide or Tauri plugins yet:

```json
{
  "name": "yinmi",
  "private": true,
  "version": "0.1.0",
  "type": "module",
  "packageManager": "pnpm@11.7.0",
  "engines": { "node": ">=24 <25", "pnpm": "11.7.0" },
  "scripts": {
    "dev": "vite",
    "build": "vite build",
    "check": "svelte-check --tsconfig ./tsconfig.json",
    "test": "vitest run",
    "lint": "eslint .",
    "format": "prettier --write .",
    "format:check": "prettier --check .",
    "verify:config": "node scripts/verify-config.mjs",
    "quality": "pnpm format:check && pnpm lint && pnpm check && pnpm test && pnpm build && pnpm verify:config",
    "tauri": "tauri"
  },
  "dependencies": {
    "@tauri-apps/api": "2.11.1",
    "svelte": "5.56.5"
  },
  "devDependencies": {
    "@eslint/js": "10.0.1",
    "@sveltejs/vite-plugin-svelte": "7.2.0",
    "@tailwindcss/vite": "4.3.2",
    "@tauri-apps/cli": "2.11.4",
    "@testing-library/svelte": "5.4.2",
    "@types/node": "24.13.3",
    "eslint": "10.7.0",
    "eslint-plugin-svelte": "3.20.0",
    "globals": "17.7.0",
    "jsdom": "29.1.1",
    "prettier": "3.9.5",
    "prettier-plugin-svelte": "4.1.1",
    "svelte-check": "4.7.2",
    "tailwindcss": "4.3.2",
    "tslib": "2.8.1",
    "typescript": "6.0.3",
    "typescript-eslint": "8.64.0",
    "vite": "8.1.4",
    "vitest": "4.1.10",
    "yaml": "2.9.0"
  }
}
```

Create `.gitignore` with these exact repository-local exclusions:

```gitignore
node_modules/
dist/
src-tauri/target/
src-tauri/gen/schemas/
artifacts/feasibility/
.env
.env.*.local
.DS_Store
*.log
```

Create `.node-version` containing `24` and `.npmrc` containing:

```ini
engine-strict=true
save-exact=true
```

Create `.prettierignore` so source formatting does not rewrite reviewed or generated artifacts:

```gitignore
docs/
README.md
artifacts/
src-tauri/target/
dist/
```

Create `README.md` with the current authorized phase and design baseline:

```markdown
# 音觅

Phase 1 feasibility validation is in progress against design commit `5893d4340a4815677da79f74223642ac855519e7`. Product feature implementation beyond the Phase 1 gate is not yet authorized.
```

Create `.editorconfig` and `prettier.config.js`:

```ini
root = true

[*]
charset = utf-8
end_of_line = lf
insert_final_newline = true
indent_style = space
indent_size = 2

[*.rs]
indent_size = 4
```

```js
export default {
  plugins: ['prettier-plugin-svelte'],
  singleQuote: true,
  trailingComma: 'all',
  overrides: [{ files: '*.svelte', options: { parser: 'svelte' } }],
};
```

Run:

```powershell
pnpm install
```

Expected: `pnpm-lock.yaml` is created and no engine warning is printed.

- [ ] **Step 3: Add Vite, Svelte, TypeScript and ESLint configuration**

Create `svelte.config.js`, `tsconfig.json`, `vite.config.ts`, and `eslint.config.js`:

```js
import { vitePreprocess } from '@sveltejs/vite-plugin-svelte';

export default { preprocess: vitePreprocess() };
```

```json
{
  "compilerOptions": {
    "target": "ES2022",
    "useDefineForClassFields": true,
    "module": "ESNext",
    "moduleResolution": "Bundler",
    "strict": true,
    "allowJs": true,
    "checkJs": true,
    "isolatedModules": true,
    "esModuleInterop": true,
    "resolveJsonModule": true,
    "skipLibCheck": true,
    "types": ["node", "svelte", "vite/client", "vitest/globals"]
  },
  "include": ["src/**/*.d.ts", "src/**/*.ts", "src/**/*.svelte", "vite.config.ts"]
}
```

```ts
import tailwindcss from '@tailwindcss/vite';
import { svelte } from '@sveltejs/vite-plugin-svelte';
import { defineConfig } from 'vitest/config';

export default defineConfig({
  plugins: [tailwindcss(), svelte()],
  clearScreen: false,
  server: { port: 1420, strictPort: true },
  build: { target: ['chrome111', 'safari16.4'] },
  test: {
    environment: 'jsdom',
    setupFiles: ['src/test/setup.ts'],
    include: ['src/**/*.test.ts'],
  },
});
```

```js
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
```

Create `src/test/setup.ts`:

```ts
import { cleanup } from '@testing-library/svelte';
import { afterEach } from 'vitest';

afterEach(() => cleanup());
```

- [ ] **Step 4: Write the failing application smoke test**

Create `src/App.test.ts` before `src/App.svelte`:

```ts
import { render, screen } from '@testing-library/svelte';
import { describe, expect, it } from 'vitest';
import App from './App.svelte';

describe('application shell', () => {
  it('shows the confirmed product identity and phase', () => {
    render(App);
    expect(screen.getByRole('heading', { name: '音觅' })).toBeTruthy();
    expect(screen.getByText('第一阶段可行性验证')).toBeTruthy();
  });
});
```

Run:

```powershell
pnpm test -- src/App.test.ts
```

Expected: FAIL because `src/App.svelte` does not exist.

- [ ] **Step 5: Implement the minimal product shell**

Create `index.html`, `src/main.ts`, `src/App.svelte`, `src/app.css`, and `src/vite-env.d.ts`:

```html
<!doctype html>
<html lang="zh-CN">
  <head>
    <meta charset="UTF-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1.0" />
    <title>音觅</title>
  </head>
  <body>
    <div id="app"></div>
    <script type="module" src="/src/main.ts"></script>
  </body>
</html>
```

```ts
import { mount } from 'svelte';
import './app.css';
import App from './App.svelte';

const target = document.getElementById('app');
if (!target) throw new Error('missing #app mount target');

const app = mount(App, { target });
export default app;
```

```svelte
<svelte:head><title>音觅</title></svelte:head>

<main>
  <h1>音觅</h1>
  <p>第一阶段可行性验证</p>
</main>
```

```css
@import 'tailwindcss';

:root {
  font-family: 'Microsoft YaHei', 'PingFang SC', 'Noto Sans SC', sans-serif;
  color: #16283e;
  background: #f6f9fc;
}

body { margin: 0; min-width: 320px; min-height: 100vh; }
main { display: grid; min-height: 100vh; place-content: center; text-align: center; }
h1 { margin: 0; color: #0b78d0; }
```

```ts
/// <reference types="svelte" />
/// <reference types="vite/client" />
```

Run:

```powershell
pnpm test -- src/App.test.ts
```

Expected: PASS, 1 test.

- [ ] **Step 6: Write the failing configuration verifier**

Create `scripts/verify-config.mjs`:

```js
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
```

Run:

```powershell
pnpm verify:config
```

Expected: FAIL with `ENOENT` for `src-tauri/tauri.conf.json`.

- [ ] **Step 7: Add the minimal Rust/Tauri application and capability**

Create `rust-toolchain.toml`:

```toml
[toolchain]
channel = "1.97.0"
profile = "minimal"
components = ["rustfmt", "clippy"]
```

Create `src-tauri/Cargo.toml`:

```toml
[package]
name = "yinmi"
version = "0.1.0"
description = "音觅桌面音乐搜索下载工具"
edition = "2024"
rust-version = "1.97"

[lib]
name = "yinmi_lib"
crate-type = ["staticlib", "cdylib", "rlib"]

[build-dependencies]
tauri-build = { version = "2", features = [] }

[dependencies]
serde = { version = "1", features = ["derive"] }
serde_json = "1"
tauri = { version = "2.11.5", features = [] }

[features]
default = ["custom-protocol"]
custom-protocol = ["tauri/custom-protocol"]
feasibility = []
```

Create `src-tauri/build.rs`, `src-tauri/src/main.rs`, and `src-tauri/src/lib.rs`:

```rust
fn main() {
    tauri_build::build()
}
```

```rust
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    yinmi_lib::run();
}
```

```rust
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .run(tauri::generate_context!())
        .expect("failed to run yinmi");
}
```

Create `src-tauri/tauri.conf.json`:

```json
{
  "$schema": "https://schema.tauri.app/config/2",
  "productName": "音觅",
  "version": "0.1.0",
  "identifier": "io.github.xiaobing6.yinmi",
  "build": {
    "beforeDevCommand": "pnpm dev",
    "devUrl": "http://localhost:1420",
    "beforeBuildCommand": "pnpm build",
    "frontendDist": "../dist"
  },
  "app": {
    "windows": [
      {
        "label": "main",
        "title": "音觅",
        "width": 1280,
        "height": 800,
        "minWidth": 800,
        "minHeight": 480
      }
    ],
    "security": {
      "capabilities": ["main-capability"],
      "csp": "default-src 'self'; img-src 'self' data:; style-src 'self' 'unsafe-inline'; connect-src 'self' ipc: http://ipc.localhost"
    }
  },
  "bundle": {
    "active": true,
    "targets": "all",
    "windows": { "webviewInstallMode": { "type": "embedBootstrapper" } },
    "macOS": { "minimumSystemVersion": "13.3" }
  }
}
```

Create `src-tauri/capabilities/main.json` with no remote URL and no plugin permission:

```json
{
  "$schema": "../gen/schemas/desktop-schema.json",
  "identifier": "main-capability",
  "description": "Local main window only",
  "local": true,
  "windows": ["main"],
  "permissions": ["core:default"]
}
```

Run:

```powershell
pnpm verify:config
cargo generate-lockfile --manifest-path src-tauri/Cargo.toml
```

Expected: `configuration contract: PASS` and `src-tauri/Cargo.lock` created.

- [ ] **Step 8: Generate the confirmed seek-ring icon set**

Create `src-tauri/icons/icon-source.svg`:

```svg
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 512 512">
  <rect width="512" height="512" rx="112" fill="#0B78D0"/>
  <circle cx="248" cy="264" r="144" fill="none" stroke="#F7FBFF" stroke-width="36"/>
  <circle cx="248" cy="264" r="78" fill="none" stroke="#9BDEFF" stroke-width="28"/>
  <circle cx="248" cy="264" r="24" fill="#F7FBFF"/>
  <circle cx="374" cy="148" r="38" fill="#64D2AF" stroke="#F7FBFF" stroke-width="16"/>
</svg>
```

Run:

```powershell
pnpm tauri icon src-tauri/icons/icon-source.svg
```

Expected: Tauri generates PNG, ICO and ICNS assets under `src-tauri/icons/`; inspect 16 px and 32 px outputs for legibility.

- [ ] **Step 9: Run the full local scaffold gate**

Run:

```powershell
pnpm format
cargo fmt --manifest-path src-tauri/Cargo.toml --all
pnpm quality
cargo fmt --manifest-path src-tauri/Cargo.toml --check
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets --all-features -- -D warnings
cargo test --manifest-path src-tauri/Cargo.toml --all-targets
pnpm tauri build --debug --no-bundle
git status --short
```

Expected: every command exits 0; only the intended scaffold files are untracked/modified.

- [ ] **Step 10: Commit the scaffold**

```powershell
git add .editorconfig .gitignore .node-version .npmrc .prettierignore README.md package.json pnpm-lock.yaml prettier.config.js eslint.config.js tsconfig.json svelte.config.js vite.config.ts index.html src scripts/verify-config.mjs rust-toolchain.toml src-tauri
git commit -m "build: scaffold tauri svelte application"
```

### Task 2: 建立稳定名称的基础 CI

**Files:**
- Create: `.github/workflows/quality.yml`
- Create: `.github/workflows/platform-smoke.yml`
- Create: `scripts/verify-ci.mjs`
- Create: `scripts/feasibility-evidence.mjs`
- Create: `scripts/feasibility-evidence.test.mjs`
- Create: `docs/feasibility/evidence.schema.json`
- Create: `docs/feasibility/evidence-scopes.json`
- Modify: `package.json`
- Create: `docs/feasibility/toolchain-ci.md`
- Create: `docs/feasibility/toolchain-ci.json`

**Interfaces:**
- Consumes: Task 1 scripts and Cargo workspace.
- Produces: stable required check names `quality`, `platform-windows`, `platform-macos`; Intel and Apple Silicon macOS jobs feed the single required macOS aggregator. All pushes run fast checks; `phase1/**` and `master` pushes run platform smoke checks on the branch's original head SHA, while PR/`merge_group` runs remain supplemental merge-candidate checks.

- [ ] **Step 1: Write the failing CI contract checker**

Create `scripts/verify-ci.mjs`:

```js
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { parse } from 'yaml';

const qualityText = await readFile('.github/workflows/quality.yml', 'utf8');
const platformText = await readFile('.github/workflows/platform-smoke.yml', 'utf8');
const quality = parse(qualityText);
const platform = parse(platformText);

for (const event of ['push', 'pull_request', 'merge_group']) {
  assert.ok(Object.hasOwn(quality.on, event), `quality workflow missing ${event}`);
}
assert.equal(quality.name, 'quality');
assert.equal(quality.jobs.quality.name, 'quality');
assert.ok(quality.jobs.quality.steps.some((step) => step.run === 'pnpm quality'));

assert.deepEqual(platform.on.push.branches, ['master', 'phase1/**']);
assert.equal(platform.name, 'platform-smoke');
for (const event of ['pull_request', 'merge_group']) {
  assert.ok(Object.hasOwn(platform.on, event), `platform workflow missing ${event}`);
}
assert.equal(platform.jobs['platform-windows'].name, 'platform-windows');
assert.equal(platform.jobs.platform_macos_intel.name, 'platform-macos-intel');
assert.equal(platform.jobs.platform_macos_arm.name, 'platform-macos-arm');
assert.equal(platform.jobs.platform_macos.name, 'platform-macos');
assert.deepEqual(platform.jobs.platform_macos.needs, ['platform_macos_intel', 'platform_macos_arm']);
assert.equal(platform.jobs.platform_macos.if, 'always()');

for (const job of [
  quality.jobs.quality,
  platform.jobs['platform-windows'],
  platform.jobs.platform_macos_intel,
  platform.jobs.platform_macos_arm,
]) {
  const checkout = job.steps.find((step) => step.uses === 'actions/checkout@v6');
  assert.equal(checkout?.with?.['fetch-depth'], 0, `${job.name} must fetch full history`);
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
assert.ok(!platformText.includes('upload-artifact'), 'smoke workflow must not upload release artifacts');
console.log('CI contract: PASS');
```

Add `"verify:ci": "node scripts/verify-ci.mjs"` to `package.json` and append `&& pnpm verify:ci` to `quality`.

Before implementing the evidence helper, create `scripts/feasibility-evidence.test.mjs` with Node built-in tests for the exact common envelope and scope manifest. Each temporary Git repository sets its own test-only `user.name` and `user.email` rather than relying on global Git configuration. Tests cover deterministic sorted scope hashing, one-byte scope tampering, omitted/extra/substituted manifest path, caller-supplied `scopeFiles`, empty Markdown, one-byte ADR tampering, wrong design/tested commit, non-ancestor tested commit, dirty scoped file, absolute/duplicate scope path, missing ADR, nonzero command exit, duplicate/missing platform ID, and forbidden absolute path/private-key text. Run:

```powershell
node --test scripts/feasibility-evidence.test.mjs
```

Expected: FAIL because `feasibility-evidence.mjs` does not exist.

Run:

```powershell
pnpm verify:ci
```

Expected: FAIL with `ENOENT` because workflows do not exist.

- [ ] **Step 2: Add the fast quality workflow**

Implement `docs/feasibility/evidence.schema.json`, `docs/feasibility/evidence-scopes.json` and `scripts/feasibility-evidence.mjs` first. The scope manifest contains exact planned path arrays for all eight gates from the Planned File Structure and task file lists; a build validates existence only for its selected gate, so future planned paths may not exist yet. Each later task reviews and updates its own entry in the same code commit that adds or changes scoped files, and Task 10 rejects every missing/extra final entry. Export `digestScope`, `validateEvidence` and `buildEvidence`; support these CLIs:

```text
node scripts/feasibility-evidence.mjs build --input <ignored-raw.json> --markdown <file.md> --output <file.json>
node scripts/feasibility-evidence.mjs check <file.json>
node scripts/feasibility-evidence.mjs check-existing docs/feasibility
```

`build` requires `testedCommit == git rev-parse HEAD`, obtains the exact scope from the manifest, verifies every scope file is tracked and unchanged from that commit, computes the canonical scope/Markdown/ADR hashes, validates gate-specific raw fields, and writes atomically. `check` recomputes every hash and commit relationship. `check-existing` strictly validates every recognized companion present but permits later gate files to be absent; Task 10 separately requires the complete set. Add `"test:evidence": "node --test scripts/feasibility-evidence.test.mjs"` and `"verify:evidence": "node scripts/feasibility-evidence.mjs check-existing docs/feasibility"`; append only `test:evidence` to ordinary `quality`. Run the helper tests; expected all pass. Never append strict `verify:evidence` to ordinary quality because later code tasks intentionally invalidate observations before they can be refreshed.

Create `.github/workflows/quality.yml`:

```yaml
name: quality

on:
  push:
  pull_request:
  merge_group:

permissions:
  contents: read

jobs:
  quality:
    name: quality
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v6
        with:
          fetch-depth: 0
      - uses: pnpm/action-setup@v6
        with:
          version: 11.7.0
      - uses: actions/setup-node@v6
        with:
          node-version: 24
          cache: pnpm
      - uses: dtolnay/rust-toolchain@stable
        with:
          toolchain: 1.97.0
          components: rustfmt, clippy
      - uses: Swatinem/rust-cache@v2
        with:
          workspaces: src-tauri -> target
      - name: Install Tauri Linux prerequisites
        run: sudo apt-get update && sudo apt-get install -y libwebkit2gtk-4.1-dev libappindicator3-dev librsvg2-dev patchelf
      - run: pnpm install --frozen-lockfile
      - run: pnpm quality
      - run: cargo fmt --manifest-path src-tauri/Cargo.toml --check
      - run: cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets --all-features -- -D warnings
      - run: cargo test --manifest-path src-tauri/Cargo.toml --all-targets
```

- [ ] **Step 3: Add Windows and macOS smoke jobs**

Create `.github/workflows/platform-smoke.yml`:

```yaml
name: platform-smoke

on:
  push:
    branches: [master, 'phase1/**']
  pull_request:
  merge_group:

permissions:
  contents: read

jobs:
  platform-windows:
    name: platform-windows
    runs-on: windows-2025
    steps:
      - uses: actions/checkout@v6
        with:
          fetch-depth: 0
      - uses: pnpm/action-setup@v6
        with:
          version: 11.7.0
      - uses: actions/setup-node@v6
        with:
          node-version: 24
          cache: pnpm
      - uses: dtolnay/rust-toolchain@stable
        with:
          toolchain: 1.97.0
          targets: x86_64-pc-windows-msvc
      - uses: Swatinem/rust-cache@v2
        with:
          workspaces: src-tauri -> target
      - run: pnpm install --frozen-lockfile
      - run: pnpm build
      - run: cargo test --manifest-path src-tauri/Cargo.toml --all-targets
      - run: pnpm tauri build --debug --no-bundle --target x86_64-pc-windows-msvc

  platform_macos_intel:
    name: platform-macos-intel
    runs-on: macos-15-intel
    steps:
      - uses: actions/checkout@v6
        with:
          fetch-depth: 0
      - uses: pnpm/action-setup@v6
        with:
          version: 11.7.0
      - uses: actions/setup-node@v6
        with:
          node-version: 24
          cache: pnpm
      - uses: dtolnay/rust-toolchain@stable
        with:
          toolchain: 1.97.0
          targets: x86_64-apple-darwin
      - uses: Swatinem/rust-cache@v2
        with:
          workspaces: src-tauri -> target
      - run: pnpm install --frozen-lockfile
      - run: pnpm build
      - run: cargo test --manifest-path src-tauri/Cargo.toml --all-targets
      - run: cargo check --manifest-path src-tauri/Cargo.toml --target x86_64-apple-darwin
      - run: pnpm tauri build --debug --no-bundle --target x86_64-apple-darwin

  platform_macos_arm:
    name: platform-macos-arm
    runs-on: macos-15
    steps:
      - uses: actions/checkout@v6
        with:
          fetch-depth: 0
      - uses: pnpm/action-setup@v6
        with:
          version: 11.7.0
      - uses: actions/setup-node@v6
        with:
          node-version: 24
          cache: pnpm
      - uses: dtolnay/rust-toolchain@stable
        with:
          toolchain: 1.97.0
          targets: x86_64-apple-darwin,aarch64-apple-darwin
      - uses: Swatinem/rust-cache@v2
        with:
          workspaces: src-tauri -> target
      - run: pnpm install --frozen-lockfile
      - run: pnpm build
      - run: cargo test --manifest-path src-tauri/Cargo.toml --all-targets
      - run: cargo check --manifest-path src-tauri/Cargo.toml --target aarch64-apple-darwin
      - run: cargo check --manifest-path src-tauri/Cargo.toml --target x86_64-apple-darwin
      - run: pnpm tauri build --debug --no-bundle --target universal-apple-darwin
      - run: lipo -archs src-tauri/target/universal-apple-darwin/debug/yinmi

  platform_macos:
    name: platform-macos
    if: always()
    needs: [platform_macos_intel, platform_macos_arm]
    runs-on: ubuntu-latest
    steps:
      - name: Require both macOS architectures
        run: |
          test "${{ needs.platform_macos_intel.result }}" = "success"
          test "${{ needs.platform_macos_arm.result }}" = "success"
```

- [ ] **Step 4: Verify the workflow contract and local commands**

Run:

```powershell
pnpm verify:ci
pnpm exec prettier --check .github/workflows scripts/verify-ci.mjs package.json
pnpm quality
```

Expected: `CI contract: PASS`; all local checks pass. Push the `phase1/**` branch and use the push-event runs whose `head_sha` equals the branch commit—not a `pull_request` synthetic merge SHA. Confirm GitHub exposes `quality`, `platform-windows`, and aggregate `platform-macos` as stable required-check candidates; the Intel and Arm macOS checks are visible as supporting jobs but need not be configured separately in branch protection.

- [ ] **Step 5: Commit the CI implementation, verify the exact commit, then commit evidence**

First commit every implementation file and require a clean tree:

```powershell
git add .github/workflows scripts/verify-ci.mjs scripts/feasibility-evidence.mjs scripts/feasibility-evidence.test.mjs docs/feasibility/evidence.schema.json docs/feasibility/evidence-scopes.json package.json pnpm-lock.yaml
git commit -m "ci: add cross-platform smoke checks"
git status --short
```

Push the `phase1/**` branch and wait until push-event `quality`, `platform-windows`, and aggregate `platform-macos` all succeed with `head_sha` exactly equal to this commit. Do not use a PR merge run or a run from another SHA. Save event type, head SHA, three run URLs and native runner rows into ignored `artifacts/feasibility/toolchain-ci.raw.json`.

Create `docs/feasibility/toolchain-ci.md` containing the local tool versions, workflow run URLs, runner OS/architecture, three stable check names, and the statement `Conclusion: pass`. Do not record the local user profile path.

```powershell
node scripts/feasibility-evidence.mjs build --input artifacts/feasibility/toolchain-ci.raw.json --markdown docs/feasibility/toolchain-ci.md --output docs/feasibility/toolchain-ci.json
node scripts/feasibility-evidence.mjs check docs/feasibility/toolchain-ci.json
git add docs/feasibility/toolchain-ci.md docs/feasibility/toolchain-ci.json
git commit -m "docs: record cross-platform CI evidence"
```

### Task 3: 固定 GD 编码、响应与分页契约

**Files:**
- Create: `src-tauri/src/music/mod.rs`
- Create: `src-tauri/src/music/contract.rs`
- Modify: `src-tauri/src/lib.rs`
- Modify: `src-tauri/Cargo.toml`
- Modify: `src-tauri/Cargo.lock`
- Create: `src-tauri/tests/gd_contract.rs`
- Create: `src-tauri/tests/fixtures/gd/README.md`
- Create: `src-tauri/tests/fixtures/gd/search_mixed.json`
- Create: `src-tauri/tests/fixtures/gd/search_empty.json`
- Create: `src-tauri/tests/fixtures/gd/search_incompatible.json`
- Create: `src-tauri/tests/fixtures/gd/url_success.json`
- Create: `src-tauri/tests/fixtures/gd/url_empty.json`
- Create: `src-tauri/tests/fixtures/gd/url_lower_bitrate.json`
- Create: `src-tauri/tests/fixtures/gd/url_missing_bitrate.json`
- Create: `src-tauri/tests/fixtures/gd/pic_success.json`
- Create: `src-tauri/tests/fixtures/gd/lyric_success.json`
- Create: `src-tauri/tests/fixtures/gd/lyric_empty.json`
- Create: `src-tauri/tests/fixtures/gd/explicit_error.json`

**Interfaces:**
- Consumes: fixed wire contract from design Appendix A; no network and no WebView.
- Produces: `EncodedComponent`, `SignatureValue`, `GdSource`, `GdOperation`, `render_form_body`, all four response parsers, and `PaginationProbe`; Task 4 must consume these exact types rather than rebuilding strings.

- [ ] **Step 1: Add exact fixture data and failing form-body tests**

Add these dependencies to `src-tauri/Cargo.toml`:

```toml
thiserror = "2"
url = "2"
```

Create `fixtures/gd/README.md` first. It identifies every fixture as a hand-authored minimal contract sample derived from design commit `5893d4340a4815677da79f74223642ac855519e7` and official page version `2026.06.16`, maps each filename to the rule it proves, and states that no raw third-party song row or signature is stored.

Create `src-tauri/tests/fixtures/gd/search_mixed.json`:

```json
[
  {
    "id": "track-1",
    "name": "歌曲一",
    "artist": "歌手甲",
    "album": null,
    "source": "netease",
    "url_id": "url-1",
    "pic_id": "pic-1",
    "lyric_id": "lyric-1",
    "extra_data": { "duration": "123", "has_hires": true }
  },
  {
    "id": 9007199254740991,
    "name": "歌曲二",
    "artist": ["歌手乙", "歌手丙"],
    "album": "专辑",
    "source": "netease",
    "extra_data": { "duration": 10 }
  },
  {
    "id": null,
    "name": "坏记录",
    "artist": [],
    "album": null,
    "source": "netease"
  },
  {
    "id": "bad-mixed-artist",
    "name": "混合艺人类型",
    "artist": ["歌手丁", 7],
    "album": null,
    "source": "netease"
  }
]
```

Create the remaining small fixtures with exact contents:

`search_empty.json`:

```json
[]
```

`search_incompatible.json`:

```json
{ "unexpected": true }
```

`url_success.json`:

```json
{ "url": "https://cdn.example.invalid/audio", "br": 320, "size": 1024, "source": "netease" }
```

`url_empty.json`:

```json
{ "url": "", "br": "", "size": null }
```

`url_lower_bitrate.json`:

```json
{ "url": "https://cdn.example.invalid/audio", "br": 128 }
```

`url_missing_bitrate.json`:

```json
{ "url": "https://cdn.example.invalid/audio" }
```

`pic_success.json`:

```json
{ "url": "https://cdn.example.invalid/cover.jpg" }
```

`lyric_success.json`:

```json
{ "lyric": "[00:00.00]测试", "tlyric": "[00:00.00]Test" }
```

`lyric_empty.json`:

```json
{ "lyric": "", "tlyric": "" }
```

`explicit_error.json`:

```json
{ "error": "upstream failed", "code": 500 }
```

Write `src-tauri/tests/gd_contract.rs` with the six exact body assertions and special encoding assertions:

```rust
use yinmi_lib::music::contract::{
    render_form_body, EncodedComponent, GdOperation, GdSource, SearchOperation, SignatureValue,
};

const SIG: &str = "fixture-signature";

#[test]
fn renders_six_official_bodies_in_exact_order() {
    let name = EncodedComponent::encode("周杰伦");
    let id = EncodedComponent::encode("123456");
    let signature = SignatureValue::try_from(SIG).unwrap();

    let cases = [
        (
            GdOperation::Search { operation: SearchOperation::Track, count: 20, source: GdSource::NeteaseMusic, page: 1, name: name.clone() },
            "types=search&count=20&source=netease&pages=1&name=%E5%91%A8%E6%9D%B0%E4%BC%A6&s=fixture-signature",
        ),
        (
            GdOperation::Search { operation: SearchOperation::Album, count: 20, source: GdSource::NeteaseMusic, page: 1, name: name.clone() },
            "types=search_album&count=20&source=netease&pages=1&name=%E5%91%A8%E6%9D%B0%E4%BC%A6&s=fixture-signature",
        ),
        (
            GdOperation::Search { operation: SearchOperation::Playlist, count: 20, source: GdSource::NeteaseMusic, page: 1, name },
            "types=search_playlist&count=20&source=netease&pages=1&name=%E5%91%A8%E6%9D%B0%E4%BC%A6&s=fixture-signature",
        ),
        (
            GdOperation::Url { id: id.clone(), source: GdSource::NeteaseMusic, bitrate: 320 },
            "types=url&id=123456&source=netease&br=320&s=fixture-signature",
        ),
        (
            GdOperation::Pic { id: id.clone(), source: GdSource::NeteaseMusic, size: 300 },
            "types=pic&id=123456&source=netease&size=300&s=fixture-signature",
        ),
        (
            GdOperation::Lyric { id, source: GdSource::NeteaseMusic },
            "types=lyric&id=123456&source=netease&s=fixture-signature",
        ),
    ];

    for (operation, expected) in cases {
        assert_eq!(render_form_body(&operation, &signature), expected);
    }
}

#[test]
fn matches_javascript_component_encoding_without_double_encoding() {
    assert_eq!(EncodedComponent::encode("A B!'()*/?=%").as_str(), "A%20B%21%27%28%29%2A%2F%3F%3D%25");
    assert_eq!(EncodedComponent::encode("id/42?x=1%").as_str(), "id%2F42%3Fx%3D1%25");
    assert!(!EncodedComponent::encode("A B").as_str().contains("%2520"));
}
```

Add one table test covering all ten Appendix A.1 internal-code/display-name/wire-value triples and assert `GdSource::DEFAULT == GdSource::NeteaseMusic`; source strings must never enter a request constructor directly.

Run:

```powershell
cargo test --manifest-path src-tauri/Cargo.toml --test gd_contract renders_ -- --nocapture
```

Expected: FAIL because `music::contract` does not exist.

- [ ] **Step 2: Implement the encoded type boundary and body renderer**

Create `src-tauri/src/music/mod.rs` with `pub mod contract;`, export `pub mod music;` from `lib.rs`, and implement this public surface in `contract.rs`:

```rust
use std::{collections::HashSet, fmt::Write};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EncodedComponent(Box<str>);

impl EncodedComponent {
    pub fn encode(raw: &str) -> Self {
        let mut output = String::with_capacity(raw.len());
        for byte in raw.as_bytes() {
            if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
                output.push(char::from(*byte));
            } else {
                write!(&mut output, "%{byte:02X}").expect("writing to String cannot fail");
            }
        }
        Self(output.into_boxed_str())
    }

    pub fn as_str(&self) -> &str { &self.0 }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SignatureValue(Box<str>);

impl TryFrom<&str> for SignatureValue {
    type Error = ContractError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        if value.is_empty() || value.len() > 128 || value.bytes().any(|b| b.is_ascii_control() || matches!(b, b'&' | b'=')) {
            return Err(ContractError::InvalidSignature);
        }
        Ok(Self(value.into()))
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SearchOperation { Track, Album, Playlist }

impl SearchOperation {
    const fn wire_type(self) -> &'static str {
        match self { Self::Track => "search", Self::Album => "search_album", Self::Playlist => "search_playlist" }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GdSource {
    NeteaseMusic, QqMusic, KuwoMusic, Tidal, Qobuz,
    Joox, BilibiliMusic, AppleMusic, YoutubeMusic, Spotify,
}

impl GdSource {
    pub const DEFAULT: Self = Self::NeteaseMusic;

    pub const fn internal_code(self) -> &'static str {
        match self {
            Self::NeteaseMusic => "netease_music", Self::QqMusic => "qq_music",
            Self::KuwoMusic => "kuwo_music", Self::Tidal => "tidal",
            Self::Qobuz => "qobuz", Self::Joox => "joox",
            Self::BilibiliMusic => "bilibili_music", Self::AppleMusic => "apple_music",
            Self::YoutubeMusic => "youtube_music", Self::Spotify => "spotify",
        }
    }

    pub const fn display_name(self) -> &'static str {
        match self {
            Self::NeteaseMusic => "网易云音乐", Self::QqMusic => "QQ 音乐",
            Self::KuwoMusic => "酷我音乐", Self::Tidal => "TIDAL",
            Self::Qobuz => "Qobuz", Self::Joox => "JOOX",
            Self::BilibiliMusic => "哔哩哔哩", Self::AppleMusic => "Apple Music",
            Self::YoutubeMusic => "YouTube Music", Self::Spotify => "Spotify",
        }
    }

    pub const fn wire_value(self) -> &'static str {
        match self {
            Self::NeteaseMusic => "netease", Self::QqMusic => "tencent",
            Self::KuwoMusic => "kuwo", Self::Tidal => "tidal",
            Self::Qobuz => "qobuz", Self::Joox => "joox",
            Self::BilibiliMusic => "bilibili", Self::AppleMusic => "apple",
            Self::YoutubeMusic => "ytmusic", Self::Spotify => "spotify",
        }
    }
}

#[derive(Clone, Debug)]
pub enum GdOperation {
    Search { operation: SearchOperation, count: u16, source: GdSource, page: u16, name: EncodedComponent },
    Url { id: EncodedComponent, source: GdSource, bitrate: u16 },
    Pic { id: EncodedComponent, source: GdSource, size: u16 },
    Lyric { id: EncodedComponent, source: GdSource },
}

impl GdOperation {
    pub fn signature_input(&self) -> &EncodedComponent {
        match self {
            Self::Search { name, .. } => name,
            Self::Url { id, .. } | Self::Pic { id, .. } | Self::Lyric { id, .. } => id,
        }
    }
}

pub fn render_form_body(operation: &GdOperation, signature: &SignatureValue) -> String {
    let s = &signature.0;
    match operation {
        GdOperation::Search { operation, count, source, page, name } => format!(
            "types={}&count={count}&source={}&pages={page}&name={}&s={s}",
            operation.wire_type(), source.wire_value(), name.as_str(),
        ),
        GdOperation::Url { id, source, bitrate } => format!("types=url&id={}&source={}&br={bitrate}&s={s}", id.as_str(), source.wire_value()),
        GdOperation::Pic { id, source, size } => format!("types=pic&id={}&source={}&size={size}&s={s}", id.as_str(), source.wire_value()),
        GdOperation::Lyric { id, source } => format!("types=lyric&id={}&source={}&s={s}", id.as_str(), source.wire_value()),
    }
}

#[derive(Debug, Error)]
pub enum ContractError {
    #[error("invalid signature")]
    InvalidSignature,
    #[error("top-level response is not an array")]
    InvalidTopLevel,
    #[error("non-empty response contained no valid songs")]
    NoValidSongs,
    #[error("upstream returned an explicit error")]
    UpstreamFailure,
    #[error("response URL is invalid or not HTTPS")]
    InvalidUrl,
}
```

Run the two body tests. Expected: PASS.

- [ ] **Step 3: Write failing response-normalization tests**

Append tests that assert:

```rust
use yinmi_lib::music::contract::{parse_search_page, ContractError};

#[test]
fn normalizes_mixed_records_and_skips_bad_rows() {
    let report = parse_search_page(include_bytes!("fixtures/gd/search_mixed.json")).unwrap();
    assert_eq!(report.raw_records, 4);
    assert_eq!(report.songs.len(), 2);
    assert_eq!(report.skipped_records, 2);
    assert_eq!(report.songs[0].duration_ms, Some(123_000));
    assert!(report.songs[0].has_hires);
    assert_eq!(report.songs[1].id, "9007199254740991");
    assert_eq!(report.songs[1].artists, ["歌手乙", "歌手丙"]);
    assert_eq!(report.songs[1].duration_ms, Some(10_000));
    assert!(!report.songs[1].has_hires);
}

#[test]
fn distinguishes_empty_from_incompatible() {
    assert!(parse_search_page(include_bytes!("fixtures/gd/search_empty.json")).unwrap().songs.is_empty());
    assert!(matches!(parse_search_page(include_bytes!("fixtures/gd/search_incompatible.json")), Err(ContractError::InvalidTopLevel)));
}
```

The mixed fixture's fourth row is part of the assertion: because its `artist` array contains a non-string member, the entire row is invalid. Do not filter the bad member and retain the row.

Run these tests. Expected: FAIL because `parse_search_page` is absent.

- [ ] **Step 4: Implement the minimum normalized protocol model**

Add these exact types and parsing rules to `contract.rs`:

```rust
#[derive(Clone, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
pub struct ProbeSongKey { pub source: String, pub id: String }

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProbeSong {
    pub id: String,
    pub name: String,
    pub artists: Vec<String>,
    pub artist_display: String,
    pub album: Option<String>,
    pub source: String,
    pub url_id: Option<String>,
    pub pic_id: Option<String>,
    pub lyric_id: Option<String>,
    pub duration_ms: Option<u64>,
    pub has_hires: bool,
}

#[derive(Clone, Debug)]
pub struct ParsedSearchPage {
    pub raw_records: usize,
    pub skipped_records: usize,
    pub songs: Vec<ProbeSong>,
}

fn wire_string(value: &Value) -> Option<String> {
    match value {
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => n.as_u64().filter(|n| *n <= 9_007_199_254_740_991).map(|n| n.to_string()),
        _ => None,
    }
}

fn artists(value: &Value) -> Option<Vec<String>> {
    match value {
        Value::String(s) if !s.is_empty() => Some(vec![s.clone()]),
        Value::Array(items) if !items.is_empty() => {
            items
                .iter()
                .map(|item| item.as_str().filter(|s| !s.is_empty()).map(str::to_owned))
                .collect::<Option<Vec<_>>>()
        }
        _ => None,
    }
}

fn duration_ms(value: Option<&Value>) -> Option<u64> {
    let seconds = match value? { Value::Number(n) => n.as_u64(), Value::String(s) => s.parse().ok(), _ => None }?;
    seconds.checked_mul(1_000)
}

pub fn parse_search_page(bytes: &[u8]) -> Result<ParsedSearchPage, ContractError> {
    let value: Value = serde_json::from_slice(bytes).map_err(|_| ContractError::InvalidTopLevel)?;
    let rows = value.as_array().ok_or(ContractError::InvalidTopLevel)?;
    let mut songs = Vec::new();
    for row in rows {
        let Some(object) = row.as_object() else { continue };
        let (Some(id), Some(name), Some(source), Some(artists)) = (
            object.get("id").and_then(wire_string),
            object.get("name").and_then(Value::as_str).map(str::to_owned),
            object.get("source").and_then(Value::as_str).map(str::to_owned),
            object.get("artist").and_then(artists),
        ) else { continue };
        let album = match object.get("album") { Some(Value::String(s)) => Some(s.clone()), Some(Value::Null) | None => None, _ => continue };
        let extra = object.get("extra_data").and_then(Value::as_object);
        songs.push(ProbeSong {
            id,
            name,
            artist_display: artists.join(", "),
            artists,
            album,
            source,
            url_id: object.get("url_id").and_then(wire_string),
            pic_id: object.get("pic_id").and_then(wire_string),
            lyric_id: object.get("lyric_id").and_then(wire_string),
            duration_ms: duration_ms(extra.and_then(|e| e.get("duration"))),
            has_hires: extra.and_then(|e| e.get("has_hires")).and_then(Value::as_bool).unwrap_or(false),
        });
    }
    if !rows.is_empty() && songs.is_empty() { return Err(ContractError::NoValidSongs); }
    Ok(ParsedSearchPage { raw_records: rows.len(), skipped_records: rows.len() - songs.len(), songs })
}
```

Run all `gd_contract` tests. Expected: PASS.

- [ ] **Step 5: Write and implement URL, picture, and lyric response contracts**

Write failing fixture tests, then implement this exact surface:

```rust
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AudioUnavailableReason {
    EmptyUrl,
    LowerBitrate { requested: u32, reported: u32 },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AudioLocation {
    pub url: url::Url,
    pub reported_bitrate: Option<u32>,
    pub size_bytes: Option<u64>,
    pub source: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AudioAvailability {
    Available(AudioLocation),
    Unavailable(AudioUnavailableReason),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PictureLocation { pub url: url::Url }

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LyricPayload {
    pub original: Option<String>,
    pub translated: Option<String>,
}

impl LyricPayload {
    pub fn original_to_write(&self) -> Option<&str> { self.original.as_deref() }
}

pub fn parse_audio_response(bytes: &[u8], requested_bitrate: u32) -> Result<AudioAvailability, ContractError>;
pub fn parse_picture_response(bytes: &[u8]) -> Result<PictureLocation, ContractError>;
pub fn parse_lyric_response(bytes: &[u8]) -> Result<LyricPayload, ContractError>;
```

The tests must prove: a normal URL is available; empty URL is unavailable; explicitly lower `br` is unavailable; missing `br` remains available; picture URL parses; nonempty original lyric is writable without merging `tlyric`; empty original creates no attachment; and `explicit_error.json` maps to `UpstreamFailure` without exposing its message. Audio and picture URLs must be absolute HTTPS URLs with no credentials. A malformed field or unrecognized top-level shape is `InvalidTopLevel`.

Run the response tests first to see them fail, implement the minimum parsers, then rerun all `gd_contract` tests. Expected: PASS.

- [ ] **Step 6: Write and implement the deterministic pagination state tests**

Define and test this exact surface:

```rust
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum StopReason { TargetReached, RawEmptyPage, ExplicitNoMore, NoNewSongs, SafetyPageLimit, FirstPageFailed, LaterPageFailed }

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PaginationDecision { Continue { next_page: u16 }, Complete { reason: StopReason, incomplete: bool }, Failed { reason: StopReason } }

pub struct PaginationProbe {
    pub target_unique: usize,
    pub max_pages: u16,
    next_page: u16,
    seen: HashSet<ProbeSongKey>,
    pub songs: Vec<ProbeSong>,
}

impl PaginationProbe {
    pub fn new(target_unique: usize, max_pages: u16) -> Self;
    pub fn push_page(&mut self, page: Result<(ParsedSearchPage, bool), ContractError>) -> PaginationDecision;
}
```

Tests must independently cover all seven `StopReason` values, first-record-wins by `(source,id)`, a generated 1000-unique-song sequence that stops exactly at 1000, and normal exhaustion below target as `incomplete=false`. Implement the transitions exactly as the design’s §5.2 table; do not add automatic retries.

Run:

```powershell
cargo test --manifest-path src-tauri/Cargo.toml --test gd_contract -- --nocapture
cargo fmt --manifest-path src-tauri/Cargo.toml --check
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
```

Expected: all contract and pagination tests pass with zero warnings.

- [ ] **Step 7: Commit the deterministic contract**

```powershell
git add src-tauri/src/music src-tauri/src/lib.rs src-tauri/Cargo.toml src-tauri/Cargo.lock src-tauri/tests/gd_contract.rs src-tauri/tests/fixtures/gd
git commit -m "test: pin GD protocol and pagination contracts"
```

### Task 4: 验证零能力签名 WebView 与真实分页

**Files:**
- Create: `src-tauri/src/feasibility/mod.rs`
- Create: `src-tauri/src/feasibility/signature_webview.rs`
- Create: `src-tauri/src/feasibility/webview_resource_policy.rs`
- Create: `src-tauri/src/feasibility/gd_live.rs`
- Modify: `src-tauri/src/lib.rs`
- Modify: `src-tauri/Cargo.toml`
- Modify: `src-tauri/Cargo.lock`
- Modify: `src-tauri/build.rs`
- Modify: `src-tauri/tauri.conf.json`
- Create: `src-tauri/tauri.feasibility.conf.json`
- Create: `src-tauri/permissions/feasibility.toml`
- Create: `src-tauri/capabilities/feasibility-main.json`
- Modify: `vite.config.ts`
- Modify: `src/vite-env.d.ts`
- Modify: `scripts/verify-config.mjs`
- Create: `scripts/verify-default-artifacts.mjs`
- Modify: `package.json`
- Create: `src/lib/feasibility/FeasibilityPanel.svelte`
- Create: `src/lib/feasibility/GdProbe.svelte`
- Modify: `src/App.svelte`
- Modify: `src/App.test.ts`
- Create: `docs/feasibility/gd-contract-pagination.md`
- Create: `docs/feasibility/gd-contract-pagination.json`
- Create: `docs/feasibility/signature-webview.md`
- Create: `docs/feasibility/signature-webview.json`
- Create: `docs/decisions/0001-gd-pagination.md`
- Create: `docs/decisions/0002-signature-webview.md`
- Modify: `docs/feasibility/evidence-scopes.json`

**Interfaces:**
- Consumes: Task 3 `EncodedComponent`, `GdOperation`, `PaginationProbe`, `render_form_body`.
- Produces: `SignatureRuntime::initialize/sign/destroy/retry`, `run_gd_probe`, `SignatureInitReport`, `IsolationReport`, and `ProtocolProbeReport`; no production search command.

- [ ] **Step 1: Add feature isolation and write failing origin/eval tests**

Add dependencies and the feature:

```toml
futures-util = "0.3"
reqwest = { version = "0.13.4", default-features = false, features = ["rustls-tls", "stream"] }
sha2 = "0.10"
time = { version = "0.3", features = ["formatting"] }
tokio = { version = "1", features = ["macros", "rt-multi-thread", "sync", "time"] }
tokio-util = { version = "0.7", features = ["rt"] }

[target.'cfg(windows)'.dependencies]
webview2-com = "0.38"

[target.'cfg(target_os = "macos")'.dependencies]
objc2 = "0.6"
objc2-foundation = "0.3"
objc2-web-kit = { version = "0.3", features = ["WKContentRuleList", "WKContentRuleListStore", "WKUserContentController", "WKWebView", "WKWebViewConfiguration"] }

[features]
default = ["custom-protocol"]
custom-protocol = ["tauri/custom-protocol"]
feasibility = []
```

Write unit tests for `is_allowed_gd_navigation` and `validate_signature_result` before implementing them. Required cases:

```text
ALLOW https://music.gdstudio.xyz/
ALLOW https://music.gdstudio.xyz/js/player.js?v=20260616
DENY  http://music.gdstudio.xyz/
DENY  https://evil.example/
DENY  https://music.gdstudio.xyz.evil.example/
DENY  https://user:pass@music.gdstudio.xyz/
DENY  https://music.gdstudio.xyz:444/
```

Signature validation accepts 1–128 UTF-8 bytes and rejects control characters, `&`, `=`, empty and 129-byte values. Add pure resource-policy tests that allow network requests only when the URL is HTTPS, has no credentials, uses effective port 443 and has host exactly `music.gdstudio.xyz`; deny lookalike hosts, every other scheme/port, `http://ipc.localhost`, loopback canaries and cross-origin subresources.

Run:

```powershell
cargo test --manifest-path src-tauri/Cargo.toml --features feasibility signature_webview -- --nocapture
```

Expected: FAIL because `feasibility::signature_webview` is absent.

- [ ] **Step 2: Implement the bounded Rust-owned signature runtime**

Implement this exact public surface:

```rust
pub const GD_PAGE_URL: &str = "https://music.gdstudio.xyz/";
pub const SIGNATURE_WEBVIEW_LABEL: &str = "gd-signature-feasibility";
pub const INIT_TIMEOUT: Duration = Duration::from_secs(20);
pub const CALL_TIMEOUT: Duration = Duration::from_secs(5);
pub const MAX_SIGNATURE_BYTES: usize = 128;

#[derive(Clone, Debug, Serialize)]
pub struct SignatureInitReport {
    pub generation: u64,
    pub ready_in_ms: u64,
    pub final_url: String,
    pub hidden: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct IsolationReport {
    pub current_url: String,
    pub webview_runtime_version: String,
    pub resource_filter_mode: String,
    pub has_tauri_internals: bool,
    pub has_tauri_global: bool,
    pub has_tauri_ipc: bool,
    pub has_window_ipc: bool,
    pub canary_calls_from_hidden_page: u64,
    pub blocked_navigations: u64,
    pub blocked_new_windows: u64,
    pub blocked_downloads: u64,
    pub blocked_resource_requests: u64,
    pub resource_canary_hits: u64,
    pub observed_network_origins: Vec<String>,
    pub extra_window_labels: Vec<String>,
    pub passed: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum SignatureError {
    #[error("signature runtime timed out")]
    Timeout,
    #[error("signature page origin was rejected")]
    OriginRejected,
    #[error("official signing function is unavailable")]
    MissingFunction,
    #[error("official signing function returned invalid data: {0}")]
    InvalidReturn(&'static str),
    #[error("signature JavaScript evaluation failed")]
    Evaluation,
    #[error("signature WebView failed: {0}")]
    Webview(String),
}

pub struct SignatureInstance<R: tauri::Runtime> {
    webview: tauri::WebviewWindow<R>,
    resource_policy: ResourcePolicyGuard,
}

pub struct SignatureRuntime<R: tauri::Runtime> {
    app: tauri::AppHandle<R>,
    generation: AtomicU64,
    instance: tokio::sync::Mutex<Option<SignatureInstance<R>>>,
}

impl<R: tauri::Runtime> SignatureRuntime<R> {
    pub fn new(app: tauri::AppHandle<R>) -> Self;
    pub async fn initialize(&self) -> Result<SignatureInitReport, SignatureError>;
    pub async fn sign(&self, input: &EncodedComponent) -> Result<SignatureValue, SignatureError>;
    pub async fn run_isolation_probe(&self) -> Result<IsolationReport, SignatureError>;
    pub async fn destroy(&self) -> Result<(), SignatureError>;
    pub async fn retry(&self) -> Result<SignatureInitReport, SignatureError>;
}

pub async fn eval_json<R: tauri::Runtime, T: serde::de::DeserializeOwned>(
    webview: &tauri::WebviewWindow<R>,
    script: String,
    timeout: Duration,
) -> Result<T, SignatureError>;

pub struct ResourcePolicyGuard { /* owns native callback/rule-list lifetimes */ }

pub async fn build_hardened_webview<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    label: &str,
) -> Result<(tauri::WebviewWindow<R>, ResourcePolicyGuard), SignatureError>;
```

Create the hidden WebView on browser-internal `about:blank`, which performs no network request. Install the resource policy before navigating to the official page; there must be no race in which the official document or any subresource can load first. Build the window only with:

```rust
tauri::WebviewWindowBuilder::new(
    &self.app,
    SIGNATURE_WEBVIEW_LABEL,
    tauri::WebviewUrl::External(url::Url::parse("about:blank").expect("constant bootstrap URL must parse")),
)
.visible(false)
.focused(false)
.skip_taskbar(true)
.incognito(true)
.devtools(false)
.on_navigation(is_allowed_gd_navigation)
.on_new_window(|_, _| tauri::webview::NewWindowResponse::Deny)
.on_download(|_, _| false)
```

The navigation handler permits `about:blank` only as the initial internal bootstrap and otherwise applies the exact official-origin predicate. `build_hardened_webview` retains its guard inside `SignatureInstance` and navigates to `Url::parse(GD_PAGE_URL).expect("constant GD page URL must parse")` only after platform enforcement is active.

`WebviewWindowBuilder::on_web_resource_request` is not sufficient because Tauri documents that it does not run for external URLs. The policy implementation is therefore platform-native and must be installed before the first network navigation:

```text
Windows/WebView2: build hidden about:blank -> with_webview -> query runtime/interface version -> install the strongest available native filter -> navigate.
When `ICoreWebView2_22` exists, call `AddWebResourceRequestedFilterWithRequestSourceKinds("*", COREWEBVIEW2_WEB_RESOURCE_CONTEXT_ALL, COREWEBVIEW2_WEB_RESOURCE_REQUEST_SOURCE_KINDS_ALL)` and handle `WebResourceRequested`. On the supported 111 baseline, use the legacy two-argument `ICoreWebView2::AddWebResourceRequestedFilter("*", COREWEBVIEW2_WEB_RESOURCE_CONTEXT_ALL)` only as a compatibility candidate; never label it equivalent by assumption. Every raised URL is checked, and a disallowed URL receives a synthetic empty 403 response and increments the counter.

macOS/WKWebView: compile a WKContentRuleList before construction -> attach it to the WKUserContentController through the target-gated `with_webview_configuration` builder extension -> build hidden about:blank -> navigate;
the rule blocks every network URL unless the domain is exactly music.gdstudio.xyz. Keep the compiled rule list for the WebView lifetime.
```

If either native control cannot be installed before the first network request on a supported platform, the result is not `pass`. In particular, the Windows 111 row must prove that the legacy filter catches every adversarial request source; if a nested-frame request bypasses it, stop with `design-change-required` and amend the design mechanism or minimum runtime instead of silently raising the runtime floor. Do not replace request-level enforcement with post-load `performance` inspection; observations are additional evidence only.

`eval_json` must wrap `eval_with_callback` in a Tokio oneshot and `tokio::time::timeout`; ignore callbacks arriving after timeout. Windows does not propagate JavaScript exceptions, so sign with this self-catching script, after replacing `ENCODED_INPUT_JSON` using `serde_json::to_string(input.as_str())`:

```js
(() => {
  try {
    const fn = globalThis.crc32;
    if (typeof fn !== 'function') return { status: 'error', code: 'MISSING_FUNCTION' };
    const value = fn(ENCODED_INPUT_JSON);
    if (typeof value !== 'string') return { status: 'error', code: 'INVALID_TYPE' };
    if (value.length === 0) return { status: 'error', code: 'EMPTY_VALUE' };
    if (new TextEncoder().encode(value).byteLength > 128) return { status: 'error', code: 'RETURN_TOO_LARGE' };
    return { status: 'ok', value };
  } catch (_) {
    return { status: 'error', code: 'CALL_THROWN' };
  }
})()
```

Rust must repeat byte/control/`&`/`=` validation. Any init or call failure invalidates and destroys the instance; `retry()` destroys before creating a new generation. Never implement CRC32 locally.

`initialize()` polls only the fixed self-catching readiness expression for `globalThis.crc32` at 100 ms intervals inside the single 20-second deadline. It succeeds only after the current URL still passes `is_allowed_gd_navigation`; a page-load event alone is not readiness.

- [ ] **Step 3: Lock custom commands to the local main window**

Replace `build.rs` with a feature-gated ACL manifest. The default build generates no feasibility command identifiers:

```rust
fn main() {
    let commands: &[&str] = if std::env::var_os("CARGO_FEATURE_FEASIBILITY").is_some() {
        &[
            "feasibility_signature_initialize",
            "feasibility_signature_sign",
            "feasibility_signature_destroy",
            "feasibility_signature_isolation",
            "feasibility_run_gd_probe",
            "feasibility_ipc_canary",
        ]
    } else {
        &[]
    };

    tauri_build::try_build(
        tauri_build::Attributes::new().app_manifest(
            tauri_build::AppManifest::new().commands(commands),
        ),
    )
    .expect("failed to build Tauri ACL manifest");
}
```

Create `src-tauri/permissions/feasibility.toml`:

```toml
[[permission]]
identifier = "allow-feasibility-commands"
description = "Allows phase-one probes from the local main window only"
commands.allow = [
  "feasibility_signature_initialize",
  "feasibility_signature_sign",
  "feasibility_signature_destroy",
  "feasibility_signature_isolation",
  "feasibility_run_gd_probe",
  "feasibility_ipc_canary",
]
```

Create `src-tauri/capabilities/feasibility-main.json`:

```json
{
  "$schema": "../gen/schemas/desktop-schema.json",
  "identifier": "feasibility-main",
  "description": "Phase-one probes exposed only to the local main window",
  "local": true,
  "windows": ["main"],
  "permissions": ["core:default", "allow-feasibility-commands"]
}
```

Keep the default config on `["main-capability"]`. Create this overlay as `src-tauri/tauri.feasibility.conf.json`:

```json
{
  "$schema": "https://schema.tauri.app/config/2",
  "build": {
    "beforeDevCommand": "pnpm vite --mode feasibility",
    "beforeBuildCommand": "pnpm vite build --mode feasibility"
  },
  "app": {
    "security": {
      "capabilities": ["feasibility-main"]
    }
  }
}
```

Replace `vite.config.ts` with:

```ts
import tailwindcss from '@tailwindcss/vite';
import { svelte } from '@sveltejs/vite-plugin-svelte';
import { defineConfig } from 'vitest/config';

export default defineConfig(({ mode }) => ({
  plugins: [tailwindcss(), svelte()],
  define: { __FEASIBILITY__: JSON.stringify(mode === 'feasibility') },
  clearScreen: false,
  server: { port: 1420, strictPort: true },
  build: { target: ['chrome111', 'safari16.4'] },
  test: {
    environment: 'jsdom',
    setupFiles: ['src/test/setup.ts'],
    include: ['src/**/*.test.ts'],
  },
}));
```

Declare `const __FEASIBILITY__: boolean` in `src/vite-env.d.ts`. In `App.svelte`, call `import('./lib/feasibility/FeasibilityPanel.svelte')` only inside `if (__FEASIBILITY__)`; render the resolved component below the product shell. The normal-mode test asserts the feasibility panel is absent, while a feasibility-mode test imports the panel directly. Every Rust feasibility module, command registration, managed state, and updater plugin registration must be below `#[cfg(feature = "feasibility")]`.

Neither config may contain `remote`, `urls`, a wildcard window, or the hidden WebView label. Extend `scripts/verify-config.mjs` to parse both configs and every capability, assert the two exact capability lists, and fail if `gd-signature-feasibility`, `remote`, `urls`, or `"*"` appears. Add an IPC canary command with an atomic counter; the main window must increment it. From the hidden page, enumerate `__TAURI_INTERNALS__`, `__TAURI__`, `__TAURI_IPC__`, `__TAURI_INVOKE__`, `window.ipc` and any own property containing `tauri` or `ipc`, then attempt the fixed canary through every discovered callable path. Passing requires every named bridge probe to be absent and the Rust counter to remain zero. “Bridge exists but ACL rejects the command” is a failure because the design requires no IPC injection, not merely zero permission.

Run a local resource canary server on an OS-assigned loopback port. Controlled eval creates direct `fetch` and image requests, a same-page `srcdoc` iframe that creates its own fetch/image requests, a nested `srcdoc` iframe that repeats them, and an HTTPS cross-origin iframe/navigation request. Passing requires zero server hits from every depth, blocked/error results in the page, and every `PerformanceResourceTiming` network origin to equal `https://music.gdstudio.xyz`. This observation supplements the native enforcement; it cannot replace it.

Add package script `"verify:default-artifacts": "node scripts/verify-default-artifacts.mjs"`. The script requires a completed default frontend and debug Tauri build, recursively reads `dist` plus `src-tauri/target/debug/yinmi.exe` on Windows or `src-tauri/target/debug/yinmi` on macOS, and rejects these UTF-8 byte strings: `FeasibilityPanel`, `GdProbe`, `gd-signature-feasibility`, the generic command prefix `feasibility_`, and the environment prefix `YINMI_FEASIBILITY_`. Missing artifacts also fail.

- [ ] **Step 4: Write the failing fixed-case live probe tests**

Define exactly three probe cases and reports:

```rust
#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProtocolProbeCase { SingleCount1000, PagedOfficial20, RepeatSamePage }

pub const PROBE_KEYWORD: &str = "周杰伦";

#[derive(Clone, Debug, Serialize)]
pub struct ProtocolProbeReport {
    pub started_at: String,
    pub page_version: &'static str,
    pub probe_case: ProtocolProbeCase,
    pub requested_target: usize,
    pub upstream_count: u16,
    pub pages_requested: u16,
    pub raw_records: usize,
    pub valid_records: usize,
    pub unique_records: usize,
    pub duplicate_records: usize,
    pub invalid_records: usize,
    pub stop_reason: StopReason,
    pub incomplete: bool,
    pub elapsed_ms: u64,
    pub response_digests: Vec<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum GdProbeError {
    #[error(transparent)]
    Signature(#[from] SignatureError),
    #[error(transparent)]
    Contract(#[from] ContractError),
    #[error("bounded network request failed")]
    Network,
    #[error("upstream rate limited the probe")]
    RateLimited { retry_after_seconds: Option<u64> },
    #[error("upstream returned a non-success HTTP status")]
    HttpStatus,
    #[error("probe was cancelled")]
    Cancelled,
}

pub async fn run_gd_probe<R: tauri::Runtime>(
    runtime: &SignatureRuntime<R>,
    probe_case: ProtocolProbeCase,
    cancel: &tokio_util::sync::CancellationToken,
) -> Result<ProtocolProbeReport, GdProbeError>;
```

Tests must reject arbitrary URLs, keywords, sources, counts and page limits. The only keyword is `周杰伦` and the only source is `netease`; the three cases resolve to:

```text
single_count_1000: count=1000, page=1, one API request
paged_official_20: count=20, pages=1..=50, >=6500 ms between API requests
repeat_same_page: count=20, page=1 twice, >=6500 ms between requests
```

Use a private transport seam to add deterministic HTTP-boundary tests: `429` with numeric/date/missing `Retry-After` maps to `RateLimited` without retaining its body; another non-2xx status maps to `HttpStatus`; and a streamed response of exactly `5 MiB + 1 byte` aborts before the extra byte reaches the parser. Error `Display`/debug reports and frontend payloads must not contain the fixture body. These tests run locally and do not call GD.

`started_at` is UTC RFC 3339 with second precision, `page_version` is the literal `2026.06.16`, and every digest is lowercase SHA-256 hex.

Expected before implementation: tests fail because `run_gd_probe` is absent.

- [ ] **Step 5: Implement the live probe without creating product search state**

Use the exact API URL, get each signature from `SignatureRuntime`, send the pre-rendered body as bytes with `Content-Type: application/x-www-form-urlencoded`, apply Task 3 parsing/pagination, and SHA-256 each raw page. Build the temporary client with redirects disabled, system proxy disabled, a 10-second connect timeout, a 30-second total request timeout, and a streamed 5 MiB response limit. A monotonic rate limiter enforces at least 6500 ms between API request starts, including the repeated-page case; cancellation during the wait or body stream stops without retry. Task 5 must replace this fixed-host client with `fetch_checked` before any probe code can be promoted.

Reject every non-2xx response before parsing. A probe does not automatically retry 429; it reports the parsed bounded `Retry-After`, remains cancellable, and the operator starts the next fixed observation only in a fresh quota window. Map explicit upstream errors and incompatible bodies to stable report codes; neither the frontend report nor ordinary logs may include a raw response body, raw upstream error text, signature, or full request body.

Write raw pages only below `artifacts/feasibility/gd/raw/`; committed reports contain counts, stop reason and digests, never signatures or full song rows.

Do not run the live probe in unit tests or CI. The UI exposes only three fixed buttons plus copy-report; it has no arbitrary keyword, endpoint, source, count or download action.

- [ ] **Step 6: Commit the probe implementation, then verify Windows and macOS WebView isolation**

Review and finalize the planned `gd-contract-pagination` and `signature-webview` entries in `evidence-scopes.json` so their exact arrays cover Tasks 3–4 contract/fixture/probe/WebView/config/ACL/frontend files and every shared Cargo/package/lock/build input that affects them. Then format, run deterministic/default-artifact checks, and commit all code before collecting platform evidence:

```powershell
pnpm format
cargo fmt --manifest-path src-tauri/Cargo.toml --all
cargo test --manifest-path src-tauri/Cargo.toml --features feasibility signature_webview -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml --features feasibility gd_live -- --nocapture
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets --features feasibility -- -D warnings
pnpm quality
pnpm tauri build --debug --no-bundle
pnpm verify:default-artifacts
git add src-tauri/src/feasibility src-tauri/src/lib.rs src-tauri/build.rs src-tauri/permissions/feasibility.toml src-tauri/capabilities/feasibility-main.json src-tauri/Cargo.toml src-tauri/Cargo.lock src-tauri/tauri.conf.json src-tauri/tauri.feasibility.conf.json src/lib/feasibility src/App.svelte src/App.test.ts src/vite-env.d.ts vite.config.ts package.json pnpm-lock.yaml scripts/verify-config.mjs scripts/verify-default-artifacts.mjs docs/feasibility/evidence-scopes.json
git commit -m "feat: add isolated GD signature probe"
git status --short
```

Expected: status is clean. Every platform observation below must name this exact commit and use unchanged scoped files.

Run on a controlled Windows 10 22H2 x64 VM configured through `WEBVIEW2_BROWSER_EXECUTABLE_FOLDER` to use the lowest available WebView2 Fixed Version 111.0.1661.x build outside the repository, Windows 11 x64 with current Evergreen runtime, macOS 13.3 Intel, and current Apple Silicon. Record the exact runtime version and selected native filter mode, never the local fixed-runtime path; do not infer the minimum-runtime result from the current Evergreen row:

```powershell
pnpm tauri dev --config src-tauri/tauri.feasibility.conf.json --features feasibility
```

For each platform, record all of the following in `docs/feasibility/signature-webview.md`:

```text
hidden/no taskbar flash; ready <=20s; official final URL; sign <=5s; 1..128 bytes;
cross-origin navigation blocked; window.open denied; download denied;
native resource policy installed before first network navigation; exact runtime/filter mode recorded;
direct and nested-frame cross-origin fetch/image canaries produce zero server hits; observed network origins are official-only;
main canary executes; every Tauri/IPC bridge probe is absent; hidden canary count remains 0;
20 destroy/retry cycles leave no extra window; 10-minute idle and sleep/wake either work or rebuild boundedly.
```

Any missing platform row makes this task `blocked`, not `pass`. Any detected IPC bridge, direct or nested cross-origin resource hit, resource-policy installation race, or inability to enforce the whitelist on the WebView2 111 baseline or another supported platform makes the task `design-change-required` until a replacement no-IPC WebView mechanism is implemented and the full matrix is rerun; ACL denial alone is never sufficient. Save the four platform rows and check fields to ignored `artifacts/feasibility/signature-webview.raw.json`.

- [ ] **Step 7: Run the three live pagination observations in separate quota windows**

Run `single_count_1000`, wait for a fresh 5-minute window, run `paged_official_20`, wait again, then run `repeat_same_page`. Record page counts, unique counts, duplicates, invalid rows, stop reason and digests in `docs/feasibility/gd-contract-pagination.md`.

ADR `0001-gd-pagination.md` must select the observed upstream count and a numeric safety page limit no greater than 50. ADR `0002-signature-webview.md` must select either healthy reuse or “health-check then destroy/rebuild”; both must keep zero capability and 20/5-second bounds. Save all three live-case fields plus deterministic HTTP-boundary checks to ignored `artifacts/feasibility/gd-contract-pagination.raw.json`.

- [ ] **Step 8: Build and commit the validated evidence companions**

```powershell
cargo test --manifest-path src-tauri/Cargo.toml --features feasibility signature_webview -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml --features feasibility gd_live -- --nocapture
node scripts/feasibility-evidence.mjs build --input artifacts/feasibility/signature-webview.raw.json --markdown docs/feasibility/signature-webview.md --output docs/feasibility/signature-webview.json
node scripts/feasibility-evidence.mjs build --input artifacts/feasibility/gd-contract-pagination.raw.json --markdown docs/feasibility/gd-contract-pagination.md --output docs/feasibility/gd-contract-pagination.json
node scripts/feasibility-evidence.mjs check docs/feasibility/signature-webview.json
node scripts/feasibility-evidence.mjs check docs/feasibility/gd-contract-pagination.json
```

Expected: tests pass; both companions name the clean implementation commit, contain all required fields, and match their current scopes. Then commit only evidence and decisions:

```powershell
git add docs/feasibility/gd-contract-pagination.md docs/feasibility/gd-contract-pagination.json docs/feasibility/signature-webview.md docs/feasibility/signature-webview.json docs/decisions/0001-gd-pagination.md docs/decisions/0002-signature-webview.md
git commit -m "docs: record GD and WebView feasibility evidence"
```

### Task 5: 验证逐跳 DNS 固定与 SSRF 防护

**Files:**
- Create: `src-tauri/src/feasibility/network_policy.rs`
- Create: `src-tauri/tests/network_policy.rs`
- Modify: `src-tauri/src/feasibility/mod.rs`
- Modify: `src-tauri/src/feasibility/gd_live.rs`
- Modify: `src-tauri/Cargo.toml`
- Modify: `src-tauri/Cargo.lock`
- Create: `docs/feasibility/network-policy.md`
- Create: `docs/feasibility/network-policy.json`
- Create: `docs/decisions/0003-network-ssrf-policy.md`
- Modify: `docs/feasibility/gd-contract-pagination.md`
- Modify: `docs/feasibility/gd-contract-pagination.json`
- Modify: `docs/decisions/0001-gd-pagination.md`
- Modify: `docs/feasibility/evidence-scopes.json`

**Interfaces:**
- Consumes: design HTTPS/redirect/time/resource bounds; independent of GD parsing.
- Produces: `classify_public_ip`, `HostResolver`, `resolve_checked`, `fetch_checked`, `ResolvedHop`, and an ADR that Task 4’s probe client must adopt before any code is promoted.

- [ ] **Step 1: Write failing URL and IP classification tests**

Add:

```toml
async-trait = "0.1"
```

Reuse the `reqwest`, `url`, and Tokio dependencies introduced in Task 4, adding Tokio's `io-util` feature.

Write table tests that reject HTTP, URL userinfo, localhost names, and every tested non-global address class. The IPv4 table includes `0/8`, RFC1918, `100.64/10`, loopback, link-local, documentation, `198.18/15`, multicast, `240/4`, and broadcast. The IPv6 table includes unspecified, loopback, IPv4-mapped private addresses, discard-only, documentation, benchmarking, ORCHID, unique-local, link-local, and multicast. Public examples `1.1.1.1`, `8.8.8.8`, and `2606:4700:4700::1111` pass. The implementation is allow-global/fail-closed; an unknown or newly special range may not default to public.

Run:

```powershell
cargo test --manifest-path src-tauri/Cargo.toml --features feasibility --test network_policy classify_ -- --nocapture
```

Expected: FAIL because the policy module is absent.

- [ ] **Step 2: Implement the checked-resolution interface**

Use this exact surface:

```rust
#[async_trait::async_trait]
pub trait HostResolver: Send + Sync {
    async fn resolve(&self, host: &str, port: u16) -> Result<Vec<SocketAddr>, NetGuardError>;
}

pub struct ResolvedHop { pub url: url::Url, pub addrs: Vec<SocketAddr> }

pub struct CheckedRequest {
    pub method: reqwest::Method,
    pub url: url::Url,
    pub headers: reqwest::header::HeaderMap,
    pub body: Vec<u8>,
}

pub struct FetchLimits {
    pub connect_timeout: Duration,
    pub idle_timeout: Duration,
    pub total_timeout: Option<Duration>,
    pub max_redirects: usize,
    pub max_body_bytes: u64,
}

pub struct CheckedResponse {
    pub final_url: url::Url,
    pub status: reqwest::StatusCode,
    pub headers: reqwest::header::HeaderMap,
    pub remote_addr: SocketAddr,
    pub bytes_written: u64,
}

#[derive(Debug, thiserror::Error)]
pub enum NetGuardError {
    #[error("invalid or forbidden URL")]
    InvalidUrl,
    #[error("DNS returned a non-public address")]
    NonPublicAddress,
    #[error("DNS resolution failed")]
    Resolve,
    #[error("redirect policy rejected the response")]
    Redirect,
    #[error("connected peer did not match the pinned DNS set")]
    PeerMismatch,
    #[error("response body exceeded its byte limit")]
    BodyTooLarge,
    #[error("request timed out")]
    Timeout,
    #[error("request was cancelled")]
    Cancelled,
    #[error(transparent)]
    Transport(#[from] reqwest::Error),
    #[error(transparent)]
    Sink(#[from] std::io::Error),
}

pub fn classify_public_ip(ip: IpAddr) -> Result<(), NetGuardError>;
pub async fn resolve_checked<R: HostResolver + ?Sized>(resolver: &R, url: &Url) -> Result<ResolvedHop, NetGuardError>;
pub async fn fetch_checked<R, W>(
    resolver: &R,
    request: CheckedRequest,
    limits: FetchLimits,
    cancel: &tokio_util::sync::CancellationToken,
    sink: &mut W,
) -> Result<CheckedResponse, NetGuardError>
where
    R: HostResolver + ?Sized,
    W: tokio::io::AsyncWrite + Unpin + Send;
```

`resolve_checked` accepts HTTPS only, rejects userinfo and IP literals that fail classification, resolves a hostname exactly once, rejects empty answers, and rejects the entire answer set if any address is non-public. Empty bodies are permitted for GET/HEAD; POST is permitted only when redirects are zero. Apply `total_timeout` around the entire hop loop, the idle timeout around every body chunk, the byte ceiling before every sink write, and cancellation around DNS, connect, headers, reads, and writes.

- [ ] **Step 3: Write failing rebinding and redirect tests**

Implement a scripted fake resolver whose answers are queued and whose call count is observable. Tests must prove:

```text
mixed public/private DNS -> rejected before request
first public/second private answer -> one resolution for the hop; pinned public set used
relative Location -> resolved against current URL and fully revalidated
HTTPS to HTTP redirect -> rejected
redirect to private host -> zero request reaches that host
sixth redirect -> rejected when limit is five
remote_addr missing or outside pinned set -> rejected
cancel during body read -> Cancelled
declared or streamed body over the configured ceiling -> BodyTooLarge before the sink exceeds its limit
```

Expected before implementation: FAIL at the first redirect test.

- [ ] **Step 4: Implement the one-hop-at-a-time client**

For every hop, create a new client with:

```rust
reqwest::Client::builder()
    .https_only(true)
    .no_proxy()
    .redirect(reqwest::redirect::Policy::none())
    .connect_timeout(limits.connect_timeout)
    .resolve_to_addrs(host, &validated_addrs)
    .build()?
```

Keep the original hostname for Host/SNI, inspect `Response::remote_addr()`, require it to belong to the pinned set, and then handle `Location` manually. Do not forward authorization, cookie or proxy headers across hops. Apply the 5-redirect initial limit and an idle-read timeout around each stream chunk.

Reject caller-supplied `Host`, `Authorization`, `Cookie`, `Proxy-Authorization`, `Connection`, and `Transfer-Encoding` headers. Reject an oversized valid `Content-Length` before reading, but still enforce the cumulative byte ceiling while streaming because the header can be absent or false.

Only GET and HEAD may follow redirects; any redirect for POST is `NetGuardError::Redirect`. Resolve and pin each redirected hostname afresh, never reuse the previous hop's client, and wrap the whole loop in `limits.total_timeout` when present. After these tests pass, change Task 4's GD probe to call `fetch_checked` with POST, zero redirects, 10/20/30-second connect/idle/total timeouts, a 5 MiB ceiling, and a memory sink; replace `GdProbeError::Network` with a transparent `NetGuardError` source and rerun both Task 4 and Task 5 suites, including the 429, other non-2xx and `5 MiB + 1` cases.

Create the exact `network-policy` scope entry and extend the GD entry for `network_policy.rs`, its tests, changed `gd_live.rs`, Cargo manifests/lock and shared resolver dependencies. Then format and commit the complete network implementation before collecting native evidence:

```powershell
cargo fmt --manifest-path src-tauri/Cargo.toml --all
cargo test --manifest-path src-tauri/Cargo.toml --features feasibility --test network_policy -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml --features feasibility gd_live -- --nocapture
git add src-tauri/src/feasibility/network_policy.rs src-tauri/src/feasibility/gd_live.rs src-tauri/src/feasibility/mod.rs src-tauri/tests/network_policy.rs src-tauri/Cargo.toml src-tauri/Cargo.lock docs/feasibility/evidence-scopes.json
git commit -m "feat: add pinned DNS media requests"
git status --short
```

Expected: status is clean; all following network and refreshed GD reports name this commit.

- [ ] **Step 5: Run native and ignored public smoke checks**

Run normal tests on Windows and both macOS architectures. Then run one ignored HTTPS smoke that resolves `https://music.gdstudio.xyz/`, prints only scheme/host/address classification, performs GET with a 64 KiB response ceiling and 10/20/30-second connect/idle/total timeouts, and never logs query/body/signature:

```powershell
cargo test --manifest-path src-tauri/Cargo.toml --features feasibility --test network_policy -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml --features feasibility --test network_policy live_https_smoke -- --ignored --nocapture
```

Expected evidence line:

```text
network: scheme=https dns=checked peer=pinned redirects<=5 proxy=disabled
```

Save the required Windows/macOS rows and all deterministic check booleans to ignored `artifacts/feasibility/network-policy.raw.json`. Because this task changed `gd_live.rs`, rerun the three fixed GD observations in fresh quota windows through `fetch_checked`; refresh `artifacts/feasibility/gd-contract-pagination.raw.json`, Markdown and ADR if the result changed. Reusing the Task 4 companion would fail its scope digest.

- [ ] **Step 6: Record the gate and commit**

`docs/decisions/0003-network-ssrf-policy.md` must state: automatic redirects disabled, all DNS answers public, each hop uses a freshly pinned client, actual peer is verified, system proxy is disabled for media, and failure to obtain `remote_addr` is closed. Record native OS/architecture and test output in `docs/feasibility/network-policy.md`.

```powershell
node scripts/feasibility-evidence.mjs build --input artifacts/feasibility/network-policy.raw.json --markdown docs/feasibility/network-policy.md --output docs/feasibility/network-policy.json
node scripts/feasibility-evidence.mjs build --input artifacts/feasibility/gd-contract-pagination.raw.json --markdown docs/feasibility/gd-contract-pagination.md --output docs/feasibility/gd-contract-pagination.json
node scripts/feasibility-evidence.mjs check docs/feasibility/network-policy.json
node scripts/feasibility-evidence.mjs check docs/feasibility/gd-contract-pagination.json
git add docs/feasibility/network-policy.md docs/feasibility/network-policy.json docs/feasibility/gd-contract-pagination.md docs/feasibility/gd-contract-pagination.json docs/decisions/0001-gd-pagination.md docs/decisions/0003-network-ssrf-policy.md
git commit -m "docs: record pinned-network feasibility evidence"
```

### Task 6: 证明 Windows/macOS 原子无覆盖提交

**Files:**
- Create: `src-tauri/src/feasibility/atomic_commit.rs`
- Create: `src-tauri/src/bin/atomic_commit_worker.rs`
- Create: `src-tauri/tests/atomic_commit.rs`
- Modify: `src-tauri/src/feasibility/mod.rs`
- Modify: `src-tauri/Cargo.toml`
- Modify: `src-tauri/Cargo.lock`
- Create: `docs/feasibility/atomic-commit.md`
- Create: `docs/feasibility/atomic-commit.json`
- Create: `docs/decisions/0004-atomic-no-clobber.md`
- Modify: `docs/feasibility/evidence-scopes.json`

**Interfaces:**
- Consumes: same-directory `.part` and no-overwrite invariant.
- Produces: `rename_no_replace_same_dir` and `commit_no_clobber`; the later Milestone 4 download implementation may only reuse them after this gate passes.

- [ ] **Step 1: Write failing same-process semantic tests**

Extend the target dependency tables created in Task 4; do not repeat either target table header:

```toml
# under the existing [target.'cfg(windows)'.dependencies]
windows-sys = { version = "0.61", features = ["Win32_Foundation", "Win32_Security", "Win32_Storage_FileSystem"] }

# under the existing [target.'cfg(target_os = "macos")'.dependencies]
libc = "0.2"

[dev-dependencies]
tempfile = "3"
```

Write tests for:

```rust
#[test] fn commits_when_target_is_absent();
#[test] fn preserves_existing_target_byte_for_byte();
#[test] fn rejects_different_parent_directories_before_syscall();
#[test] fn conflict_cleanup_failure_is_not_reported_as_skipped();
```

Every test uses `tempfile::Builder` to create a unique `.yinmi-`-prefixed `.part` file in the target directory, then flushes and closes it first. The conflict test asserts the original target digest is unchanged and only the current task’s temp is removed.

Run:

```powershell
cargo test --manifest-path src-tauri/Cargo.toml --features feasibility --test atomic_commit -- --nocapture
```

Expected: FAIL because `atomic_commit` is absent.

- [ ] **Step 2: Implement the native no-replace abstraction without fallback**

Use these exact result types:

```rust
#[derive(Debug, thiserror::Error)]
pub enum RenameNoReplaceError {
    #[error("target already exists")]
    Exists,
    #[error("temporary and final paths must have the same existing parent")]
    InvalidLayout,
    #[error("exclusive rename unsupported: {raw_os_error:?}")]
    Unsupported { raw_os_error: Option<i32> },
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

#[derive(Debug, thiserror::Error)]
pub enum CommitError {
    #[error(transparent)]
    Rename(#[from] RenameNoReplaceError),
    #[error("failed to remove conflicting temporary file {path}")]
    Cleanup { path: PathBuf, #[source] source: std::io::Error },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CommitOutcome { Committed, Conflict }

pub fn rename_no_replace_same_dir(temp: &Path, target: &Path) -> Result<(), RenameNoReplaceError>;
pub fn commit_no_clobber(temp: &Path, target: &Path) -> Result<CommitOutcome, CommitError>;
```

Implementation rules are mandatory:

```text
common: canonicalize and compare existing parent directories; source and target must share one parent
common: source file is sync_all'ed and closed before rename
Windows: SetFileInformationByHandle(FileRenameInfo), ReplaceIfExists=FALSE
Windows: open the source with DELETE access and FILE_SHARE_READ|FILE_SHARE_WRITE|FILE_SHARE_DELETE; build the variable-length FILE_RENAME_INFO buffer from the absolute UTF-16 target
Windows: ERROR_FILE_EXISTS/ERROR_ALREADY_EXISTS -> Exists; ERROR_NOT_SUPPORTED/ERROR_INVALID_FUNCTION -> Unsupported
Windows: do not set MOVEFILE_REPLACE_EXISTING or MOVEFILE_COPY_ALLOWED
macOS: renameatx_np(AT_FDCWD, temp_path, AT_FDCWD, target_path, RENAME_EXCL)
macOS: EEXIST -> Exists; ENOTSUP/EOPNOTSUPP/EXDEV -> Unsupported
other OS: return Unsupported; never emulate with exists()+rename/copy
```

On `Exists`, `commit_no_clobber` removes only `temp`; successful cleanup returns `Conflict`, failed cleanup returns `CommitError::Cleanup`. The native rename success is the linearization point. Do not claim full power-loss durability; ADR wording is “atomic directory-entry visibility with no replacement.”

- [ ] **Step 3: Write a failing process-race test**

Declare the worker binary in `Cargo.toml`:

```toml
[[bin]]
name = "atomic_commit_worker"
path = "src/bin/atomic_commit_worker.rs"
required-features = ["feasibility"]
```

The binary accepts exactly `--temp`, `--target`, `--start-gate`; it waits until the gate file exists, calls `commit_no_clobber`, prints one JSON line, and exits `0` for committed, `10` for conflict, any other code for failure.

The integration test creates 32 temp files with distinct sentinel contents, spawns 32 workers, creates the gate, and asserts:

```text
committed == 1
conflicts == 31
failures == 0
final content equals the winner sentinel
all 32 .part files are absent
```

Run before the worker implementation. Expected: FAIL because the binary or JSON result is missing.

- [ ] **Step 4: Implement the worker and cancel/commit linearization test**

Implement the fixed CLI and add a controlled hook immediately before the native rename for a unit test inside `atomic_commit.rs`. Race a cancellation flag against that hook:

```text
cancel wins before syscall -> temp removed, no final, terminal Cancelled
native rename wins -> final remains, terminal Committed; late cancel cannot delete final
```

The hook is compiled only with `cfg(test)` and cannot be enabled in application builds.

Create the exact `atomic-commit` scope entry including the worker, tests, platform bindings, Cargo manifests/lock and toolchain inputs. Then format, run the deterministic race suite, and commit the implementation before native platform evidence:

```powershell
cargo fmt --manifest-path src-tauri/Cargo.toml --all
cargo test --manifest-path src-tauri/Cargo.toml --features feasibility --test atomic_commit -- --nocapture --test-threads=1
git add src-tauri/src/feasibility/atomic_commit.rs src-tauri/src/feasibility/mod.rs src-tauri/src/bin/atomic_commit_worker.rs src-tauri/tests/atomic_commit.rs src-tauri/Cargo.toml src-tauri/Cargo.lock docs/feasibility/evidence-scopes.json
git commit -m "feat: add native atomic no-clobber commits"
git status --short
```

- [ ] **Step 5: Run native filesystem evidence**

Run the full test on Windows 10/11 NTFS, macOS Intel APFS, and macOS Apple Silicon APFS. Also probe exFAT, HFS+ or SMB only when available; unsupported results are acceptable only if returned explicitly as `FS_COMMIT_UNSUPPORTED`.

```powershell
cargo test --manifest-path src-tauri/Cargo.toml --features feasibility --test atomic_commit -- --nocapture --test-threads=1
```

Expected evidence:

```text
atomic: committed=1 conflicts=31 failures=0 leftovers=0 target_intact=true
```

Save every required filesystem/platform row, race count and cancel linearization field to ignored `artifacts/feasibility/atomic-commit.raw.json`, naming the clean implementation commit.

- [ ] **Step 6: Record the ADR and commit**

Record OS build, architecture, filesystem, native API, race counts and cancel linearization in `docs/feasibility/atomic-commit.md`. ADR `0004` must reject all check-then-rename and copy/delete fallbacks.

```powershell
node scripts/feasibility-evidence.mjs build --input artifacts/feasibility/atomic-commit.raw.json --markdown docs/feasibility/atomic-commit.md --output docs/feasibility/atomic-commit.json
node scripts/feasibility-evidence.mjs check docs/feasibility/atomic-commit.json
git add docs/feasibility/atomic-commit.md docs/feasibility/atomic-commit.json docs/decisions/0004-atomic-no-clobber.md
git commit -m "docs: record atomic commit feasibility evidence"
```

### Task 7: 固定首版媒体容器允许列表

**Files:**
- Create: `src-tauri/src/feasibility/media_probe.rs`
- Create: `src-tauri/tests/media_probe.rs`
- Create: `src-tauri/tests/fixtures/media/README.md`
- Create: `src-tauri/tests/fixtures/media/minimal.mp3`
- Create: `src-tauri/tests/fixtures/media/minimal-320.mp3`
- Create: `src-tauri/tests/fixtures/media/minimal.flac`
- Create: `src-tauri/tests/fixtures/media/minimal.mp2`
- Create: `src-tauri/tests/fixtures/media/truncated-id3.bin`
- Create: `src-tauri/tests/fixtures/media/truncated-flac.bin`
- Create: `src-tauri/tests/fixtures/media/cover.png`
- Create: `scripts/generate-media-fixtures.mjs`
- Modify: `src-tauri/src/feasibility/mod.rs`
- Modify: `src-tauri/Cargo.toml`
- Modify: `src-tauri/Cargo.lock`
- Create: `docs/feasibility/media-containers.md`
- Create: `docs/feasibility/media-containers.json`
- Create: `docs/decisions/0005-media-container-allowlist.md`
- Modify: `docs/feasibility/evidence-scopes.json`

**Interfaces:**
- Consumes: actual-content recognition and no-transcode design rule.
- Produces: conservative candidate `AllowedMedia::{Mp3, Flac}`, content-derived extension, tag/cover round-trip evidence; all other formats are rejected until a later reviewed expansion.

- [ ] **Step 1: Generate and freeze auditable project-owned fixtures**

Create `scripts/generate-media-fixtures.mjs`. It must invoke `ffmpeg` with argument arrays, never a shell string, and generate one second of synthesized silence at 44.1 kHz stereo as MP3 Layer III at 128 and 320 kbit/s, native FLAC, and MP2; it must also generate a 16x16 solid-black PNG. Strip input/global metadata. The exact encoder settings are `libmp3lame` with Xing disabled, `flac`/compression level 5, and `mp2`/192 kbit/s. The script writes the malformed files from these exact bytes:

```js
Buffer.from([0x49, 0x44, 0x33, 0x04, 0x00, 0x00, 0x00, 0x00, 0x00, 0x7f]);
Buffer.from([0x66, 0x4c, 0x61, 0x43, 0x00]);
```

It refuses to overwrite an existing fixture unless `--force` is supplied and fails closed if `ffmpeg -version`, any encoder, or any output validation fails. Run once on the authoring machine:

```powershell
node scripts/generate-media-fixtures.mjs
```

Commit the generated binaries; CI only reads them and must not require FFmpeg. `README.md` records the exact FFmpeg version and argv for every generated file, identifies the silence and solid pixel as created for this repository and dedicated to CC0-1.0, and lists SHA-256, byte length, codec/container, and redistribution rationale. Do not copy music returned by the live GD probe or substitute an internet download. If the authoring machine cannot generate and validate this exact set, mark this task `blocked` before writing the media parser.

The fixed fixture set is:

```text
minimal.mp3: MPEG Layer III with valid nonzero sample rate/channels
minimal-320.mp3: MPEG Layer III at 320 kbit/s
minimal.flac: native FLAC with valid STREAMINFO
minimal.mp2: valid MPEG Layer II negative case
truncated-id3.bin: starts like ID3 but cannot parse
truncated-flac.bin: starts fLaC but lacks valid STREAMINFO
cover.png: small valid PNG used only for round-trip metadata
```

Run `Get-FileHash -Algorithm SHA256` on Windows and `shasum -a 256` on macOS; all hashes must match the committed README before tests start.

- [ ] **Step 2: Write failing allowlist and round-trip tests**

Add:

```toml
lofty = "0.24.0"
```

Reuse the existing `sha2 = "0.10"` dependency introduced in Task 4; do not add a duplicate Cargo key.

Write tests that require:

```rust
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AllowedMedia { Mp3, Flac }

impl AllowedMedia {
    pub const fn extension(self) -> &'static str {
        match self { Self::Mp3 => "mp3", Self::Flac => "flac" }
    }
}

pub struct ValidatedMedia {
    pub kind: AllowedMedia,
    pub sample_rate_hz: u32,
    pub channels: u8,
}

pub struct MediaMetadata {
    pub title: String,
    pub artists: Vec<String>,
    pub album: Option<String>,
}

pub struct ValidatedCover {
    pub bytes: Vec<u8>,
    pub mime: &'static str,
    pub sha256: [u8; 32],
}

#[derive(Debug, thiserror::Error)]
pub enum MediaValidationError {
    #[error("unsupported media container or codec")]
    Unsupported,
    #[error("invalid media: {0}")]
    Invalid(&'static str),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error("metadata parser failed: {0}")]
    Parser(String),
}

#[derive(Debug, thiserror::Error)]
pub enum MediaWriteError {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error("metadata writer failed: {0}")]
    Writer(String),
    #[error(transparent)]
    Verification(#[from] MediaValidationError),
}

pub fn validate_media(file: &mut File) -> Result<ValidatedMedia, MediaValidationError>;
pub fn write_metadata_and_cover(file: &mut File, media: &ValidatedMedia, metadata: &MediaMetadata, cover: Option<&ValidatedCover>) -> Result<(), MediaWriteError>;
pub fn verify_after_write(
    file: &mut File,
    expected: AllowedMedia,
    expected_metadata: &MediaMetadata,
    expected_cover_digest: Option<[u8; 32]>,
) -> Result<(), MediaValidationError>;
```

Positive cases must ignore filename extension and MIME, recognize both MP3 fixtures plus FLAC from contents, write title/artist/album plus cover to MP3 and FLAC copies, flush and `sync_all`, close the writer, reopen the path into a new `File`, then call `verify_after_write` to compare the media kind, every metadata field and cover SHA-256. The 320 kbit/s fixture must remain MP3 Layer III and report a bitrate compatible with 320 kbit/s after the round trip. Negative cases include MP2 renamed `.mp3`, both truncated inputs, random bytes, representative AAC/MP4/Ogg/Opus/WAV headers, and in-memory PNG/ZIP prefixes with valid audio bytes appended.

Run before implementation. Expected: FAIL because `media_probe` is absent.

- [ ] **Step 3: Implement strict concrete-type validation**

Implement exactly these checks:

```text
Probe::new(file).guess_file_type(); never trust URL, Content-Type or extension
MPEG -> parse concrete MpegFile; require Layer::Layer3, nonzero sample rate and channels
FLAC -> require fLaC, parse concrete FlacFile, valid STREAMINFO, nonzero sample rate/channels
MP3 cover -> ID3v2 APIC
FLAC cover -> native Picture Block
after save -> flush, reopen, re-probe and compare type, fields and cover digest
all other FileType values -> DOWNLOAD_MEDIA
```

If the installed Lofty API cannot prove MPEG Layer III or safe picture round-trip, mark this gate `design-change-required`; do not widen by extension.

Create the exact `media-containers` scope entry including generator, every frozen fixture, parser/tests, Cargo manifests/lock and dependency/toolchain inputs. Then format, run the deterministic suite, and commit the parser plus frozen fixtures before cross-platform evidence:

```powershell
cargo fmt --manifest-path src-tauri/Cargo.toml --all
cargo test --manifest-path src-tauri/Cargo.toml --features feasibility --test media_probe -- --nocapture
git add src-tauri/src/feasibility/media_probe.rs src-tauri/src/feasibility/mod.rs src-tauri/tests/media_probe.rs src-tauri/tests/fixtures/media scripts/generate-media-fixtures.mjs src-tauri/Cargo.toml src-tauri/Cargo.lock docs/feasibility/evidence-scopes.json
git commit -m "feat: add strict MP3 and FLAC validation"
git status --short
```

- [ ] **Step 4: Run cross-platform media tests and record evidence**

```powershell
cargo test --manifest-path src-tauri/Cargo.toml --features feasibility --test media_probe -- --nocapture
```

Expected:

```text
media: accepted=mp3,flac rejected=mp2,aac,mp4,ogg,opus,wav,truncated roundtrip=true
```

Run on Windows and both macOS architectures because tag save behavior can vary by filesystem and library backend.

Save the three required platform rows, every accepted/rejected family and metadata/cover round-trip fields to ignored `artifacts/feasibility/media-containers.raw.json`, naming the clean implementation commit.

- [ ] **Step 5: Commit the allowlist decision**

ADR `0005` fixes MP3 Layer III and native FLAC for the first release. It must state that an unsupported container fails with `DOWNLOAD_MEDIA`; attachment write failure still follows the design’s audio-success warning rule once the full download flow exists.

```powershell
node scripts/feasibility-evidence.mjs build --input artifacts/feasibility/media-containers.raw.json --markdown docs/feasibility/media-containers.md --output docs/feasibility/media-containers.json
node scripts/feasibility-evidence.mjs check docs/feasibility/media-containers.json
git add docs/feasibility/media-containers.md docs/feasibility/media-containers.json docs/decisions/0005-media-container-allowlist.md
git commit -m "docs: record media-container feasibility evidence"
```

### Task 8: 验证 Updater 取消分类与统一退出屏障

**Files:**
- Create: `src-tauri/src/feasibility/updater_probe.rs`
- Create: `src-tauri/tests/updater_probe.rs`
- Create: `scripts/slow-update-server.mjs`
- Modify: `src-tauri/build.rs`
- Modify: `src-tauri/permissions/feasibility.toml`
- Modify: `src-tauri/src/feasibility/mod.rs`
- Modify: `src-tauri/src/lib.rs`
- Modify: `src/lib/feasibility/FeasibilityPanel.svelte`
- Modify: `src-tauri/Cargo.toml`
- Modify: `src-tauri/Cargo.lock`
- Modify: `src-tauri/tauri.feasibility.conf.json`
- Modify: `scripts/verify-config.mjs`
- Create: `docs/feasibility/updater-exit-barrier.md`
- Create: `docs/feasibility/updater-exit-barrier.json`
- Create: `docs/decisions/0006-updater-exit-barrier.md`
- Modify: `docs/feasibility/signature-webview.md`
- Modify: `docs/feasibility/signature-webview.json`
- Modify: `docs/decisions/0002-signature-webview.md`
- Modify: `docs/feasibility/evidence-scopes.json`

**Interfaces:**
- Consumes: Tauri updater `Update::download`, not `download_and_install`; design §6.5/§9.
- Produces: pure `ExitBarrier`, `UpdateStopMode`, an actual local slow-download probe, and one of two bounded ADR outcomes.

- [ ] **Step 1: Write failing pure exit-barrier tests**

Add:

```toml
tauri-plugin-updater = "2.10.1"
```

Define and test this surface before implementation:

```rust
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExitIntent { Close, InstallVerifiedUpdate }

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UpdateStopMode {
    DropFuture { settle_timeout: Duration },
    WaitForRequestDeadline { total_timeout: Duration, settle_grace: Duration },
}

#[derive(Debug)]
pub enum UpdateDownloadError { Transport(String), Verification(String), TimedOut }
pub enum UpdateDownloadTerminal { Verified(Vec<u8>), Cancelled, Failed(UpdateDownloadError) }

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WaitReason { MusicCancellation, UpdateCancellation, UpdateCompletion, MusicAndUpdate }

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UpdateTransferSnapshot {
    pub bytes_received: u64,
    pub total_bytes: Option<u64>,
    pub finished: bool,
}

pub enum BarrierEvent {
    MusicStarted,
    UpdateStarted(UpdateStopMode),
    ExitRequested(ExitIntent),
    ConfirmCancelAndExit,
    ReturnToApp,
    MusicSettled,
    UpdateSettled(UpdateDownloadTerminal),
    DeadlineReached,
}

pub enum ExitDecision {
    Proceed(ExitIntent),
    NeedConfirmation { music_active: bool, update_active: bool },
    Waiting { reason: WaitReason, deadline: Option<Instant> },
    StayOpen { code: &'static str },
}
pub enum BarrierAction { CancelMusic, CancelUpdate, ExitProcess, InstallVerifiedUpdate }

pub struct ExitBarrier {
    music_active: bool,
    update_mode: Option<UpdateStopMode>,
    pending: Option<ExitIntent>,
    confirmed: bool,
    deadline: Option<Instant>,
}

impl ExitBarrier {
    pub fn new() -> Self;
    pub fn apply(&mut self, event: BarrierEvent, now: Instant) -> (ExitDecision, Vec<BarrierAction>);
}
```

Tests cover idle close, active music, cancelable update, wait-only update, both active, install while active, update failure, return-to-app, repeated close idempotency, and deadline+grace exceeded. Assert no `ExitProcess` or `InstallVerifiedUpdate` action appears before all active operations report terminal.

The fake wait-only adapter is only a deterministic unit-test seam for `ExitBarrier`; its result can never satisfy the updater feasibility gate. Gate evidence must come from the real `tauri_plugin_updater::Update::download` path in Steps 4–6.

Run:

```powershell
cargo test --manifest-path src-tauri/Cargo.toml --features feasibility --test updater_probe barrier_ -- --nocapture
```

Expected: FAIL because `ExitBarrier` is absent.

- [ ] **Step 2: Implement the deterministic barrier model**

Implement the transition table from the design and these invariants:

```text
programmatic app.exit() may only be called by BarrierAction::ExitProcess
Update::install(bytes) may only be called by BarrierAction::InstallVerifiedUpdate
CloseRequested always prevent_close() while any operation is active
download_and_install is forbidden
duplicate close/cancel events are idempotent
wait-only deadline+grace -> StayOpen, never forced exit
```

Run the pure tests. Expected: PASS.

- [ ] **Step 3: Create a deterministic slow signed update fixture**

`scripts/slow-update-server.mjs` must bind only `127.0.0.1`, serve `/latest.json` plus one fixed `/update.bin` route, stream the artifact in 64 KiB chunks, log request start/chunk/connection-close times, and expose no filesystem path supplied by an HTTP request. Its `--prepare <path>` mode creates exactly 8 MiB from a repeated fixed 64 KiB byte pattern and exits; normal mode accepts only explicit `--artifact`, `--signature` and `--profile cancelable|wait-only|classification`, and rejects any port other than `38475`. `cancelable` sends a chunk every 250 ms. `wait-only` sends a chunk every 750 ms, so a real updater configured with a three-second request timeout cannot finish the signed artifact. `classification` applies `cancelable` to the first artifact request and `wait-only` to the second, then rejects further artifact requests. The dynamic manifest contains exactly version `0.1.1`, URL `http://127.0.0.1:38475/update.bin`, and the complete `.sig` contents.

Generate and use a disposable key with these exact commands:

```powershell
New-Item -ItemType Directory -Force artifacts/feasibility/updater
node scripts/slow-update-server.mjs --prepare artifacts/feasibility/updater/update-0.1.1.bin
pnpm tauri signer generate --ci -p yinmi-feasibility-only -w artifacts/feasibility/updater/test.key
pnpm tauri signer sign -f artifacts/feasibility/updater/test.key -p yinmi-feasibility-only artifacts/feasibility/updater/update-0.1.1.bin
if (!(Test-Path artifacts/feasibility/updater/test.key.pub)) { throw 'missing public key' }
if (!(Test-Path artifacts/feasibility/updater/update-0.1.1.bin.sig)) { throw 'missing artifact signature' }
$env:YINMI_FEASIBILITY_UPDATER_PUBKEY_PATH=(Resolve-Path artifacts/feasibility/updater/test.key.pub)
$env:YINMI_FEASIBILITY_UPDATER_ENDPOINT='http://127.0.0.1:38475/latest.json'
```

The generated `.sig`, public key, private key, and artifact remain below the ignored artifact directory and are never committed. `yinmi-feasibility-only` is an intentionally public test-only password, not a production secret, and must never be reused. With the Cargo feature enabled, `src-tauri/src/lib.rs` reads the two `YINMI_FEASIBILITY_*` variables and supplies the public-key contents and the single endpoint to the updater builder. If both variables are absent, the other feasibility probes still start and the updater button is disabled; if only one is present, the endpoint is not loopback, or the key is unreadable, startup fails closed. The default build does not register the updater plugin.

Add only this probe exception to `tauri.feasibility.conf.json`:

```json
{
  "plugins": {
    "updater": {
      "dangerousInsecureTransportProtocol": true
    }
  }
}
```

Merge it with the existing overlay rather than replacing the Task 4 fields. Extend the config test to prove the production config has no updater endpoint, public key, or insecure flag, and that the feasibility overlay has the insecure flag but no hard-coded endpoint or key.

- [ ] **Step 4: Write the failing real-download cancellation and bounded wait-only probes**

Use the exact outcomes from Step 1 and add this download wrapper:

```rust
pub async fn download_update<R: tauri::Runtime>(
    update: &tauri_plugin_updater::Update<R>,
    cancel: CancellationToken,
    mode: UpdateStopMode,
    progress: tokio::sync::watch::Sender<UpdateTransferSnapshot>,
) -> UpdateDownloadTerminal;
```

Expose only the fixed feature command `feasibility_run_updater_classification_probe` and one matching button in `FeasibilityPanel.svelte`. The command returns both real scenarios in one report:

```rust
pub struct UpdaterClassificationReport {
    pub drop_future: RealDownloadProbeReport,
    pub wait_only: RealDownloadProbeReport,
    pub feedback_text: String,
}
```

Cancelable scenario: start the actual updater download against server profile `cancelable`, cancel after the first chunk and drop the download future. Passing requires wrapper `Cancelled`, server disconnect within 5 seconds, `on_finish` false, install never called, and no app-owned file remains.

Wait-only scenario: create a fresh real updater with `UpdaterBuilder::timeout(Duration::from_secs(3))`, check again against server profile `wait-only`, request exit after the first chunk, and keep polling the real `Update::download` future until it returns. Passing requires a real updater timeout/error terminal within 3 seconds plus 2 seconds settle grace, server connection close, `on_finish` false, install never called, no app-owned file, and no `ExitProcess` action before that terminal. The UI must visibly show bounded wording equivalent to `更新无法立即取消；最多等待 5 秒，可返回应用` while waiting and `更新下载已停止，未安装` on terminal. A fake adapter may test state transitions but cannot produce either report.

Expected before implementation: both scenarios fail.

- [ ] **Step 5: Implement and classify updater behavior**

Add `feasibility_run_updater_classification_probe` to the feature-only `AppManifest`, permission, and handler. The existing default-artifact prefix check covers it automatically. Build the updater for both scenarios through `UpdaterExt::updater_builder().timeout(...).no_proxy()` with the fixed loopback endpoint and test public key. Implement both modes without claiming the result in advance.

Run the pure/fake tests first:

```powershell
cargo test --manifest-path src-tauri/Cargo.toml --features feasibility --test updater_probe -- --nocapture --test-threads=1
```

Create the exact `updater-exit-barrier` scope entry and extend the signature entry for every changed command/ACL/config/frontend/Cargo file. Then format, verify the default build remains clean, and commit every updater/ACL change before real platform observations:

```powershell
pnpm format
cargo fmt --manifest-path src-tauri/Cargo.toml --all
pnpm tauri build --debug --no-bundle
pnpm verify:default-artifacts
git add src-tauri/src/feasibility/updater_probe.rs src-tauri/src/feasibility/mod.rs src-tauri/src/lib.rs src-tauri/build.rs src-tauri/permissions/feasibility.toml src-tauri/tests/updater_probe.rs src/lib/feasibility/FeasibilityPanel.svelte src-tauri/Cargo.toml src-tauri/Cargo.lock src-tauri/tauri.feasibility.conf.json scripts/verify-config.mjs scripts/slow-update-server.mjs docs/feasibility/evidence-scopes.json
git commit -m "feat: add bounded updater exit classification"
git status --short
```

Expected: status is clean. The real classification report and refreshed WebView isolation report must name this commit.

Then start the two-request classification profile in terminal A:

```powershell
node scripts/slow-update-server.mjs --artifact artifacts/feasibility/updater/update-0.1.1.bin --signature artifacts/feasibility/updater/update-0.1.1.bin.sig --profile classification --port 38475
```

Start the feature app in terminal B:

```powershell
$env:YINMI_FEASIBILITY_UPDATER_PUBKEY_PATH=(Resolve-Path artifacts/feasibility/updater/test.key.pub)
$env:YINMI_FEASIBILITY_UPDATER_ENDPOINT='http://127.0.0.1:38475/latest.json'
pnpm tauri dev --config src-tauri/tauri.feasibility.conf.json --features feasibility
```

Press the single classification button once. It runs the drop-future request first and the real wait-only request second, then returns one combined report. Confirm terminal A logged exactly two artifact requests and a terminal connection state for both. Stop both processes after the report is saved.

Decision matrix:

```text
drop future disconnects <=5s and cleans -> ADR selects cancelable mode
drop future cannot be proven + real wait-only reaches true terminal within configured timeout + 2s -> ADR may select wait-only mode
fake-only wait-only result or missing real server terminal -> blocked, never pass
neither mode is bounded -> design-change-required; stop Phase 1
```

For the wait-only candidate, ADR `0006` must select a numeric production total timeout, show the maximum expected signed package size and minimum supported throughput used to derive it, cap the additional settle grace, and freeze the exact waiting/failure/return-to-app user feedback. Validate the timeout path with the real updater on every target platform. If a defensible finite value cannot be derived, the result is `design-change-required`; infinite waiting is forbidden.

- [ ] **Step 6: Record platform evidence and commit**

Run both real profiles on Windows x64 and both macOS architectures. `docs/feasibility/updater-exit-barrier.md` records profile, configured real updater timeout, chunk count, cancellation/exit-request time, disconnect observation, cleanup, terminal time, visible feedback states, whether install was invoked, and every barrier decision. Save the required machine fields to ignored `artifacts/feasibility/updater-exit-barrier.raw.json`. No signing secret or raw private key enters the document.

This task changed the command manifest, permissions and feature app. Rerun the full four-platform signature/WebView isolation matrix from Task 4, refresh `artifacts/feasibility/signature-webview.raw.json` and rebuild its Markdown/JSON companion. Reusing the old signature companion must fail its scope digest.

After the feature probe, run a default `pnpm tauri build --debug --no-bundle` followed by `pnpm verify:default-artifacts`; both must pass so the updater probe, environment-variable names, and commands do not enter the normal artifact.

```powershell
node scripts/feasibility-evidence.mjs build --input artifacts/feasibility/updater-exit-barrier.raw.json --markdown docs/feasibility/updater-exit-barrier.md --output docs/feasibility/updater-exit-barrier.json
node scripts/feasibility-evidence.mjs build --input artifacts/feasibility/signature-webview.raw.json --markdown docs/feasibility/signature-webview.md --output docs/feasibility/signature-webview.json
node scripts/feasibility-evidence.mjs check docs/feasibility/updater-exit-barrier.json
node scripts/feasibility-evidence.mjs check docs/feasibility/signature-webview.json
git add docs/feasibility/updater-exit-barrier.md docs/feasibility/updater-exit-barrier.json docs/feasibility/signature-webview.md docs/feasibility/signature-webview.json docs/decisions/0002-signature-webview.md docs/decisions/0006-updater-exit-barrier.md
git commit -m "docs: record updater and refreshed isolation evidence"
```

### Task 9: 建立独立的 1000 条结果性能基准

**Files:**
- Create: `benchmarks/results-1000/BenchmarkApp.svelte`
- Create: `benchmarks/results-1000/ResultsTablePrototype.svelte`
- Create: `benchmarks/results-1000/budgets.ts`
- Create: `benchmarks/results-1000/dataset.ts`
- Create: `benchmarks/results-1000/env.d.ts`
- Create: `benchmarks/results-1000/selection.ts`
- Create: `benchmarks/results-1000/virtual-range.ts`
- Create: `benchmarks/results-1000/metrics.ts`
- Create: `benchmarks/results-1000/report.ts`
- Create: `benchmarks/results-1000/dataset.test.ts`
- Create: `benchmarks/results-1000/metrics.test.ts`
- Create: `benchmarks/results-1000/report.test.ts`
- Create: `benchmarks/results-1000/selection.test.ts`
- Create: `benchmarks/results-1000/virtual-range.test.ts`
- Create: `benchmarks/results-1000/results-1000.spec.ts`
- Create: `benchmarks/results-1000/playwright.config.ts`
- Create: `benchmarks/results-1000/playwright-reporter.ts`
- Create: `benchmarks/results-1000/vite.config.ts`
- Create: `benchmarks/results-1000/index.html`
- Create: `benchmarks/results-1000/main.ts`
- Create: `benchmarks/results-1000/tsconfig.json`
- Create: `benchmarks/results-1000/report.schema.json`
- Create: `scripts/perf/capture-windows-baseline.ps1`
- Create: `scripts/perf/validate-report.mjs`
- Create: `.github/workflows/perf-results.yml`
- Create: `src-tauri/tauri.perf.conf.json`
- Modify: `scripts/verify-config.mjs`
- Modify: `package.json`
- Modify: `pnpm-lock.yaml`
- Modify: `docs/feasibility/evidence-scopes.json`
- Create: `scripts/perf/run-browser.mjs`
- Create: `scripts/perf/validate-report.test.mjs`
- Create: `docs/feasibility/result-list-performance.md`
- Create: `docs/feasibility/result-list-performance.json`
- Create: `docs/decisions/0007-result-list-performance.md`

**Interfaces:**
- Consumes: only the visual/result fields from design; no formal search DTO.
- Produces: isolated fixed-row benchmark, JSON report schema, Windows minimum baseline, macOS auxiliary evidence, and a rendering ADR.

- [ ] **Step 1: Add browser benchmark dependencies and immutable budgets**

Add `@playwright/test` `1.61.1` as an exact dev dependency and these scripts:

```json
{
  "perf:results:unit": "vitest run --config benchmarks/results-1000/vite.config.ts",
  "perf:results:check": "svelte-check --tsconfig benchmarks/results-1000/tsconfig.json",
  "perf:results:dev": "vite --config benchmarks/results-1000/vite.config.ts --host 127.0.0.1 --port 1430",
  "perf:results:build": "vite build --config benchmarks/results-1000/vite.config.ts --mode perf-memory",
  "perf:results:shell:build": "tauri build --debug --no-bundle --config src-tauri/tauri.perf.conf.json",
  "perf:results:browser": "node scripts/perf/run-browser.mjs --project chromium --platform-id windows-10-authority --output artifacts/feasibility/perf/browser.json",
  "perf:results:capture": "powershell -NoProfile -File scripts/perf/capture-windows-baseline.ps1 -OutputPath artifacts/feasibility/perf/webview2-memory.json",
  "perf:results:finalize": "node scripts/perf/validate-report.mjs --browser artifacts/feasibility/perf/browser.json --memory artifacts/feasibility/perf/webview2-memory.json --macos-intel artifacts/feasibility/perf/macos-intel.json --macos-arm artifacts/feasibility/perf/macos-arm64.json --markdown docs/feasibility/result-list-performance.md --decision docs/decisions/0007-result-list-performance.md --write docs/feasibility/result-list-performance.json",
  "perf:results:validate": "node scripts/perf/validate-report.mjs --check docs/feasibility/result-list-performance.json"
}
```

Before the harness commit in Step 5, append `&& pnpm perf:results:check && pnpm perf:results:unit` to the existing `quality` script. Do not append `perf:results:validate`: strict historical evidence validation belongs to `phase1:gate`, while ordinary quality must remain runnable after a scoped file changes and before measurements are refreshed.

Create `src-tauri/tauri.perf.conf.json` as an IPC-free local shell:

```json
{
  "$schema": "https://schema.tauri.app/config/2",
  "productName": "音觅性能基准",
  "identifier": "io.github.xiaobing6.yinmi.perf",
  "build": {
    "beforeDevCommand": "pnpm perf:results:dev",
    "devUrl": "http://127.0.0.1:1430",
    "beforeBuildCommand": "pnpm perf:results:build",
    "frontendDist": "../benchmarks/results-1000/dist"
  },
  "app": {
    "security": {
      "capabilities": [],
      "csp": "default-src 'self'; style-src 'self' 'unsafe-inline'; connect-src http://127.0.0.1:1431"
    }
  }
}
```

Use this benchmark Vite shape:

```ts
import { svelte } from '@sveltejs/vite-plugin-svelte';
import { defineConfig } from 'vitest/config';

export default defineConfig(({ mode }) => ({
  root: 'benchmarks/results-1000',
  plugins: [svelte()],
  define: {
    __PERF_MEMORY_AUTORUN__: JSON.stringify(mode === 'perf-memory'),
    __PERF_HANDSHAKE_URL__: JSON.stringify(
      mode === 'perf-memory' ? process.env.YINMI_PERF_HANDSHAKE_URL ?? null : null,
    ),
  },
  server: { host: '127.0.0.1', port: 1430, strictPort: true },
  build: { outDir: 'dist', emptyOutDir: true, target: ['chrome111', 'safari16.4'] },
  test: { environment: 'jsdom', include: ['*.test.ts'] },
}));
```

Declare both constants in `env.d.ts`; the benchmark `tsconfig.json` extends the root config and replaces `include` with every local `.ts`, `.d.ts`, and `.svelte` file. Its Playwright config has `testDir` equal to this benchmark directory, `testMatch: 'results-1000.spec.ts'`, `workers: 1`, `retries: 0`, base URL `http://127.0.0.1:1430`, web server command `pnpm perf:results:dev`, explicit Chromium and WebKit projects, reporter `./playwright-reporter.ts`, and `outputDir: path.resolve(process.cwd(), 'artifacts/feasibility/perf/playwright-output')`, resolved from the repository root and kept below the ignored artifact tree; no `test-results` file may appear under the benchmark tree. Extend `verify-config.mjs` to assert the performance overlay's distinct identifier, exact ports, repository-root-resolved output path, benchmark-only CSP, and empty capability list.

Create `budgets.ts`:

```ts
export const RESULTS_BUDGETS = {
  firstVisibleMs: 1_000,
  selectAllMs: 200,
  clearAllMs: 200,
  incrementalMemoryBytes: 100 * 1024 * 1024,
  maxMainThreadStallMs: 250,
} as const;
```

- [ ] **Step 2: Write failing dataset, selection and virtual-range tests**

Use benchmark-only types:

```ts
export interface BenchmarkSong {
  id: string;
  name: string;
  artists: readonly string[];
  album: string | null;
  sourceLabel: '网易云音乐';
  durationMs: number | null;
  capabilities: { audio: boolean; cover: boolean; lyric: boolean };
  downloadState: 'idle';
}

export interface VisibleRange { start: number; end: number; offsetTop: number; totalHeight: number }
```

Tests require exactly 1000 stable unique IDs, immutable select-all/clear, and bounded visible ranges at top/middle/bottom/empty/overscrolled positions. Run unit tests first; expected FAIL because functions do not exist.

- [ ] **Step 3: Implement the minimum fixed-row virtual prototype**

Implement `createBenchmarkSongs(count=1000)`, `selectAll`, `clearSelection`, and `computeVisibleRange` with fixed row height plus overscan. `ResultsTablePrototype.svelte` renders only `[start,end)` rows and two spacer regions. Do not import a data-grid or virtualization package and do not reuse these benchmark types in product code.

Run unit tests. Expected: PASS.

- [ ] **Step 4: Implement repeatable browser measurements**

Measurement rules are exact:

```text
one warm-up + 10 recorded samples
start first-visible clock immediately before assigning 1000 rows
stop after Svelte tick + two requestAnimationFrame callbacks + first row intersects viewport
select/clear stop after tick + two frames + visible checkbox state matches
record every sample, median, p95 and max; acceptance uses max <= budget
observe Long Tasks where available and always observe requestAnimationFrame gaps
any long task or frame gap >250ms fails
rendered result rows must remain <=100
```

Playwright tests cover first visible, select all, clear all, top-to-bottom scroll, rendered node cap, and Chromium Long Tasks. A test intentionally lowers each budget first and must fail, proving the validator is active; restore real budgets and rerun to pass. `run-browser.mjs` accepts only `chromium|webkit`, `preflight` or one of the three fixed evidence platform IDs, and a repository-relative output below `artifacts/feasibility/perf/`; `preflight` is explicitly rejected by finalization. It spawns Playwright without a shell and passes the output path to `playwright-reporter.ts`. The reporter atomically replaces only that ignored path after all ten samples and environment metadata are complete. Every raw reporter records `git rev-parse HEAD`, `git status --porcelain` cleanliness, OS/architecture and engine/runtime version.

Implement `validate-report.mjs` and `validate-report.test.mjs` now, before the harness commit. Export raw and committed-report validators. Synthetic tests cover every budget boundary, different SHA, dirty input, missing/extra platform, malformed sample counts, Windows-only candidate mode, all four final inputs, one-byte scoped-file tampering, and Markdown/ADR hash tampering. The committed report uses the exact `result-list-performance` entry from `evidence-scopes.json`, the common canonical scope digest, Markdown hash and ADR hash. Keep Node filesystem/process code in the reporter and `scripts/perf`, never in browser-imported `report.ts`.

Implement `capture-windows-baseline.ps1` in this step as well. It accepts only `-OutputPath`, optional `-ExpectedSha`, and `-SelfTest`; self-test exercises argument rejection, handshake ordering, sample-count validation and `finally` cleanup without launching Tauri. The real path must own every spawned process and listener and terminate the app plus all tracked WebView2 descendants in a bounded `try/finally`, then assert that none remain. Step 6 only executes this already-tested script; it does not finish its implementation.

- [ ] **Step 5: Add automation and commit the complete benchmark harness before measuring**

Create `.github/workflows/perf-results.yml` now, before authoritative measurements:

```yaml
name: perf-results

on:
  workflow_dispatch:

permissions:
  contents: read

jobs:
  perf-results:
    runs-on: [self-hosted, windows, x64, yinmi-perf]
    steps:
      - uses: actions/checkout@v6
        with:
          fetch-depth: 0
      - uses: pnpm/action-setup@v6
        with:
          version: 11.7.0
      - uses: actions/setup-node@v6
        with:
          node-version: 24
          cache: pnpm
      - uses: dtolnay/rust-toolchain@stable
        with:
          toolchain: 1.97.0
      - run: pnpm install --frozen-lockfile
      - run: pnpm exec playwright install chromium
      - run: pnpm perf:results:check
      - run: pnpm perf:results:unit
      - run: pnpm perf:results:browser
      - run: pnpm perf:results:capture
      - run: node scripts/perf/validate-report.mjs --browser artifacts/feasibility/perf/browser.json --memory artifacts/feasibility/perf/webview2-memory.json --expect-sha ${{ github.sha }} --require-clean --windows-only
      - run: pnpm perf:results:validate
```

The workflow is manual only, uses the dedicated runner label, validates a candidate report without changing the checkout, and never silently replaces the committed authority profile.

Run the deterministic checks and a preflight browser pass, then commit every file that can affect the authoritative measurements:

```powershell
pnpm perf:results:check
pnpm perf:results:unit
node --test scripts/perf/validate-report.test.mjs
powershell -NoProfile -File scripts/perf/capture-windows-baseline.ps1 -SelfTest
pnpm exec playwright install chromium
node scripts/perf/run-browser.mjs --project chromium --platform-id preflight --output artifacts/feasibility/perf/preflight-browser.json
pnpm perf:results:shell:build
pnpm verify:config
git add benchmarks/results-1000 scripts/perf .github/workflows/perf-results.yml src-tauri/tauri.perf.conf.json scripts/verify-config.mjs package.json pnpm-lock.yaml docs/feasibility/evidence-scopes.json
git commit -m "perf: add isolated 1000-result benchmark"
git status --short
```

Expected: all commands pass and status is clean. The ignored `preflight-browser.json` is not evidence; Step 6 creates the authoritative `browser.json` after this commit. The `result-list-performance` scope entry explicitly enumerates the benchmark tree, all `scripts/perf` files, the common evidence helper/schema/scope manifest, both Tauri configs and Rust shell manifests/sources, root toolchain/Svelte/package/lock/config inputs, `verify-config.mjs`, and the perf workflow. Do not edit any scoped file between this commit and all four authoritative captures.

- [ ] **Step 6: Capture the authoritative Windows browser and memory baseline from one clean commit**

Require a clean worktree and capture the exact harness commit before either reporter starts:

```powershell
$expectedSha = (git rev-parse HEAD).Trim()
if (git status --porcelain) { throw 'authoritative performance capture requires a clean worktree' }
pnpm perf:results:browser
pnpm perf:results:capture -- -ExpectedSha $expectedSha
node scripts/perf/validate-report.mjs --browser artifacts/feasibility/perf/browser.json --memory artifacts/feasibility/perf/webview2-memory.json --expect-sha $expectedSha --require-clean --windows-only
if (git status --porcelain) { throw 'performance capture modified tracked files' }
```

Expected: both ignored raw reports contain `$expectedSha`, both contain `clean=true`, the raw-pair validator exits 0, and the tracked worktree remains clean.

In `perf-memory` mode, the capture script first binds a private `TcpListener` HTTP handshake at `127.0.0.1:1431`, generates an unguessable path token, sets `YINMI_PERF_HANDSHAKE_URL`, and builds the shell. The listener accepts only token-matching `POST ready` and `POST sequence-complete`, returns the minimal loopback CORS header, and rejects every other method/path. After mount, `BenchmarkApp.svelte` sends `ready` and starts its clock only when the endpoint acknowledges it. It remains empty through acknowledged second 15; at second 16 it assigns the 1000 rows, then selects all, clears all, and animates a complete top-to-bottom scroll. After the final DOM state plus two animation frames it sends `sequence-complete` and remains loaded. The browser mode never starts this clock or handshake.

`capture-windows-baseline.ps1` starts the built app and recursively tracks that process plus every descendant whose executable is the app or WebView2. Relative to the acknowledged ready time—not process launch—it records per-process and summed Private Bytes for five one-second samples at seconds 10–14. It then waits for the authenticated `sequence-complete` event, waits ten additional seconds, and records five more one-second samples. Missing/out-of-order handshakes, a missing process, early exit, unexpected additional app instance, fewer than five samples, timeout, or any residual tracked process after `finally` fails the capture.

The capture writes only the ignored path supplied by `-OutputPath`. It removes machine usernames, the random handshake token and absolute paths. Finalization happens only after both macOS raw reports are present; `perf:results:validate` validates the committed file, current exact scope and evidence-document hashes without modifying them.

Freeze the authority profile in the report:

```text
PERF-WIN-MIN-v1
Windows 10 22H2 x64 build 19045
4 logical processors, 8 GiB RAM
1280x800, 100% scaling, 60 Hz, AC/Balanced
WebView2 Evergreen >=111; exact version recorded
network excluded; nonessential background apps stopped
```

Passing requires loaded median minus empty median `<=104857600`. JS heap is auxiliary only. If this exact environment is unavailable, mark the gate `blocked` rather than substituting a hosted runner.

- [ ] **Step 7: Collect non-authoritative macOS evidence from the same harness commit**

Check out the exact `$expectedSha` from Step 6 with full history on macOS 13.3 Intel and current Apple Silicon. On each host require a clean worktree and run the already committed WebKit project:

```bash
expectedSha="$(git rev-parse HEAD)"
test -z "$(git status --porcelain)"
pnpm exec playwright install webkit
node scripts/perf/run-browser.mjs --project webkit --platform-id macos-13-intel --output artifacts/feasibility/perf/macos-intel.json
# On Apple Silicon use instead:
# node scripts/perf/run-browser.mjs --project webkit --platform-id macos-current-arm64 --output artifacts/feasibility/perf/macos-arm64.json
test -z "$(git status --porcelain)"
```

Transfer the two ignored JSON files to the same ignored paths on the primary validation host. Each records commit, clean flag, OS/WebKit version and samples; RSS is auxiliary, not a replacement for Windows Private Bytes. A report from another commit, dirty checkout, wrong architecture or wrong fixed platform ID is rejected rather than merged.

- [ ] **Step 8: Validate, document and commit the evidence**

```powershell
$expectedSha = (git rev-parse HEAD).Trim()
node scripts/perf/validate-report.mjs --browser artifacts/feasibility/perf/browser.json --memory artifacts/feasibility/perf/webview2-memory.json --macos-intel artifacts/feasibility/perf/macos-intel.json --macos-arm artifacts/feasibility/perf/macos-arm64.json --expect-sha $expectedSha --require-clean
```

Do not rerun any authoritative measurement after creating or editing evidence files. Create the Markdown evidence and ADR `0007` directly from the validated raw set; the ADR states whether fixed-row windowing is required for the production results table and does not select a third-party package. The generated JSON records sanitized environment fields, the Windows authority row plus two macOS rows from all four raw inputs, the shared clean Git SHA, every sample, median/p95/max, both memory sample sets, exact scope digest, Markdown digest and ADR digest.

Run:

```powershell
pnpm perf:results:finalize
pnpm perf:results:validate
pnpm quality
```

```powershell
git add docs/feasibility/result-list-performance.md docs/feasibility/result-list-performance.json docs/decisions/0007-result-list-performance.md
git commit -m "perf: prove 1000-result interaction budgets"
pnpm perf:results:validate
git status --short
```

Expected: final status is clean.

### Task 10: 执行第一阶段硬退出门

**Files:**
- Create: `scripts/check-phase1-gate.mjs`
- Create: `scripts/check-phase1-gate.test.mjs`
- Create: `docs/feasibility/phase-1-results.json`
- Modify: `scripts/feasibility-evidence.mjs`
- Modify: `package.json`
- Modify: `.github/workflows/quality.yml`
- Modify: `.github/workflows/platform-smoke.yml`
- Modify: `docs/feasibility/evidence-scopes.json`
- Modify: `docs/feasibility/toolchain-ci.md`
- Modify: `docs/feasibility/toolchain-ci.json`
- Modify: `docs/feasibility/gd-contract-pagination.md`
- Modify: `docs/feasibility/gd-contract-pagination.json`
- Modify: `docs/feasibility/signature-webview.md`
- Modify: `docs/feasibility/signature-webview.json`
- Modify: `docs/feasibility/network-policy.md`
- Modify: `docs/feasibility/network-policy.json`
- Modify: `docs/feasibility/atomic-commit.md`
- Modify: `docs/feasibility/atomic-commit.json`
- Modify: `docs/feasibility/media-containers.md`
- Modify: `docs/feasibility/media-containers.json`
- Modify: `docs/feasibility/updater-exit-barrier.md`
- Modify: `docs/feasibility/updater-exit-barrier.json`
- Modify: `docs/feasibility/result-list-performance.md`
- Modify: `docs/feasibility/result-list-performance.json`
- Modify: `docs/decisions/0001-gd-pagination.md` through `docs/decisions/0007-result-list-performance.md`
- Modify: `README.md`

**Interfaces:**
- Consumes: Tasks 1–9 evidence and ADRs.
- Produces: generated `docs/feasibility/phase-1-results.json` and `pnpm phase1:gate`; only locally passing validation plus successful stable checks on the exact final commit permits writing the Milestone 2 plan.

- [ ] **Step 1: Write the failing gate-validator tests**

Create Node built-in tests against a temporary Git repository and exported `collectPhase1Results`/`validatePhase1Results` functions. Configure test-only local Git identity inside each temporary repository; never depend on the developer's global Git config. Cases:

```text
all eight validated companions, exact platform matrices and gate-specific checks -> pass
missing ID -> fail
duplicate ID -> fail
blocked or design-change-required -> fail
empty Markdown or ADR -> fail
missing or extra platform ID -> fail
nonzero command exit or failed required check -> fail
stale scope digest after one-byte source change -> fail
missing, extra or substituted exact scope-manifest path -> fail
testedCommit not an ancestor or dirty-scope evidence -> fail
one-byte ADR change after evidence generation -> fail
signature bridge present, missing WebView2 111 baseline, unknown filter mode or direct/nested resource canary hit -> fail
network peer pin/body limit/proxy check false -> fail
atomic winner count other than one -> fail
media negative family accepted -> fail
updater fake-only wait result, early exit/install, or missing numeric timeout/feedback -> fail
performance inputs with different SHA, dirty flag, missing macOS row, stale scope or exceeded budget -> fail
missing required decision or result file -> fail
wrong design commit -> fail
handwritten aggregate pass with one invalid companion -> fail
```

Run:

```powershell
node --test scripts/check-phase1-gate.test.mjs
```

Expected: FAIL because the validator is absent.

- [ ] **Step 2: Implement the strict gate validator**

Implement `scripts/check-phase1-gate.mjs` with these constants:

```js
export const DESIGN_COMMIT = '5893d4340a4815677da79f74223642ac855519e7';
export const REQUIRED = new Map([
  ['toolchain-ci', { result: 'docs/feasibility/toolchain-ci.json', decisions: [], platforms: ['windows-x64', 'macos-intel', 'macos-arm64'], rule: 'toolchain' }],
  ['gd-contract-pagination', { result: 'docs/feasibility/gd-contract-pagination.json', decisions: ['docs/decisions/0001-gd-pagination.md'], platforms: [], rule: 'gd' }],
  ['signature-webview', { result: 'docs/feasibility/signature-webview.json', decisions: ['docs/decisions/0002-signature-webview.md'], platforms: ['windows-10-webview2-111-x64', 'windows-11-x64', 'macos-13-intel', 'macos-current-arm64'], rule: 'signature' }],
  ['network-policy', { result: 'docs/feasibility/network-policy.json', decisions: ['docs/decisions/0003-network-ssrf-policy.md'], platforms: ['windows-x64', 'macos-intel', 'macos-arm64'], rule: 'network' }],
  ['atomic-commit', { result: 'docs/feasibility/atomic-commit.json', decisions: ['docs/decisions/0004-atomic-no-clobber.md'], platforms: ['windows-ntfs-x64', 'macos-apfs-intel', 'macos-apfs-arm64'], rule: 'atomic' }],
  ['media-containers', { result: 'docs/feasibility/media-containers.json', decisions: ['docs/decisions/0005-media-container-allowlist.md'], platforms: ['windows-x64', 'macos-intel', 'macos-arm64'], rule: 'media' }],
  ['updater-exit-barrier', { result: 'docs/feasibility/updater-exit-barrier.json', decisions: ['docs/decisions/0006-updater-exit-barrier.md'], platforms: ['windows-x64', 'macos-intel', 'macos-arm64'], rule: 'updater' }],
  ['result-list-performance', { result: 'docs/feasibility/result-list-performance.json', decisions: ['docs/decisions/0007-result-list-performance.md'], platforms: ['windows-10-authority', 'macos-13-intel', 'macos-current-arm64'], rule: 'performance' }],
]);
```

Import and call `validateEvidence` for the first seven companions and the Task 9 report validator for performance. Load `evidence-scopes.json` and require exact set equality for each gate; raw or companion JSON cannot choose its own scope. Require exact platform-set equality, current nonempty Markdown and ADR hashes, current scope hash, valid clean ancestor `testedCommit`, and each gate-specific predicate from the Machine-Readable Evidence Contract. `collectPhase1Results` derives aggregate entries from validated companions; no caller can supply status `pass`. Support only:

```text
node scripts/check-phase1-gate.mjs --write docs/feasibility/phase-1-results.json
node scripts/check-phase1-gate.mjs --check docs/feasibility/phase-1-results.json
```

`--write` atomically generates the aggregate with each result path, tested commit, scope hash, Markdown hash and ADR path/hash pairs. `--check` first recollects current companions and requires byte-for-byte semantic equality with the committed aggregate. Print one line per gate and `phase 1 gate: PASS` only at the end.

- [ ] **Step 3: Prove the validator red and green**

Run the Node tests. Expected: all synthetic cases pass. Then check the repository before the generated aggregate exists:

```powershell
node scripts/check-phase1-gate.mjs --check docs/feasibility/phase-1-results.json
```

Expected: FAIL because the aggregate file is absent. This proves ordinary validation cannot be bypassed by code completion or handwritten statuses alone.

- [ ] **Step 4: Add platform feature tests to CI**

Extend `platform-windows`, `platform_macos_intel`, and `platform_macos_arm` to run:

```text
cargo test --manifest-path src-tauri/Cargo.toml --features feasibility --test atomic_commit -- --nocapture --test-threads=1
cargo test --manifest-path src-tauri/Cargo.toml --features feasibility --test media_probe -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml --features feasibility --test network_policy -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml --features feasibility signature_webview -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml --features feasibility gd_live -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml --features feasibility --test updater_probe -- --nocapture --test-threads=1
```

Do not add live GD, absolute performance or actual updater probes to ordinary hosted CI. Add `"test:phase1-gate": "node --test scripts/check-phase1-gate.test.mjs"` to package scripts and append that deterministic test, not strict historical evidence validation, to ordinary `quality`; `quality.yml` continues to run `pnpm quality`. In `platform-windows`, after the default debug build, add a final step guarded by `hashFiles('docs/feasibility/phase-1-results.json') != ''` that runs `pnpm phase1:gate`; the pre-aggregate gate-mechanics commit skips it, while the final evidence commit must execute it. The macOS jobs run the feature suites above, so their aggregate check remains the stable macOS proof.

- [ ] **Step 5: Commit gate mechanics and verify their exact code commit in CI**

Add the package script before committing:

```json
{
  "phase1:gate": "node scripts/check-phase1-gate.mjs --check docs/feasibility/phase-1-results.json && pnpm verify:evidence && pnpm perf:results:validate && pnpm quality && cargo fmt --manifest-path src-tauri/Cargo.toml --check && cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets --all-features -- -D warnings && cargo test --manifest-path src-tauri/Cargo.toml --all-targets --all-features && pnpm tauri build --debug --no-bundle && pnpm verify:default-artifacts"
}
```

Update the exact scope manifest before committing: `toolchain-ci` now includes both workflows, package/lock, evidence helper/schema/scope tests and gate validator/tests; every other entry must enumerate its final Task 1–9 shared manifests and gate-specific files, including `Cargo.toml`/`Cargo.lock` wherever they affect the binary and the complete performance harness scope from Task 9. Keep README at `Phase 1 final verification pending`; do not claim pass yet. Format and commit scripts, package, scope manifest and workflows without changing the aggregate:

```powershell
pnpm format
cargo fmt --manifest-path src-tauri/Cargo.toml --all
pnpm quality
node --test scripts/check-phase1-gate.test.mjs
git add scripts/check-phase1-gate.mjs scripts/check-phase1-gate.test.mjs scripts/feasibility-evidence.mjs package.json pnpm-lock.yaml .github/workflows/quality.yml .github/workflows/platform-smoke.yml docs/feasibility/evidence-scopes.json README.md
git commit -m "ci: enforce phase one evidence gate"
git status --short
```

Push the `phase1/**` branch and wait for push-event `quality`, `platform-windows`, and aggregate `platform-macos` to succeed with `head_sha` exactly equal to this code commit. The conditional full gate remains skipped because the aggregate does not exist. Refresh only ignored `artifacts/feasibility/toolchain-ci.raw.json` with these runs; do not edit tracked evidence yet.

- [ ] **Step 6: Re-observe every gate on the frozen code commit, then generate the aggregate**

Set `$finalCodeSha` to the Step 5 commit and require a clean tracked worktree. Before editing any Markdown, JSON or ADR, check out that exact SHA with full history on every required host and rerun all raw observations from Tasks 4–9: four-row signature isolation including fixed WebView2 111, three GD cases in separate quota windows, network on three architectures, NTFS/APFS atomic races, media round trips, both real updater profiles, and the Windows plus two macOS performance captures. Transfer only ignored sanitized raw JSON to the primary host. All raw inputs must name `$finalCodeSha`, record `clean=true`, and use unchanged scoped files. The Step 5 CI runs are the toolchain raw input.

While the tracked tree is still clean, validate the four performance inputs together:

```powershell
node scripts/perf/validate-report.mjs --browser artifacts/feasibility/perf/browser.json --memory artifacts/feasibility/perf/webview2-memory.json --macos-intel artifacts/feasibility/perf/macos-intel.json --macos-arm artifacts/feasibility/perf/macos-arm64.json --expect-sha $finalCodeSha --require-clean
```

If any observation is missing, `blocked` or `design-change-required`, stop Phase 1, keep README at `Phase 1 final verification pending`, and resolve that gate through a design amendment before continuing. Otherwise refresh all eight Markdown reports and all affected ADRs from the final raw set. Build every common companion and finalize performance:

```powershell
node scripts/feasibility-evidence.mjs build --input artifacts/feasibility/toolchain-ci.raw.json --markdown docs/feasibility/toolchain-ci.md --output docs/feasibility/toolchain-ci.json
node scripts/feasibility-evidence.mjs build --input artifacts/feasibility/gd-contract-pagination.raw.json --markdown docs/feasibility/gd-contract-pagination.md --output docs/feasibility/gd-contract-pagination.json
node scripts/feasibility-evidence.mjs build --input artifacts/feasibility/signature-webview.raw.json --markdown docs/feasibility/signature-webview.md --output docs/feasibility/signature-webview.json
node scripts/feasibility-evidence.mjs build --input artifacts/feasibility/network-policy.raw.json --markdown docs/feasibility/network-policy.md --output docs/feasibility/network-policy.json
node scripts/feasibility-evidence.mjs build --input artifacts/feasibility/atomic-commit.raw.json --markdown docs/feasibility/atomic-commit.md --output docs/feasibility/atomic-commit.json
node scripts/feasibility-evidence.mjs build --input artifacts/feasibility/media-containers.raw.json --markdown docs/feasibility/media-containers.md --output docs/feasibility/media-containers.json
node scripts/feasibility-evidence.mjs build --input artifacts/feasibility/updater-exit-barrier.raw.json --markdown docs/feasibility/updater-exit-barrier.md --output docs/feasibility/updater-exit-barrier.json
pnpm perf:results:finalize
node scripts/feasibility-evidence.mjs check-existing docs/feasibility
pnpm perf:results:validate
node scripts/check-phase1-gate.mjs --write docs/feasibility/phase-1-results.json
node scripts/check-phase1-gate.mjs --check docs/feasibility/phase-1-results.json
```

Expected: all eight companions bind the same final code/harness state where their scopes overlap, all current Markdown/ADR hashes validate, the aggregate contains no caller-authored status, and the final line is `phase 1 gate: PASS`. Only now update README from pending to `Phase 1 passed`; name writing the Milestone 2 plan as the next authorized activity. Run `pnpm phase1:gate` before staging. Any failure leaves README pending and forbids the closing commit.

- [ ] **Step 7: Commit the generated closure, re-verify clean, and require final-SHA CI**

```powershell
pnpm format:check
cargo fmt --manifest-path src-tauri/Cargo.toml --check
pnpm phase1:gate
git add docs/feasibility docs/decisions README.md
git commit -m "docs: close phase one feasibility gate"
pnpm phase1:gate
git status --short
```

Expected: the gate passes both before and after the commit and the worktree is clean. Push the `phase1/**` branch, record this closing commit's exact SHA, and wait for push-event `quality`, `platform-windows`, and aggregate `platform-macos` to succeed with that same `head_sha`. The Windows conditional `pnpm phase1:gate` must run rather than skip because the aggregate now exists. Do not substitute a PR merge SHA, amend the commit or make further tracked edits after those checks; any edit creates a new final SHA and requires another complete final-SHA run. Only after the three checks succeed is Phase 1 complete and a separate Milestone 2 implementation plan allowed.

## Spec Coverage Map

| Design area | Phase 1 task |
| --- | --- |
| §1–§3 identity, platform, toolchain | Tasks 1–2 |
| §4 trust boundary | Tasks 1, 4, 5, 8 |
| §5.1–§5.4 API/body/normalization/pagination | Tasks 3–4 |
| §5.5 zero-capability signature WebView | Task 4 |
| §5.6 HTTPS/SSRF/DNS/redirect bounds | Task 5 |
| §6.1–§6.4 product state, concurrency, DTO and errors | Deferred to Milestones 2–4 after the gate |
| §6.5 exit/update barrier feasibility | Task 8 |
| §7.1/§7.3 storage and dedupe | Deferred to Milestone 4 after the gate |
| §7.2 actual media format | Task 7 |
| §7.4 atomic no-clobber | Task 6 |
| §7.5 attachment round-trip feasibility | Task 7; production attachment flow is Milestone 4 |
| §7.6 logs | Deferred to Milestones 2 and 5 after the gate |
| §8.1 startup page | Deferred to Milestone 5 after the gate |
| §8.2 default Netease source and 1–1000 boundary | Tasks 3–4; production controls are Milestone 3 |
| §8.3 1000-result feasibility | Task 9; production result UI is Milestone 3 |
| §8.4–§8.8 remaining product UI and accessibility | Deferred to Milestones 4–5 after the gate |
| §9 updater target behavior | Task 8 |
| §10 release artifacts, signing and publishing | Deferred to Milestone 6 after the gate |
| §11 automated checks and CI | Tasks 1–2, 10 |
| §11 manual product/release acceptance | Deferred to Milestones 5–6 after the gate |
| §12 hard feasibility gate | Task 10 |
| Appendix A request/response fixtures, including HTTP/error/body bounds | Tasks 3–5 |
| Appendix B relevant initial bounds | Tasks 4, 5, 8, 9 |

## Official References

- [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/)
- [Tauri with Vite](https://v2.tauri.app/start/frontend/vite/)
- [Tauri GitHub Actions pipelines](https://v2.tauri.app/distribute/pipelines/github/)
- [GitHub-hosted runner labels and architectures](https://docs.github.com/en/actions/reference/runners/github-hosted-runners)
- [Tauri capabilities](https://v2.tauri.app/security/capabilities/)
- [Tauri permissions](https://v2.tauri.app/security/permissions/)
- [Tauri runtime authority](https://v2.tauri.app/security/runtime-authority/)
- [Tauri WebviewWindowBuilder 2.11.5](https://docs.rs/tauri/2.11.5/tauri/webview/struct.WebviewWindowBuilder.html)
- [Tauri WebviewWindow 2.11.5](https://docs.rs/tauri/2.11.5/tauri/webview/struct.WebviewWindow.html)
- [Tauri `with_webview`](https://docs.rs/tauri/2.11.5/tauri/webview/struct.WebviewWindow.html#method.with_webview)
- [Tauri macOS `with_webview_configuration`](https://docs.rs/tauri/2.11.5/tauri/webview/struct.WebviewWindowBuilder.html#method.with_webview_configuration)
- [Tauri updater guide](https://v2.tauri.app/plugin/updater/)
- [Tauri Update API](https://docs.rs/tauri-plugin-updater/2.10.1/tauri_plugin_updater/struct.Update.html)
- [Tauri UpdaterBuilder API](https://docs.rs/tauri-plugin-updater/2.10.1/tauri_plugin_updater/struct.UpdaterBuilder.html)
- [Reqwest ClientBuilder](https://docs.rs/reqwest/0.13.4/reqwest/struct.ClientBuilder.html)
- [Reqwest response peer address](https://docs.rs/reqwest/0.13.4/reqwest/struct.Response.html#method.remote_addr)
- [WebView2 legacy `AddWebResourceRequestedFilter`](https://learn.microsoft.com/en-us/microsoft-edge/webview2/reference/win32/icorewebview2#addwebresourcerequestedfilter)
- [WebView2 source-kind filter (`ICoreWebView2_22`)](https://learn.microsoft.com/en-us/microsoft-edge/webview2/reference/win32/icorewebview2_22#addwebresourcerequestedfilterwithrequestsourcekinds)
- [WebView2 Fixed Version distribution](https://learn.microsoft.com/en-us/microsoft-edge/webview2/concepts/distribution#the-fixed-version-runtime-distribution-mode)
- [Apple `WKContentRuleList`](https://developer.apple.com/documentation/webkit/wkcontentrulelist)
- [Microsoft `SetFileInformationByHandle`](https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-setfileinformationbyhandle)
- [Microsoft `FILE_RENAME_INFO`](https://learn.microsoft.com/en-us/windows/win32/api/winbase/ns-winbase-file_rename_info)
- [Apple APFS safe-save APIs](https://developer.apple.com/library/archive/documentation/FileManagement/Conceptual/APFS_Guide/ToolsandAPIs/ToolsandAPIs.html)
- [Lofty Probe 0.24](https://docs.rs/lofty/0.24.0/lofty/probe/struct.Probe.html)
- [Vite build target](https://vite.dev/guide/build)
- [Svelte documentation](https://svelte.dev/docs/svelte/overview)
- [Playwright Page API](https://playwright.dev/docs/api/class-page)
- [Long Tasks API](https://www.w3.org/TR/longtasks-1/)
- [GD official page](https://music.gdstudio.xyz/)
- [GD current AJAX script](https://music.gdstudio.xyz/js/ajax.js?v=20260616)
- [GD current encoding script](https://music.gdstudio.xyz/js/functions.js?v=20260616)
- [GD current page config](https://music.gdstudio.xyz/js/player.js?v=20260616)
- [GD current dynamic signing script](https://music.gdstudio.xyz/js/crc32.min.js?v=20260616)
