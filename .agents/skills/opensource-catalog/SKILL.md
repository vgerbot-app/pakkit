---
name: opensource-catalog
description: Curate and validate Pakkit schema-3 open-source catalog entries on the opensource-catalog branch. Use when adding or updating applications, mapping package-manager IDs or GitHub release assets, reviewing install methods and permissions, verifying signatures/digests/archives, or accepting real installations.
---

# Official install catalog

收录目标不是“能下载”，而是：用户确认后，客户端能安装**同一来源、同一平台、同一架构、同一份已校验产物**，并如实报告结果。

## 工作边界

- 在 `opensource-catalog` 分支修改 `catalog/*.json`；客户端安装能力或解析器变更先在 `master` 验证、发布，再发布目录。
- 保留现有 application/variant ID、分类、OS/架构覆盖和元数据，除非任务明确要求迁移。
- 当前契约是 schema 3。不要使用 `official_page`、download-only 结果、浏览器安装或网页兜底。
- `homepage`、`sourceRepository`、`licenseUrl` 和 `sourceUrl` 是来源与审阅链接，不是安装回退。
- 映射不确定、资产不唯一、缺少摘要或安装语义不受支持时，**停止并修正映射或客户端**；不要猜包名、放宽选择器或改为打开网页。

## 1. 先证明软件可收录

逐项取得一手证据，优先项目官网、源码仓库、release API 和上游包索引：

1. **身份**：名称、发布者、官网和实际维护的源码仓库一致；不要用镜像站或第三方重新打包地址冒充上游。
2. **许可证**：使用有效的 SPDX 表达式；每个许可证必须经 OSI 批准且未弃用。链接到上游许可证原文，而非许可证介绍页。
3. **分发权与产物**：确认下载产物确实由该项目发布，且对应声明的 OS、CPU 架构和包格式。
4. **元数据**：五种 locale 的摘要和安装说明描述当前行为；不要复制另一平台的依赖、权限或路径说明。
5. **ID 与覆盖**：application/variant ID 全局唯一；同一软件的 `(os, architecture)` 不重复，声明的每个架构都有真实安装路径。

许可证、源码或分发来源无法证实时，不收录。

## 2. 按可信度选择安装方法

优先级不是机械规则；选择能被上游权威数据验证、且客户端真正支持的方法。

| 方法 | 适用情况 | 必须核实 |
|---|---|---|
| `package_manager` | winget、Flathub、Homebrew、npm 中有权威映射 | 精确包 ID、来源、OS 支持；Homebrew 的 formula/cask 类型 |
| `github_release` | 上游稳定 release 提供可唯一匹配的安装包或便携归档 | 仓库、格式、架构、资产选择器、SHA-256、安装动作 |
| `script` | 项目明确提供并维护官方安装脚本 | 完整命令、shell、权限、交互、PATH/依赖/副作用 |
| `portable` | 客户端显式支持的固定 Windows 便携产物 | 固定版本 URL、文件名、SHA-256；当前仅 Rufus 固定变体 |

### 包管理器

- winget 必须来自 `winget` 源并使用精确 ID；Flathub 必须固定 `flathub` remote。Homebrew 必须区分 formula 与 cask。
- 用上游索引验证，不以搜索引擎结果、相似名称或网页显示名称作为包 ID。
- 包管理器不存在、ID 下架或平台不支持时，如实失败；不要偷偷换另一个包。
- `latest` 之类的包管理器版本解析属于该管理器契约，不得把它误写成固定 release 摘要。

### 官方脚本

- 只使用上游正式文档给出的命令，保留 HTTPS 和失败即停参数；不要自行添加未证实的静默开关。
- Windows 使用 `powershell`，其他平台使用 `sh`。含 `sudo` 时必须 `interactive: true` 且 `requiresAdmin: true`。
- 说明会修改的 PATH/shell 配置、需要的依赖、账户、付费服务、覆盖/卸载行为和管理员权限。
- 脚本本质上会执行远程代码；发布前审阅实际脚本内容，并记录其动态性。不能用脚本绕过不可变 release 的安全要求。

## 3. 正确映射 GitHub release

只接受正式稳定版；拒绝 draft、prerelease、未上传完成的资产和无效 tag。

### 资产选择器必须唯一

- 以 release API 返回的**完整资产清单**设计 glob，不看一两个文件名就提交。
- 明确包含项目、OS、架构和格式；区分 `amd64`/`x86_64`、`arm64`/`aarch64`，不要依赖上游命名恰好与目录枚举一致。
- 排除 checksum、签名、源码包、SBOM、调试符号、包管理器元数据及其他 OS/架构。
- 同一个 `(格式, 后端, 架构)` 必须恰好匹配一个资产。零个和多个都失败；**绝不取第一个、最后一个或模糊最佳匹配**。
- 检查资产 URL 必须精确属于 `https://github.com/{repository}/releases/download/{tag}/{asset}`；重定向也要重新校验 HTTPS。
- 资产必须有 GitHub 提供的 `sha256:` digest，且解析为 64 位十六进制 SHA-256。缺少 digest 时停止，不改成网页下载。

经验：lazygit 这类项目的 Linux 资产必须区分 `x86_64` 与 `arm64`；DBX 这类多格式发布必须同时按格式和包管理器选择，不能只匹配项目名称。

### 格式、后端与安装动作必须一致

| 格式 | 安装配置 |
|---|---|
| `deb` | `manager: apt`，`install: package` |
| `rpm` | `manager: dnf` 或 `zypper`，`install: package` |
| `msi` | `install: install_package`，并提供精确 `expectedSigner` |
| `exe` | `install: run_installer`，并提供精确 `expectedSigner` |
| `pkg` / `dmg` | `install: run_installer` |
| `zip` / `tar_gz` / `tar_zst` | `kind: archive`，声明入口、strip、启动链接 |
| `appimage` | `kind: app_image`，声明启动链接 |

Linux 每个声明架构都要覆盖 Apt、DNF、Zypper 和无系统包管理器的主机上下文；没有适用格式时如实报告“不支持”，不要错装另一种包。

### 归档入口按实际布局声明

- 下载样例归档并列出路径，确认顶层目录是否需 `stripComponents`；当前只允许 0 或 1。
- `executableByArch` 是 strip 后的相对路径，覆盖 `assets` 中每个架构；不得含绝对路径、反斜杠、`.`、`..` 或空段。
- `linkName` 是启动链接名，不是任意路径；GUI 应用可提供合法 `desktopName`。
- 不要通过给所有文件加执行位来“修复”入口；只给声明的入口设置执行权限。
- AppImage 只需安装、加执行位、创建链接/desktop entry；不要自动执行。

参考 lazygit：Linux 归档入口为 `lazygit`，`stripComponents: 0`。参考 Ventoy：归档有顶层目录，入口按架构分别为 `VentoyGUI.x86_64` 和 `VentoyGUI.aarch64`，`stripComponents: 1`。

## 4. 固定产物与签名

### 固定便携版

固定 URL、文件名和小写 SHA-256。摘要必须来自实际下载文件计算，或来自可验证的上游签名校验文件；两者不一致时停止。当前 `portable` 是 Rufus 专用能力，不要套用到其他软件。

### Windows 安装包

- EXE/MSI 必须声明 `expectedSigner`，执行前同时验证签名状态和完整 subject 相等；只检查“有签名”不够。
- EXE 还要验证 PE machine 与当前架构；MSI 由选择和安装流程处理对应架构。
- 从文件本身提取签名验证，不能仅抄网页上的公司名。MSI 是 OLE 容器，可提取 embedded PKCS#7，再用 `openssl pkcs7` 查看 signer；最终仍须在 Windows 用平台 API 验收。
- MSI：`0` 表示安装成功，`3010` 表示需要重启，`1602` 是用户取消，其他非零码失败。
- EXE 启动安装器、PKG/DMG 打开安装器只表示 **installer started/opened**，绝不能报告为已安装。

### 不可变确认快照

动态 release 在用户确认时固定：版本、完整资产 URL、文件名、大小、SHA-256、格式、安装动作、预期签名者和权限要求。入队与执行必须消费这份快照：

- 下载前不重新请求 `releases/latest`。
- 不重新选择资产，不接受“更新的版本”。
- 用户重试解析可取得新快照，但这是一次新的明确确认。

## 5. 下载与解包是不可退让的安全边界

- 生产 URL 和每次重定向必须是 HTTPS；HTTP 只允许显式开发覆盖下的 loopback fixture，且不能传播到生产配置。
- 流式下载并同时计算 SHA-256，验证声明大小；下载和解压均有 2 GiB 上限，不能先把无限内容读入内存。
- 使用 `.part`/staging，再原子重命名；失败删除临时文件和 staging，不覆盖已有版本。
- ZIP/TAR 使用 Rust 库解析，不调用系统 `tar`/`unzip` 来绕过检查。
- 拒绝绝对路径、路径逃逸、反斜杠技巧、空/`.`/`..` 段、重复目标、符号/硬链接、设备节点、过多条目和超大展开。
- 解压后验证入口位于 staging 内、是普通文件且不是链接；安装路径中的符号链接组件也必须拒绝。
- 只有 staging 成功提交后才创建 launcher/desktop entry。失败时清理 staging 和本次新建的空应用目录；若版本已提交但 launcher 失败，保留版本供安全重试，不删除用户文件。
- 保留下载文件在用户可见目录；XDG download directory 不可用时回退 `~/Downloads`，不要无提示改用缓存。

必须做恶意归档验收：`../escape` 应在队列失败，安装根外没有 `escape`，没有版本目录、launcher 或 desktop entry。

## 6. 高风险软件只安装，不自动行动

- Rufus、Ventoy 等写盘工具绝不自动运行；说明目标磁盘会被格式化、需备份和核对设备。
- 不自动登录、接受额外协议、关闭安全机制、添加未知仓库或执行安装后脚本。
- `requiresAdmin`、`interactive` 和说明必须反映真实行为，不能为了“一键”隐藏提权或交互。

## 7. 验证要覆盖真实消费者

### 每次目录变更

从客户端 `master` 对 `opensource-catalog` 的**完整目录**运行：

```bash
PAKKIT_OFFICIAL_CATALOG_PATH=/path/to/catalog/index.json \
  cargo test --locked real_catalog -- --ignored --nocapture

PAKKIT_OFFICIAL_CATALOG_PATH=/path/to/catalog/index.json \
PAKKIT_OFFICIAL_VERIFY_RELEASES=1 \
  cargo test --locked real_catalog -- --ignored --nocapture
```

第二条会访问 GitHub，验证所有声明主机上下文的唯一资产、URL、格式和 SHA-256；CI 应提供 `GITHUB_TOKEN` 降低 API 限流风险。它**不替代** winget、Flathub、Homebrew、npm 的上游索引检查，也不证明实际安装成功。

客户端变更还要运行格式检查、完整测试和 Clippy；不要只跑新增测试。

### 实际 UI 与安装验收

在隔离 HOME 中使用本地 HTTPS/loopback fixture，覆盖：

1. 首次 release 请求失败，UI 显示错误且不能确认；Retry 成功。
2. 对话框展示固定版本、URL、大小、SHA-256 和格式。
3. 入队后只下载已确认资产，release API 不再被调用。
4. ZIP 与 AppImage 队列完成，版本目录、执行位、launcher symlink 和 desktop entry 均存在且指向正确文件。
5. 不自动执行便携程序；需要时仅用显式命令做功能检查。
6. 恶意 ZIP 在队列失败且无路径逃逸或安装残留。
7. 失败有可操作的 Details 和执行日志；成功文案与真实结果一致。

自动化 UI 每步后重新截图/观察状态；不要连续盲点。OCR 整屏可能漏掉按钮，必要时应裁剪放大，不能据漏识别重构布局。

### 平台与发布声明

- Linux 上完成的验证不能证明 Windows/macOS 安装行为；报告必须明确标为未验证。
- 包 ID/资产解析成功不等于安装成功；安装器启动不等于安装完成。
- 先发布并验证客户端，再发布目录。生产验收从 raw URL 重新下载目录，并从远端新克隆客户端构建，防止漏提交新模块。
- 暂存前检查新增源码文件；本地测试会包含未跟踪模块，不能据此证明远端提交完整。

## 收录完成清单

- [ ] 上游身份、源码、OSI 许可证和原文链接已核实。
- [ ] ID 唯一，分类正确，OS/架构覆盖真实且无重复。
- [ ] 五种 locale 的摘要、权限、依赖和副作用说明准确。
- [ ] 安装方法是客户端支持的最可信方法，没有网页或下载兜底。
- [ ] 包管理器 ID/来源或 release 资产选择器经权威数据验证。
- [ ] 每个主机上下文恰好一个产物；摘要、大小、格式、动作和签名要求完整。
- [ ] 归档布局、入口、strip、launcher 和 desktop 配置已用真实样例核对。
- [ ] 不可变快照、HTTPS、大小/SHA-256、原子 staging 和解包安全未被削弱。
- [ ] 完整目录、实时 release、客户端测试和适用的真实 UI/安装验收通过。
- [ ] 报告区分已验证与未验证平台，并按客户端先于目录的顺序发布。
