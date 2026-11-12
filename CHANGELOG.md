# 变更记录

格式参考 [Keep a Changelog](https://keepachangelog.com/zh-CN/1.1.0/)，
版本号遵循 [语义化版本](https://semver.org/lang/zh-CN/)。

## [未发布]

### 变更
- 统一真实桌面界面的四档字体、0.5px 必要边线与克制的表面层级；重排首页、搜索、官网直装和设置的信息结构，移除装饰光斑、重复说明及中点分隔。
- 图标按钮补齐可访问名称；状态栏显示真实系统架构；官网目录加载改用骨架卡片；五语言文案保持一致。
- 队列按等待、运行、完成、失败状态显示真实进度；减少动画开关接入框架平台偏好并控制运行态动画。
- 修复首页统计卡底色、首页操作按钮高度、最近更新列表名称垂直对齐，以及官网搜索框、筛选下拉框、刷新目录和提交软件按钮的高度对齐。

- 官网直装目录改为 `catalog/index.json` 与按分类拆分的 JSON 文件。客户端下载完整索引和全部分类后统一校验、一次性更新界面；任何文件缺失或非法都会拒绝整份目录，不回退到旧的单文件 `catalog.json`。`PAKKIT_OFFICIAL_CATALOG_URL` 和 `PAKKIT_OFFICIAL_CATALOG_PATH` 现在指向索引文件。

- 「官网直装」改为「开源软件」。目录升级为 `schemaVersion: 2`，每款软件必须提供源码仓库、当前 OSI 批准的 SPDX 许可证表达式及许可证原文链接；详情与提交弹窗展示并收集完整开源证明。缺失元数据、非法或弃用 SPDX、`LicenseRef` 与非 OSI 许可证均拒绝整份目录，schema 1 不再兼容。
- 远程目录仅保留经许可证原文核验的开源软件；`official-install` 分支推送与拉取请求也会使用客户端解析器执行完整目录校验。

### 新增

- 归档安装支持通过 `additionalExecutables` 显式声明必需的辅助可执行文件；只给声明的普通文件入口添加执行权限，复用已有版本时也校验并修复这些入口。

- 新增 dnf 后端（Fedora/RHEL，兼容 dnf5）：只读查询直跑 `dnf`，变更
  操作优先走 PackageKit D-Bus 通道、不可用时回退 `pkexec dnf …`；
  `check-update` 的退出码 100（有待更新包）按良性处理；源管理不开放
  （`/etc/yum.repos.d` 为 root 所有）。
- 新增 Zypper 后端（openSUSE）：所有调用携带 `--non-interactive`；变更
  操作走 PackageKit（zypp 后端）并回退 `pkexec zypper …`；zypper 的
  信息类退出码 102/103（需重启系统/会话）按成功处理。
- 新增 apk 后端（Alpine）：只读查询直跑 `apk`，变更经 `pkexec apk …`
  （Alpine 通常无 PackageKit/polkit，此时 spawn 失败会显示在队列错误
  详情中）；源管理不开放（`/etc/apk/repositories` 为 root 所有）。
- 新增 Homebrew 后端（macOS 与 Linux/linuxbrew，首个 macOS 平台后端）：
  brew 拒绝 root，全程不提权；cask（图形应用）以 id 前缀 `cask/` 与
  formula 区分并贯穿搜索/安装/升级全流程；源管理对应 taps
  （`brew tap`/`untap`）；可执行文件除 `PATH` 外还探测
  `/opt/homebrew`、`/usr/local` 与 linuxbrew 固定前缀。
- 新增 Scoop 后端（Windows，设置中可与 winget/Chocolatey 切换）：scoop
  免管理员全程不提权；`全部更新` 合并为一次 `scoop update *`；源管理
  对应 buckets（`scoop bucket add/rm`）；可执行文件解析 `scoop.exe`
  shim（`PATH` 或 `%USERPROFILE%\scoop\shims`）。
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