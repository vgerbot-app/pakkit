# WingetGUI

Windows 上的 winget 图形客户端。Rust + [gpui-kit](https://crates.io/crates/gpui-kit) 0.8，原生窗口，无 Electron/WebView。

所有操作都是对 `winget.exe` 的一层封装：界面显示的命令行就是实际执行的命令行，不隐藏、不改写。

界面支持简体中文、繁体中文、English、日本語、한국어，在「设置 → 外观 → 语言」中切换，立即生效并持久化；首次启动按操作系统语言自动选择。

![预览](./assets/preview.png)
