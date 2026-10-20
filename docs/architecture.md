# 架构

## 模块依赖

底层完全无环。UI 层有两处刻意保留的互相引用（Rust 允许模块互引，只要不是
类型递归）：

```
winget/{mod,exec,parse,exit}    ← 无内部依赖，最底层
model ← icons, winget
theme  ← model
state  ← model, queue, theme, winget
queue  ← model, state, winget
       （state.queue: Vec<QueueItem> 与 queue 的主要输入，二者互相引用）

widgets ← icons, model, queue, state, theme, winget
views   ← dialogs, icons, model, queue, shell, state, theme, widgets
dialogs ← icons, model, queue, shell, theme, widgets, winget
shell   ← icons, model, queue, state, theme, views, widgets, winget
```

互相引用的两处：

- `views ↔ shell`：`views` 需要 `&mut AppView` 作事件回调；
  `shell` 需要 `views::render` 与 `ShellInputs`。
- `state ↔ queue`：`AppState` 持有 `Vec<QueueItem>`，而 `queue` 的函数收 `&mut AppState`。

`dialogs` 单向依赖 `shell`（不收 `&AppState`，而是收 `&mut AppView`），不构成环。

判据：`winget/` 不依赖任何 UI 类型，只返回 `Vec<String>` 参数或解析后的结构体；
`model` / `state` / `queue` 不出现 `Entity`、`Window` 等窗口类型，因此都能
脱离窗口做单元测试。94 个测试的分布：`model` 10、`state` 20、`queue` 22、
`winget/*` 29、`theme` 8、`shell` 3、`icons` 2。`views` / `dialogs` / `widgets`
没有测试，靠实机验证。

**状态变更只有两条入口**：`AppView` 自己的方法，和 `dialogs` 里通过
`cx.listener` 拿到的 `&mut AppView`。`views` 与 `widgets` 只拿到 `&AppState`
或纯数据，无法直接改。

## 状态所有权

```
AppView (shell.rs)
├── state: AppState           ← 所有可渲染数据
├── winget: Option<Winget>    ← 已解析的 winget 可执行文件
├── search_input / install_location_input / proxy_input / winget_path_input
│                             ← Entity<InputState>，构造时创建一次
├── refresh_tasks: Vec<Task>  ← 在途刷新（见「任务取消」）
├── ticker: Option<Task>      ← 队列进度计时器
├── debounce: Option<Task>    ← 搜索防抖
└── _subscriptions: Vec<Subscription>
```

`AppState` 内部：`route`、`query`、`packages`、`sources`、`settings`、`queue`、
`status`、`winget_error` 等。全部 `pub`，视图直接读。

**视图永远不持有可变状态**。`views::render(state: &AppState, inputs: &ShellInputs, cx)`
返回 `AnyElement`；要改数据，视图调用 `AppView` 的 `pub fn`（`cx.listener` 拿到 `&mut AppView`）。

## 一次数据加载的完整路径

以「输入搜索词」为例：

```
InputEvent::Change                       (shell.rs, subscribe_in)
  → state.query = text
  → 若当前不在 Search 路由：navigate(Route::Search)
  → debounce_search(cx)                  // 180ms 后触发
      → refresh_packages(cx)
          → cx.background_spawn {        // 后台线程，绝不阻塞 UI
                winget search --source …
                winget list
                winget export -o %TEMP%\wingetgui-export.json
            }
          → merge_catalog(search, list, installed)
          → state.packages = …
          → cx.notify()                  // 触发重绘
```

`merge_catalog`（shell.rs）是关键合并点，原因见 `docs/winget-integration.md`。

## 任务取消语义（最易踩的坑）

gpui 的 `Task` **在 drop 时取消**。因此：

- 不能用单个 `Option<Task>` 槽存刷新任务——第二次刷新会 drop 第一个，
  导致启动时的 `refresh_sources` 取消在途的 `refresh_packages`。
- 现在用 `refresh_tasks: Vec<Task<()>>`，`spawn_refresh` 先 `retain(|t| !t.is_ready())`
  剪掉已完成的，再 push。
- `debounce` / `ticker` 则**故意**用单槽：新的一次应当取消旧的。

## 队列状态机（queue.rs）

```
Pending ──begin()──> Running ──complete(Ok)──> Done
   │                    └─────complete(Err)──> Error
   └──remove()──> 消失
```

- `enqueue()` 拒绝重复：同 id 已在队列且 `is_active()` 时返回 `Enqueued::Duplicate`。
- `startable()` 按 `settings.effective_concurrency()`（钳制在 1..=3）算出还能启动几个。
- `pump()`（shell.rs）循环启动可启动项，每项 `cx.spawn(...).detach()`；
  完成后回调 `pump` 启动下一个，并 `start_ticker` 驱动进度条。
- 队列项持有 `uid: u64`（单调递增，永不复用），因为 `Vec` 索引会因删除而失效。

## 渲染路径的硬约束

**渲染中禁止 `cx.entity().read(cx)`**。`AppView` 正在被更新，重入会 panic
（`cannot read … while it is already being updated`）。

设置页需要三个 `InputState`，因此通过 `ShellInputs` 结构体从 `AppView`
克隆 `Entity` 句柄后传入 `views::render`。新增需要 `Entity` 的视图时照此办理。

同理，`widgets::btn_ghost` 曾对 `Button` 调用 `Div::hover`，与 `Button::render`
内部注册的 hover 冲突（`hover style already set` 断言），现改用
`ButtonCustomVariant`。**自定义 Button 外观时走 variant，不要叠加 `.hover()`。**

## 主题

两层：

1. `Palette`（`theme.rs`）——原型 `app.css` 的 `:root` / `.dark` 两列色值，`const fn` 转换。
   组件绝大多数直接用 `Palette` 字段。
2. `themes/wingetgui.json`——gpui-kit 内置组件（滚动条、下拉、tooltip 等）用的语义 token。
   `include_str!` 编译进二进制，经 `Theme::apply_config` 装载后再由 `Palette` 覆盖
   `apply()` 显式命名的那些 token。

`init()` 的顺序是有意的：`Theme::change` 先装好全局，`Theme::update` 才能工作。

## model ↔ icons 的循环

`CategoryId::icon()` 返回 `IconKey`，而 `IconKey::for_category()` 调用它。
两者都是 `const fn`，编译期求值，不产生运行时环。若要把这两个改成非 const，
需要拆开（例如让 `icons` 持有 id→glyph 的映射表）。