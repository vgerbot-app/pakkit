# WingetGUI

Windows 上的 winget 图形客户端。Rust + [gpui-kit](https://crates.io/crates/gpui-kit) 0.8，原生窗口，无 Electron/WebView。

所有操作都是对 `winget.exe` 的一层封装：界面显示的命令行就是实际执行的命令行，不隐藏、不改写。

