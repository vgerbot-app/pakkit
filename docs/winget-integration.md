# winget 集成

`src/winget/` 的四个文件分工：`mod.rs` 造参数，`exec.rs` 起进程，`parse.rs` 解输出，
`exit.rs` 译退出码。参数构造与解析都是纯函数，可以不启动窗口直接断言；
但测试并非全模块覆盖——`upgrade_all` / `import_json` / `source_reset` 三个构造器
至今没有测试（见下文）。改这里请先跑 `cargo test`。

## 一、标志位必须按命令派生

winget 对每个子命令单独校验选项。把选项放在子命令之前，它会拒绝整条调用
（v1.29.380 实测）：

```
winget --accept-source-agreements list
  → stdout: Argument name was not recognized for the current command:
            '--accept-source-agreements'
  → 后面跟着完整 --help 文本，退出码 2
```

两个坑叠加才让这个 bug 难以发现：

1. **错误写在 stdout**，stderr 是空的。只看 stderr 会以为成功。
2. 退出码 2 不是 `is_benign` 认可的良性码，所以 `Output::success()` 返回 false。
   早期代码不检查它，把 stdout 直接喂给解析器，解析器看不到表头行，得到空表，
   界面一片空白。

现在两条防线都有：`run_checked` 把非良性退出码转成 `WingetError::Exit`
（`first_message` 从 stdout 里挑出那条 `Argument name…`），标志位则按子命令派生：

```rust
const COMMON_FLAGS: &[&str] = &["--disable-interactivity", "--accept-source-agreements", "--nowarn"];
const SOURCE_FLAGS: &[&str] = &["--disable-interactivity", "--nowarn"];

fn base_flags_for(command: &str) -> &'static [&'static str] {
    match command {
        "source" => SOURCE_FLAGS,
        _ => COMMON_FLAGS,
    }
}
```

`run_with` 把标志插在**子命令之后**；`source` 还有子动词（`add`/`remove`/…），标志插在子动词之后。

### 不该出现的标志位

三个名字有不同性质，别混为一谈（v1.29.380 实测）：

- `--output-locale` —— **不是** winget 选项。传入后退出码 2，整条调用失败。
  所以解析器不能靠它稳定输出，只能按表头行推断列位置。
- `--no-progress`、`--no-vt` —— winget 接受（退出码 0）但**没有出现在任何
  `--help` 里**；在这台机器上输出与不加时逐字节相同。属于未文档化的兼容标志，
  依赖它就是依赖未承诺的行为，所以不开。
- `--accept-package-agreements` —— 真实选项，`install` 与 `upgrade` 都接受，
  `list` / `search` / `show` 拒绝。它必须绑定用户的同意开关，所以由
  `winget::install` 单独发出，不进 `COMMON_FLAGS`。

`COMMON_FLAGS` / `SOURCE_FLAGS` 由 `exec.rs` 的
`flags_never_contain_options_winget_rejects` 守住，`base_flags_for` 的分派
由 `source_takes_no_agreement_flags` 守住。这两个测试只能防止误加，
无法验证「加了还能用」——加新标志前务必跑一次真实命令。

`--version` 走 `run_bare`。实测：`--version` 后跟 `--disable-interactivity`
或 `--nowarn` 都仍只输出干净的一行 `v1.29.380`（退出码 0），但跟
`--accept-source-agreements` 会失败（退出码 2，输出横幅加
`Argument name was not recognized`）。因为 `COMMON_FLAGS` 含这个标志，
版本探测必须绕过它——用裸调用最省事，也最不容易随 `COMMON_FLAGS` 变化而失效。

## 二、命令映射表

| UI 动作 | 构造器 | 实际命令 |
|---|---|---|
| 搜索 | `search()` | `search -q <q> [--exact\|--moniker\|--tag\|--command] [--source <s>] [--count <1..=1000>]` |
| 全局查询/列表 | `list()` | `list [--source <s>]` |
| 更新检查 | `list_upgrade_available()` | `list --upgrade-available [--include-unknown] [--include-pinned]` |
| 详情 | `show()` | `show --id <id> --exact [--source <s>]` |
| 安装 | `install()` | `install --id <id> --exact [--source] --architecture <a> --scope <s> [--location <p>] [--silent] [--accept-package-agreements]` |
| 卸载 | `uninstall()` | `uninstall --id <id> --exact` |
| 单项升级 | `upgrade_one()` | `upgrade --id <id> --exact` |
| 全部升级 | `upgrade_all()` | `upgrade --all` |
| 导出 | `export_json()` | `export -o <path> [--source <s>]` |
| 导入 | `import_json()` | `import -i <path>` |
| 源列表 | `source_list()` | `source list` |
| 源详情 | `source_export()` | `source export [--name <n>]` |
| 加源 | `source_add()` | `source add --name <n> --arg <url> [--type <t>] --trust-level <trusted\|none>` |
| 改源 | `source_edit()` | `source edit --name <n> [--explicit <true\|false>]` |
| 删源 | `source_remove()` | `source remove --name <n>` |
| 重置源 | `source_reset()` | `source reset [--force]` |
| 更新源 | `source_update()` | `source update [--name <n>]` |

`source add --type` 的取值由 winget 决定：`Microsoft.PreIndexed.Package`（默认）、
`Microsoft.Rest`、`Microsoft.PreIndexed.Manifest`。
加源对话框把类型固定为 `Microsoft.PreIndexed.Package`、信任级别固定为 `trusted`，
只让用户填名称和 URL；`source_add()` 本身支持任意 `--type` 与
`--trust-level none`，接更复杂的表单时直接用。

**`winget source edit` 只接受 `--name` 和 `--explicit`**——不能改 URL，不能改类型。
所以「编辑源」对话框把 URL/类型显示为只读，只开放「从发现中排除」。

### 参数语义陷阱

- `search()` 的 `source = Some("all")` 表示「所有源」，此时**不发出** `--source`
  （`normalize_source` 返回 `None`）。传 `winget`/`msstore` 才真正加参数。
- `list_upgrade_available` 的 `--include-unknown` / `--include-pinned` **只能**跟着
  `--upgrade-available`，所以只在这里发。
- `uninstall` 不发 `--purge`，也不发 `--all-versions`：原型语义是「卸载已装版本」。
- `install` 的 `--location` 空串时跳过（`Some("")` 也算空）。


### 已实现但界面尚未接线的构造器

以下三个函数已经写好，但**目前没有任何调用方，也没有单元测试**——它们不会随
其他改动一起被发现，改 `Winget` 的标志位逻辑时尤其容易漏掉：

- `upgrade_all()` — 界面上的「全部升级」按钮实际是逐项 `enqueue(id, Upgrade)`，
  走 `upgrade_one`，而不是一条 `upgrade --all`。这样做是为了让每项都能单独显示
  进度与失败原因。
- `import_json()` — 「导入清单」尚无入口；目前只有「导出清单」。
- `source_reset()` — 「重置源」尚无入口。

若接线上这些功能，记得同时：接 UI → 补单元测试断言完整参数向量 →
确认标志位归属正确（`source reset` 走 `SOURCE_FLAGS`）→ 实机验证。

## 三、输出解析

winget 没有机器可读的表格模式，只能解析人类可读输出。

### 关键：按显示宽度切列，不是字节/字符

表格按**显示列宽**对齐。包名含 CJK 时，一个汉字占两列但只算一个 `char`，
按字符数切列会错位。`parse.rs` 因此统一走：从表头行推断每列的起止显示列，
再按显示宽度切数据行。

**表头可能缺少某一列**。例如 `winget search -q vscode --source winget` 的输出
没有 `Source` 列。解析器按表头实际存在的列名定位，缺列时该字段取默认值。

**不要**加 `--output-locale` 之类去稳定输出——那不是 winget 的选项。

### 有 JSON 的两处

- `winget export -o <file>` 输出 JSON，用 `serde_json` 解析，
  抽 `Sources[].Packages[].PackageIdentifier`。
- `winget source export` 输出 **JSON Lines**（每行一个源对象），逐行解析。

### 解析函数

`parse_search` / `parse_list` / `parse_show` / `parse_source_list` /
`parse_export_json` / `parse_source_export`。样本在 `tests/fixtures/`，以 `include_str!`
编译进测试（因此夹具必然参与构建，改动会立刻暴露）。

## 四、退出码

`exit.rs` 把 winget 的 `0x8A15xxxx` 译成中文，供通知与队列日志显示。

良性码（`is_benign`）——描述「目标状态已达成」，按成功处理：

| 码 | 含义 |
|---|---|
| `0x8A150061` | 该软件包已安装 |
| `0x8A15010D` | 已安装其他版本 |
| `0x8A15002B` | 没有可用的更新 |
| `0x8A150109` | 需要重启以完成操作 |

`0x8A150019` 单独由 `needs_elevation` 标记（需要管理员权限）。

**注意**：`Output::success()` 判的是 `status == 0 || is_benign(status)`。
加新命令时若某个非零码其实无害，应加进 `is_benign` 而不是在调用点特判。

## 五、进程启动

- `Winget::discover()`：先扫 `PATH`，再试 `%LOCALAPPDATA%\Microsoft\WindowsApps\winget.exe`。
- `Winget::resolve(configured)`：配置了非空路径就用它（必须是文件），否则自动发现。
  设置页的「检测」按钮走这条。
- 每个子进程都设 `CREATE_NO_WINDOW`（`0x0800_0000`），否则会闪黑框。
  该 API 在 Windows 上是安全的 `CommandExt::creation_flags`，不需要 `unsafe`
  （crate 禁止 `unsafe_code`）。
- stdin 设为 null；stdout/stderr 都捕获（winget 会把通知分流到两处，
  `Output::combined()` 按 stdout→stderr 拼接）。

## 六、冷启动为什么需要三份数据

`refresh_packages` 一次跑三条命令：

1. `winget search`（按当前查询词）
2. `winget list`
3. `winget export -o %TEMP%\pakkit-export.json`

原因是 `merge_catalog` 的合并规则：

- **`winget search` 没有查询词时不是「返回空」，而是直接报错退出**（退出码 2，
  `No package selection argument was provided`）。代码里 `.unwrap_or_default()`
  把它吞成空 `Vec`，所以冷启动时搜索结果是空，已安装/更新视图会全空——
  必须靠 `winget list` 补上已安装项。
- **`winget list` 还会报告 ARP/MSIX 条目**（注册表卸载项、Store 包），
  这些东西 winget 无法升级。用 `winget export` 的集合作为「winget 真的管得动」
  的白名单，来过滤 `list` 的结果。
- 先遍历 search，再遍历 list，已存在的 id 跳过——避免重复卡片。

三条规则各有回归测试（`src/shell.rs` 的 `mod tests`）：

- `installed_packages_survive_an_empty_search`
- `unmanaged_list_rows_are_not_offered_as_installed_packages`
- `a_search_row_is_not_duplicated_by_the_installed_pass`

本机实测（i7-5500U，winget v1.29.380）：`winget list` 约 16 s，
`winget search -q …` 约 1 s，`winget export` 约 5 s。因此一次完整刷新约 22 s，
其中 `list` 占大头。这是 `refresh_packages` 用后台执行器的直接原因——
放在 UI 线程会冻结窗口二十多秒。

## 七、改这里的检查清单

1. 参数构造改动 → 补 `winget/mod.rs` 的单元测试（断言完整参数向量）。
2. 解析改动 → 更新 `tests/fixtures/` 里的真实输出样本，并补边界（缺列、CJK、空表）。
3. 新命令 → 确认标志位归属（`COMMON_FLAGS` 还是 `SOURCE_FLAGS`），
   跑一次真实 `winget <cmd> --help` 核对选项名。
4. 任何非零退出码的处理 → 优先考虑加进 `is_benign`，而非调用点特判。
5. 提交前三条门禁全绿：

   ```bash
   cargo fmt --all -- --check
   cargo clippy --workspace --all-targets --all-features -- -D warnings
   cargo test
   ```

   当前基线 94 个单元测试、clippy 零告警。