# Pakkit 官网直装目录

本分支只维护 `catalog.json` 和本说明。Pakkit 启动时从本分支读取远程目录；目录中的脚本和安装包均来自软件官方来源，Pakkit 不托管安装包，也不因为脚本结束、安装器启动、文件下载或官网打开而宣称软件已安装。

## 顶层字段

- `schemaVersion`：目录结构版本，目前必须为整数 `1`。
- `catalogVersion`：本次目录发布版本，用于界面和诊断。修改目录时应同步更新。
- `homepage`：目录维护页面的 HTTPS 地址。
- `submissionUrl`：预填软件提交内容所使用的 GitHub 新 Issue HTTPS 地址。
- `categories`：按数组顺序显示的分类。
- `apps`：软件条目数组。

## 本地化与分类

分类和软件的本地化文本使用对象，`zh-CN`、`zh-TW`、`en`、`ja`、`ko` 五个键均必填，文本不能为空。应用不会用内置翻译补全缺失文案。

每个分类包含：

- `id`：只允许 `dev`、`network`、`remote`、`tools`。
- `name`、`description`：完整的五语言文本。
- `icon`：依次为 `code`、`network`、`package`、`wrench`。
- `accent`：以 `#` 开头的六位十六进制颜色。

## 软件与变体

每个软件包含：

- `id`：全局唯一，只使用小写 ASCII 字母、数字和连字符。
- `name`、`publisher`：软件名称和官方开发者名称。
- `category`：已定义的分类 ID。
- `summary`：完整的五语言摘要。
- `homepage`：官方 HTTPS 页面。
- `mark`：卡片标识，最多两个字符。
- `accent`：以 `#` 开头的六位十六进制颜色。
- `variants`：非空安装方式数组。

每个变体必须有全局唯一的 `id`、`os`、`architectures`、`requiresAdmin`、`interactive`、五语言 `note`、HTTPS `sourceUrl` 和 `method`。`os` 为 `linux`、`windows` 或 `macos`；`architectures` 只能包含 `x86_64`、`aarch64`，且同一软件在同一 OS 下不得有重复架构覆盖。不同 OS 或不同架构使用不同变体，不用单个不确定的下载地址代表多个安装方式。

`detection` 可省略；省略表示安装状态未知，不表示未安装。命令检测格式为：

```json
{"type":"command","program":"example","args":["--version"]}
```

检测直接启动程序和参数，不经过 shell。退出码为 0 表示已安装，非 0 或无法启动表示未安装，信号终止表示检测错误。

## 安装方法

`method` 使用 `type` 区分三种形式：

1. `script`：`shell` 为 `sh` 或 `powershell`，`command` 是未经 Pakkit 改写的完整官方命令。需要用户回答提示时使用 `interactive: true`，在可见终端执行。包含 `sudo` 的命令必须同时设置 `interactive: true` 和 `requiresAdmin: true`。Pakkit 不静默提权。
2. `artifact`：包含 HTTPS `url`、普通文件名 `fileName`、`action`，以及可选 `sha256`、`makeExecutable`。`action` 为 `run_installer`、`download_package` 或 `download_portable`。EXE、PKG、DMG 和 MSI 可运行安装器；DEB、RPM 只下载；AppImage 便携版使用 `download_portable` 和 `makeExecutable: true`。文件名不得为空、包含路径分隔符或 `..`。`sha256` 必须为 64 位小写十六进制；有值时下载后强制校验，无值时界面必须明确说明目录未提供 SHA-256。
3. `official_page`：仅包含 HTTPS `url`，用于没有确定文件直链的官方发布页。确认后只在系统浏览器打开页面，不报告下载或安装成功。

## 添加或更新软件

1. 核对开发者官网、官方安装文档或官方发布页。记录准确的名称、开发者、脚本、下载地址和注意事项；不要把原型中的演示安装状态迁入目录。
2. 选择现有分类，新增全局唯一软件 ID，并补齐摘要的五语言文本。
3. 按 Linux、Windows、macOS 和 x86_64、aarch64 拆分变体。确定直链使用 `artifact`，有明确官方命令使用 `script`，只有通用发布页使用 `official_page`。
4. 如实设置权限与交互要求。添加能够可靠判断状态的命令检测；无法可靠判断时省略 `detection`，不要伪造检测结果。
5. 为下载填写安全、明确的普通文件名。若官方提供可核验的 SHA-256，填入小写哈希；不能用未经验证的值补齐。
6. 更新 `catalogVersion`，执行下方本地校验，并人工复核新增或变更的所有 URL、命令、文件格式和架构。
7. 只提交本分支的 `catalog.json` 与 `README.md`。Pakkit 的“提交软件”按钮只打开预填 GitHub Issue，不代表目录维护者已经审核，也不会直接修改本分支。

## 本地校验

先用标准 JSON 解析器检查语法和应用数量：

```sh
jq empty catalog.json
jq '.apps | length' catalog.json
```

当前首版应返回 `11`。发布前还应在 Pakkit 主工作树运行目录契约校验，以实际应用模型验证未知枚举、URL、颜色、ID、重复架构覆盖、文件名、哈希和权限约束。不要仅凭 `jq empty` 判断目录满足完整契约。

以下检查覆盖主要数据约束；在包含 `catalog.json` 的目录运行：

```sh
jq -e '
  .schemaVersion == 1 and
  (.catalogVersion | type == "string" and length > 0) and
  (.categories | map(.id) == ["dev", "network", "remote", "tools"]) and
  (.apps | length == 11)
' catalog.json

jq -e '
  def localized: (. | type == "object") and
    ((keys | sort) == ["en", "ja", "ko", "zh-CN", "zh-TW"]) and
    (all(.[]; type == "string" and length > 0));
  def https: type == "string" and startswith("https://");
  def color: type == "string" and test("^#[0-9a-f]{6}$");
  (.categories | all(
    (.name | localized) and (.description | localized) and (.accent | color)
  )) and
  (.apps | all(
    (.id | test("^[a-z0-9-]+$")) and
    (.homepage | https) and (.summary | localized) and
    (.mark | utf8bytelength >= 1 and utf8bytelength <= 2) and
    (.accent | color) and
    (.variants | type == "array" and length > 0) and
    (.variants | all(
      (.os == "linux" or .os == "windows" or .os == "macos") and
      (.architectures | length > 0 and
        (map(. == "x86_64" or . == "aarch64") | all) and
        (unique | length == length)) and
      (.requiresAdmin | type == "boolean") and
      (.interactive | type == "boolean") and
      (.note | localized) and (.sourceUrl | https) and
      (if .method.type == "script" then
        (.method.shell == "sh" or .method.shell == "powershell") and
        (.method.command | type == "string" and length > 0) and
        ((.method.command | contains("sudo") | not) or
          (.requiresAdmin and .interactive))
      elif .method.type == "artifact" then
        (.method.url | https) and
        (.method.fileName | type == "string" and length > 0 and
          (contains("/") or contains("\\") or contains("..") | not)) and
        (.method.action == "run_installer" or
          .method.action == "download_package" or
          .method.action == "download_portable") and
        ((.method.sha256 // null) == null or
          (.method.sha256 | test("^[0-9a-f]{64}$")))
      elif .method.type == "official_page" then
        (.method.url | https)
      else false end)
    ))
  ))
' catalog.json

jq -e '
  ([.apps[].id] | unique | length == length) and
  ([.apps[].variants[].id] | unique | length == length) and
  (all(.apps[];
    ([.variants[] as $v | $v.architectures[] | "\($v.os)/\(.)"] |
      unique | length == length)
  ))
' catalog.json
```

完整发布流程还必须由 Pakkit 自身的目录解析与契约校验通过，并确认远程原始 JSON 与已校验的本地内容一致。
