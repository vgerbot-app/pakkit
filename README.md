# Pakkit 开源软件目录

本分支维护开源软件分类目录 `catalog/`、拆分脚本 `scripts/split_catalog.py` 和本说明。每个软件必须具有规范源码仓库、当前有效且经 OSI 批准的 SPDX 许可证表达式，以及经维护者人工核对的许可证原文链接。Pakkit 启动时从本分支读取远程目录；目录中的脚本和安装包均来自软件官方来源，Pakkit 不托管安装包，也不因为脚本结束、安装器启动、文件下载或官网打开而宣称软件已安装。

本批目录按逐条审核表收录 10 个分类、36 款软件、108 个安装变体；数量是本次验收结果，不是长期固定契约。

## 文件布局

- `catalog/index.json`：包含 `schemaVersion`、`catalogVersion`、`homepage`、`submissionUrl` 和按显示顺序排列的 `categories`。
- `catalog/<category-id>.json`：仅包含 `apps` 数组，存放对应分类的软件条目；条目必须保持所要求的分类内顺序。
- `scripts/split_catalog.py`：从已验收的单文件目录生成完整索引和分类文件，逐文件原子替换，并校验合并结果与源目录一致。遇到未列入索引的旧 JSON 文件会拒绝运行，不会删除未知文件。

客户端先下载索引，再按索引顺序下载每个分类文件，组合为完整目录后执行同一套全目录校验。分类文件缺失、重复、JSON 非法、软件放在错误分类、跨分类出现重复 ID，或任一软件缺少、填错开源许可元数据，都会使整个目录不可用；不会显示部分分类或回退旧单文件。目录运行时不联网验证许可证原文，原文真实性由发布前人工审核保证。

完整索引与所有分类文件的总大小上限为 4 MiB。

## 顶层字段

- `schemaVersion`：目录结构版本，目前必须为整数 `2`。旧客户端不兼容此契约，新客户端也不会接受旧版 `schemaVersion: 1` 目录；不提供旧目录兼容或部分回退。
- `catalogVersion`：本次目录发布版本，用于界面和诊断。每批已验收条目发布时单独更新。
- `homepage`：目录维护页面的 HTTPS 地址。
- `submissionUrl`：预填软件提交内容所使用的 GitHub 新 Issue HTTPS 地址。
- `categories`：按数组顺序显示的数据驱动分类。

## 分类文件

`apps` 为软件条目数组，当前数量不是契约，不为凑数保留未验收条目。每个条目的 `category` 必须与文件名中的分类 ID 一致。

## 本地化与分类

分类和软件的本地化文本使用对象，`zh-CN`、`zh-TW`、`en`、`ja`、`ko` 五个键均必填，文本不能为空。应用不会用内置翻译补全缺失文案。

每个分类包含：

- `id`：全局唯一，只使用小写 ASCII 字母、数字和连字符，不再限制为固定分类列表。
- `name`、`description`：完整的五语言文本。
- `icon`：只可使用客户端白名单中的 `code`、`cpu`、`driver`、`file`、`flask`、`gamepad`、`media`、`network`、`office`、`package`、`shield`、`wrench`。未知图标会拒绝整份目录，不会静默回退。
- `accent`：以 `#` 开头的六位小写十六进制颜色。

新增分类、图标或目录契约前，必须先发布支持该契约的 Pakkit 客户端，再发布目录，避免旧客户端拒绝整份远程 JSON。

## 软件与变体

每个软件包含：

- `id`：全局唯一，只使用小写 ASCII 字母、数字和连字符。
- `name`、`publisher`：软件名称和规范开发者名称。
- `category`：已定义的分类 ID。
- `summary`：完整的五语言摘要；可包含常用别名，便于跨界面语言搜索。
- `homepage`：官方 HTTPS 首页、安装文档或规范仓库发布页。
- `mark`：卡片标识，最多两个字符。
- `accent`：以 `#` 开头的六位小写十六进制颜色。
- `sourceRepository`：所分发软件的规范源码仓库 HTTPS 地址，不能只填发布下载页。
- `license`：SPDX 许可证表达式。客户端严格解析表达式，并要求其中每个许可证 ID 当前有效且经 OSI 批准；拒绝弃用 ID、非 OSI 许可证和 `LicenseRef`。`AND` 与 `OR` 中的每个候选项都执行相同检查。
- `licenseUrl`：与上述许可证对应的许可证原文 HTTPS 地址。发布前必须人工核对其内容与所分发软件及 SPDX 表达式一致，不能把 GitHub Releases 的 `repository` 字段当作许可证证明。
- `variants`：非空安装方式数组。

每个变体必须有全局唯一的 `id`、`os`、`architectures`、`requiresAdmin`、`interactive`、五语言 `note`、HTTPS `sourceUrl` 和 `method`。`os` 为 `linux`、`windows` 或 `macos`；`architectures` 只能包含 `x86_64`、`aarch64`，且同一软件在同一 OS 下不得有重复架构覆盖。

不同 OS 或不同架构使用不同变体。不得从 x86_64 支持推断 ARM64 支持，也不得用一个未确认的资产 URL 同时代表两个架构。默认只收录 stable；test、nightly、canary 或滚动最新资产不能冒充稳定版。

摘要和说明应如实标出试用或订阅、账号要求、GUI／CLI／便携／服务形态，以及权限、驱动和系统修改风险。许可元数据只声明所分发软件，不把账号服务、单独模型或第三方内容声称为开源。共用安装器的多个产品不建立重复入口，区域品牌别名可在摘要中说明。

`detection` 可省略；省略表示安装状态未知，不表示未安装。命令检测格式为：

```json
{"type":"command","program":"example","args":["--version"]}
```

检测直接启动程序和参数，不经过 shell。退出码为 0 表示已安装，非 0 或无法启动表示未安装，信号终止表示检测错误。只使用快速、可靠、非交互的命令；GUI 软件没有可靠检测时省略该字段。

## 安装方法

`method` 使用 `type` 区分三种形式：

1. `script`：`shell` 为 `sh` 或 `powershell`，`command` 是未经 Pakkit 改写的完整官方命令。只采用官方安装文档明确给出的命令，并理解其提示和系统修改。需要用户回答提示时使用 `interactive: true`，在可见终端执行。包含 `sudo` 的命令必须同时设置 `interactive: true` 和 `requiresAdmin: true`。Pakkit 不静默提权。
2. `artifact`：包含 HTTPS `url`、普通文件名 `fileName`、`action`，以及可选 `sha256`、`makeExecutable`。`action` 为 `run_installer`、`download_package` 或 `download_portable`。EXE、PKG、DMG 和 MSI 可运行安装器；DEB、RPM 只下载；AppImage 便携版使用 `download_portable` 和 `makeExecutable: true`。
3. `official_page`：仅包含 HTTPS `url`，用于 Microsoft Store、Creative Cloud、可变 latest 下载、`releases/latest` 或不满足固定资产条件的条目。确认后只在系统浏览器打开页面，不报告下载或安装成功。

### 新增固定资产的发布门槛

- 新资产必须来自官方来源，使用稳定、带版本且可复核的 HTTPS URL；不使用会自动变化的 latest 地址代替版本固定。
- 复核时下载准确资产，独立计算并填写 64 位小写 SHA-256。维护者计算的哈希用于变更检测，不替代官方来源和签名核验。
- Windows 安装器和便携程序必须核验实际签名者与 PE 架构；MSI 还要核验安装包架构。无法完成这些检查时改用 `official_page` 或暂不发布。
- 文件名不得为空、包含路径分隔符或 `..`。下载后强制校验已填写的 SHA-256；未填写时界面必须明确提示目录未提供哈希。
- 便携版只下载，不自动运行、解压或宣称安装。未经专门实现和测试的 MSIX 使用官方页面或下载包语义，不冒充自动安装。

## 添加或更新软件

1. 核对规范厂商／项目身份、官方 HTTPS 首页、安装文档或规范仓库 Release，记录准确的名称、开发者、命令、下载地址和注意事项。排除第三方 Windows 构建、非官方重打包、网页／移动／服务器-only 项目、停止维护项目和不明镜像。
2. 核对规范源码仓库与许可证原文，准确填写 `sourceRepository`、`license` 和 `licenseUrl`。许可证表达式中的每个 ID 都必须当前有效且经 OSI 批准，不接受 `LicenseRef`、弃用 ID、非 OSI 许可证或专有许可；无法确认所分发软件的完整许可范围时不收录。
3. 选择现有分类，新增全局唯一软件 ID，并补齐五语言摘要和说明。
4. 按实际支持的 OS 和架构拆分变体。只有满足上述固定资产门槛才使用 `artifact`；有明确官方命令才使用 `script`；其余使用 `official_page`。
5. 如实设置权限与交互要求。只添加可靠的快速命令检测；无法可靠判断时省略 `detection`。
6. 逐项复核许可原文、账号、稳定通道、资产哈希、Windows 签名者与架构，并在该批发版检查单记录结果。核验失败的条目保留为 held，不为凑数降低标准。
7. 只提交本分支的 `catalog/`、拆分脚本和 `README.md`。Pakkit 的「提交软件」按钮只打开预填 GitHub Issue，不代表目录维护者已经审核，也不会直接修改本分支。

## 本地校验与发布顺序

需要整体迁移单文件目录时，用脚本生成拆分文件：

```sh
python3 scripts/split_catalog.py catalog.json catalog
```

脚本默认参数就是上述源文件和目标目录，可直接运行 `python3 scripts/split_catalog.py`。它拒绝重复或不安全的分类 ID、未知分类和未列入新索引的旧 JSON 文件；生成后重新读取所有文件，确认分类正确且合并结果与源目录一致。

日常更新直接编辑索引和对应分类文件。执行 JSON 语法预检，并输出数量供审阅；数量不是固定断言：

```sh
jq empty catalog/*.json
jq -s '{categories: (.[0].categories | length), apps: ([.[1:][].apps[]] | length), variants: ([.[1:][].apps[].variants[]] | length)}' catalog/index.json catalog/dev.json catalog/remote.json catalog/tools.json catalog/browser.json catalog/office.json catalog/media.json catalog/education.json catalog/security.json catalog/system.json catalog/ai.json
```

Pakkit 的 Rust 解析器是完整契约的唯一权威，负责逐软件校验三项开源元数据、HTTPS 和严格的 SPDX／OSI 规则。不要另维护一套会逐渐偏离客户端的 `jq` 政策校验器。在包含对应客户端源码的主工作树运行：

```sh
PAKKIT_OFFICIAL_CATALOG_PATH=/path/to/catalog/index.json cargo test --locked real_catalog -- --ignored --nocapture
```

该测试读取索引及其同目录的全部索引分类文件，检查合计大小，不访问网络；普通测试也不依赖本分支 checkout。独立 GitHub Actions 工作流会在 `opensource-catalog` 推送、拉取请求、定时或手动运行时 checkout `master` 与 `opensource-catalog`，用客户端解析器验证已发布目录。

发布顺序必须为：先合并并发布支持 `schemaVersion: 2` 的客户端，再用该客户端验证并发布目录，最后确认远程索引和全部分类 JSON 与本地已验证内容一致，并用实际客户端刷新。不得先发布新目录，也不得为兼容旧客户端退回 schema 1。当前候选池不写入已发布目录；后续每批只发布已验收条目。
