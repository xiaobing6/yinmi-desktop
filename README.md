# 音觅

音觅是一款适用于 Windows 和 macOS 的桌面音乐搜索与批量下载工具，由 GD 音乐台接口驱动。它聚焦于“搜索、选择、下载、查看结果”这一条简单流程，不包含播放器、账号系统或本地音乐库。

## 下载

当前版本：`v0.1.2`

[前往 GitHub Releases 下载](https://github.com/xiaobing6/yinmi-desktop/releases/latest)

| 平台 | 安装包 | 支持范围 |
| --- | --- | --- |
| Windows | `音觅_0.1.2_x64-setup.exe` | Windows 10 22H2、Windows 11，x64 |
| macOS | `音觅_0.1.2_universal.dmg` | macOS 13.3 或更高版本，Intel 与 Apple 芯片 |

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

环境要求：

- Node.js 24
- pnpm 11.7.0
- Rust 1.97
- Windows 或 macOS 对应的 Tauri 2 系统依赖

```powershell
pnpm install --frozen-lockfile
pnpm tauri dev
```

常用命令：

```powershell
pnpm run quality
pnpm tauri build
```

技术栈：Tauri 2、Rust、Svelte 5、TypeScript、Vite 和原生 CSS。

## 使用说明

请遵守所在地法律法规、内容版权要求和相关平台服务条款，只下载你有权使用的内容。本项目不隶属于任何音乐平台。
