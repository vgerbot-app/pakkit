# 变更记录

格式参考 [Keep a Changelog](https://keepachangelog.com/zh-CN/1.1.0/)，
版本号遵循 [语义化版本](https://semver.org/lang/zh-CN/)。

## [未发布]

### 新增

- 发布流水线扩展为多平台：Windows / Linux × x86_64 / arm64 四组合原生构建
  （Linux arm64 使用 GitHub 原生 ARM runner），产物新增 Linux `.deb` /
  `.rpm` / `.AppImage` 与 Windows `.msi` 安装包（MSI 仅 x86_64——
  cargo-wix 捆绑的 WiX 3.x 无法生成 arm64 安装包），并统一附带
  `pakkit-<版本>-checksums.txt` 校验和文件；release.yml 支持
  `workflow_dispatch` 手动 dry-run（跳过发布）。
- CI 新增 cargo-deny 检查：依赖许可证白名单、安全公告与 yanked 包审计。
- 新增 pacman 后端（检测到 yay 时附带 AUR 支持）：只读查询直跑
  `pacman`，变更操作与 apt 一样优先走 PackageKit D-Bus 通道、不可用时
  回退 `pkexec pacman …`；搜索在有 yay 时追加 AUR 结果（id 前缀
  `aur/`），AUR 安装/升级经 `--from-aur` 标记改道直跑
  `yay -S --noconfirm`——yay 须以普通用户身份构建并自行调 sudo，不能
  套 pkexec；本应用以空 stdin 运行子进程，交互式 sudo 会失败，故 AUR
  安装需要免密 sudo，失败原因显示在队列的错误详情里。已安装列表用
  `pacman -Qmq` 标记 AUR 包，卸载统一走 `pacman -R`。
- 新增 snap 后端：snapd 自行弹 polkit 授权（同 flatpak，无需 pkexec）；
  Snap Store 是唯一来源，不开放源管理；`全部更新` 合并为一次
  `snap refresh` 事务。
- 新增 Chocolatey 后端（Windows，设置中可与 winget 切换）：面向 choco
  v2 的 `--limit-output` 管道输出（`name|version`）解析；`全部更新`
  合并为一次 `choco upgrade all` 事务，被固定（pinned）的包不列入
  更新；源管理支持添加/删除（`choco source …`），禁用的源显示为
  未验证；可执行文件除 `PATH` 外还回退到 `%ChocolateyInstall%\bin`。
- apt 后端的安装/升级/卸载/刷新改走 PackageKit D-Bus 通道：
  其 polkit 授权（`auth_admin_keep`）缓存约 5 分钟，一批队列任务最多弹
  一次密码框，替代此前每个任务一次 pkexec 弹窗；PackageKit 不可用时
  自动回退 pkexec。授权弹窗被取消/拒绝时以退出码 126/127 报告并给出
  本地化说明。
- 七个视图：发现、分类、全部包、已安装、更新、源管理、设置。
- 队列抽屉：安装/升级串行执行，可配置并发（1–3），逐项显示命令行与进度。
- 对话框：软件包详情、高级筛选、添加源、编辑源、卸载确认、删除源确认、导出清单。
- 软件包详情改用 `winget show --id --exact` 的数据：打开详情时后台抓取并缓存，
  展示多行描述、发布说明、标签、发布者/许可/主页/安装类型/发布日期等字段；
  解析器支持 `Description:`、`Release Notes:` 多行缩进段及 Installer 段的
  SHA256、离线分发等字段，并从 `Found 名称 [Id]` 行提取名称。
- 深色/浅色/跟随系统三档主题，5 个强调色。
- winget 退出码→ 中文说明映射，良性码按成功处理。
- 多语言界面：简体中文、繁体中文、English、日本語、한국어，
  词条编译期内置（rust-i18n），设置页切换立即生效；
  首次启动按操作系统语言选择，设置持久化到 `settings.json`。
- 设置持久化：主题、强调色、语言及全部设置项写入
  `配置目录/wingetgui/settings.json`，下次启动自动恢复。

## [0.1.0]

首个可运行版本。