# 音觅

音觅是一款适用于 Windows 和 macOS 的桌面音乐搜索与批量下载工具，由 GD 音乐台接口驱动。它聚焦于“搜索、选择、下载、查看结果”这一条简单流程，不包含播放器、账号系统或本地音乐库。

## 下载

当前版本：`v0.1.4`

[前往 GitHub Releases 下载](https://github.com/xiaobing6/yinmi-desktop/releases/latest)

| 平台 | 安装包 | 支持范围 |
| --- | --- | --- |
| Windows | `音觅_0.1.4_x64-setup.exe` | Windows 10 22H2、Windows 11，x64 |
| macOS | `音觅_0.1.4_universal.dmg` | macOS 13.3 或更高版本，Intel 与 Apple 芯片 |

Windows 安装包目前没有 Authenticode 商业代码签名，可能出现 SmartScreen 提示。macOS 安装包目前没有 Developer ID 公证，首次打开时可能需要在“系统设置 → 隐私与安全性”中允许运行。请只从本仓库的 GitHub Releases 页面下载安装包。

## 主要功能

- 支持网易云音乐、QQ 音乐、酷我音乐、TIDAL、Qobuz、JOOX 和哔哩哔哩等固定音源。
- 支持单曲、专辑和歌单三种匹配模式，默认使用网易云音乐。
- 单次搜索数量可设置为 1–1000，支持批量选择和顺序下载。
- 支持 128、192、320、740 无损和 999 Hi-Res 等音质选项。
- 支持写入歌曲元数据和封面，并可保存同名 `.lrc` 歌词文件。
- 下载时不会覆盖已有文件，支持实时去重、安全临时文件和原子提交。
- 支持下载进度、取消、失败重试、运行日志和快速打开下载目录。
- 支持带签名验证的应用内更新。

实际可用音源、音质和歌曲资源取决于上游接口当时的返回结果。

## 使用方法

1. 选择音源、搜索模式和搜索数量，输入关键词后开始搜索。
2. 在结果中勾选需要的歌曲，也可以全选当前结果。
3. 设置音质、封面、歌词和基础下载目录，将所选歌曲加入下载。
4. 在底部下载栏查看进度、取消任务、重试失败项或打开下载目录。
5. 需要排查问题时，打开右上角的“运行日志”。

当前版本不会持久化搜索和下载配置，重新启动后会恢复默认设置。

## 本地开发

本项目同时包含 Svelte 前端和 Rust/Tauri 桌面端。仅运行 `pnpm dev` 只会启动前端页面；开发完整桌面应用前，请先安装以下环境。

### 1. 安装系统依赖

Windows：

1. 安装 [Microsoft C++ 生成工具](https://visualstudio.microsoft.com/visual-cpp-build-tools/)，并在安装器中勾选“使用 C++ 的桌面开发”。
2. Windows 10 1803 及更高版本通常已安装 WebView2；如未安装，请安装 [WebView2 Evergreen Bootstrapper](https://developer.microsoft.com/microsoft-edge/webview2/#download-section)。
3. 在 PowerShell 中安装 Rust（选择默认的 MSVC 工具链），完成后重新打开终端：

```powershell
winget install --id Rustlang.Rustup
rustup default stable-msvc
```

macOS：

1. 安装 Xcode Command Line Tools：

```bash
xcode-select --install
```

2. 安装 Rust，完成后重新打开终端：

```bash
curl --proto '=https' --tlsv1.2 https://sh.rustup.rs -sSf | sh
```

更完整的平台说明请参考 [Tauri 2 环境准备文档](https://v2.tauri.app/start/prerequisites/)。本项目要求 Rust 1.97，仓库根目录的 `rust-toolchain.toml` 会让 rustup 自动使用该版本。

### 2. 安装 Node.js 和 pnpm

安装 [Node.js 24](https://nodejs.org/en/download/)，然后启用 Corepack。项目已在 `package.json` 中固定 pnpm 11.7.0，无需全局安装 Tauri CLI。

```powershell
corepack enable
corepack install
node --version
pnpm --version
rustc --version
```

### 3. 安装项目依赖并启动

```powershell
pnpm install --frozen-lockfile
pnpm tauri dev
```

`pnpm tauri dev` 会同时启动 Vite 前端开发服务器、编译 Rust 后端并打开桌面应用。首次编译 Rust 依赖可能需要较长时间。

### 常用命令

```powershell
# 仅启动浏览器中的前端页面
pnpm dev

# 类型检查、前端构建和 Rust 检查
pnpm run quality

# Rust 测试与 Clippy
pnpm run test:rust
pnpm run lint:rust
```

### 平台构建

以下命令与 GitHub Release 工作流使用的目标和安装包格式一致。

Windows x64 构建 NSIS 安装包：

```powershell
pnpm tauri build --target x86_64-pc-windows-msvc --bundles nsis --config tauri.release.json
```

macOS 构建同时支持 Apple 芯片与 Intel 的通用应用和 DMG。首次构建前先安装两个 Rust 目标：

```bash
rustup target add aarch64-apple-darwin x86_64-apple-darwin
pnpm tauri build --target universal-apple-darwin --bundles app,dmg --config tauri.release.json
```

`tauri.release.json` 会启用应用内更新产物。完整复现正式发布前，必须在当前终端通过 `TAURI_SIGNING_PRIVATE_KEY` 提供私钥内容，或通过 `TAURI_SIGNING_PRIVATE_KEY_PATH` 提供本地私钥路径，并设置 `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`。GitHub Actions 会从仓库 Secrets 注入私钥内容和密码。请勿将私钥提交到仓库。

如果只想验证 Release 可执行文件能否编译，不生成安装包和更新签名，可以使用与 GitHub Quality 工作流一致的命令：

```powershell
# Windows x64
pnpm tauri build --no-bundle --target x86_64-pc-windows-msvc --config tauri.release.json
```

```bash
# macOS universal
pnpm tauri build --no-bundle --target universal-apple-darwin --config tauri.release.json
```

技术栈：Tauri 2、Rust、Svelte 5、TypeScript、Vite 和原生 CSS。

## 使用说明

请遵守所在地法律法规、内容版权要求和相关平台服务条款，只下载你有权使用的内容。本项目不隶属于任何音乐平台。
