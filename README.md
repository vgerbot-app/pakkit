# Pakkit 开源软件目录

本分支维护开源软件分类目录 `catalog/`、拆分脚本 `scripts/split_catalog.py` 和本说明。每个软件必须具有规范源码仓库、当前有效且经 OSI 批准的 SPDX 许可证表达式，以及经维护者人工核对的许可证原文链接。Pakkit 启动时从本分支读取远程目录；目录中的脚本和安装包均来自软件官方来源，Pakkit 不托管安装包，也不因为脚本结束、安装器启动、文件下载或官网打开而宣称软件已安装。

当前目录经客户端校验包含 10 个分类、126 款软件、209 个安装变体；数量是本次验收结果，不是长期固定契约。

## 文件布局

- `catalog/index.json`：包含 `schemaVersion`、`catalogVersion`、`homepage`、`submissionUrl` 和按显示顺序排列的 `categories`。
- `catalog/<category-id>.json`：仅包含 `apps` 数组，存放对应分类的软件条目；条目必须保持所要求的分类内顺序。
- `scripts/split_catalog.py`：从已验收的单文件目录生成完整索引和分类文件，逐文件原子替换，并校验合并结果与源目录一致。遇到未列入索引的旧 JSON 文件会拒绝运行，不会删除未知文件。

客户端先下载索引，再按索引顺序下载每个分类文件，组合为完整目录后执行同一套全目录校验。分类文件缺失、重复、JSON 非法、软件放在错误分类、跨分类出现重复 ID，或任一软件缺少、填错开源许可元数据，都会使整个目录不可用；不会显示部分分类或回退旧单文件。目录运行时不联网验证许可证原文，原文真实性由发布前人工审核保证。

完整索引与所有分类文件的总大小上限为 4 MiB。

## 顶层字段

- `schemaVersion`：目录结构版本，目前必须为整数 `3`。客户端不接受旧版目录，也不提供旧目录兼容或部分回退。
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

`method` 使用 `type` 区分四种形式：

1. `package_manager`：精确指定 `winget`、`brew`、`flatpak` 或 `npm`。winget 固定 `winget` 源，Flathub 固定 `flathub` remote，Homebrew 必须区分 formula 与 cask。客户端仅在真实支持的 OS 上使用对应管理器；包不存在或映射不确定时如实失败，不替换为相似名称的包。
2. `github_release`：声明规范仓库和按格式、后端、架构区分的稳定 release 包。选择器必须在上游完整资产清单中恰好匹配一个资产，并具有 GitHub 提供的 SHA-256 digest。归档入口、strip、launcher、desktop entry 和 Linux 主机上下文必须与实际产物一致。
3. `script`：`shell` 为 `sh` 或 `powershell`，`command` 是未经 Pakkit 改写的完整官方安装命令。只采用上游正式文档明确给出的命令，发布前审阅实际脚本。包含 `sudo` 时必须同时设置 `interactive: true` 和 `requiresAdmin: true`；不得隐藏提权、PATH 修改、账户或依赖要求。
4. `portable`：固定版本 URL、普通文件名和 64 位小写 SHA-256。目前只用于经过客户端专门支持的 Rufus 固定变体，不能套用到其他应用。

禁止 `official_page`、download-only 结果、浏览器安装和网页兜底。`homepage`、`sourceRepository`、`licenseUrl` 与 `sourceUrl` 仅供来源审阅，不是安装方法。

### 安装产物的发布门槛

- 包管理器 ID、来源、平台和架构必须由上游索引或项目安装文档验证；包管理器的版本解析不等于固定 release 的不可变摘要。
- GitHub release 只接受正式稳定版。不得取第一个或模糊匹配资产，不得使用 checksum、签名、源码包、SBOM、调试符号或其他 OS／架构资产代替目标安装包。
- Windows EXE／MSI 必须从实际文件核验完整 `expectedSigner`；EXE 还需验证 PE machine。PKG／DMG 使用真实支持的安装动作，安装器启动或打开不能报告为已安装。
- Linux `github_release` 必须为每个声明架构覆盖 Apt、DNF、Zypper 和无系统包管理器上下文；缺适用产物时不得错装另一种包。
- 下载、重定向、大小／摘要校验、原子 staging 和解包路径安全不得因目录条目而放宽。高风险工具只安装，不自动运行、解锁、挂载、写盘或接受额外协议。

## 添加或更新软件

1. 核对规范厂商／项目身份、官方 HTTPS 首页、安装文档或规范仓库 Release，记录准确的名称、开发者、命令、下载地址和注意事项。排除第三方 Windows 构建、非官方重打包、网页／移动／服务器-only 项目、停止维护项目和不明镜像。
2. 核对规范源码仓库与许可证原文，准确填写 `sourceRepository`、`license` 和 `licenseUrl`。许可证表达式中的每个 ID 都必须当前有效且经 OSI 批准，不接受 `LicenseRef`、弃用 ID、非 OSI 许可证或专有许可；无法确认所分发软件的完整许可范围时不收录。
3. 选择现有分类，新增全局唯一软件 ID，并补齐五语言摘要和说明。
4. 按实际支持的 OS 和架构拆分变体。优先选择客户端已支持且能由权威数据验证的包管理器；稳定 release 必须具备完整、唯一且有 digest 的资产映射；只有上游明确维护的完整命令才使用 `script`。映射或产物证据不足时保留为 held，不创建网页或下载兜底。
5. 如实设置权限与交互要求。只添加可靠的快速命令检测；无法可靠判断时省略 `detection`。
6. 逐项复核许可原文、账号、稳定通道、资产哈希、Windows 签名者与架构，并在该批发版检查单记录结果。核验失败的条目保留为 held，不为凑数降低标准。
7. 只提交本分支的 `catalog/`、拆分脚本和 `README.md`。Pakkit 的「提交软件」按钮只打开预填 GitHub Issue，不代表目录维护者已经审核，也不会直接修改本分支。


## 来源审计

- `scripts/audit_appsgit.py` 将 Appsgit 的 1,404 项发现快照与现有目录、Homebrew formula/cask（含架构 variations）和 Flathub 名称线索对照，输出逐项台账。候选映射只用于安排人工审阅，不是许可证、平台或安装验收结论。
- `scripts/index_winget.py` 从 winget 稳定清单的精确上游仓库 URL 建立反向索引，使用 NUL 分隔的 Git 批处理协议，支持含空格路径。partial clone 缺失对象时的网络失败必须保留为未决，不能视为没有包。
- `scripts/import_verified_apps.py` 仅包含逐项审阅后的应用定义、五语言说明和安装事实，不直接导入 Appsgit 描述或许可证。新增 cask 前必须核实架构，不能默认双架构。

全部 1,404 项的处理结论应区分已存在、新增、许可排除、无支持安装方式、稳定通道暂缓与证据未决；未取证项目不能因网络失败而被排除。若完整目录将超过 4 MiB，必须先修改、验证并发布支持新容量方案的客户端，再发布目录，不得删除必需的五语言文案或许可元数据规避上限。
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

设置 `PAKKIT_OFFICIAL_VERIFY_RELEASES=1` 后，同一测试还会访问 GitHub，验证所有 release 主机上下文的唯一资产、URL、格式和 digest。该测试不替代 winget、Flathub、Homebrew、npm 的索引检查，也不证明实际安装成功；Linux 上的测试不能证明 Windows 或 macOS 安装行为。

发布顺序必须为：先合并并发布支持 `schemaVersion: 3` 的客户端，再用该客户端验证并发布目录，最后确认远程索引和全部分类 JSON 与本地已验证内容一致，并用实际客户端刷新。不得先发布新目录，也不得为兼容旧客户端退回旧 schema。当前候选池不写入已发布目录；后续每批只发布已验收条目。
