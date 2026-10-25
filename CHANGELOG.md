# 变更记录

格式参考 [Keep a Changelog](https://keepachangelog.com/zh-CN/1.1.0/)，
版本号遵循 [语义化版本](https://semver.org/lang/zh-CN/)。

## [未发布]

### 新增

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

### 修复

- winget 参数标志位曾放在子命令之前，导致每次调用都报
  `Argument name was not recognized for the current command`（写在 **stdout**，
  stderr 为空，退出码 2 且输出的是帮助文本），界面始终为空。
  现在按命令派生标志位并插在子命令（`source` 为子动词）之后，并用
  `run_checked` 把非良性退出码转成错误而不是喂给解析器。
- 冷启动时「已安装」与「更新」为空：`winget search` 无查询词匹配不到任何东西。
  现在合并 `winget list`，并用 `winget export` 的集合排除 winget 无法升级的
  ARP/MSIX 条目。
- 单个刷新任务槽会取消在途任务，导致启动时的源加载取消软件包加载。
- 在非搜索路由输入时只导航不查询，过滤的是陈旧数据。
- winget 的 moniker/tag 命中（如 `Moniker: vscode`）被客户端子串过滤丢弃。
- 幽灵按钮对 `Button` 设置 hover 触发 `hover style already set` 断言崩溃。
- 设置页在自身渲染中读取实体，触发 `cannot read … while already being updated` 崩溃。
- `themes/wingetgui.json` 从未被加载，内置组件一直使用 gpui-kit 默认配色。

## [0.1.0]

首个可运行版本。