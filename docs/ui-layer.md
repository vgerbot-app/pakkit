# UI 层

## 四个文件的分工

| 文件 | 职责 | 是否持有状态 |
|---|---|---|
| `shell.rs` | `AppView`：窗口、标题栏、侧栏、状态栏、队列抽屉、所有事件处理 | **是**，唯一持有 |
| `views.rs` | 七个路由的内容区，纯渲染 | 否 |
| `dialogs.rs` | 模态对话框 | 否（收 `&mut AppView` 以便按钮回调） |
| `widgets.rs` | 无状态的组件构造器 | 否 |

`views::render` 的签名即契约：

```rust
pub fn render(state: &AppState, inputs: &ShellInputs, cx: &mut Context<AppView>) -> AnyElement
```

只读 `&AppState`。要改数据，只能用 `cx.listener(|this: &mut AppView, …| …)`。

## 与原型的关系

`prototype/` 是**权威视觉契约**，别改它，改代码去对齐它。

| 原型 | Rust |
|---|---|
| `assets/css/app.css` 的 `:root` / `.dark` | `theme.rs` 的 `Palette::light()` / `dark()` |
| `assets/js/data.js` 的 `ICONS` | `icons.rs` 的 `IconKey` |
| `assets/js/data.js` 的 `CATEGORIES` / `PACKAGES` | `model.rs` |
| `assets/js/views.js` | `views.rs` |
| `assets/js/app.js` 的路由与队列 | `state.rs` / `queue.rs` / `shell.rs` |
| `index.html` 的窗口框 | `shell.rs` 的几何常量 |

外壳几何（与 `index.html` 一致）：

- 标题栏 52 px
- 侧栏 224 px，视口窄于 1280 px 时折叠为 64 px（只留图标）
- 状态栏 28 px
- 队列抽屉 360 px，从右侧覆盖

窗口默认 1280×820，最小 720×520。

## 新增/修改一个路由

按顺序改四处，缺一处就会出问题：

1. `state.rs` — `Route` 加变体；若需要，`sidebar_key()` 加映射
   （`Route::Category(_)` 归到 `"categories"` 就是这么做的）。
2. `views.rs` — `render()` 的 `match` 加分支，写渲染函数。
3. `shell.rs::sidebar()` — `routes` 数组加一项
   `(key, 标签, IconKey, Route)`。**key 必须与 `sidebar_key()` 返回的值一致**，
   否则高亮不会生效。
4. `shell.rs::navigate()` — 若进入该路由需要拉数据，加一个 `match` 分支
   （参考 `Route::Updates => self.refresh_updates(cx)`）。

## 组件构建器（widgets.rs）

一切颜色来自 `Palette`，一切几何是 `px(...)` 常量。常用：

```rust
widgets::card(p)                                  // 卡片容器（返回 Div，自己加 child）
widgets::btn(cx, p, id, BtnKind::Primary, BtnSize::Sm, "标签")  // 标准按钮
widgets::pill(p, PillKind::Accent, "文本")         // 胶囊标签
widgets::seg_control(p, id, &items, selected, on_select)        // 分段控件
widgets::page_head(p, "标题", Some("副标题".into()), actions)    // 页面头
widgets::field(p, &input_entity)                  // 输入框
widgets::empty_state(p, IconKey::Search, "标题", "提示", actions)
```

注意 `btn` 的第一个参数是 `cx: &App`（因为 `ButtonCustomVariant::new` 需要它）。

### 按钮的两个陷阱

1. **不要对 `Button` 调 `.hover()`**。`Button::render` 内部已在同一个 base div 上
   注册了 hover，再设一次会触发 `hover style already set` 调试断言并崩溃。
   自定义外观（例如幽灵按钮 `btn_ghost`）请用
   `ButtonCustomVariant::new(cx).color(..).foreground(..).hover(..).active(..)`。

2. `ButtonCustomVariant::new` 需要 `&App`，所以构造器签名要带 `cx`。
   `Context<AppView>` 可以 `Deref` 成 `&App`，直接传 `cx` 即可。

## 设置页的输入框为什么特殊

设置页要渲染三个 `InputState`（安装位置、代理、winget 路径）。它们必须在
`AppView::new` 时创建一次，然后以 `Entity` 句柄传入渲染。

**不能**在渲染函数里 `cx.entity().read(cx)` 去取——`AppView` 正在被更新，
重入会 panic：`cannot read … while it is already being updated`。

所以有一个 `ShellInputs`：

```rust
pub struct ShellInputs {
    pub install_location: Entity<InputState>,
    pub proxy: Entity<InputState>,
    pub winget_path: Entity<InputState>,
}
```

由 `AppView::shell_inputs()` 克隆出来（克隆 `Entity` 是廉价的引用计数），
经 `views::render(state, &inputs, cx)` 传到 `settings()`。

**新增任何需要 `Entity` 的视图，照此办理。**

## 事件与异步

- 搜索框的 `InputEvent::Change` → 更新 `state.query` → **若不在 Search 路由则先导航**
  → `debounce_search`（180 ms 防抖）。
  导航那一步曾经漏掉，导致别的路由下输入只过滤陈旧目录。
- `InputEvent::PressEnter` → 取消防抖，立即 `refresh_packages`。
- 所有 winget 调用都在 `cx.background_spawn` 里跑，回到 UI 线程再 `cx.notify()`。
- 只在 `AppView` 上注册了一个快捷键：`ctrl-k` → `FocusSearch`（context `"App"`）。

## 通知

```rust
window.push_notification(Notification::info("已加入队列：…"), cx);
```

`info` / `warning` / `error` 三档。对话框需要在 `cx.spawn_in(window, …)` 的
回调里用 `this.update_in(cx, |this, window, cx| …)` 才能拿到 `window`。

## 空状态

`winget` 找不到时应用仍会启动，`state.winget_error` 有值，各数据视图显示空状态。
这是**有意的降级**，别改成 panic。

## 验证方式

视图层没有自动化测试。改动 UI 后：

```bash
cargo build && ./target/debug/pakkit.exe
```

然后在真实窗口里点一遍受影响的路径。历史上有多次「编译通过、运行即崩」的
问题（hover 断言、渲染期重入、任务取消），只有实机能看到。