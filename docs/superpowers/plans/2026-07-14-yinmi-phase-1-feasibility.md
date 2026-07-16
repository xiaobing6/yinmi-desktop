# 音觅第一阶段：可行性验证与工程脚手架 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 从已确认设计提交 `782b30d8eb1075cce708ddef878cd236d2fa7dc2` 建立可重复构建的 Tauri 2/Svelte 5 工程，并用可审计原型关闭协议分页、零应用能力 raw WRY 签名宿主、SSRF/DNS、原子无覆盖提交、媒体容器、更新取消和 1000 条结果性能风险。

**Architecture:** 第一阶段只建立最小产品壳、纯函数契约模块、受 Cargo feature 与 Vite mode 双重隔离的可行性探针，以及 Windows/macOS 基础 CI。签名原型使用无 managed WebView 的独立 Tauri 原生宿主窗口，在 UI 主线程注册表中持有 raw WRY 与平台策略；可复用的协议代码进入 `music`，未验证的平台机制全部进入 `feasibility`。原始探针输出只进入被忽略的 `artifacts/feasibility/`，仓库仅提交脱敏结论、ADR 和自动门控清单。

**Tech Stack:** Node.js 24 LTS、pnpm 11.7.0、Svelte 5.56.5、TypeScript 6.0.3、Vite 8.1.4、Tailwind CSS 4.3.2、Vitest 4.1.10、Rust 1.97.0、Tauri 2.11.5、WRY 0.55.1、WebView2/WKWebView、Tokio、Reqwest/rustls、Serde、Thiserror、Lofty、GitHub Actions。

## Global Constraints

- 设计来源固定为 `docs/plans/2026-07-14-music-desktop-design.md`，基线提交为 `782b30d8eb1075cce708ddef878cd236d2fa7dc2`；任何改变产品行为、安全不变量、外部契约或发布结果的发现都必须先回修设计。
- 在独立 worktree 的 `phase1/<slug>` 分支中执行，不直接在 `master` 上开发；开始执行时先使用 `superpowers:using-git-worktrees`。该分支前缀也是平台 CI 获取实现分支原始提交 SHA 的固定触发契约。
- Node.js 使用 24 LTS，`packageManager` 固定 `pnpm@11.7.0` 并提交 `pnpm-lock.yaml`。
- TypeScript 固定为 `6.0.3`，与 `typescript-eslint@8.64.0` 的官方支持范围兼容；在 ESLint 工具链正式支持 TypeScript 7 前不得升至 7.x。
- Rust 使用 1.97.0，提交 `rust-toolchain.toml` 与 `Cargo.lock`；Tauri 使用主版本 2。
- 用户可见名称与日志目录名为“音觅”，工程名为 `yinmi`，Bundle Identifier 为 `io.github.xiaobing6.yinmi`，首版版本为 `0.1.0`。
- Windows 支持 Windows 10 22H2/Windows 11 x64 与 WebView2 `111.0.1661.0` 以上；macOS 支持 13.3 以上 Intel/Apple Silicon，最终产物为 Universal。
- 前端构建目标为 Chrome 111 与 Safari 16.4；首版只使用简体中文集中式文案。
- 除设计 §5.5 明确允许的签名官方页及其官方来源子资源由隔离 raw WRY 加载外，所有音乐、媒体和更新网络请求由 Rust 发起；主前端不得获得通用网络、文件系统或 Shell 权限。
- 隐藏签名宿主不包含 Tauri managed WebView，不匹配任何 capability，也不配置 `remote.urls`。应用不得向 raw WRY 注册初始化脚本、IPC handler、custom protocol、插件、事件、文件或系统能力；WRY 0.55.1 自带但未绑定应用 handler 的 inert `window.ipc` shim 可以存在，但不得产生响应、命令、事件或状态副作用。
- 仅 feature-only 隔离探针可创建同样零应用能力的 raw WRY counterfactual，并把策略允许源替换为 runner 持有证书的受控本机 TLS 源，用于逐向量证明“无规则可达/生产规则拦截”和允许首跳后的 redirect；该注入器、证书、源和无规则模式不得进入默认构建、正式签名宿主或 live GD 探针。
- GD API 固定为 `https://music.gdstudio.xyz/api.php`；在线探针仍受全局 5 分钟 50 次请求限制，不把原始第三方响应提交到仓库。
- 搜索默认音源为网易云音乐，搜索数量初始值为 20、范围为 1–1000；第一阶段只探测真实分页行为，不实现正式搜索 UI。
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
  "schemaVersion": 2,
  "gateId": "signature-webview",
  "status": "pass",
  "designCommit": "782b30d8eb1075cce708ddef878cd236d2fa7dc2",
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
| `gd-contract-pagination` | default `GdSource::NeteaseMusic`, initial count 20 and typed 1–1000 search-count boundary, six body fixtures, strict mixed-record parser, 429/other non-2xx/5 MiB+1 tests, all three live cases, numeric safety page limit `<=50` |
| `signature-webview` | Exact four-ID `checks.byPlatform` map and derived aggregates; runner-verified host OS plus child architecture/translation, exact runtime versions plus platform-correlated runtime/policy modes; Windows 10 uses the lowest available WebView2 111.0.1661.x fixed runtime; raw WRY host true, managed WebView false, zero application IPC/capability side effects, policy-before-first-network-navigation and official-finish-before-poll, official-only origins, explicit nonpersistent storage with no recovery, exact 20-key per-vector attempt/availability/barrier/hit results with zero canary-server hits, timeout/retry/fault/late-callback isolation, composite native/policy/tombstone ordinary exit and leak-free lifecycle |
| `network-policy` | Windows x64, macOS Intel and macOS ARM rows; all-address-set, redirect, peer-pin, body-limit and proxy-disabled checks true |
| `atomic-commit` | NTFS Windows plus APFS Intel/ARM rows; exactly one winner, zero overwrite, zero leftovers, cancel linearization true |
| `media-containers` | Windows x64, macOS Intel and macOS ARM rows; MP3/FLAC round trips true and `negativeFamiliesRejected` is exactly the unique string set `mp2,aac,mp4,ogg,opus,wav,truncated` |
| `updater-exit-barrier` | Exact Windows x64, macOS Intel and macOS ARM rows with verified host/child architecture; real drop-future and real bounded wait-only classification plus final-SHA production-policy validation, exact derived timeout/size/throughput/text contract, no early exit/install, nonce-bound two-profile ordinal traces, and a derived active-signature-host cleanup map proving a host existed before the exit request, native/manager/policy-store/tombstone/TLS checks preceded the `app.exit()` invocation boundary, and the process exited afterward on every platform |
| `result-list-performance` | its own schema; shared clean harness SHA, all budgets, Windows authority profile and macOS auxiliary rows |

Code affecting a gate is committed before platform/manual evidence is collected. Evidence Markdown/JSON and ADR are committed separately after the helper validates them. Schema version 2 and the revised design identity intentionally invalidate every schema-v1 common-envelope companion; they are never hand-upgraded. The performance companion continues to use its independent strict schema, but common-scope and final-tested-SHA changes still force a fresh capture/finalization. A missing platform is `blocked`; an empty, stale, dirty-tree or hand-edited result cannot become `pass`. Ordinary `pnpm quality` runs evidence-validator unit tests but deliberately does not validate previously committed observations: Tasks 3–10 share manifests and configs, so doing that would self-lock the branch before refreshed evidence could be collected. Strict current-scope validation runs immediately after each new companion is built and in `pnpm phase1:gate`. Task 10 freezes the final gate-mechanics commit, reruns all eight observations against it, and rebuilds every companion before generating the aggregate.

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
│  ├─ verify-signature-host.mjs
│  ├─ verify-signature-host.test.mjs
│  ├─ run-signature-lifecycle-probe.mjs
│  ├─ run-signature-lifecycle-probe.test.mjs
│  ├─ feasibility-evidence.mjs
│  ├─ feasibility-evidence.test.mjs
│  ├─ check-phase1-gate.mjs
│  ├─ check-phase1-gate.test.mjs
│  ├─ generate-media-fixtures.mjs
│  ├─ slow-update-server.mjs
│  ├─ run-updater-exit-probe.mjs
│  ├─ run-updater-exit-probe.test.mjs
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
│  │  │  ├─ signature_host.rs
│  │  │  ├─ signature_probe.rs
│  │  │  ├─ signature_webview.rs
│  │  │  ├─ webview_resource_policy.rs
│  │  │  ├─ webview_resource_policy/
│  │  │  │  ├─ macos.rs
│  │  │  │  └─ windows.rs
│  │  │  ├─ updater_policy.rs
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
- Consumes: design identity and platform floors from commit `782b30d8eb1075cce708ddef878cd236d2fa7dc2`.
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

Phase 1 feasibility validation is in progress against design commit `782b30d8eb1075cce708ddef878cd236d2fa7dc2`. Product feature implementation beyond the Phase 1 gate is not yet authorized.
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
- Produces: `EncodedComponent`, `SignatureValue`, `GdSource`, bounded `SearchCount`, `GdOperation`, `render_form_body`, all four response parsers, and `PaginationProbe`; Task 4 must consume these exact types rather than rebuilding strings.

- [ ] **Step 1: Add exact fixture data and failing form-body tests**

Add these dependencies to `src-tauri/Cargo.toml`:

```toml
thiserror = "2"
url = "2"
```

Create `src-tauri/tests/fixtures/gd/README.md` first. It identifies every fixture as a hand-authored minimal protocol sample historically derived from design commit `5893d4340a4815677da79f74223642ac855519e7` and official page version `2026.06.16`, maps each filename to the rule it proves, and states that no raw third-party song row or signature is stored. This historical fixture provenance remains unchanged; all current evidence envelopes use authoritative design commit `782b30d8eb1075cce708ddef878cd236d2fa7dc2`.

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
    render_form_body, EncodedComponent, GdOperation, GdSource, SearchCount, SearchOperation,
    SignatureValue,
};

const SIG: &str = "fixture-signature";

#[test]
fn renders_six_official_bodies_in_exact_order() {
    let name = EncodedComponent::encode("周杰伦");
    let id = EncodedComponent::encode("123456");
    let signature = SignatureValue::try_from(SIG).unwrap();

    let cases = [
        (
            GdOperation::Search { operation: SearchOperation::Track, count: SearchCount::try_from(20).unwrap(), source: GdSource::NeteaseMusic, page: 1, name: name.clone() },
            "types=search&count=20&source=netease&pages=1&name=%E5%91%A8%E6%9D%B0%E4%BC%A6&s=fixture-signature",
        ),
        (
            GdOperation::Search { operation: SearchOperation::Album, count: SearchCount::try_from(20).unwrap(), source: GdSource::NeteaseMusic, page: 1, name: name.clone() },
            "types=search_album&count=20&source=netease&pages=1&name=%E5%91%A8%E6%9D%B0%E4%BC%A6&s=fixture-signature",
        ),
        (
            GdOperation::Search { operation: SearchOperation::Playlist, count: SearchCount::try_from(20).unwrap(), source: GdSource::NeteaseMusic, page: 1, name },
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

Add an exact search-count boundary test: `SearchCount::DEFAULT.get() == 20`; `SearchCount::try_from(1)` and `SearchCount::try_from(1000)` succeed and preserve those values; `0` and `1001` return `ContractError::InvalidSearchCount`. Every `GdOperation::Search` constructor, fixture and live probe accepts `SearchCount`, never a bare integer, so the 1–1000 rule cannot be bypassed at the request-rendering boundary.

Run:

```powershell
cargo test --manifest-path src-tauri/Cargo.toml --test gd_contract renders_ -- --nocapture
```

Expected: FAIL because `music::contract` does not exist.

- [ ] **Step 2: Implement the encoded type boundary and body renderer**

Create `src-tauri/src/music/mod.rs` with `pub mod contract;`, export `pub mod music;` from `lib.rs`, and implement this public surface in `contract.rs`:

```rust
use std::{collections::HashSet, fmt::{self, Write}};
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SearchCount(u16);

impl SearchCount {
    pub const DEFAULT: Self = Self(20);
    pub const MIN: u16 = 1;
    pub const MAX: u16 = 1000;
    pub const fn get(self) -> u16 { self.0 }
}

impl TryFrom<u16> for SearchCount {
    type Error = ContractError;

    fn try_from(value: u16) -> Result<Self, Self::Error> {
        (Self::MIN..=Self::MAX)
            .contains(&value)
            .then_some(Self(value))
            .ok_or(ContractError::InvalidSearchCount)
    }
}

impl fmt::Display for SearchCount {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.0, formatter)
    }
}

#[derive(Clone, Debug)]
pub enum GdOperation {
    Search { operation: SearchOperation, count: SearchCount, source: GdSource, page: u16, name: EncodedComponent },
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
    #[error("search count must be between 1 and 1000")]
    InvalidSearchCount,
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

### Task 4: 验证零应用能力 raw WRY 签名宿主与真实分页

**Files:**
- Create: `src-tauri/src/feasibility/mod.rs`
- Create: `src-tauri/src/feasibility/signature_webview.rs`
- Create: `src-tauri/src/feasibility/signature_host.rs`
- Create: `src-tauri/src/feasibility/signature_probe.rs`
- Create: `src-tauri/src/feasibility/webview_resource_policy.rs`
- Create: `src-tauri/src/feasibility/webview_resource_policy/windows.rs`
- Create: `src-tauri/src/feasibility/webview_resource_policy/macos.rs`
- Create: `src-tauri/src/feasibility/gd_live.rs`
- Modify: `.prettierignore`
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
- Create: `scripts/verify-signature-host.mjs`
- Create: `scripts/verify-signature-host.test.mjs`
- Create: `scripts/run-signature-lifecycle-probe.mjs`
- Create: `scripts/run-signature-lifecycle-probe.test.mjs`
- Modify: `scripts/feasibility-evidence.mjs`
- Modify: `scripts/feasibility-evidence.test.mjs`
- Modify: `docs/feasibility/evidence.schema.json`
- Modify: `docs/feasibility/evidence-scopes.json`
- Modify: `package.json`
- Create: `src/lib/feasibility/FeasibilityPanel.svelte`
- Create: `src/lib/feasibility/GdProbe.svelte`
- Modify: `src/App.svelte`
- Modify: `src/App.test.ts`
- Modify: `README.md`
- Create: `docs/feasibility/gd-contract-pagination.md`
- Create: `docs/feasibility/gd-contract-pagination.json`
- Create: `docs/feasibility/signature-webview.md`
- Create: `docs/feasibility/signature-webview.json`
- Create: `docs/decisions/0001-gd-pagination.md`
- Create: `docs/decisions/0002-signature-webview.md`

**Interfaces:**
- Consumes: Task 3 `EncodedComponent`、`SearchCount`、`GdOperation`、`PaginationProbe`、`render_form_body`。
- Produces: concrete `SignatureRuntime` facade、raw WRY host actor、`run_gd_probe`、`SignatureInitReport`、`IsolationReport` and `ProtocolProbeReport`；不产生正式搜索命令。
- Thread boundary: background tasks normally carry only `AppHandle<tauri::Wry>`、generation/operation IDs、strings、counters and serializable DTOs. The sole host-creation exception is the `Send` Tauri `Window` handle returned by the non-event-loop builder and immediately moved into a UI-thread closure. After that handoff, the host, `wry::WebView`, WebView2 COM and WK/ObjC objects remain in the UI-thread registry for their entire lifetime.
- Security boundary: pinned WRY 0.55.1 is expected to expose its inert `window.ipc` shim, and the probe must observe it without treating its mere existence as failure. Passing still requires no Tauri globals, no application initialization script, no application-bound IPC handler, no response, no command/event/state side effect, and no capability matching the host.

The GD companion's `checks.searchDefaultsAndBounds` is a required exact-key object with `additionalProperties: false`:

```json
{
  "defaultInternalCode": "netease_music",
  "defaultDisplayName": "网易云音乐",
  "defaultWireValue": "netease",
  "defaultCount": 20,
  "minimumCount": 1,
  "maximumCount": 1000,
  "boundaryTestsPassed": true,
  "singleCount1000RequestedCount": 1000,
  "singleCount1000ApiRequests": 1
}
```

The helper derives the first seven fields from the typed contract test result and the last two from the live case; raw input cannot override constants. Missing/extra fields, a default-source/count mismatch, a renderable value outside 1–1000, or a count-1000 case that made other than one API request fails the GD gate.

**Execution note:** this worktree already contains an interrupted Task 4 draft. Preserve the approved `.prettierignore` change and reusable pure origin/signature/bootstrap/eval/counter tests. Replace the managed-WebView installer and every cross-thread COM/WK guard with patches; do not reset the worktree, discard unrelated user changes, or stage files outside the explicit commit list.

- [ ] **Step 1: Migrate the design/evidence identity and write failing schema tests**

First update the seven common-envelope gates from schema version 1 to 2 and design commit `782b30d8eb1075cce708ddef878cd236d2fa7dc2`. Do not hand-edit any existing companion to make it look current: `docs/feasibility/toolchain-ci.json` and every other old companion remain intentionally stale until Task 10 re-observes and rebuilds all gates. The performance gate remains on its independent schema and is refreshed in Task 10 because its scope/final SHA changes. Keep the Task 3 fixture README's old SHA as the historical source of the protocol fixtures; it is not the current evidence identity.

Add failing tests in `scripts/feasibility-evidence.test.mjs` that require:

```text
schemaVersion is exactly 2
designCommit is exactly 782b30d8eb1075cce708ddef878cd236d2fa7dc2
gd-contract-pagination requires exact checks.searchDefaultsAndBounds and rejects any default/count/live-request mismatch
signature-webview rejects the legacy ipcBridgeAbsent key
usesTauriManagedWebView must be false
all required true/zero/set-valued checks are present with exact types
resourcePolicyModes values belong to the fixed platform-mode enum
runtimeModes values are native-host-raw-wry-0.55.1
webviewRuntimeVersions has the exact four platform keys and nonempty exact versions
hostPlatform, hostArch, osVersion, binaryTargetOs, binaryTargetArch and translatedProcess match the fixed four-ID host/child matrix
resourceVectorResults has exactly the fixed unique vector keys and exact per-vector result objects
resourceVectorsCovered is derived from those keys and is exactly the fixed unique set
crossOriginCanaryServerHits is exactly 0 on every platform
checks.byPlatform has exactly the four platform keys and no extra fields
```

The exact signature checks are:

```text
rawWryHost=true
usesTauriManagedWebView=false
tauriGlobalsAbsent=true
applicationInitializationScriptsAbsent=true
applicationIpcHandlerAbsent=true
inertWryShimPresent=true
hiddenIpcCanaryDeltaZero=true
hiddenIpcProducedNoResponse=true
appStateUnchanged=true
capabilityMatchAbsent=true
policyInstalledBeforeFirstNetworkNavigation=true
officialFinishedBeforePolling=true
officialOnlyOrigins=true
storageNonPersistent=true
newInstanceStorageRecovered=false
restartStorageRecovered=false
timeoutCheck=true
retryCheck=true
policyFaultInvalidatesInstance=true
lateCallbackIsolated=true
destroyConfirmedBeforeRetry=true
resourcePolicyCleanupAcknowledged=true
policyTombstonesEmptyBeforeExit=true
lifecycleNoMonotonicGrowth=true
noOrphanHostWindows=true
visibleWindowLeakAbsent=true
unexpectedActivationAbsent=true
ordinaryExitCleanupAcknowledged=true
crossOriginCanaryServerHits=0
```

The exact resource vector set is:

```text
document, iframe, script, style, image, media, fetch, xhr,
worker, service_worker, websocket, sse, beacon, redirect,
popup, download, top_level_data, top_level_blob, top_level_file,
top_level_custom_protocol
```

`checks.byPlatform` is the authoritative observation map. Its keys are exactly the four platform IDs, and each value has exactly this key set with `additionalProperties: false`. This is a Windows-row example; host/child and platform-nullability differences are fixed immediately below:

```json
{
  "hostPlatform": "win32",
  "hostArch": "x64",
  "osVersion": "10.0.19045",
  "binaryTargetOs": "windows",
  "binaryTargetArch": "x86_64",
  "translatedProcess": null,
  "webviewRuntimeVersion": "exact nonempty version",
  "runtimeMode": "native-host-raw-wry-0.55.1",
  "resourcePolicyMode": "platform-valid fixed enum",
  "strongSourceKindsInterfaceAvailable": true,
  "rawWryHost": true,
  "usesTauriManagedWebView": false,
  "tauriGlobalsAbsent": true,
  "applicationInitializationScriptsAbsent": true,
  "applicationIpcHandlerAbsent": true,
  "inertWryShimPresent": true,
  "hiddenIpcCanaryDeltaZero": true,
  "hiddenIpcProducedNoResponse": true,
  "appStateUnchanged": true,
  "capabilityMatchAbsent": true,
  "policyInstalledBeforeFirstNetworkNavigation": true,
  "officialFinishedBeforePolling": true,
  "officialOnlyOrigins": true,
  "storageNonPersistent": true,
  "newInstanceStorageRecovered": false,
  "restartStorageRecovered": false,
  "timeoutCheck": true,
  "retryCheck": true,
  "policyFaultInvalidatesInstance": true,
  "lateCallbackIsolated": true,
  "destroyConfirmedBeforeRetry": true,
  "resourcePolicyCleanupAcknowledged": true,
  "policyTombstonesEmptyBeforeExit": true,
  "lifecycleNoMonotonicGrowth": true,
  "noOrphanHostWindows": true,
  "visibleWindowLeakAbsent": true,
  "unexpectedActivationAbsent": true,
  "ordinaryExitCleanupAcknowledged": true,
  "crossOriginCanaryServerHits": 0,
  "blockedCanaryAttempts": 0,
  "resourceVectorResults": {
    "document": {
      "runtimeAttempted": true,
      "availabilityOutcome": "available",
      "deterministicBarrierSeamCovered": true,
      "expectedBarrier": "webview2-web-resource-requested",
      "enforcedBarrier": "webview2-web-resource-requested",
      "barrierEvidenceMode": "native-callback",
      "counterfactualServerHits": null,
      "allowedRedirectHopHits": 0,
      "serverHits": 0
    }
  }
}
```

`resourceVectorResults` contains all 20 exact vector keys listed above, not only the representative `document` entry. Every result object uses the nine exact keys shown and `additionalProperties: false`. `runtimeAttempted`, `deterministicBarrierSeamCovered`, `enforcedBarrier == expectedBarrier` and zero protected `serverHits` are mandatory. `availabilityOutcome` is `available` for every vector except that `service_worker` alone may be `service-worker-api-absent`, and only after the fixed feature-detection expression proves `!("serviceWorker" in navigator)`. A script exception, timeout, rejected promise, CSP/mixed-content/certificate failure or other probe error is never “unavailable”; it fails the row. The absent-service-worker case uses `barrierEvidenceMode=deterministic-seam-only`, `counterfactualServerHits=null` and makes no runtime-interception claim.

Expected barriers are exact and platform-correlated: `document` through `redirect` use `webview2-web-resource-requested` on Windows and `wk-content-rule-list` on macOS; `popup` uses `new-window-handler`; `download` uses `download-handler`; and the four `top_level_*` vectors use `navigation-handler`. For available Windows resource vectors, `barrierEvidenceMode=native-callback`; for available macOS resource vectors, which expose no per-request blocked callback, it is `paired-counterfactual`; handler vectors use `handler-callback`. A macOS paired counterfactual uses the same zero-application-capability raw WK actor and exact trigger twice against controlled local TLS origins: the feature-only unprotected run must produce a positive `counterfactualServerHits`, while the production-policy run has zero `serverHits`. The result deliberately calls the derived value `enforcedBarrier`, not an observed callback. Browser preflight only establishes certificate/network reachability and cannot substitute for this pair.

The `redirect` vector is the only row with `allowedRedirectHopHits=2`: a feature-private test policy allows one controlled TLS origin, `/redirect/one` redirects to `/redirect/two` on that allowed origin, and the second response redirects to the blocked canary; `serverHits` counts only final blocked-origin hits. The counterfactual reaches both allowed hops plus the final canary, while the protected run reaches the two allowed hops and the final origin zero times. Every other row has `allowedRedirectHopHits=0`. The injectable test origin/policy mode is compiled only under `feasibility`, accepts only the runner-owned loopback certificate/origins and is absent from default/live signing paths.

Host and child-binary fields are exact and runner-correlated: both Windows rows use host `win32/x64`, child `windows/x86_64`, and `translatedProcess=null`; `windows-10-webview2-111-x64` additionally requires native OS build `10.0.19045`; `windows-11-x64` requires the frozen observed Windows 11 build and build number at least 22000. `macos-13-intel` uses host `darwin/x64`, child `macos/x86_64`, `translatedProcess=false`, and product version matching `13.3` or `13.3.x`; `macos-current-arm64` uses host `darwin/arm64`, child `macos/aarch64`, `translatedProcess=false`, and a frozen exact product version at least 13.3. `strongSourceKindsInterfaceAvailable` is boolean on Windows and `null` on macOS. `blockedCanaryAttempts` is a nonnegative integer when the platform exposes a reliable native counter and `null` otherwise; it is informational and never substitutes for zero server hits.

The helper derives each platform row's `crossOriginCanaryServerHits` from its 20 protected `serverHits`, derives the top-level value from those four row sums, and rejects disagreement at either level. Counterfactual and allowed-redirect-hop hits are excluded from the protected total but validated by their own exact relations. `resourceVectorsCovered` is derived from `resourceVectorResults` keys and accepted only if every row has the same exact unique set. `runtimeModes`, `resourcePolicyModes` and `webviewRuntimeVersions` are exact four-key objects derived from the corresponding row fields.

Platform correlation is strict:

```text
windows-10-webview2-111-x64:
  webviewRuntimeVersion matches 111.0.1661.x;
  use webview2-22-all-source-kinds when the _22 interface is available,
  otherwise webview2-legacy-all-contexts-candidate.
windows-11-x64:
  exact frozen nonempty runtime version;
  strongSourceKindsInterfaceAvailable=true;
  resourcePolicyMode=webview2-22-all-source-kinds.
macos-13-intel and macos-current-arm64:
  exact frozen nonempty WebKit version;
  strongSourceKindsInterfaceAvailable=null;
  resourcePolicyMode=wk-content-rule-list-exact-origin.
```

Add an `assertFalseChecks` path for `usesTauriManagedWebView`, `newInstanceStorageRecovered` and `restartStorageRecovered` instead of encoding false as an untyped exception. The schema-v2 common envelope and the gate-specific signature `checks`/`byPlatform`/per-vector branches use `additionalProperties: false`. Rename `filterModes` to `resourcePolicyModes`; the only accepted modes are `webview2-22-all-source-kinds`, `webview2-legacy-all-contexts-candidate` and `wk-content-rule-list-exact-origin`. Add negative tests for missing/extra/wrongly typed keys, missing/extra platform-map/vector entries, wrong host OS/architecture label, swapped Windows/macOS modes, Windows 11 legacy mode, Windows 10 choosing legacy while reporting the strong interface available, Windows 10 choosing v22 while reporting the interface unavailable, any non-service-worker unavailable result, a probe error mislabeled unavailable, wrong evidence/barrier mapping, missing macOS counterfactual, wrong redirect hop counts and any protected per-vector server hit.

Run:

```powershell
node --test scripts/feasibility-evidence.test.mjs
```

Expected: FAIL on the old schema/design identity and old signature predicate.

- [ ] **Step 2: Implement schema v2 and a source-level raw-host gate**

Update `evidence.schema.json`, the helper, tests and README to the new design identity. Ordinary `pnpm quality` must continue to test validation mechanics without validating stale committed observations; strict current-scope validation remains Task 10's responsibility.

Create `scripts/verify-signature-host.mjs` and its built-in Node tests. The verifier reads only the Task 4 signature source set and must reject:

```text
tauri::WebviewWindowBuilder
tauri::WebviewWindow
.with_webview( on a Tauri managed WebView
.with_ipc_handler(
.with_initialization_script(
.with_initialization_script_for_main_only(
.with_custom_protocol(
.with_asynchronous_custom_protocol(
unsafe impl Send
unsafe impl Sync
```

It must positively require `tauri::window::WindowBuilder`, `wry::WebViewBuilder`, `with_id(SIGNATURE_WEBVIEW_ID)`, `build_as_child`, `run_on_main_thread`, `with_on_page_load_handler`, `SIGNATURE_HOST_WINDOW_LABEL`, the `Pending` and `Destroying` slot states, `WindowEvent::Destroyed`, `maybe_complete_teardown`, `LATE_POLICY_TOMBSTONES`, WebView2's `AddWebResourceRequestedFilterWithRequestSourceKinds` and matching `RemoveWebResourceRequestedFilter`, macOS `WKWebsiteDataStore::nonPersistentDataStore` plus `with_webview_configuration` and store-removal completion marshalled to UI, and every raw-host/probe/policy source file listed in this task. The verifier is a defense-in-depth gate, not a substitute for runtime probes.

Add:

```json
{
  "test:signature-host": "node --test scripts/verify-signature-host.test.mjs",
  "verify:signature-host": "node scripts/verify-signature-host.mjs"
}
```

Append `test:signature-host` to `quality` now, but do not append the repository source check until Step 8 makes it green; this keeps the intermediate evidence-contract commit CI-clean while preserving the explicit red source check. Add the helper, tests, all current raw-host/platform-policy files, Cargo/build/config/ACL files and related frontend tests to both the `signature-webview` and `gd-contract-pagination` exact scope arrays; Step 8 adds the newly created lifecycle runner and its tests before the implementation commit. Because the signature facade consumes Task 3 types, the signature scope also contains `src-tauri/src/music/contract.rs`, `src-tauri/src/music/mod.rs`, `src-tauri/tests/gd_contract.rs` and every exact `src-tauri/tests/fixtures/gd/` path enumerated in Task 3's Files list; the final manifest spells out each path and uses no glob. Tests must prove deleting, adding or substituting any one final path fails.

Run:

```powershell
node --test scripts/feasibility-evidence.test.mjs
node --test scripts/verify-signature-host.test.mjs
node scripts/verify-signature-host.mjs
```

Expected: the first two suites pass; the final command fails because the managed draft has not yet been replaced. This is the recorded red source check and is not yet part of `quality`.

After recording that expected failure, run the intermediate green gate and commit only the evidence-contract/source-gate migration:

```powershell
pnpm quality
git add README.md docs/feasibility/evidence.schema.json docs/feasibility/evidence-scopes.json scripts/feasibility-evidence.mjs scripts/feasibility-evidence.test.mjs scripts/verify-signature-host.mjs scripts/verify-signature-host.test.mjs package.json pnpm-lock.yaml
git commit -m "test: migrate signature host evidence contract"
```

- [ ] **Step 3: Add exact dependencies and write failing pure/runtime-state tests**

Use optional dependencies so the default artifact has no feasibility implementation. Remove the obsolete `dispatch2` dependency. The feature table must include `tauri/unstable` because Tauri 2.11.5 exposes the bare native `WindowBuilder` through that feature:

```toml
futures-util = { version = "0.3", optional = true }
reqwest = { version = "0.13.4", default-features = false, features = ["rustls", "stream"], optional = true }
sha2 = { version = "0.10", optional = true }
time = { version = "0.3", features = ["formatting", "parsing"], optional = true }
tokio = { version = "1", features = ["macros", "rt-multi-thread", "sync", "time"], optional = true }
tokio-util = { version = "0.7", features = ["rt"], optional = true }
wry = { version = "=0.55.1", default-features = false, features = ["os-webview"], optional = true }

[target.'cfg(windows)'.dependencies]
webview2-com = { version = "0.38", optional = true }
windows-core = { version = "0.61", optional = true }

[target.'cfg(target_os = "macos")'.dependencies]
block2 = { version = "0.6", optional = true }
libc = { version = "0.2", optional = true }
objc2 = { version = "0.6", optional = true }
objc2-foundation = { version = "0.3", optional = true }
objc2-web-kit = { version = "0.3", features = [
  "WKContentRuleList",
  "WKContentRuleListStore",
  "WKUserContentController",
  "WKWebView",
  "WKWebViewConfiguration",
  "WKWebsiteDataStore",
], optional = true }

[features]
default = ["custom-protocol"]
custom-protocol = ["tauri/custom-protocol"]
feasibility = [
  "tauri/unstable",
  "dep:futures-util",
  "dep:reqwest",
  "dep:sha2",
  "dep:time",
  "dep:tokio",
  "dep:tokio-util",
  "dep:wry",
  "dep:webview2-com",
  "dep:windows-core",
  "dep:block2",
  "dep:libc",
  "dep:objc2",
  "dep:objc2-foundation",
  "dep:objc2-web-kit",
]
```

Before implementation, retain or add unit tests for:

- exact official origin and one-use `about:blank` bootstrap;
- signature return length/control/`&`/`=` validation;
- self-catching readiness and signing JavaScript;
- official `Finished` before the first readiness poll;
- one 20-second initialization deadline shared by host creation, policy preparation, navigation, page finish and 100 ms polling;
- 5-second signing timeout and late callback isolation by generation plus operation ID;
- every failure transition poisons the current generation and schedules destruction;
- cancellation before native host creation returns with a unique host label, a bounded fail-closed destroy result, and mandatory UI destruction if the host arrives late;
- cancellation during macOS rule compilation with a generation/operation-specific identifier, immediate detachment into a late-result tombstone, and no transition/navigation by that callback;
- duplicate destroy plus immediate retry pressure still produces exactly one native `WindowEvent::Destroyed` acknowledgement and cannot reuse the old slot;
- retry increments generation only after a confirmed destroy;
- inert WRY shim present is allowed, while Tauri globals, an application handler, any response or state delta fails;
- full resource URL classification plus the exact per-vector attempt/availability/barrier/hit result contract;
- 20 fake create/destroy cycles return registry/host/process counts to baseline.

Use these public DTOs and facade names:

```rust
pub const GD_PAGE_URL: &str = "https://music.gdstudio.xyz/";
pub const SIGNATURE_HOST_WINDOW_LABEL: &str = "gd-signature-host-feasibility";
pub const SIGNATURE_WEBVIEW_ID: &str = "gd-signature-raw-wry";
pub const INIT_TIMEOUT: Duration = Duration::from_secs(20);
pub const CALL_TIMEOUT: Duration = Duration::from_secs(5);
pub const DESTROY_TIMEOUT: Duration = Duration::from_secs(5);
pub const MAX_SIGNATURE_BYTES: usize = 128;

pub struct SignatureRuntime {
    app: tauri::AppHandle<tauri::Wry>,
    generation: AtomicU64,
    operation_id: AtomicU64,
    state: tokio::sync::Mutex<RuntimeState>,
}

impl SignatureRuntime {
    pub fn new(app: tauri::AppHandle<tauri::Wry>) -> Self;
    pub async fn initialize(&self) -> Result<SignatureInitReport, SignatureError>;
    pub async fn sign(&self, input: &EncodedComponent) -> Result<SignatureValue, SignatureError>;
    pub async fn run_isolation_probe(&self) -> Result<IsolationReport, SignatureError>;
    pub async fn destroy(&self) -> Result<(), SignatureError>;
    pub async fn retry(&self) -> Result<SignatureInitReport, SignatureError>;
}
```

One `IsolationReport` represents one platform observation: it contains generation, operation ID, current/final URL, counters and host labels after destroy plus a nested `checks: PlatformSignatureChecks`. That nested value maps one-to-one, using camelCase Serde names, to `checks.byPlatform[platformId]`; report metadata is stored outside the exact-key checks object. The evidence helper validates all four reports and derives the top-level aggregate fields/maps; Rust does not claim a cross-platform aggregate from one run. A macOS content rule may not expose a per-request blocked counter; never fabricate one or convert `null` to zero.

Run:

```powershell
cargo test --manifest-path src-tauri/Cargo.toml --features feasibility signature_webview -- --nocapture
```

Expected: FAIL on missing runtime/state-machine behavior, without a cross-thread COM/ObjC type error.

- [ ] **Step 4: Implement the UI-thread raw WRY host actor**

Implement `signature_host.rs` around a UI-thread TLS registry. A slot exists before any raw child or asynchronous platform-policy work starts, so destroy can own both in-progress and ready generations:

```rust
thread_local! {
    static RAW_SIGNATURE_SLOT: RefCell<MainThreadSignatureSlot> =
        const { RefCell::new(MainThreadSignatureSlot::Empty) };
}

enum MainThreadSignatureSlot {
    Empty,
    Pending(PendingMainThreadSignatureInstance),
    Ready(MainThreadSignatureInstance),
    Destroying(DestroyingMainThreadSignatureInstance),
}

struct CreationTicket {
    generation: u64,
    operation_id: u64,
    host_label: String,
    cancelled: AtomicBool,
    native_destroyed: AtomicBool,
    native_destroyed_notify: tokio::sync::Notify,
    teardown_complete: AtomicBool,
    teardown_notify: tokio::sync::Notify,
}

struct OperationTicket {
    generation: u64,
    operation_id: u64,
    cancelled: AtomicBool,
}

struct PendingMainThreadSignatureInstance {
    generation: u64,
    operation_id: u64,
    host: tauri::Window<tauri::Wry>,
    ticket: Arc<CreationTicket>,
    builder_spec: RawWebViewBuilderSpec,
    policy_build: PendingResourcePolicy,
}

struct MainThreadSignatureInstance {
    generation: u64,
    operation_id: u64,
    host: tauri::Window<tauri::Wry>,
    webview: wry::WebView,
    policy: ResourcePolicyGuard,
    counters: Arc<IsolationCounters>,
    ticket: Arc<CreationTicket>,
}

struct DestroyingMainThreadSignatureInstance {
    generation: u64,
    operation_id: u64,
    host_label: String,
    ticket: Arc<CreationTicket>,
}

enum RuntimeState {
    Idle,
    Creating { ticket: Arc<CreationTicket> },
    Ready { ticket: Arc<CreationTicket> },
    Poisoned { ticket: Arc<CreationTicket>, reason: SignatureError },
    Destroying { ticket: Arc<CreationTicket> },
    TerminalPoisoned { generation: u64, reason: SignatureError },
}

#[cfg(target_os = "macos")]
thread_local! {
    static LATE_POLICY_TOMBSTONES: RefCell<BTreeMap<(u64, u64), String>> =
        const { RefCell::new(BTreeMap::new()) };
}
```

`RawWebViewBuilderSpec` is a pure UI-registry value containing the fixed ID/URL/settings and callback generation IDs, not a `wry::WebViewBuilder` or native object. `PendingResourcePolicy` is a `cfg`-split, deliberately non-`Send` UI-thread type. On macOS it owns the fresh `WKWebViewConfiguration`, the in-flight compilation state and the unique rule-list identifier `yinmi-gd-signature-{generation}-{operation_id}`; on Windows it records the synchronous registration phase. The native host label is also unique: `format!("{SIGNATURE_HOST_WINDOW_LABEL}-{generation}-{operation_id}")`; the public constant is a prefix, never a reused full label. Every active `RuntimeState` variant owns the same creation ticket, and every transition validates its generation before replacement. No code may hold the async state mutex across `spawn_blocking`, `run_on_main_thread`, a oneshot, `Notify` or a destroy-event wait; it clones the ticket, releases the mutex, awaits, then reacquires and revalidates generation/state before committing the result.

`CreationTicket` is only for initialization and native-host lifetime. It has separate acquire/release flags and `Notify` loops for `native_destroyed` and composite `teardown_complete`. The exact host `WindowEvent::Destroyed` path may acknowledge only the first flag after manager absence; `maybe_complete_teardown` may acknowledge the second only when native destruction, manager absence, platform-policy cleanup and zero generation tombstones are all true. macOS rule-store completion blocks never mutate TLS directly: they schedule a generation-checked UI-main-thread closure, which clears the tombstone, records cleanup acknowledgement and calls `maybe_complete_teardown`. Each `sign()` or other evaluation allocates a Send-only `Arc<OperationTicket>` after incrementing `operation_id`; its callback checks cancellation plus generation/operation equality, and the awaiting task revalidates the current `Ready` state after reacquiring the mutex. Both ticket waits use the standard register-notified/recheck loop, so duplicate destroy callers cannot miss or duplicate either barrier. Raw WRY, COM and ObjC objects may never be returned through a Tokio channel. The Tauri host handle is the single explicit exception during worker-side host creation; it is immediately consumed by the UI handoff. Every raw create/evaluate/current-URL/destroy operation after that handoff calls `AppHandle::run_on_main_thread`; oneshots return only `Result<()>`, strings or serializable metadata. Never add an unsafe `Send`/`Sync` implementation.

Creation order is fixed:

1. Before starting `tokio::task::spawn_blocking`, publish `RuntimeState::Creating` with its ticket and unique label. In the blocking task call `tauri::window::WindowBuilder` to create a 1×1 invisible, unfocused, unfocusable, undecorated, shadowless, nonresizable native host with no managed WebView. Apply `skip_taskbar(true)` only on Windows. Tauri window creation must not originate on the event-loop thread because that request can deadlock. If it fails before a native window exists, mark teardown complete. If cancellation or the bounded destroy deadline wins, any host that arrives later is still handed to UI and destroyed under its unique label; the runtime remains fail-closed and never reuses that generation.
2. Move the host into a UI-main-thread closure, atomically require `Empty`, and insert `Pending` with the pure builder spec before constructing raw WRY or starting policy compilation. The spec fixes `SIGNATURE_WEBVIEW_ID`, `about:blank`, invisible/unfocused/devtools-off, incognito, clipboard/autofill disabled, exact navigation handling, denied new windows/downloads and page-load handler IDs. It contains no initialization script, IPC handler or custom protocol. Never keep a live `wry::WebViewBuilder` across the asynchronous macOS compile or capture it in an Objective-C block.
3. Windows sequence: materialize `wry::WebViewBuilder::new().with_id(SIGNATURE_WEBVIEW_ID)` from the spec, apply Windows settings that disable browser accelerator keys and default context menus, and call `build_as_child(&host)` for the internal `about:blank` child; obtain the native WebView2 interfaces; install and acknowledge the filter/handler; atomically transition the same generation/operation from `Pending` to `Ready`; only then call `load_url(GD_PAGE_URL)`. Acquire-check cancellation after raw build, after interface lookup, after filter/handler registration, before the transition and immediately before navigation. Any cancelled check locally reverses installed native state, drops raw WRY, enters `Destroying` and never loads a URL.
4. macOS sequence: while the slot remains `Pending`, create the custom WK configuration on the UI thread, set its explicit nonpersistent store and start compilation under the generation/operation-specific rule identifier. The completion block looks up the exact pending slot, acquire-checks cancellation, attaches the returned rule, materializes a fresh WRY builder from the stored pure spec, applies the configuration with `with_webview_configuration`, acknowledges policy ready and builds the `about:blank` child. It acquire-checks again before transition and before `load_url(GD_PAGE_URL)`; cancellation detaches/removes the rule, tombstones store removal, drops local native objects and destroys the host. WRY's incognito flag alone is not evidence on this custom-configuration path.
5. The one-use internal `about:blank` bootstrap is the only pre-policy navigation and performs no network request. Any official, external or otherwise network-capable navigation before platform policy acknowledgement is a test failure; the bootstrap allowance is consumed and cannot authorize a second navigation. Accept only the current generation's official-page `PageLoadEvent::Finished`; ignore `about:blank` and stale events. Start callback-based 100 ms readiness polling only after that event.

The single `Arc<CreationTicket>` is the cancellation and composite teardown barrier for a generation. Timeout marks it cancelled before requesting cleanup. `destroy()` clones the ticket and records cancellation under the async state mutex, releases that mutex, schedules UI cleanup and waits at most `DESTROY_TIMEOUT` for `teardown_complete`. Destroying `Pending` immediately detaches and drops its configuration on UI, records the unique macOS rule ID in `LATE_POLICY_TOMBSTONES` if compilation is outstanding, transitions the slot to `Destroying`, and queues `host.destroy()`; it never blocks the UI thread on the compile callback. A late callback captures only generation, operation ID and rule ID. It must not build WRY, navigate or inspect a newer slot. A compile error returns through a UI closure that records policy cleanup and clears the tombstone; a late success drops the returned rule on UI, requests store removal by the old identifier, and clears the tombstone only from that removal completion on UI. Retry and application-owned exit both require native destroy acknowledgement, manager absence, policy-store cleanup acknowledgement and zero tombstones for that generation. If any part does not drain within `DESTROY_TIMEOUT`, transition to `TerminalPoisoned`, return a bounded error and keep the app open instead of hanging or leaving a persistent rule behind.

`tauri::Window::destroy()` is only a queued request, never the acknowledgement. Dropping/detaching a `Ready` or `Pending` instance transitions TLS to `Destroying` before calling it. The `RunEvent::WindowEvent` handler matches the exact unique host label and `WindowEvent::Destroyed`, then schedules one next UI turn to prove `app.get_window(label).is_none()`; that verified path acknowledges native/manager removal and calls `maybe_complete_teardown`, but cannot set `Empty` while platform cleanup or a tombstone remains. Only the generation-checked composite completion path may set `Empty`, complete `teardown_complete` and release destroy/exit callers. If no verified native event or policy-store completion arrives within `DESTROY_TIMEOUT`, `destroy()` returns a stable timeout error, stores `TerminalPoisoned`, forbids retry and lets the exit coordinator keep the app open. A builder or policy callback that returns after this terminal timeout still hands its unique native state to UI cleanup; completing teardown may permit a later close attempt, but can never make the poisoned runtime reusable. Page events and evaluation callbacks check their correct ticket, generation and operation ID and return without state mutation.

The signing expression remains self-catching because Windows does not reliably surface JavaScript exceptions:

```js
(() => {
  try {
    const fn = globalThis.crc32;
    if (typeof fn !== "function") return { status: "error", code: "MISSING_FUNCTION" };
    const value = fn(ENCODED_INPUT_JSON);
    if (typeof value !== "string") return { status: "error", code: "INVALID_TYPE" };
    if (value.length === 0) return { status: "error", code: "EMPTY_VALUE" };
    if (new TextEncoder().encode(value).byteLength > 128) {
      return { status: "error", code: "RETURN_TOO_LARGE" };
    }
    return { status: "ok", value };
  } catch (_) {
    return { status: "error", code: "CALL_THROWN" };
  }
})()
```

Replace `ENCODED_INPUT_JSON` with `serde_json::to_string(input.as_str())` and repeat all Rust-side validation. Any initialization, policy or signing failure poisons the generation and invokes the same main-thread destroy path.

For `Ready`, destroy order is fixed: transition out of `Ready`; on Windows remove and acknowledge the exact filter tuple/handler; on macOS remove the controller rule and place its unique store identifier into the same removal-completion tombstone path; drop raw WRY; enter `Destroying`; queue `host.destroy()`; then await composite teardown. Native host destruction and asynchronous store removal may finish in either order, but both acknowledgements plus zero tombstones are mandatory before `Empty`, retry, install or exit. For `Pending`, detach/tombstone unfinished work before the same `Destroying` path. Add deterministic seams for cancellation before host creation returns, cancellation after each Windows native step, cancellation during macOS compilation/build, late success/error completion, Ready-store removal completion/failure, store callback delivered off-main then marshalled to UI, missing/delayed native destroy, duplicate destroy, exit pressure and retry pressure; every case must prove one composite teardown acknowledgement and no old-generation transition/navigation.

Handle ordinary shutdown explicitly. Task 4 adds a minimal signature-only background exit coordinator around the same public `SignatureRuntime::destroy()` future; Task 8 later composes that hook into the unified music/updater barrier instead of adding a second cleanup path. The `main` window's `CloseRequested` calls `prevent_close()` and only queues this coordinator while any signature slot/ticket is active. A defensive `main` `Destroyed` handler also initiates teardown if another code path destroyed it first. `RunEvent::ExitRequested` calls `prevent_exit()` when cleanup is active and reissues programmatic exit only after composite teardown proves verified host destruction/manager absence, policy-store cleanup, zero tombstones and TLS `Empty`. `RunEvent::Exit` is final: it performs idempotent UI-thread detach/drop without waiting for callbacks and is expected to observe both `Empty` and zero policy tombstones for every application-owned exit. Add an active-signature-host main-close integration test proving no hidden host/rule entry keeps the process alive and no UI-thread wait occurs.

- [ ] **Step 5: Implement native resource enforcement before first network navigation**

In `webview_resource_policy.rs` define pure URL classification, `IsolationCounters`, `ResourcePolicyMetadata` and a deliberately non-`Send` `ResourcePolicyGuard`. Allow only HTTPS, no credentials, effective port 443 and host exactly `music.gdstudio.xyz`. The navigation handler separately allows the single internal bootstrap and rejects top-level `data:`, `blob:`, `file:` and custom protocols.

Windows implementation:

- obtain the controller/environment/core WebView through `wry::WebViewExtWindows`;
- prefer `ICoreWebView2_22::AddWebResourceRequestedFilterWithRequestSourceKinds` with all contexts and all source kinds;
- only on the WebView2 111 baseline, allow the legacy all-context filter as `webview2-legacy-all-contexts-candidate`;
- return a synthetic empty 403 for every disallowed raised request;
- retain the handler registration token plus the exact filter URI/context/source-kinds/mode tuple on the UI thread, then call the matching `RemoveWebResourceRequestedFilter*` overload with that tuple before removing the handler token; WebView2 filters themselves have no token;
- any registration, callback or response-construction fault poisons and destroys the instance.

macOS implementation:

- create a fresh `WKWebViewConfiguration` on the UI thread;
- explicitly set `WKWebsiteDataStore::nonPersistentDataStore`; WRY's incognito flag is insufficient when a custom configuration is supplied;
- compile the exact-origin `WKContentRuleList` asynchronously, attach it to the configuration's user-content controller, and acknowledge success before raw WRY construction/navigation;
- use WRY `WebViewBuilderExtMacos::with_webview_configuration`;
- keep configuration, controller rule, rule-list store and unique identifier in the TLS guard; Ready destroy removes the controller rule, requests store removal by identifier and clears its tombstone only on removal completion;
- invoke compilation from the UI thread and require the completion block to obtain/assert an Objective-C main-thread marker before it touches the configuration, rule or TLS. A callback without that marker fails closed and triggers host cleanup; the block sends only a `Send` acknowledgement/error and never sends a retained WK object.

Add tests for policy JSON, mode selection, 403 behavior through a seam, handler-token plus exact-filter-tuple uninstall, compilation failure, generation-specific rule identifiers/tombstones, Pending/Ready store-removal completion, destroy-during-compilation native-window acknowledgement ordering, late completion cancellation and policy-fault poisoning. On WebView2 111, any missing resource source is `design-change-required`, not permission to raise the runtime floor silently.

Run:

```powershell
cargo test --manifest-path src-tauri/Cargo.toml --features feasibility signature_webview -- --nocapture
node scripts/verify-signature-host.mjs
```

Expected: all pure/actor/platform-seam tests and the source gate pass on the current platform.

- [ ] **Step 6: Add isolation, persistence and lifecycle probes**

First write `scripts/run-signature-lifecycle-probe.test.mjs` for exact two-phase event grammar, nonce generation/exact matching/replay rejection, monotonic ordinals, external-only process-exit event, clean child exit, 60/130-second deadline handling, marker/path/endpoint redaction and bad/missing/extra phase rejection, then run it once and record the expected failure because the runner is absent:

```powershell
node --test scripts/run-signature-lifecycle-probe.test.mjs
```

Implement the runner and `signature_probe.rs` to make that test pass. Establish the Rust IPC-canary baseline by proving the local main window can increment it, then reset/snapshot it. From the hidden raw WRY page:

- assert Tauri named globals and any own property containing `tauri` are absent;
- observe the WRY 0.55.1 inert `window.ipc` shim, post a unique payload if callable, wait a bounded interval and prove there is no response;
- prove the application handler is unconfigured, hidden-page canary delta is zero and application state/event/command snapshots are unchanged;
- prove `host.webviews().is_empty()` while the raw child exists.

Freeze these exact triggers; the probe may not substitute a different browser primitive under the same token:

```text
document: append <object type="text/html" data=BLOCKED_HTTPS_URL>
iframe: append <iframe src=BLOCKED_HTTPS_URL>
script: append <script src=BLOCKED_HTTPS_URL>
style: append <link rel="stylesheet" href=BLOCKED_HTTPS_URL>
image: set new Image().src = BLOCKED_HTTPS_URL
media: append <audio preload="auto" src=BLOCKED_HTTPS_URL> and call load()
fetch: await fetch(BLOCKED_HTTPS_URL, { mode: "no-cors", cache: "no-store" })
xhr: XMLHttpRequest GET BLOCKED_HTTPS_URL
worker: create a blob Worker whose only statement is importScripts(BLOCKED_HTTPS_URL)
service_worker: from the controlled allowed TLS page, register same-origin /sw.js; its install handler fetches BLOCKED_HTTPS_URL
websocket: new WebSocket(BLOCKED_WSS_URL)
sse: new EventSource(BLOCKED_HTTPS_SSE_URL)
beacon: navigator.sendBeacon(BLOCKED_HTTPS_URL, one fixed byte)
redirect: fetch ALLOWED_HTTPS_URL/redirect/one, which redirects through /redirect/two to BLOCKED_HTTPS_URL
popup: window.open(BLOCKED_HTTPS_URL, "_blank", "noopener")
download: click a connected <a download href=BLOCKED_HTTPS_URL>
top_level_data: location.assign("data:text/html,yinmi-probe")
top_level_blob: create one text/html Blob URL, call location.assign(url), then revoke it
top_level_file: location.assign("file:///yinmi-feasibility-denied")
top_level_custom_protocol: location.assign("yinmi-feasibility-denied://probe")
```

Run an OS-assigned loopback canary server with the exact HTTP(S), two-hop redirect, SSE, WebSocket/WSS and service-worker-support routes above. Use an ignored per-run test certificate/root trusted only on the controlled VM, prove HTTPS/WSS reachability with a browser preflight, reset the protected counters, and remove the temporary trust after capture; this prevents mixed-content or certificate rejection from masquerading as enforcement. The feature-only counterfactual uses the same raw actor, trigger and controlled origins without the production resource rule; it has no app handler/capability and is destroyed before the protected generation. On macOS every available resource vector must first reach its counterfactual canary, then produce zero protected hits with the exact rule attached. The protected redirect must reach both allowed hops and stop before the final canary. For every vector emit the exact `resourceVectorResults` row: attempt, availability outcome, deterministic seam, expected/enforced barrier, evidence mode, counterfactual hits, allowed redirect hops and protected hits. Count server observations, not merely JavaScript errors or browser performance entries. Windows blocked-attempt counters may be positive; macOS may have no per-request counter. Passing requires zero protected blocked-origin hits, official-only network origins in the real signing generation and no popup/download/top-level scheme escape.

For persistence, write a unique Cookie/cache/Web Storage marker, destroy and create a new instance; the new instance may not read it. For lifecycle, run 20 initialize/sign/destroy/retry cycles, then 10 minutes idle plus sleep/wake. Compare native host labels, late-policy tombstones, browser-process counts and the macOS rule-list store identifiers matching `yinmi-gd-signature-*` to a pre-probe baseline: no monotonic growth or leftover prefix entry, no orphan host, no taskbar/window flash and no unexpected macOS activation.

`feasibility_signature_isolation` takes no caller-selected fault name. It internally runs the fixed private in-process scenario set `policy-registration-fault`, `initialization-finished-delay-past-20s`, `sign-callback-delay-past-5s`, `destroy-during-pending-policy`, `late-callback-after-new-generation` and `main-close-state-machine-seam`. The last item stops at the pure/coordinator would-exit boundary and does not claim a real process close. The feature-only injector is below the runtime facade and unavailable in the default build. Each scenario emits its fixed ID, generation/operation IDs, ordered actor events and terminal state; the report booleans are derived from those traces. Unit tests use paused time, while platform evidence exercises the real 20/5-second boundaries and native host-destroy acknowledgement.

Cross-process restart and ordinary-exit evidence comes from `scripts/run-signature-lifecycle-probe.mjs`, never from the in-process command. Its tested CLI accepts one exact `--app` path, one of the four signature platform IDs and an ignored `--output` below `artifacts/feasibility/signature/`. Before spawn it derives `process.platform`/`process.arch`, obtains Windows `os.release()` or macOS `/usr/bin/sw_vers -productVersion`, and checks the host half of the exact correlation above. The runner binds an OS-assigned loopback recorder, generates a fresh 128-bit lowercase-hex nonce and launches the feature child with exactly `YINMI_FEASIBILITY_SIGNATURE_AUTORUN=write-marker-and-close-main|verify-marker-absent`, `YINMI_FEASIBILITY_SIGNATURE_TRACE_ENDPOINT=<runner endpoint>` and `YINMI_FEASIBILITY_SIGNATURE_RUN_ID=<nonce>`.

Each child must first post once to the derived fixed `/process-info` endpoint with exact body `{runId,phase,binaryTargetOs,binaryTargetArch,translatedProcess}`. The two target strings come from Rust `std::env::consts`; on macOS the feature-only native helper calls `libc::sysctlbyname` for `sysctl.proc_translated`, treats a nonzero integer as translated, treats `ENOENT` as native/nontranslated, and fails closed on other errors or malformed values, while Windows reports `null`. The runner requires both phases to agree and validates child target, Node host and platform ID three ways; a Rosetta-translated process or any Windows/Intel/ARM mismatch fails before lifecycle events are accepted. The sanitized output carries all six host/child fields, and the evidence helper requires equality with its platform row. The event server exact-matches nonce/phase and accepts at most 4 KiB of `{runId, phase, event}` with no extra keys; process-info has the same 4 KiB cap and exact-key/replay checks. Neither child may submit `process-exit-observed`; the runner appends it as the next ordinal only after a zero-code, non-forced child exit.

After process-info acknowledgement, Phase 1's exact event grammar is `process-started`, `active-host-ready`, `marker-written`, `main-close-requested`, `host-destroyed`, `manager-host-absent`, `policy-cleanup-acknowledged`, `policy-tombstones-empty`, `tls-entry-absent`, `app-exit-invoked`, then external `process-exit-observed`. Phase 2 replaces only `marker-written` with `marker-absent`. All ordinals are strictly increasing and each name appears once. Store-removal callbacks must marshal to the UI thread before the last three cleanup events can be emitted. Each phase has a 60-second deadline and the two-phase run a 130-second total deadline; timeout saves a sanitized diagnostic, force-kills for local cleanup, records failure and exits nonzero. The runner writes both traces for the evidence helper but omits nonce, endpoint, absolute app path and marker value. Config/default-artifact tests prove all three autorun variables and child modes are feature-only.

Add `"test:signature-lifecycle-runner": "node --test scripts/run-signature-lifecycle-probe.test.mjs"` to package scripts and append it to `quality` before Step 8. Its tests include wrong host/child architecture combinations, missing/duplicate process-info, Rosetta translation, missing policy cleanup/tombstone events, trace grammar, nonce, deadline and redaction; this makes those checks part of ordinary CI and Task 10, not only a manual Task 4 command.

Policy faults, timeouts and deliberately delayed callbacks must invalidate the instance. Each negative test must confirm verified native destroy acknowledgement before retry and must show that the stale callback cannot change the new generation. A late macOS policy tombstone must drain before retry or produce the bounded `TerminalPoisoned` result; it may never create an unbounded wait.

- [ ] **Step 7: Lock feature ACL/UI and fixed GD live probes**

Keep the default build free of all feasibility commands. The feature-only command manifest contains only:

```text
feasibility_signature_initialize
feasibility_signature_sign
feasibility_signature_destroy
feasibility_signature_isolation
feasibility_run_gd_probe
feasibility_ipc_canary
```

`feasibility-main.json` is local-only, matches exactly `["main"]` and grants only `core:default` plus the feature permission. Neither config/capability may contain `remote`, `urls`, wildcard windows, the host label or raw WebView ID. The hidden native host has no Tauri-managed WebView and therefore no capability consumer.

Use Vite feasibility mode for dynamic frontend import; normal-mode tests prove the panel is absent. `verify-default-artifacts.mjs` rejects `FeasibilityPanel`, `GdProbe`, the host label, raw ID, `feasibility_` and `YINMI_FEASIBILITY_` from completed default frontend/Tauri artifacts.

Keep exactly the three Task 3 live cases:

```rust
pub enum ProtocolProbeCase {
    SingleCount1000,
    PagedOfficial20,
    RepeatSamePage,
}

pub async fn run_gd_probe(
    runtime: &SignatureRuntime,
    probe_case: ProtocolProbeCase,
    cancel: &CancellationToken,
) -> Result<ProtocolProbeReport, GdProbeError>;
```

The only keyword is `周杰伦`. Every case constructs the source as `GdSource::DEFAULT` and asserts its rendered wire value is `netease`; every count is created with `SearchCount::try_from`, including the upper-bound value 1000:

```text
single_count_1000: count=1000, page=1, one API request
paged_official_20: count=20, pages=1..=50, at least 6500 ms between starts
repeat_same_page: count=20, page=1 twice, at least 6500 ms between starts
```

The private HTTP seam tests numeric/date/missing `Retry-After`, another non-2xx status, cancellation and a streamed 5 MiB + 1 response. Build reqwest with redirects/proxy disabled, `tls_backend_rustls()`, 10-second connect and 30-second total timeouts. Never automatically retry 429, retain raw bodies, log signatures/full form bodies or run live GD in unit tests/CI.

- [ ] **Step 8: Verify and commit the clean implementation checkpoint**

Before formatting, confirm no managed builder/installer or cross-thread COM/WK owner remains. Append `verify:signature-host` to `quality` now that the repository source check is expected to pass. Then run:

```powershell
pnpm format
cargo fmt --manifest-path src-tauri/Cargo.toml --all
node --test scripts/feasibility-evidence.test.mjs
node --test scripts/verify-signature-host.test.mjs
node --test scripts/run-signature-lifecycle-probe.test.mjs
node scripts/verify-signature-host.mjs
cargo test --manifest-path src-tauri/Cargo.toml --features feasibility signature_webview -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml --features feasibility gd_live -- --nocapture
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets --features feasibility -- -D warnings
pnpm quality
$env:CARGO_TARGET_DIR=(Join-Path (Resolve-Path src-tauri) 'target/feasibility')
pnpm tauri build --debug --no-bundle --config src-tauri/tauri.feasibility.conf.json --features feasibility
$env:CARGO_TARGET_DIR=(Join-Path (Resolve-Path src-tauri) 'target/default')
pnpm tauri build --debug --no-bundle
pnpm verify:default-artifacts
Remove-Item Env:CARGO_TARGET_DIR
git diff --check
```

`verify-default-artifacts.mjs` resolves the default binary/frontend artifact root from `CARGO_TARGET_DIR` when set and rejects accidentally inspecting the feasibility target. Expected: all deterministic checks pass; default artifacts contain no feasibility sentinel, while the frozen feature executable remains under `src-tauri/target/feasibility/debug/`. Do not collect platform evidence from a dirty tree.

Stage only the implementation/source scope, preserving unrelated user files:

```powershell
git add .prettierignore src-tauri/src/feasibility src-tauri/src/lib.rs src-tauri/build.rs src-tauri/permissions/feasibility.toml src-tauri/capabilities/feasibility-main.json src-tauri/Cargo.toml src-tauri/Cargo.lock src-tauri/tauri.conf.json src-tauri/tauri.feasibility.conf.json src/lib/feasibility src/App.svelte src/App.test.ts src/vite-env.d.ts vite.config.ts scripts/verify-config.mjs scripts/verify-default-artifacts.mjs scripts/run-signature-lifecycle-probe.mjs scripts/run-signature-lifecycle-probe.test.mjs docs/feasibility/evidence-scopes.json package.json pnpm-lock.yaml
git commit -m "feat: add isolated raw WRY GD signature probe"
git status --short
```

Set `$implementationSha` to this exact clean commit. Every observation below names it, its full 40-character SHA and unchanged scope files.

- [ ] **Step 9: Run the four-platform signature isolation matrix**

Run the feasibility app on:

```text
windows-10-webview2-111-x64: Windows 10 22H2 x64 + lowest available fixed WebView2 111.0.1661.x
windows-11-x64: frozen exact Windows 11 and current Evergreen runtime
macos-13-intel: macOS 13.3 Intel
macos-current-arm64: frozen exact current macOS/Apple Silicon runtime
```

On each matching host, require a clean checkout of `$implementationSha`, rebuild the feature executable into its isolated target, then run the matching lifecycle command:

```powershell
$env:CARGO_TARGET_DIR=(Join-Path (Resolve-Path src-tauri) 'target/feasibility')
pnpm tauri build --debug --no-bundle --config src-tauri/tauri.feasibility.conf.json --features feasibility
Remove-Item Env:CARGO_TARGET_DIR
node scripts/run-signature-lifecycle-probe.mjs --app src-tauri/target/feasibility/debug/yinmi.exe --platform-id windows-10-webview2-111-x64 --output artifacts/feasibility/signature/windows-10-webview2-111-x64-lifecycle.json
node scripts/run-signature-lifecycle-probe.mjs --app src-tauri/target/feasibility/debug/yinmi.exe --platform-id windows-11-x64 --output artifacts/feasibility/signature/windows-11-x64-lifecycle.json
node scripts/run-signature-lifecycle-probe.mjs --app src-tauri/target/feasibility/debug/yinmi --platform-id macos-13-intel --output artifacts/feasibility/signature/macos-13-intel-lifecycle.json
node scripts/run-signature-lifecycle-probe.mjs --app src-tauri/target/feasibility/debug/yinmi --platform-id macos-current-arm64 --output artifacts/feasibility/signature/macos-current-arm64-lifecycle.json
```

Use `WEBVIEW2_BROWSER_EXECUTABLE_FOLDER` only for the fixed Windows runtime and keep the absolute runtime path outside reports. Read the Windows runtime version from the created WebView2 environment's `BrowserVersionString`; read macOS from `wry::webview_version()` and cross-check the WebKit framework `CFBundleVersion`. Record the frozen exact value, OS version, architecture, command, clean tested SHA, `runtimeModes` = `native-host-raw-wry-0.55.1` and the exact resource policy mode.

Each row must prove every schema-v2 check and exact per-vector result map, including: host has zero managed WebViews; policy acknowledgement precedes official navigation; official `Finished` precedes polling; inert shim has no app effect/response; cross-origin canary server sees zero hits; new instance/restart cannot recover storage; the fixed fault/timeout/late-callback/retry scenarios; active-host main close with native, manager, policy-store, tombstone and TLS acknowledgements; 20-cycle/idle/sleep-wake lifecycle stability; and no host/taskbar/window/activation leak. Run the cross-process lifecycle runner for that platform and merge only its validated sanitized phase traces/process-info into the raw observation.

The Windows 10 row is accepted with legacy mode only if every source/vector is caught on the fixed 111 runtime. Any application IPC capability, Tauri global, managed WebView, resource/persistence/lifecycle bypass or creation race is `design-change-required`. A missing platform is `blocked`. Save sanitized rows to ignored `artifacts/feasibility/signature-webview.raw.json`.

- [ ] **Step 10: Run the three live pagination observations in separate quota windows**

Run `single_count_1000`, wait for a fresh five-minute quota window, run `paged_official_20`, wait again, then run `repeat_same_page`. Record the typed default source triple, requested bounded count, API-request count, page/valid/unique/duplicate/invalid counts, stop reason, incomplete flag, duration and response digests only.

ADR `0001-gd-pagination.md` selects the observed upstream count and numeric safety page limit no greater than 50. ADR `0002-signature-webview.md` records dedicated native host + raw WRY 0.55.1, capability-based inert-shim interpretation, selected platform policy modes, explicit nonpersistent storage, destroy-before-retry and the 20/5-second bounds. It may select healthy reuse or health-check-then-rebuild, but cannot weaken any isolation predicate.

Save sanitized live results to ignored `artifacts/feasibility/gd-contract-pagination.raw.json`. Raw pages remain below ignored `artifacts/feasibility/gd/raw/`.

- [ ] **Step 11: Build and commit validated evidence companions**

```powershell
cargo test --manifest-path src-tauri/Cargo.toml --features feasibility signature_webview -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml --features feasibility gd_live -- --nocapture
node scripts/verify-signature-host.mjs
node scripts/feasibility-evidence.mjs build --input artifacts/feasibility/signature-webview.raw.json --markdown docs/feasibility/signature-webview.md --output docs/feasibility/signature-webview.json
node scripts/feasibility-evidence.mjs build --input artifacts/feasibility/gd-contract-pagination.raw.json --markdown docs/feasibility/gd-contract-pagination.md --output docs/feasibility/gd-contract-pagination.json
node scripts/feasibility-evidence.mjs check docs/feasibility/signature-webview.json
node scripts/feasibility-evidence.mjs check docs/feasibility/gd-contract-pagination.json
```

Expected: both schema-v2 companions bind `$implementationSha`, exact current scopes, current Markdown/ADR hashes and every required machine field. Then commit only evidence and decisions:

```powershell
git add docs/feasibility/gd-contract-pagination.md docs/feasibility/gd-contract-pagination.json docs/feasibility/signature-webview.md docs/feasibility/signature-webview.json docs/decisions/0001-gd-pagination.md docs/decisions/0002-signature-webview.md
git commit -m "docs: record GD and raw WRY isolation evidence"
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
- Create: `src-tauri/src/feasibility/updater_policy.rs`
- Create: `src-tauri/tests/updater_probe.rs`
- Create: `scripts/slow-update-server.mjs`
- Create: `scripts/run-updater-exit-probe.mjs`
- Create: `scripts/run-updater-exit-probe.test.mjs`
- Modify: `src-tauri/build.rs`
- Modify: `src-tauri/permissions/feasibility.toml`
- Modify: `src-tauri/src/feasibility/mod.rs`
- Modify: `src-tauri/src/feasibility/signature_host.rs`
- Modify: `src-tauri/src/feasibility/signature_webview.rs`
- Modify: `src-tauri/src/feasibility/signature_probe.rs`
- Modify: `src-tauri/src/lib.rs`
- Modify: `src/lib/feasibility/FeasibilityPanel.svelte`
- Modify: `src-tauri/Cargo.toml`
- Modify: `src-tauri/Cargo.lock`
- Modify: `src-tauri/tauri.feasibility.conf.json`
- Modify: `scripts/verify-config.mjs`
- Modify: `package.json`
- Modify: `docs/feasibility/evidence.schema.json`
- Modify: `scripts/feasibility-evidence.mjs`
- Modify: `scripts/feasibility-evidence.test.mjs`
- Create: `docs/feasibility/updater-exit-barrier.md`
- Create: `docs/feasibility/updater-exit-barrier.json`
- Create: `docs/decisions/0006-updater-exit-barrier.md`
- Modify: `docs/feasibility/gd-contract-pagination.md`
- Modify: `docs/feasibility/gd-contract-pagination.json`
- Modify: `docs/feasibility/signature-webview.md`
- Modify: `docs/feasibility/signature-webview.json`
- Modify: `docs/decisions/0001-gd-pagination.md`
- Modify: `docs/decisions/0002-signature-webview.md`
- Modify: `docs/feasibility/evidence-scopes.json`

**Interfaces:**
- Consumes: Tauri updater `Update::download`, not `download_and_install`; design §6.5/§9; Task 4's acknowledged raw-host destroy path.
- Produces: pure `ExitBarrier`, `UpdateStopMode`, an actual local slow-download probe, one bounded ADR outcome, and one shared termination integration that completes active signature-host/policy teardown before either process exit or verified update installation.

The updater gate adds this exact machine-enforced object under `checks`; the map and every row use `additionalProperties: false`, all seven keys are required, and no aggregate or Markdown statement may substitute for a row:

```json
{
  "activeSignatureHostCleanupByPlatform": {
    "windows-x64": {
      "activeHostObservedBeforeExitRequest": true,
      "destroyAcknowledgedBeforeAppExitInvocation": true,
      "resourcePolicyCleanupAcknowledgedBeforeAppExitInvocation": true,
      "policyTombstonesEmptyBeforeAppExitInvocation": true,
      "tlsEntryAbsentBeforeAppExitInvocation": true,
      "hostWindowAbsentBeforeAppExitInvocation": true,
      "processExitObservedAfterAppExitInvocation": true
    },
    "macos-intel": {
      "activeHostObservedBeforeExitRequest": true,
      "destroyAcknowledgedBeforeAppExitInvocation": true,
      "resourcePolicyCleanupAcknowledgedBeforeAppExitInvocation": true,
      "policyTombstonesEmptyBeforeAppExitInvocation": true,
      "tlsEntryAbsentBeforeAppExitInvocation": true,
      "hostWindowAbsentBeforeAppExitInvocation": true,
      "processExitObservedAfterAppExitInvocation": true
    },
    "macos-arm64": {
      "activeHostObservedBeforeExitRequest": true,
      "destroyAcknowledgedBeforeAppExitInvocation": true,
      "resourcePolicyCleanupAcknowledgedBeforeAppExitInvocation": true,
      "policyTombstonesEmptyBeforeAppExitInvocation": true,
      "tlsEntryAbsentBeforeAppExitInvocation": true,
      "hostWindowAbsentBeforeAppExitInvocation": true,
      "processExitObservedAfterAppExitInvocation": true
    }
  }
}
```

The same `checks` object also contains `activeSignatureHostExitTracesByPlatform`. It has exactly the same three platform keys. Each platform value has `additionalProperties: false` and exactly `hostPlatform`, `hostArch`, `binaryTargetOs`, `binaryTargetArch`, `translatedProcess`, and `profiles`; `profiles` has exactly `cancelable` and `waitOnly`; and each profile value has `additionalProperties: false` plus exactly these committed sanitized fields:

```json
{
  "activeHostReadyOrdinal": 1,
  "exitRequestedOrdinal": 2,
  "updateTerminalOrdinal": 3,
  "signatureDestroyAcknowledgedOrdinal": 4,
  "hostWindowAbsentOrdinal": 5,
  "resourcePolicyCleanupAcknowledgedOrdinal": 6,
  "policyTombstonesEmptyOrdinal": 7,
  "tlsEntryAbsentOrdinal": 8,
  "appExitInvokedOrdinal": 9,
  "processExitObservedOrdinal": 10,
  "childExitCode": 0,
  "forcedKill": false
}
```

These are the three release-architecture rows for the updater gate, distinct from Task 4's four runtime-authority rows. Exact correlations are `windows-x64 -> host win32/x64, child windows/x86_64, translatedProcess=null`; `macos-intel -> host darwin/x64, child macos/x86_64, translatedProcess=false`; and `macos-arm64 -> host darwin/arm64, child macos/aarch64, translatedProcess=false`. Each platform row aggregates two fresh app processes, one per real updater profile. The loopback recorder assigns strictly increasing positive integer ordinals to the first nine events; the external runner appends `processExitObservedOrdinal` as the last server ordinal plus one only after the child actually exits. The child is forbidden from submitting that event. The real exit invoker posts and awaits the `app-exit-invoked` recorder acknowledgement immediately before calling `app.exit(0)`. Passing requires, in both committed profile traces, active host before exit request, update terminal before cleanup, native destroy/manager absence before policy cleanup, store cleanup and zero tombstones before TLS absence, every cleanup proof before app-exit invocation, and externally observed zero-code/non-forced process exit afterward. The helper derives `activeSignatureHostCleanupByPlatform` exclusively from this trace map and rejects any disagreement.

Schema/helper tests must reject a missing/extra platform/profile, missing/extra/wrongly typed trace or summary field, any summary `false`, a host/child/translation mismatch, duplicate/non-increasing/nonpositive ordinals, a missing host-ready prefix, policy cleanup/tombstone/TLS events out of order or at/after app-exit invocation, process exit at or before invocation, nonzero/forced exit, and caller-supplied summaries that disagree with either committed trace. Ignored raw input is only the source for building these sanitized traces; later `check` and Task 10 validate the committed ordinals directly.

The updater gate also requires exact-key `checks.updaterClassificationByPlatform` and `checks.productionPolicy` objects. `updaterClassificationByPlatform` has exactly the same three release-architecture keys and the same exact host/child/translation correlations. Each row has `additionalProperties: false` and exactly `hostPlatform`, `hostArch`, `binaryTargetOs`, `binaryTargetArch`, `translatedProcess`, `dropFuture`, `waitOnly`, and `productionValidation`:

```text
dropFuture exact fields:
  realUpdaterDownload=true
  probeRequestTimeoutMs=30000
  exitRequestedAfterFirstChunk=true
  terminalKind="cancelled"
  terminalElapsedMs integer in 0..5000
  serverDisconnectElapsedMs integer in 0..5000
  onFinishCalled=false
  installCalled=false
  appOwnedFileCount=0
  exitActionBeforeTerminal=false
  wouldInvokeAppExitAfterTerminal=true

waitOnly exact fields:
  realUpdaterDownload=true
  probeRequestTimeoutMs=3000
  settleGraceMs=2000
  exitRequestedAfterFirstChunk=true
  terminalKind exactly one of "timed-out" | "transport-error"
  terminalElapsedMs integer in 0..5000
  serverDisconnectElapsedMs integer in 0..5000
  onFinishCalled=false
  installCalled=false
  appOwnedFileCount=0
  exitActionBeforeTerminal=false
  wouldInvokeAppExitAfterTerminal=true

productionValidation exact fields:
  realUpdaterDownload=true
  selectedMode equals checks.productionPolicy.selectedMode
  requestTimeoutMs equals checks.productionPolicy.requestTimeoutMs
  settleGraceMs equals checks.productionPolicy.settleGraceMs
  exitRequestedAfterFirstChunk=true
  terminalKind is "cancelled" for drop-future, otherwise "timed-out" | "transport-error"
  terminalElapsedMs integer in 0..checks.productionPolicy.maximumExitWaitMs
  serverDisconnectElapsedMs integer in 0..checks.productionPolicy.maximumExitWaitMs
  onFinishCalled=false
  installCalled=false
  appOwnedFileCount=0
  waitingTextMatched=true
  failureTextMatched=true
  returnToAppTextMatched=true
  exitActionBeforeTerminal=false
  wouldInvokeAppExitAfterTerminal=true
  validated=true
```

`checks.productionPolicy` has `additionalProperties: false` and exactly these fields:

```text
selectedMode exactly one of "drop-future" | "bounded-wait-only"
requestTimeoutMs positive integer
settleGraceMs nonnegative integer
maximumExitWaitMs positive integer
maxSignedPackageBytes positive integer
minimumSupportedThroughputBytesPerSecond positive integer
derivedMinimumTransferMs = ceil(maxSignedPackageBytes * 1000 / minimumSupportedThroughputBytesPerSecond)
requestTimeoutCoversDerivedMinimum = (requestTimeoutMs >= derivedMinimumTransferMs) = true
waitingText nonempty exact Simplified Chinese string
failureText nonempty exact Simplified Chinese string
returnToAppText nonempty exact Simplified Chinese string
validatedByPlatform exact map {windows-x64:true, macos-intel:true, macos-arm64:true}
```

For `drop-future`, `maximumExitWaitMs` equals `settleGraceMs`; for `bounded-wait-only`, it equals `requestTimeoutMs + settleGraceMs`. All three `productionValidation` rows must be captured after these values and strings are frozen in code, and ADR `0006` must reproduce them exactly. Probe timeouts are classification inputs only and never silently become production values.

- [ ] **Step 1: Write failing pure exit-barrier tests**

Add:

```toml
tauri-plugin-updater = { version = "2.10.1", optional = true }

[features]
feasibility = [
  # retain every Task 4 entry
  "dep:tauri-plugin-updater",
]
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

The fake wait-only adapter is only a deterministic unit-test seam for `ExitBarrier`; its result can never satisfy the updater feasibility gate. Gate evidence must come from the real `tauri_plugin_updater::Update::download` path in Steps 4–7.

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
ExitProcess and InstallVerifiedUpdate both call the same prepare_for_process_termination() future first
CloseRequested always prevent_close() while any operation is active
CloseRequested only records intent and spawns/queues the async exit coordinator; it never waits inline
the exit coordinator and BarrierAction executor run on Tokio/background work, never inline on the event-loop/UI thread
prepare_for_process_termination first awaits Task 4 composite teardown, then verifies native/manager absence, resource-policy cleanup, zero tombstones and TLS Empty
only after that shared barrier may ExitProcess invoke app.exit() or InstallVerifiedUpdate invoke Update::install(bytes)
signature cleanup timeout/failure keeps the app open with stable feedback and invokes neither app.exit() nor install()
RunEvent::ExitRequested prevents/queues background cleanup while a host ticket is active; final RunEvent::Exit only performs idempotent nonwaiting UI detach and should observe Empty
download_and_install is forbidden
duplicate close/cancel events are idempotent
wait-only deadline+grace -> StayOpen, never forced exit
```

Run the pure tests. Expected: PASS.

- [ ] **Step 3: Create a deterministic slow signed update fixture**

`scripts/slow-update-server.mjs` must bind only `127.0.0.1`, serve `/latest.json` plus one fixed `/update.bin` route, stream the artifact in 64 KiB chunks, log request start/chunk/connection-close times, and expose no filesystem path supplied by an HTTP request. Its `--prepare <path>` mode creates exactly 8 MiB from a repeated fixed 64 KiB byte pattern and exits; normal mode accepts only explicit `--artifact`, `--signature` and `--profile cancelable|wait-only|classification|production-validation`, and rejects any port other than `38475`. `cancelable` sends a chunk every 250 ms. `wait-only` sends a chunk every 750 ms, so a real updater configured with a three-second probe request timeout cannot finish the signed artifact. `classification` applies `cancelable` to the first artifact request and `wait-only` to the second, then rejects further artifact requests. `production-validation` accepts exactly one artifact request and requires `--selected-mode drop-future|bounded-wait-only`; it uses the corresponding cancelable/stalling stream and is accepted only after that same mode exists in `updater_policy.rs`. The dynamic manifest contains exactly version `0.1.1`, URL `http://127.0.0.1:38475/update.bin`, and the complete `.sig` contents.

The same loopback server enables fixed `POST /probe-process` and `POST /probe-events` for all runner profiles, `POST /probe-report` only for non-exiting classification/production validation, and `POST /probe-policy` only for production validation. The runner generates a fresh 128-bit cryptographically random lowercase-hex run ID, passes it directly to the in-memory recorder as the only accepted ID, and injects it into the child as `YINMI_FEASIBILITY_UPDATER_RUN_ID`. Process bodies are at most 4 KiB and contain exactly `{runId,profile,binaryTargetOs,binaryTargetArch,translatedProcess}`; event bodies are at most 4 KiB and contain exactly `{runId,profile,event}`; report bodies are at most 32 KiB and contain exactly `{runId,profile,report}`; the policy declaration is at most 4 KiB and contains exactly `{runId,profile:"production-validation",selectedMode,maximumExitWaitMs}`. All endpoints exact-match run ID/profile and reject client ordinals/timestamps, stale/wrong/replayed IDs, unknown/duplicate/out-of-order messages, extra keys, second process/declaration/report messages and every request after the one-shot run completes.

Before any updater request or lifecycle event, the child reports Rust `std::env::consts::{OS,ARCH}` and, on macOS, the native `sysctl.proc_translated` result; Windows uses `null`. The runner requires exact agreement among child data, Node `process.platform/process.arch`, CLI platform ID and expected translation state. Rosetta or any mismatched child binary fails before the artifact route opens. These verified values, not caller labels, populate both updater platform maps.

After process-info acknowledgement, the exact classification event grammar is `drop-future-exit-requested`, `drop-future-terminal`, `wait-only-exit-requested`, `wait-only-terminal`, followed by exactly one report POST. Production validation first posts the policy declaration before any updater request, then uses `production-exit-requested`, `production-terminal`, followed by exactly one report POST; the final `ProductionPolicyReport.selectedMode` and `maximumExitWaitMs` must equal the declared values and selected CLI mode. After the first chunk, the app posts the matching `*-exit-requested` event and waits for its acknowledgement before starting its local `Instant` and applying `BarrierEvent::ExitRequested`; therefore the recorder's earlier start is a conservative bound rather than an understated cross-process timestamp. The recorder stops that clock only when the matching artifact socket emits connection close and supplies `serverDisconnectElapsedMs`, never the Rust DTO or a terminal log. A `*-terminal` event response is withheld until that matching socket-close observation arrives or the profile bound fails, and classification cannot open its second artifact request before the first terminal acknowledgement. The app measures `terminalElapsedMs` from the local barrier call to its real updater terminal. The runner waits for the exact app report, all expected connection-close observations and a zero-code child exit, validates both clock origins, and merges them with its verified host/child architecture into one sanitized machine report. No human-entered disconnect time can pass.

Real-exit profiles use the nine-event grammar `active-host-ready`, `exit-requested`, `update-terminal`, `signature-destroy-acknowledged`, `host-window-absent`, `resource-policy-cleanup-acknowledged`, `policy-tombstones-empty`, `tls-entry-absent`, `app-exit-invoked`. The recorder assigns strictly increasing ordinals server-side. Only these nine events are accepted from the child; `process-exit-observed` is forbidden on HTTP and appended by the parent runner as the last server ordinal plus one after actual child exit. The download routes reject more than the request count fixed by the selected profile. Output is supplied only by the local CLI under the ignored artifact directory, never by an HTTP parameter. `scripts/run-updater-exit-probe.test.mjs` proves nonce generation/propagation, missing/wrong/replayed-ID rejection, process-info/translation checks, every profile grammar, report/schema/connection-close merge, both elapsed origins, policy cleanup/tombstone ordering, child-exit handling, platform correlation and sanitization before implementation.

Runner deadlines are exact: 60 seconds for `classification`, `cancelable` and `wait-only`. Production validation allows 30 seconds from spawn for the nonce-bound policy declaration; after accepting it, the runner replaces that startup timer with checked addition of declared `maximumExitWaitMs + 30000` milliseconds and later requires exact equality with the final policy report/evidence. Overflow, a nonpositive value, missing/mismatched declaration or a second declaration fails closed. A timeout first writes sanitized diagnostic state, then kills only for local cleanup and exits nonzero; forced output is never gate evidence.

```powershell
node --test scripts/run-updater-exit-probe.test.mjs
```

Expected: FAIL because the external runner and recorder support are absent.

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

The generated `.sig`, public key, private key, and artifact remain below the ignored artifact directory and are never committed. `yinmi-feasibility-only` is an intentionally public test-only password, not a production secret, and must never be reused. With the Cargo feature enabled, `src-tauri/src/lib.rs` reads `YINMI_FEASIBILITY_UPDATER_PUBKEY_PATH` and `YINMI_FEASIBILITY_UPDATER_ENDPOINT` and supplies the public-key contents and the single endpoint to the updater builder. If both are absent, the other feasibility probes still start and the updater button is disabled; if only one is present, the endpoint is not fixed loopback, or the key is unreadable, startup fails closed.

Only `scripts/run-updater-exit-probe.mjs` may additionally set `YINMI_FEASIBILITY_UPDATER_AUTORUN=classify|validate-production|exit-cancelable|exit-wait-only`, `YINMI_FEASIBILITY_UPDATER_RUN_ID=<32 lowercase hex>`, and the fixed `YINMI_FEASIBILITY_UPDATER_TRACE_ENDPOINT=http://127.0.0.1:38475/probe-events`; process/report/policy endpoints are derived from that fixed origin, not supplied separately. Autorun without the base pair, exact endpoint, valid runner nonce, or exact host/child/platform correlation fails closed; these variables in an ordinary interactive process also fail closed. The runner accepts only `windows-x64`, `macos-intel`, or `macos-arm64`, verifies the Node host plus child process-info matrix above, and rejects a caller label mismatch or translated macOS child.

`classify` and `validate-production` use `RecordingExitInvoker`, post the exact report, await the recorder's response and request ordinary main-window close; they never call the real exit invoker or contribute to the active-host cleanup map. `exit-cancelable` and `exit-wait-only` first initialize a real active signature host, execute through `RealExitInvoker`, post/await the final invocation-boundary event and call `app.exit(0)`; they never post a report or submit the external process-exit event. The default build does not register the updater plugin or contain any autorun path.

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

Expose only the fixed feature command `feasibility_run_updater_classification_probe` and initially one matching classification button in `FeasibilityPanel.svelte`. Both scenarios use the real updater download. At the final exit-invocation boundary only, this in-process classification command injects `RecordingExitInvoker`, which records `wouldInvokeAppExit` and returns instead of terminating the process; it cannot satisfy the real-exit cleanup map. Freeze this DTO surface so implementation and evidence code do not invent fields:

```rust
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum UpdaterTerminalKind { Cancelled, TimedOut, TransportError }

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum UpdateStopModeName { DropFuture, BoundedWaitOnly }

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RealDownloadProbeReport {
    pub real_updater_download: bool,
    pub request_timeout_ms: u64,
    pub settle_grace_ms: u64,
    pub exit_requested_after_first_chunk: bool,
    pub terminal_kind: UpdaterTerminalKind,
    pub terminal_elapsed_ms: u64,
    pub on_finish_called: bool,
    pub install_called: bool,
    pub app_owned_file_count: u32,
    pub exit_action_before_terminal: bool,
    pub would_invoke_app_exit_after_terminal: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdaterClassificationReport {
    pub drop_future: RealDownloadProbeReport,
    pub wait_only: RealDownloadProbeReport,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdaterFeedbackReport {
    pub waiting: String,
    pub failure: String,
    pub return_to_app: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProductionPolicyReport {
    pub selected_mode: UpdateStopModeName,
    pub request_timeout_ms: u64,
    pub settle_grace_ms: u64,
    pub maximum_exit_wait_ms: u64,
    pub max_signed_package_bytes: u64,
    pub minimum_supported_throughput_bytes_per_second: u64,
    pub derived_minimum_transfer_ms: u64,
    pub feedback: UpdaterFeedbackReport,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProductionPolicyValidationReport {
    pub policy: ProductionPolicyReport,
    pub observation: RealDownloadProbeReport,
}

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum UpdaterProbeAction { Classify, ValidateProductionPolicy }

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", content = "report", rename_all = "kebab-case")]
pub enum UpdaterProbeReport {
    Classification(UpdaterClassificationReport),
    ProductionValidation(ProductionPolicyValidationReport),
}
```

The preliminary harness commit implements only `UpdaterProbeAction::Classify`; `ValidateProductionPolicy` is enabled only in the second policy-freeze commit after `updater_policy.rs` contains non-placeholder constants and exact strings. Both variants remain behind the same command/permission, so no second updater IPC surface is added.

`RealDownloadProbeReport` intentionally has no server-disconnect field: the app cannot observe the server's socket-close clock. Only the nonce-bound runner may add `serverDisconnectElapsedMs` while merging the recorder observation. A standalone UI/IPC report, copied terminal log or caller-supplied elapsed value is never valid gate input.

The evidence helper requires the three final-SHA `ProductionPolicyReport` values to be byte-for-byte equal, flattens their nested feedback strings into `checks.productionPolicy`, recomputes `derivedMinimumTransferMs`, and derives `validatedByPlatform` from the three validated runner rows. Raw input cannot supply that aggregate map.

Cancelable scenario: start the actual updater download against server profile `cancelable`, request exit after the first chunk, cancel and drop the download future. Passing requires wrapper `Cancelled`, server disconnect within 5 seconds, `on_finish` false, install never called, no app-owned file, no `ExitProcess` action before cancellation terminal, and a recorded would-exit invocation only afterward.

Wait-only scenario: create a fresh real updater with `UpdaterBuilder::timeout(Duration::from_secs(3))`, check again against server profile `wait-only`, request exit after the first chunk, and keep polling the real `Update::download` future until it returns. Passing requires a real updater timeout/error terminal within 3 seconds plus 2 seconds settle grace, server connection close, `on_finish` false, install never called, no app-owned file, no `ExitProcess` action before that terminal, and a recorded would-exit invocation only after terminal/cleanup. During this preliminary classification the UI labels the values as probe-only; production waiting/failure/return strings are not selected yet. A fake download adapter may test state transitions but cannot produce either real-download report; the injected exit recorder is accepted only for these non-exiting classification/production-validation reports.

Expected before implementation: both scenarios fail.

- [ ] **Step 5: Implement the preliminary classifier and collect three-platform observations**

Implement the slow server's recorder route and `run-updater-exit-probe.mjs` until its Node tests pass. Add `feasibility_run_updater_classification_probe` to the feature-only `AppManifest`, permission, and handler. The existing default-artifact prefix check covers it automatically. Build the drop-future updater through `UpdaterExt::updater_builder().timeout(Duration::from_secs(30)).no_proxy()` and the wait-only updater through `UpdaterExt::updater_builder().timeout(Duration::from_secs(3)).no_proxy()`, both with the fixed loopback endpoint and test public key. Put `RecordingExitInvoker` and `RealExitInvoker` behind the same private executor seam; tests must prove only the feature autorun path can construct the real invoker. Implement both probe modes without claiming the production result in advance.

Add package script `"test:updater-exit-runner": "node --test scripts/run-updater-exit-probe.test.mjs"` and append it to `quality`; this script-only package change does not alter `pnpm-lock.yaml`. Runner tests cover all four profiles, app-report/server-close merging, exit-request clock origins, nonce replay and wrong host/child/translation cases for all three exact correlations. Add active-signature-host integration tests for both terminal actions: `ExitProcess` and `InstallVerifiedUpdate` must pass the same native/manager/policy/tombstone/TLS composite barrier before entering their respective `app.exit()` or `Update::install` invocation boundary; repeated actions remain idempotent, and every cleanup timeout keeps both boundaries untouched.

Run the pure/fake, runner and isolation tests, then build feature and default artifacts into separate absolute target directories so the default build cannot overwrite the executable reserved for later real-exit runs:

```powershell
pnpm format
cargo fmt --manifest-path src-tauri/Cargo.toml --all
pnpm test:updater-exit-runner
cargo test --manifest-path src-tauri/Cargo.toml --features feasibility --test updater_probe -- --nocapture --test-threads=1
cargo test --manifest-path src-tauri/Cargo.toml --features feasibility signature_webview -- --nocapture
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets --features feasibility -- -D warnings
node scripts/verify-signature-host.mjs
pnpm quality
$env:CARGO_TARGET_DIR=(Join-Path (Resolve-Path src-tauri) 'target/feasibility')
pnpm tauri build --debug --no-bundle --config src-tauri/tauri.feasibility.conf.json --features feasibility
$env:CARGO_TARGET_DIR=(Join-Path (Resolve-Path src-tauri) 'target/default')
pnpm tauri build --debug --no-bundle
pnpm verify:default-artifacts
Remove-Item Env:CARGO_TARGET_DIR
git add src-tauri/src/feasibility/updater_probe.rs src-tauri/src/feasibility/mod.rs src-tauri/src/feasibility/signature_host.rs src-tauri/src/feasibility/signature_webview.rs src-tauri/src/feasibility/signature_probe.rs src-tauri/src/lib.rs src-tauri/build.rs src-tauri/permissions/feasibility.toml src-tauri/tests/updater_probe.rs src/lib/feasibility/FeasibilityPanel.svelte src-tauri/Cargo.toml src-tauri/Cargo.lock src-tauri/tauri.feasibility.conf.json scripts/verify-config.mjs scripts/slow-update-server.mjs scripts/run-updater-exit-probe.mjs scripts/run-updater-exit-probe.test.mjs package.json
git commit -m "feat: add updater exit classification probe"
git status --short
```

Expected: status is clean. On Windows x64, macOS Intel and macOS ARM64, check out this exact preliminary commit with a clean tree and build the isolated feature executable into `src-tauri/target/feasibility`. Then run the one matching command; the runner owns the server, nonce, autorun child, app report, server disconnect observations and child cleanup:

```powershell
# Windows x64
node scripts/run-updater-exit-probe.mjs --app src-tauri/target/feasibility/debug/yinmi.exe --artifact artifacts/feasibility/updater/update-0.1.1.bin --signature artifacts/feasibility/updater/update-0.1.1.bin.sig --pubkey artifacts/feasibility/updater/test.key.pub --platform-id windows-x64 --profile classification --output artifacts/feasibility/updater/preliminary/windows-x64-classification.json

# macOS Intel
node scripts/run-updater-exit-probe.mjs --app src-tauri/target/feasibility/debug/yinmi --artifact artifacts/feasibility/updater/update-0.1.1.bin --signature artifacts/feasibility/updater/update-0.1.1.bin.sig --pubkey artifacts/feasibility/updater/test.key.pub --platform-id macos-intel --profile classification --output artifacts/feasibility/updater/preliminary/macos-intel-classification.json

# macOS ARM64
node scripts/run-updater-exit-probe.mjs --app src-tauri/target/feasibility/debug/yinmi --artifact artifacts/feasibility/updater/update-0.1.1.bin --signature artifacts/feasibility/updater/update-0.1.1.bin.sig --pubkey artifacts/feasibility/updater/test.key.pub --platform-id macos-arm64 --profile classification --output artifacts/feasibility/updater/preliminary/macos-arm64-classification.json
```

Each run performs real drop-future then real wait-only through `RecordingExitInvoker`, merges the exact app report with recorder-owned disconnect times, and exits the child normally. Transfer all three sanitized reports to the primary host. They select the policy but are not final gate inputs because the policy commit does not exist yet.

- [ ] **Step 6: Freeze, commit and validate one production policy**

Apply the three-platform preliminary results mechanically:

```text
all three drop-future rows cancel and disconnect <=5s -> select drop-future
otherwise, all three real wait-only rows reach a true terminal within probe request timeout + 2s -> select bounded-wait-only
fake-only wait result, missing platform/server terminal, or mixed unbounded behavior -> blocked, never pass
neither candidate is bounded on every platform -> design-change-required; stop Phase 1
```

Only after one candidate passes, create `updater_policy.rs` with non-placeholder constants for `SELECTED_MODE`, `REQUEST_TIMEOUT_MS`, `SETTLE_GRACE_MS`, `MAXIMUM_EXIT_WAIT_MS`, `MAX_SIGNED_PACKAGE_BYTES`, `MINIMUM_SUPPORTED_THROUGHPUT_BYTES_PER_SECOND`, and the exact `WAITING_TEXT`, `FAILURE_TEXT`, and `RETURN_TO_APP_TEXT`. Derive transfer time with checked integer arithmetic, enforce the relations in `checks.productionPolicy`, and add compile-time/unit assertions where possible. A finite production request timeout must cover the stated maximum signed package at the minimum supported throughput; if it cannot, stop with `design-change-required`. Infinite waiting is forbidden.

Enable `UpdaterProbeAction::ValidateProductionPolicy` through the existing command and add a second feature-only button. It must build the updater through the same private builder factory used by the product path, use the frozen request timeout/mode, inject only `RecordingExitInvoker`, exercise the `production-validation` server profile, and return `ProductionPolicyValidationReport`. Tests compare the three displayed strings byte-for-byte with policy constants and prove waiting, failure, return-to-app, no early exit/install, terminal/disconnect bounds and selected-mode consistency.

Now extend schema/helper tests first, then implementation, for the exact classification, production-policy, host/child correlation, composite cleanup and trace contracts above. Create the exact `updater-exit-barrier` scope entry and extend both `signature-webview` and `gd-contract-pagination` for every changed raw-host/lifecycle/command/ACL/config/frontend/Cargo file. The updater scope includes `updater_policy.rs`, `signature_host.rs`, `signature_webview.rs`, `signature_probe.rs`, all three resource-policy files, deterministic tests/source gate, both updater runner files, `package.json`, and both active-host termination-action integration tests.

```powershell
pnpm format
cargo fmt --manifest-path src-tauri/Cargo.toml --all
node --test scripts/feasibility-evidence.test.mjs
pnpm test:updater-exit-runner
cargo test --manifest-path src-tauri/Cargo.toml --features feasibility --test updater_probe -- --nocapture --test-threads=1
cargo test --manifest-path src-tauri/Cargo.toml --features feasibility signature_webview -- --nocapture
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets --features feasibility -- -D warnings
node scripts/verify-signature-host.mjs
pnpm quality
git add src-tauri/src/feasibility/updater_policy.rs src-tauri/src/feasibility/updater_probe.rs src-tauri/src/feasibility/mod.rs src-tauri/src/feasibility/signature_host.rs src-tauri/src/feasibility/signature_webview.rs src-tauri/src/feasibility/signature_probe.rs src-tauri/src/lib.rs src-tauri/tests/updater_probe.rs src/lib/feasibility/FeasibilityPanel.svelte src-tauri/Cargo.toml src-tauri/Cargo.lock docs/feasibility/evidence.schema.json docs/feasibility/evidence-scopes.json scripts/feasibility-evidence.mjs scripts/feasibility-evidence.test.mjs
git commit -m "feat: freeze bounded updater exit policy"
git status --short
```

Expected: status is clean. This second commit is the only Task 8 `testedCommit`. On each target host check out that exact commit and build the final feature executable and default artifact into separate targets:

```powershell
$env:CARGO_TARGET_DIR=(Join-Path (Resolve-Path src-tauri) 'target/feasibility')
pnpm tauri build --debug --no-bundle --config src-tauri/tauri.feasibility.conf.json --features feasibility
$env:CARGO_TARGET_DIR=(Join-Path (Resolve-Path src-tauri) 'target/default')
pnpm tauri build --debug --no-bundle
pnpm verify:default-artifacts
Remove-Item Env:CARGO_TARGET_DIR
```

Rerun final-SHA classification with the matching platform command below; preliminary-commit reports cannot be copied forward:

```powershell
# Windows x64
node scripts/run-updater-exit-probe.mjs --app src-tauri/target/feasibility/debug/yinmi.exe --artifact artifacts/feasibility/updater/update-0.1.1.bin --signature artifacts/feasibility/updater/update-0.1.1.bin.sig --pubkey artifacts/feasibility/updater/test.key.pub --platform-id windows-x64 --profile classification --output artifacts/feasibility/updater/windows-x64-classification.json

# macOS Intel
node scripts/run-updater-exit-probe.mjs --app src-tauri/target/feasibility/debug/yinmi --artifact artifacts/feasibility/updater/update-0.1.1.bin --signature artifacts/feasibility/updater/update-0.1.1.bin.sig --pubkey artifacts/feasibility/updater/test.key.pub --platform-id macos-intel --profile classification --output artifacts/feasibility/updater/macos-intel-classification.json

# macOS ARM64
node scripts/run-updater-exit-probe.mjs --app src-tauri/target/feasibility/debug/yinmi --artifact artifacts/feasibility/updater/update-0.1.1.bin --signature artifacts/feasibility/updater/update-0.1.1.bin.sig --pubkey artifacts/feasibility/updater/test.key.pub --platform-id macos-arm64 --profile classification --output artifacts/feasibility/updater/macos-arm64-classification.json
```

Then run exactly one production-validation command on that host according to the committed `SELECTED_MODE`:

```powershell
# Windows x64; choose exactly one selected-mode value
node scripts/run-updater-exit-probe.mjs --app src-tauri/target/feasibility/debug/yinmi.exe --artifact artifacts/feasibility/updater/update-0.1.1.bin --signature artifacts/feasibility/updater/update-0.1.1.bin.sig --pubkey artifacts/feasibility/updater/test.key.pub --platform-id windows-x64 --profile production-validation --selected-mode drop-future --output artifacts/feasibility/updater/windows-x64-production-validation.json
node scripts/run-updater-exit-probe.mjs --app src-tauri/target/feasibility/debug/yinmi.exe --artifact artifacts/feasibility/updater/update-0.1.1.bin --signature artifacts/feasibility/updater/update-0.1.1.bin.sig --pubkey artifacts/feasibility/updater/test.key.pub --platform-id windows-x64 --profile production-validation --selected-mode bounded-wait-only --output artifacts/feasibility/updater/windows-x64-production-validation.json

# macOS Intel; choose exactly one selected-mode value
node scripts/run-updater-exit-probe.mjs --app src-tauri/target/feasibility/debug/yinmi --artifact artifacts/feasibility/updater/update-0.1.1.bin --signature artifacts/feasibility/updater/update-0.1.1.bin.sig --pubkey artifacts/feasibility/updater/test.key.pub --platform-id macos-intel --profile production-validation --selected-mode drop-future --output artifacts/feasibility/updater/macos-intel-production-validation.json
node scripts/run-updater-exit-probe.mjs --app src-tauri/target/feasibility/debug/yinmi --artifact artifacts/feasibility/updater/update-0.1.1.bin --signature artifacts/feasibility/updater/update-0.1.1.bin.sig --pubkey artifacts/feasibility/updater/test.key.pub --platform-id macos-intel --profile production-validation --selected-mode bounded-wait-only --output artifacts/feasibility/updater/macos-intel-production-validation.json

# macOS ARM64; choose exactly one selected-mode value
node scripts/run-updater-exit-probe.mjs --app src-tauri/target/feasibility/debug/yinmi --artifact artifacts/feasibility/updater/update-0.1.1.bin --signature artifacts/feasibility/updater/update-0.1.1.bin.sig --pubkey artifacts/feasibility/updater/test.key.pub --platform-id macos-arm64 --profile production-validation --selected-mode drop-future --output artifacts/feasibility/updater/macos-arm64-production-validation.json
node scripts/run-updater-exit-probe.mjs --app src-tauri/target/feasibility/debug/yinmi --artifact artifacts/feasibility/updater/update-0.1.1.bin --signature artifacts/feasibility/updater/update-0.1.1.bin.sig --pubkey artifacts/feasibility/updater/test.key.pub --platform-id macos-arm64 --profile production-validation --selected-mode bounded-wait-only --output artifacts/feasibility/updater/macos-arm64-production-validation.json
```

The runner rejects the unselected mode by comparing the CLI value with the child report and committed policy. Run both real-exit profiles in fresh child processes; the runner starts its own fixed-port server/recorder, sets the strict nonce/autorun environment, observes child exit, and never kills a passing child:

```powershell
# Windows x64
node scripts/run-updater-exit-probe.mjs --app src-tauri/target/feasibility/debug/yinmi.exe --artifact artifacts/feasibility/updater/update-0.1.1.bin --signature artifacts/feasibility/updater/update-0.1.1.bin.sig --pubkey artifacts/feasibility/updater/test.key.pub --platform-id windows-x64 --profile cancelable --output artifacts/feasibility/updater/windows-x64-cancelable.json
node scripts/run-updater-exit-probe.mjs --app src-tauri/target/feasibility/debug/yinmi.exe --artifact artifacts/feasibility/updater/update-0.1.1.bin --signature artifacts/feasibility/updater/update-0.1.1.bin.sig --pubkey artifacts/feasibility/updater/test.key.pub --platform-id windows-x64 --profile wait-only --output artifacts/feasibility/updater/windows-x64-wait-only.json

# macOS Intel
node scripts/run-updater-exit-probe.mjs --app src-tauri/target/feasibility/debug/yinmi --artifact artifacts/feasibility/updater/update-0.1.1.bin --signature artifacts/feasibility/updater/update-0.1.1.bin.sig --pubkey artifacts/feasibility/updater/test.key.pub --platform-id macos-intel --profile cancelable --output artifacts/feasibility/updater/macos-intel-cancelable.json
node scripts/run-updater-exit-probe.mjs --app src-tauri/target/feasibility/debug/yinmi --artifact artifacts/feasibility/updater/update-0.1.1.bin --signature artifacts/feasibility/updater/update-0.1.1.bin.sig --pubkey artifacts/feasibility/updater/test.key.pub --platform-id macos-intel --profile wait-only --output artifacts/feasibility/updater/macos-intel-wait-only.json

# macOS ARM64
node scripts/run-updater-exit-probe.mjs --app src-tauri/target/feasibility/debug/yinmi --artifact artifacts/feasibility/updater/update-0.1.1.bin --signature artifacts/feasibility/updater/update-0.1.1.bin.sig --pubkey artifacts/feasibility/updater/test.key.pub --platform-id macos-arm64 --profile cancelable --output artifacts/feasibility/updater/macos-arm64-cancelable.json
node scripts/run-updater-exit-probe.mjs --app src-tauri/target/feasibility/debug/yinmi --artifact artifacts/feasibility/updater/update-0.1.1.bin --signature artifacts/feasibility/updater/update-0.1.1.bin.sig --pubkey artifacts/feasibility/updater/test.key.pub --platform-id macos-arm64 --profile wait-only --output artifacts/feasibility/updater/macos-arm64-wait-only.json
```

The tested CLI accepts only the three correlated platform IDs and four profiles `classification|production-validation|cancelable|wait-only`, plus an existing feature binary/artifact/signature/public key and output path under the ignored updater directory. `--selected-mode` is required only for production validation and forbidden otherwise. It uses a fresh run ID per child. On timeout it saves sanitized diagnostic state, kills the child only for local cleanup, records `forcedKill=true` and exits nonzero; such output can never pass. Transfer only final-SHA runner outputs; the evidence helper consumes merged classification/production records and the exact two real-exit traces per platform, never standalone app DTOs or server logs.

- [ ] **Step 7: Record platform evidence and commit**

Use only the final policy commit's non-exiting real-download classification, production-policy validation and both external real-exit profiles from Windows x64 and both macOS architectures. `docs/feasibility/updater-exit-barrier.md` records both probe timeouts, selected production mode/constants/derivation, chunk count, cancellation/exit-request time, disconnect/terminal time, exact visible feedback strings, whether install was invoked, and every barrier decision. ADR `0006` reproduces the policy constants and all three strings byte-for-byte and states why the rejected mode was not selected. For both external traces in each platform row, a real signature host must be ready before the exit request; server-assigned ordinals must put native destroy, manager absence, resource-policy cleanup, zero tombstones and TLS absence before the `app.exit()` invocation boundary, and the runner must observe a clean child exit afterward. Deterministic integration tests separately prove the identical composite barrier before `Update::install`. The helper commits exact `updaterClassificationByPlatform`, `productionPolicy`, and `activeSignatureHostExitTracesByPlatform`, derives the seven-field `activeSignatureHostCleanupByPlatform` summary, and rejects any policy/ADR/report/host/child mismatch. Save source observations to ignored `artifacts/feasibility/updater-exit-barrier.raw.json`; no signing secret, private key, run ID, absolute path or marker value enters either raw transfer or committed companion.

This task changed the lifecycle, command manifest, permissions and feature app. Rerun the complete schema-v2 four-platform raw WRY matrix from Task 4, including all application-IPC predicates, exact per-vector results, zero canary-server hits, nonpersistent storage, policy-before-first-network-navigation, fault/late-callback isolation, active-host ordinary main close and 20-cycle leak checks. Refresh `artifacts/feasibility/signature-webview.raw.json` and rebuild its Markdown/JSON companion. Updater-specific real exit is proven only by the three release-architecture rows above; it is not duplicated into Task 4's four runtime-authority rows. Reusing or field-copying the old signature companion must fail its schema/design/scope checks.

The same raw-host and ACL files are in the GD gate scope. Therefore rerun the three fixed GD cases in fresh quota windows, refresh `artifacts/feasibility/gd-contract-pagination.raw.json` and rebuild its Markdown/JSON companion; do not merely rehash the Task 5 result.

The separate-target final feature/default builds in Step 6 and `pnpm verify:default-artifacts` must pass on every architecture; never rebuild default into `src-tauri/target/feasibility`. This proves updater probe commands and environment-variable names do not enter the normal artifact while preserving the exact feature executable used by the external runner.

```powershell
node scripts/feasibility-evidence.mjs build --input artifacts/feasibility/updater-exit-barrier.raw.json --markdown docs/feasibility/updater-exit-barrier.md --output docs/feasibility/updater-exit-barrier.json
node scripts/feasibility-evidence.mjs build --input artifacts/feasibility/signature-webview.raw.json --markdown docs/feasibility/signature-webview.md --output docs/feasibility/signature-webview.json
node scripts/feasibility-evidence.mjs build --input artifacts/feasibility/gd-contract-pagination.raw.json --markdown docs/feasibility/gd-contract-pagination.md --output docs/feasibility/gd-contract-pagination.json
node scripts/feasibility-evidence.mjs check docs/feasibility/updater-exit-barrier.json
node scripts/feasibility-evidence.mjs check docs/feasibility/signature-webview.json
node scripts/feasibility-evidence.mjs check docs/feasibility/gd-contract-pagination.json
git add docs/feasibility/updater-exit-barrier.md docs/feasibility/updater-exit-barrier.json docs/feasibility/signature-webview.md docs/feasibility/signature-webview.json docs/feasibility/gd-contract-pagination.md docs/feasibility/gd-contract-pagination.json docs/decisions/0001-gd-pagination.md docs/decisions/0002-signature-webview.md docs/decisions/0006-updater-exit-barrier.md
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
GD default source is not Netease, default count is not 20, a search count outside 1–1000 reaches rendering, the count=1000 live case is missing, or pagination/response predicates differ -> fail
signature schema is not exactly 2, legacy ipcBridgeAbsent appears, rawWryHost/managed-WebView flags are wrong, Tauri globals/application IPC handler/response/state effect appears, fixed WebView2 111 is missing, host/child/translation or resource-policy/runtime version correlation is invalid, policy-before-first-network-navigation/official-finish/storage/lifecycle/fault/late-callback checks fail, policy-store cleanup or zero-tombstone-before-exit is absent, a per-vector key/result/evidence/barrier mapping differs, a non-service-worker or probe-error result is labeled unavailable, a required macOS counterfactual is absent, redirect hop counts differ, either row/top-level protected-hit sum disagrees, or any protected canary hit occurs -> fail
network peer pin/body limit/proxy check false -> fail
atomic winner count other than one -> fail
media negative family accepted -> fail
updater fake-only wait result, wrong host/child architecture or translated macOS process, missing nonce-bound app-report/server-close merge, caller-supplied disconnect time or wrong exit-request clock origin, classification exact key/type/timeout/bound mismatch, missing or invalid derived production constant, request timeout below derived transfer minimum, policy/ADR/text mismatch, selected mode inconsistent with validation, any platform productionValidation not true/bounded, early exit/install, either terminal action bypasses the shared composite teardown, missing/extra active-signature-host platform/profile row, no active-host-ready prefix, invalid/non-increasing ordinal, native/manager/policy/tombstone/TLS check not before app-exit invocation, process exit not observed afterward, nonzero/forced exit, or caller-supplied cleanup aggregate disagreement -> fail
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
export const DESIGN_COMMIT = '782b30d8eb1075cce708ddef878cd236d2fa7dc2';
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

Import and call `validateEvidence` for the first seven companions and the Task 9 report validator for performance. Require schema version 2 and the exact revised design commit on the first seven common-envelope companions; reject legacy signature keys rather than tolerating mixed v1/v2 semantics. The performance validator instead enforces its independent schema, exact final tested SHA, platform set and current common scope/Markdown/ADR hashes. Load `evidence-scopes.json` and require exact set equality for each gate; raw or companion JSON cannot choose its own scope. Require exact platform-set equality, current nonempty Markdown and ADR hashes, current scope hash, valid clean ancestor `testedCommit`, and each gate-specific predicate from the Machine-Readable Evidence Contract. The signature rule validates exact-key per-platform rows, all derived fields/maps, host/child/translation correlations and composite teardown. The updater rule independently recomputes classification bounds, policy derivation, host/child correlation, profile ordinals, seven-field cleanup summary, production-validation equality and ADR text/constants. `collectPhase1Results` derives aggregate entries from validated companions; no caller can supply status `pass`. Support only:

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

Do not add live GD, absolute performance or actual updater probes to ordinary hosted CI. Retain Task 8's deterministic `test:updater-exit-runner` inside `quality`. Add `"test:phase1-gate": "node --test scripts/check-phase1-gate.test.mjs"` to package scripts and append that deterministic test, not strict historical evidence validation, to ordinary `quality`; `quality.yml` continues to run `pnpm quality`. In `platform-windows`, after the default debug build, add a final step guarded by `hashFiles('docs/feasibility/phase-1-results.json') != ''` that runs `pnpm phase1:gate`; the pre-aggregate gate-mechanics commit skips it, while the final evidence commit must execute it. The macOS jobs run the feature suites above, so their aggregate check remains the stable macOS proof.

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

Set `$finalCodeSha` to the Step 5 commit and require a clean tracked worktree. Before editing any Markdown, JSON or ADR, check out that exact SHA with full history on every required host and rerun all raw observations from Tasks 4–9: the complete schema-v2 four-row raw WRY isolation/lifecycle matrix including fixed WebView2 111, exact per-vector results, child architecture/process-translation proof and composite active-host ordinary main close; three GD cases in separate quota windows; network on three architectures; NTFS/APFS atomic races; media round trips; all four nonce-bound updater runner profiles—final-SHA non-exiting classification, production-policy validation, cancelable real exit and wait-only real exit—on each of the three updater architecture rows; and the Windows plus two macOS performance captures. Both lifecycle runners must reject any platform ID inconsistent with the Node host or reported child binary and reject translated macOS children; the updater runner also merges app terminal reports with recorder-owned disconnect clocks for the first two profiles. Transfer only ignored sanitized raw JSON to the primary host. All raw inputs must name `$finalCodeSha`, record `clean=true`, and use unchanged scoped files. The Step 5 CI runs are the toolchain raw input. Because the design identity, common schema/helper and final scope changed, rebuild every companion from fresh raw inputs; no common-envelope schema-v1 JSON may be copied or edited forward, and the independent performance report must be finalized anew.

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
| §5.5 zero-application-capability native host + raw WRY signature WebView | Task 4 |
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
- [Tauri native `WindowBuilder` 2.11.5](https://docs.rs/tauri/2.11.5/tauri/window/struct.WindowBuilder.html)
- [Tauri `AppHandle::run_on_main_thread` 2.11.5](https://docs.rs/tauri/2.11.5/tauri/struct.AppHandle.html#method.run_on_main_thread)
- [WRY `WebViewBuilder` 0.55.1](https://docs.rs/wry/0.55.1/wry/struct.WebViewBuilder.html)
- [WRY `WebView` 0.55.1](https://docs.rs/wry/0.55.1/wry/struct.WebView.html)
- [WRY Windows extension 0.55.1](https://docs.rs/wry/0.55.1/x86_64-pc-windows-msvc/wry/trait.WebViewExtWindows.html)
- [WRY macOS builder extension 0.55.1](https://docs.rs/wry/0.55.1/x86_64-apple-darwin/wry/trait.WebViewBuilderExtMacos.html)
- [WRY macOS WebView extension 0.55.1](https://docs.rs/wry/0.55.1/x86_64-apple-darwin/wry/trait.WebViewExtMacOS.html)
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
