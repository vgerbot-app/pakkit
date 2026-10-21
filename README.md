# WingetGUI

Windows 上的 winget 图形客户端。Rust + [gpui-kit](https://crates.io/crates/gpui-kit) 0.8，原生窗口，无 Electron/WebView。

所有操作都是对 `winget.exe` 的一层封装：界面显示的命令行就是实际执行的命令行，不隐藏、不改写。

界面支持简体中文、繁体中文、English、日本語、한국어，在「设置 → 外观 → 语言」中切换，立即生效并持久化；首次启动按操作系统语言自动选择。

![预览](./assets/preview.png)

## 下载

从 [Releases](../../releases) 页面下载对应版本的压缩包（`wingetgui-*-windows-x86_64.zip`，ARM 设备选 `arm64`），解压后运行 `wingetgui.exe` 即可，无需安装。每个 zip 附带 `.sha256` 校验文件。

## 发布流程（维护者）

1. 更新 `Cargo.toml` 的 `version`，并在 `CHANGELOG.md` 增加对应的 `## [x.y.z]` 小节；
2. 提交后打标签并推送：`git tag vx.y.z && git push origin vx.y.z`；
3. GitHub Actions 自动构建 x86_64 与 ARM64 两个架构，校验 tag 与 `Cargo.toml` 版本一致后创建 Release，发布说明取自 CHANGELOG 对应小节。
