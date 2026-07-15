# Cross-platform CI feasibility evidence

## Observation

- Tested commit: `323538ca9b0e33173b67778310657966196b0dc5`
- Event: `push`
- Observed through: `2026-07-15T14:11:33.226Z`
- Workflows: `quality` run `29421799110`; `platform-smoke` run `29421799009`
- Stable required-check candidates: `quality`, `platform-windows`, `platform-macos`

Every check and supporting native job below completed successfully against the exact tested commit.

## Stable checks

| Check | Workflow | Result | Job |
| --- | --- | --- | --- |
| `quality` | `quality` | `success` | [87373863329](https://github.com/xiaobing6/yinmi-desktop/actions/runs/29421799110/job/87373863329) |
| `platform-windows` | `platform-smoke` | `success` | [87373863351](https://github.com/xiaobing6/yinmi-desktop/actions/runs/29421799009/job/87373863351) |
| `platform-macos` | `platform-smoke` aggregate | `success` | [87376084020](https://github.com/xiaobing6/yinmi-desktop/actions/runs/29421799009/job/87376084020) |

## Native supporting jobs

| Job | Platform ID | OS | Architecture | Runner image | Runner | Result |
| --- | --- | --- | --- | --- | --- | --- |
| [platform-windows](https://github.com/xiaobing6/yinmi-desktop/actions/runs/29421799009/job/87373863351) | `windows-x64` | Microsoft Windows Server 2025, 10.0.26100 Datacenter | `x86_64` | `windows-2025-vs2026` `20260714.173.1` | `2.335.1` | `success` |
| [platform-macos-intel](https://github.com/xiaobing6/yinmi-desktop/actions/runs/29421799009/job/87373863412) | `macos-intel` | macOS 15.7.7 (24G720) | `x86_64` | `macos-15` `20260629.0276.1` | `2.335.1` | `success` |
| [platform-macos-arm](https://github.com/xiaobing6/yinmi-desktop/actions/runs/29421799009/job/87373863321) | `macos-arm64` | macOS 15.7.7 (24G720) | `aarch64` | `macos-15-arm64` `20260706.0213.1` | `2.335.1` | `success` |

The macOS aggregate required both supporting macOS jobs to conclude `success`.

## Successful native command sequences

- Windows x64: `pnpm install --frozen-lockfile && pnpm build && cargo test --manifest-path src-tauri/Cargo.toml --all-targets && pnpm tauri build --debug --no-bundle --target x86_64-pc-windows-msvc`
- macOS Intel: `pnpm install --frozen-lockfile && pnpm build && cargo test --manifest-path src-tauri/Cargo.toml --all-targets && cargo check --manifest-path src-tauri/Cargo.toml --target x86_64-apple-darwin && pnpm tauri build --debug --no-bundle --target x86_64-apple-darwin`
- macOS ARM: `pnpm install --frozen-lockfile && pnpm build && cargo test --manifest-path src-tauri/Cargo.toml --all-targets && cargo check --manifest-path src-tauri/Cargo.toml --target aarch64-apple-darwin && cargo check --manifest-path src-tauri/Cargo.toml --target x86_64-apple-darwin && pnpm tauri build --debug --no-bundle --target universal-apple-darwin && lipo -archs src-tauri/target/universal-apple-darwin/debug/yinmi`

## Local tool versions

- Node.js: `v24.16.0`
- pnpm: `11.7.0`
- rustc: `1.97.0 (2d8144b78 2026-07-07)`
- cargo: `1.97.0 (c980f4866 2026-06-30)`
- Tauri CLI: `2.11.4`

Conclusion: pass
