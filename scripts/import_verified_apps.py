#!/usr/bin/env python3
"""Apply manually reviewed AppsGit candidates to the schema-3 catalog.

This is a migration for the reviewed records below, not a bulk importer.
Package IDs, upstream identities and license texts are verified separately.
"""
from __future__ import annotations

import json
import re
from pathlib import Path

import yaml

ROOT = Path(__file__).resolve().parents[1]
CACHE = ROOT / "temp" / "pakkit"
FORMULAS = {p["name"]: p for p in json.loads((CACHE / "homebrew-formula.json").read_text())}
CASKS = {p["token"]: p for p in json.loads((CACHE / "homebrew-cask.json").read_text())}
WINGET_PATHS = json.loads((CACHE / "audit" / "winget-packages.json").read_text())


def version_key(version: str) -> tuple:
    return tuple(int(part) if part.isdigit() else part for part in re.split(r"([0-9]+)", version))


def winget(package_id: str) -> tuple[dict, dict]:
    path = CACHE / "winget-reviewed" / package_id / "installer.yaml"
    if not path.exists():
        raise FileNotFoundError(f"Run scripts/download_review_cache.py --winget-manifests before generating: {path}")
    return {}, yaml.safe_load(path.read_text())


def note(manager: str, package_id: str, extra: dict[str, str]) -> dict[str, str]:
    locales = ("zh-CN", "zh-TW", "en", "ja", "ko")
    if manager == "winget":
        result = {
            "zh-CN": f"通过 winget 安装 {package_id}（winget 源）；需要已安装并配置该包管理器。",
            "zh-TW": f"透過 winget 安裝 {package_id}（winget 來源）；需要已安裝並設定該套件管理器。",
            "en": f"Installs {package_id} with winget from the winget source. The package manager must already be installed and configured.",
            "ja": f"winget で {package_id} をインストールします（winget ソース）。パッケージマネージャーを事前にインストールして設定する必要があります。",
            "ko": f"winget로 {package_id}를 설치합니다(winget 소스). 패키지 관리자가 미리 설치되어 설정되어 있어야 합니다.",
        }
    elif manager == "flatpak":
        result = {
            "zh-CN": f"通过 Flatpak 从 Flathub 安装 {package_id}；需要已安装 Flatpak 并配置 Flathub remote。",
            "zh-TW": f"透過 Flatpak 從 Flathub 安裝 {package_id}；需要已安裝 Flatpak 並設定 Flathub remote。",
            "en": f"Installs {package_id} from Flathub with Flatpak. Flatpak and the Flathub remote must already be configured.",
            "ja": f"Flatpak で Flathub から {package_id} をインストールします。Flatpak と Flathub リモートを事前に設定する必要があります。",
            "ko": f"Flatpak으로 Flathub에서 {package_id}를 설치합니다. Flatpak과 Flathub 리모트가 미리 설정되어 있어야 합니다.",
        }
    else:
        result = {
            "zh-CN": f"通过 Homebrew 安装 {package_id}；需要已安装并配置 brew。",
            "zh-TW": f"透過 Homebrew 安裝 {package_id}；需要已安裝並設定 brew。",
            "en": f"Installs {package_id} with Homebrew. brew must already be installed and configured.",
            "ja": f"Homebrew で {package_id} をインストールします。brew を事前にインストールして設定する必要があります。",
            "ko": f"Homebrew로 {package_id}를 설치합니다. brew가 미리 설치되어 설정되어 있어야 합니다.",
        }
    for locale in locales:
        result[locale] += " " + extra[locale]
    return result


def package_variant(app_id: str, os: str, manager: str, package_id: str, architectures: list[str], cask: bool, extra: str = "") -> dict:
    method = {"type": "package_manager", "manager": manager, "packageId": package_id}
    if manager in {"winget", "flatpak"}:
        method["source"] = "winget" if manager == "winget" else "flathub"
    else:
        method["cask"] = cask
    return {
        "id": f"{app_id}-{os}" + (f"-{architectures[0].replace('_', '-')}" if os == "linux" else ""),
        "os": os,
        "architectures": architectures,
        "requiresAdmin": REVIEWED[app_id].get("linuxRequiresAdmin", REVIEWED[app_id].get("requiresAdmin", False)) if os == "linux" else REVIEWED[app_id].get("requiresAdmin", False) if os != "windows" else REVIEWED[app_id].get("windowsRequiresAdmin", False),
        "interactive": REVIEWED[app_id].get("linuxInteractive", REVIEWED[app_id].get("interactive", False)) if os == "linux" else REVIEWED[app_id].get("interactive", False) if os != "windows" else REVIEWED[app_id].get("windowsInteractive", False),
        "note": note(manager, package_id, extra),
        "sourceUrl": REVIEWED[app_id]["homepage"],
        "method": method,
    }


def release_note(link_name: str, extra: dict[str, str]) -> dict[str, str]:
    result = {
        "zh-CN": f"下载、校验并解压匹配的正式稳定版 GitHub Release 到用户目录，创建 {link_name} 启动链接；不修改系统 PATH。",
        "zh-TW": f"下載、驗證並解壓符合的正式穩定版 GitHub Release 到使用者目錄，建立 {link_name} 啟動連結；不修改系統 PATH。",
        "en": f"Downloads, verifies and extracts the matching stable GitHub Release into the user directory and creates a {link_name} launch link. System PATH is not modified.",
        "ja": f"一致する安定版 GitHub Release をダウンロード、検証し、ユーザーディレクトリに展開して {link_name} の起動リンクを作成します。システム PATH は変更しません。",
        "ko": f"일치하는 안정 GitHub Release를 다운로드하고 검증한 후 사용자 디렉터리에 압축을 풀고 {link_name} 실행 링크를 만듭니다. 시스템 PATH는 수정하지 않습니다.",
    }
    for locale in ("zh-CN", "zh-TW", "en", "ja", "ko"):
        result[locale] += " " + extra[locale]
    return result


def release_variant(app_id: str, repository: str, release: dict[str, object], extra: dict[str, str]) -> dict:
    arch = release["architecture"]
    link_name = release["linkName"]
    return {
        "id": f"{app_id}-linux-{arch.replace('_', '-')}",
        "os": "linux",
        "architectures": [arch],
        "requiresAdmin": REVIEWED[app_id].get("linuxRequiresAdmin", REVIEWED[app_id].get("requiresAdmin", False)),
        "interactive": REVIEWED[app_id].get("linuxInteractive", REVIEWED[app_id].get("interactive", False)),
        "note": release_note(link_name, extra),
        "sourceUrl": REVIEWED[app_id]["homepage"],
        "method": {
            "type": "github_release",
            "repository": repository,
            "packages": [
                {
                    "format": release["format"],
                    "assets": {arch: release["asset"]},
                    "install": {
                        "kind": "archive",
                        "executableByArch": {arch: release["executable"]},
                        "stripComponents": release.get("stripComponents", 0),
                        "linkName": link_name,
                    },
                }
            ],
        },
        "detection": {"type": "command", "program": release["detectProgram"], "args": ["--version"]},
    }


# Explicit, evidence-backed migrations away from Linux Homebrew variants.
# Schema 3 permits brew only on macOS. Replace IDs wherever a supported release
# exists; list an ID here only when the current stable upstream has no
# schema-3-installable Linux artifact for that architecture.
REMOVED_LINUX_BREW_VARIANTS: dict[str, dict[str, str]] = {
    "cava": {
        "cava-linux-x86-64": "stable 1.0.0 has no Linux binary release asset; source tarball and Android APK are not schema-3 Linux install routes",
        "cava-linux-aarch64": "stable 1.0.0 has no Linux binary release asset; source tarball and Android APK are not schema-3 Linux install routes",
    },
    "taskwarrior": {
        "taskwarrior-linux-x86-64": "stable v3.5.0 provides a source tarball only; no schema-3 Linux package or archive asset",
        "taskwarrior-linux-aarch64": "stable v3.5.0 provides a source tarball only; no schema-3 Linux package or archive asset",
    },
    "termusic": {
        "termusic-linux-x86-64": "stable v0.13.2 Linux artifacts use tar.xz, which the current schema-3 client archive formats do not accept",
        "termusic-linux-aarch64": "stable v0.13.2 Linux artifacts use tar.xz, which the current schema-3 client archive formats do not accept",
    },
    "gowall": {
        "gowall-linux-aarch64": "stable v0.2.4 has no Linux ARM64 asset; only Linux x86_64 has a verified release archive",
    },
}


def brew_cask_arches(package: dict) -> list[str]:
    # Each registered application must explicitly establish its cask's
    # supported macOS architectures from its stable package evidence.
    app_id = next(app_id for app_id, app in REVIEWED.items() if app.get("cask") == package["token"])
    return CASK_ARCHITECTURES[app_id]


FLATHUB_ARCHITECTURES: dict[str, list[str]] = {
    "lutris": ["x86_64"],
    "easyeffects": ["x86_64"],
    "feishin": ["x86_64"],
    "solaar": ["x86_64"],
    "fsearch": ["x86_64"],
    "rawtherapee": ["x86_64"],
    "strawberry": ["x86_64"],
    "warzone2100": ["x86_64"],
    "antimicrox": ["x86_64"],
    "kooha": ["x86_64"],
    "legcord": ["x86_64"],
    "seer": ["x86_64"],
    "lrcget": ["x86_64"],
    "nomacs": ["x86_64"],
    "media-downloader": ["x86_64"],
    "saber": ["x86_64"],
    "ghostwriter": ["x86_64"],
    "blanket": ["x86_64"],
    "gittyup": ["x86_64"],
    "openmw": ["x86_64"],
    "ytmdesktop": ["x86_64"],
    "stretchly": ["x86_64"],
    "librecad": ["x86_64"],
    "ludusavi": ["x86_64"],
    "piper": ["x86_64"],
    "pdf-arranger": ["x86_64"],
    "jellyfin-desktop": ["x86_64"],
    "qownnotes": ["x86_64"],
    "wayback": ["x86_64"],
}


def flathub_arches(app_id: str, package_id: str) -> list[str]:
    registered = FLATHUB_ARCHITECTURES[app_id]
    available = {
        line.split("\t")[1]
        for line in (CACHE / "flatpak-apps.tsv").read_text().splitlines()
        if line.startswith(package_id + "\t")
    }
    if not available or not set(registered) <= available:
        raise ValueError(f"Flathub architectures for {app_id} ({registered}) are not all present: {sorted(available)}")
    return registered

FORMULA_SOURCE_ARCHITECTURES: dict[str, dict[str, list[str]]] = {
    # yt-dlp's existing Intel macOS route uses the portable Python source build;
    # its absence from current Intel bottles is not evidence of incompatibility.
    "yt-dlp": {"macos": ["x86_64"]},
}


FORMULA_ARCHITECTURES: dict[str, dict[str, list[str]]] = {
    "yt-dlp": {"macos": ["x86_64", "aarch64"]},
    "cava": {"macos": ["x86_64", "aarch64"]},
    "taskwarrior": {"macos": ["x86_64", "aarch64"]},
    "dua-cli": {"macos": ["aarch64"]},
    "gowall": {"macos": ["aarch64"]},
    "termusic": {"macos": ["x86_64", "aarch64"]},
    "slides": {"macos": ["x86_64", "aarch64"]},
    "micro": {"macos": ["x86_64", "aarch64"]},
    "glow": {"macos": ["x86_64", "aarch64"]},
    "restic": {"macos": ["x86_64", "aarch64"]},
    "helix": {"macos": ["aarch64"]},
    "nushell": {"macos": ["aarch64"]},
    "croc": {"macos": ["aarch64"]},
    "zellij": {"macos": ["aarch64"]},
    "lazydocker": {"macos": ["x86_64", "aarch64"]},
}


def formula_arches(app_id: str, package: dict, os: str) -> list[str]:
    if package["deprecated"] or package["disabled"] or package["bottle"]["stable"] is None:
        raise ValueError(f"Unavailable stable formula for {app_id}")
    registered = FORMULA_ARCHITECTURES[app_id][os]
    files = package["bottle"]["stable"]["files"]
    keys = {
        ("macos", "x86_64"): ("sequoia", "sonoma"),
        ("macos", "aarch64"): ("arm64_golden_gate", "arm64_tahoe", "arm64_sequoia"),
        ("linux", "x86_64"): ("x86_64_linux",),
        ("linux", "aarch64"): ("arm64_linux",),
    }
    available = {
        arch
        for (platform, arch), bottle_keys in keys.items()
        if platform == os and ("all" in files or any(key in files for key in bottle_keys))
    }
    available |= set(FORMULA_SOURCE_ARCHITECTURES.get(app_id, {}).get(os, []))
    if not set(registered) <= available:
        raise ValueError(f"Formula architectures for {app_id}/{os} ({registered}) lack bottle or reviewed source-build evidence: {sorted(available)}")
    return registered


CASK_ARCHITECTURES = {
    "draw-io-desktop": ["x86_64", "aarch64"],
    "joplin": ["x86_64", "aarch64"],
    "tabby": ["x86_64", "aarch64"],
    "ghostty": ["x86_64", "aarch64"],
    "stats": ["x86_64", "aarch64"],
    "amethyst": ["x86_64", "aarch64"],
    "adrafinil": ["aarch64"],
    "rectangle": ["x86_64", "aarch64"],
    "maccy": ["x86_64", "aarch64"],
    "motrix": ["x86_64", "aarch64"],
    "netron": ["x86_64", "aarch64"],
    "vscodium": ["x86_64", "aarch64"],
    "wezterm": ["x86_64", "aarch64"],
    "lossless-cut": ["x86_64", "aarch64"],
    "appflowy": ["x86_64", "aarch64"],
    "cherry-studio": ["x86_64", "aarch64"],
    "freecad": ["x86_64", "aarch64"],
    "siyuan": ["x86_64", "aarch64"],
    "keepassxc": ["x86_64", "aarch64"],
    "cryptomator": ["x86_64", "aarch64"],
    "utm": ["x86_64", "aarch64"],
    "balenaetcher": ["x86_64", "aarch64"],
    "karabiner-elements": ["x86_64", "aarch64"],
    "monitorcontrol": ["x86_64", "aarch64"],
    "insomnia": ["x86_64", "aarch64"],
    "devtoys": ["x86_64", "aarch64"],
    "mockoon": ["x86_64", "aarch64"],
    "godot-engine": ["x86_64", "aarch64"],
    "lapce": ["x86_64", "aarch64"],
    "kitty": ["x86_64", "aarch64"],
    "anki": ["x86_64", "aarch64"],
    "shotcut": ["x86_64", "aarch64"],
    "musescore": ["x86_64", "aarch64"],
    "zettlr": ["x86_64", "aarch64"],
    "notesnook": ["x86_64", "aarch64"],
    "standard-notes": ["x86_64", "aarch64"],
    "tagspaces": ["x86_64", "aarch64"],
    "super-productivity": ["x86_64", "aarch64"],
    "activitywatch": ["x86_64", "aarch64"],
    "hammerspoon": ["x86_64", "aarch64"],
    "espanso": ["x86_64", "aarch64"],
    "bruno": ["x86_64", "aarch64"],
    "zen-browser": ["x86_64", "aarch64"],
    "spacedrive": ["x86_64", "aarch64"],
    "folo": ["x86_64", "aarch64"],
    "trilium": ["x86_64", "aarch64"],
    "recordly": ["x86_64", "aarch64"],
    "another-redis-desktop-manager": ["x86_64", "aarch64"],
    "vorssaint-utils": ["aarch64"],
    "gdevelop": ["x86_64", "aarch64"],
    "wox": ["x86_64", "aarch64"],
    "picgo": ["x86_64", "aarch64"],
    "switchhosts": ["x86_64", "aarch64"],
    "readest": ["x86_64", "aarch64"],
    "amical": ["x86_64", "aarch64"],
    "beutl": ["aarch64"],
    "buzz": ["x86_64", "aarch64"],
    "keycastr": ["x86_64", "aarch64"],
    "flycut": ["x86_64", "aarch64"],
    "cc-switch": ["x86_64", "aarch64"],
    "wailbrew": ["x86_64", "aarch64"],
    "open-design": ["x86_64", "aarch64"],
    "hidden": ["x86_64", "aarch64"],
}


# App IDs that were reviewed and rejected. The generator is otherwise additive,
# so exclusions must be removed explicitly instead of silently surviving in catalog JSON.
EXCLUDED_APP_IDS = {"alttab"}


REVIEWED = {
    "yt-dlp": {
        "name": "yt-dlp", "publisher": "yt-dlp", "category": "media", "mark": "YT", "accent": "#c94b4b",
        "repo": "yt-dlp/yt-dlp", "license": "Unlicense", "licenseFile": "LICENSE",
        "homepage": "https://github.com/yt-dlp/yt-dlp#installation", "winget": "yt-dlp.yt-dlp", "formula": "yt-dlp",
        "linuxReleases": [
            {"architecture": "x86_64", "format": "zip", "asset": "yt-dlp_linux.zip", "executable": "yt-dlp_linux", "linkName": "yt-dlp", "detectProgram": "yt-dlp"},
            {"architecture": "aarch64", "format": "zip", "asset": "yt-dlp_linux_aarch64.zip", "executable": "yt-dlp_linux_aarch64", "linkName": "yt-dlp", "detectProgram": "yt-dlp"},
        ],
        "summary": {
            "zh-CN": "命令行音视频下载工具，支持大量网站、格式选择、字幕与元数据处理。",
            "zh-TW": "命令列影音下載工具，支援大量網站、格式選擇、字幕與中繼資料處理。",
            "en": "A command-line audio/video downloader supporting many sites, format selection, subtitles and metadata.",
            "ja": "多数のサイトに対応し、形式選択、字幕、メタデータ処理を行えるコマンドライン音声・動画ダウンローダー。",
            "ko": "다양한 사이트, 형식 선택, 자막 및 메타데이터 처리를 지원하는 명령줄 오디오/비디오 다운로더.",
        },
        "extra": {
            "zh-CN": "这是命令行工具；媒体下载、格式转换和某些站点需要另行配置 FFmpeg 或其他依赖。请遵守站点条款与适用法律。",
            "zh-TW": "這是命令列工具；媒體下載、格式轉換和某些網站需要另行設定 FFmpeg 或其他相依套件。請遵守網站條款與適用法律。",
            "en": "This is a command-line tool. Media downloads, conversion and some sites require FFmpeg or other dependencies. Follow site terms and applicable law.",
            "ja": "コマンドラインツールです。メディアのダウンロード、変換、一部サイトには FFmpeg などの依存関係が必要です。サイトの規約と適用法を遵守してください。",
            "ko": "명령줄 도구입니다. 미디어 다운로드, 변환 및 일부 사이트에는 FFmpeg 등의 종속성이 필요합니다. 사이트 약관과 관련 법률을 준수하세요.",
        },
    },
    "draw-io-desktop": {
        "name": "draw.io", "publisher": "JGraph", "category": "office", "mark": "D", "accent": "#f08705",
        "repo": "jgraph/drawio-desktop", "license": "GPL-3.0-only", "licenseFile": "LICENSE",
        "homepage": "https://www.diagrams.net/", "winget": "JGraph.Draw", "cask": "drawio",
        "summary": {
            "zh-CN": "桌面流程图和图表编辑器，支持 UML、网络图、思维导图及本地文件。",
            "zh-TW": "桌面流程圖與圖表編輯器，支援 UML、網路圖、心智圖及本機檔案。",
            "en": "A desktop diagram editor for flowcharts, UML, network diagrams, mind maps and local files.",
            "ja": "フローチャート、UML、ネットワーク図、マインドマップ、ローカルファイルに対応するデスクトップ図表エディター。",
            "ko": "순서도, UML, 네트워크 다이어그램, 마인드맵 및 로컬 파일을 지원하는 데스크톱 다이어그램 편집기.",
        },
        "extra": {
            "zh-CN": "桌面版默认在本地编辑图表；外部图片、更新检查和云存储功能可能发起网络连接。",
            "zh-TW": "桌面版預設在本機編輯圖表；外部圖片、更新檢查和雲端儲存功能可能發起網路連線。",
            "en": "The desktop app edits diagrams locally by default. External images, update checks and cloud storage may make network connections.",
            "ja": "デスクトップ版はデフォルトでローカルに図を編集します。外部画像、更新確認、クラウドストレージではネットワーク接続が発生する場合があります。",
            "ko": "데스크톱 앱은 기본적으로 다이어그램을 로컬에서 편집합니다. 외부 이미지, 업데이트 확인 및 클라우드 저장소 사용 시 네트워크에 연결할 수 있습니다.",
        },
    },
    "joplin": {
        "name": "Joplin", "publisher": "Joplin", "category": "office", "mark": "J", "accent": "#356da0",
        "repo": "laurent22/joplin", "license": "AGPL-3.0-or-later", "licenseFile": "LICENSE",
        "homepage": "https://joplinapp.org/", "winget": "Joplin.Joplin", "cask": "joplin",
        "summary": {
            "zh-CN": "支持 Markdown、笔记本、待办事项与端到端加密同步的开源笔记应用。",
            "zh-TW": "支援 Markdown、筆記本、待辦事項與端對端加密同步的開源筆記應用程式。",
            "en": "An open-source note-taking app with Markdown, notebooks, to-dos and end-to-end encrypted synchronization.",
            "ja": "Markdown、ノートブック、ToDo、エンドツーエンド暗号化同期に対応するオープンソースのノートアプリ。",
            "ko": "Markdown, 노트북, 할 일 및 종단 간 암호화 동기화를 지원하는 오픈 소스 노트 애플리케이션.",
        },
        "extra": {
            "zh-CN": "本地笔记无需账号；使用 Joplin Cloud、Nextcloud、Dropbox、OneDrive 等同步服务时需要相应账号，部分服务可能收费。",
            "zh-TW": "本機筆記無需帳號；使用 Joplin Cloud、Nextcloud、Dropbox、OneDrive 等同步服務時需要相應帳號，部分服務可能收費。",
            "en": "Local notes require no account. Joplin Cloud, Nextcloud, Dropbox, OneDrive and other sync services require their own accounts; some may charge fees.",
            "ja": "ローカルノートにアカウントは不要です。Joplin Cloud、Nextcloud、Dropbox、OneDrive などの同期サービスには各アカウントが必要で、有料の場合があります。",
            "ko": "로컬 노트에는 계정이 필요하지 않습니다. Joplin Cloud, Nextcloud, Dropbox, OneDrive 등의 동기화 서비스에는 각 계정이 필요하며 일부는 유료입니다.",
        },
    },
    "tabby": {
        "name": "Tabby", "publisher": "Eugene Pankov", "category": "dev", "mark": "T", "accent": "#6658b3",
        "repo": "Eugeny/tabby", "license": "MIT", "licenseFile": "LICENSE",
        "homepage": "https://tabby.sh/", "winget": "Eugeny.Tabby", "cask": "tabby",
        "summary": {
            "zh-CN": "可配置的终端模拟器，集成 SSH、Telnet、串口连接、分屏和主题。",
            "zh-TW": "可設定的終端機模擬器，整合 SSH、Telnet、序列埠連線、分割畫面與主題。",
            "en": "A configurable terminal emulator with integrated SSH, Telnet, serial connections, split panes and themes.",
            "ja": "SSH、Telnet、シリアル接続、分割ペイン、テーマを統合した設定可能なターミナルエミュレーター。",
            "ko": "SSH, Telnet, 시리얼 연결, 분할 창 및 테마를 통합한 구성 가능한 터미널 에뮬레이터.",
        },
        "extra": {
            "zh-CN": "连接远程主机或串口设备时需要相应凭据和访问权限；同步或插件功能可能需要网络。",
            "zh-TW": "連線遠端主機或序列埠裝置時需要相應憑證和存取權限；同步或外掛功能可能需要網路。",
            "en": "Remote hosts and serial devices require appropriate credentials and access. Synchronization and plugin features may require network access.",
            "ja": "リモートホストやシリアルデバイスへの接続には、適切な認証情報とアクセス権が必要です。同期やプラグイン機能にはネットワークが必要な場合があります。",
            "ko": "원격 호스트나 시리얼 장치에 연결하려면 적절한 자격 증명과 접근 권한이 필요합니다. 동기화 및 플러그인 기능에는 네트워크가 필요할 수 있습니다.",
        },
    },
    "ghostty": {
        "name": "Ghostty", "publisher": "Mitchell Hashimoto", "category": "dev", "mark": "G", "accent": "#4e7ac7",
        "repo": "ghostty-org/ghostty", "license": "MIT", "licenseFile": "LICENSE",
        "homepage": "https://ghostty.org/download", "cask": "ghostty",
        "summary": {
            "zh-CN": "使用平台原生界面和 GPU 加速的快速终端模拟器，支持多窗口、标签页与分屏。",
            "zh-TW": "使用平台原生介面與 GPU 加速的快速終端機模擬器，支援多視窗、分頁與分割畫面。",
            "en": "A fast terminal emulator using platform-native UI and GPU acceleration, with windows, tabs and split panes.",
            "ja": "プラットフォームネイティブ UI と GPU アクセラレーションを採用し、ウィンドウ、タブ、分割ペインに対応する高速ターミナルエミュレーター。",
            "ko": "플랫폼 네이티브 UI와 GPU 가속을 사용하며 창, 탭, 분할 창을 지원하는 빠른 터미널 에뮬레이터.",
        },
        "extra": {
            "zh-CN": "需要 macOS 13 或更高版本。此变体仅安装 macOS 应用，不提供 Windows 或 Linux 安装路径。",
            "zh-TW": "需要 macOS 13 或更高版本。此變體僅安裝 macOS 應用程式，不提供 Windows 或 Linux 安裝路徑。",
            "en": "Requires macOS 13 or later. This variant installs only the macOS app; no Windows or Linux installation path is provided.",
            "ja": "macOS 13 以降が必要です。このバリアントは macOS アプリのみをインストールし、Windows や Linux のインストール経路は提供しません。",
            "ko": "macOS 13 이상이 필요합니다. 이 변형은 macOS 앱만 설치하며 Windows 또는 Linux 설치 경로는 제공하지 않습니다.",
        },
    },
    "amethyst": {
        "name": "Amethyst", "publisher": "Ian Ynda-Hummel and contributors", "category": "system", "mark": "Am", "accent": "#557fa8",
        "repo": "ianyh/Amethyst", "license": "MIT", "licenseFile": "LICENSE.md",
        "homepage": "https://github.com/ianyh/Amethyst", "cask": "amethyst",
        "summary": {
            "zh-CN": "macOS 平铺式窗口管理器，支持键盘快捷键、空间布局和自动窗口排列。",
            "zh-TW": "macOS 平鋪式視窗管理員，支援鍵盤快捷鍵、空間佈局和自動視窗排列。",
            "en": "A tiling window manager for macOS with keyboard shortcuts, Space layouts and automatic window arrangement.",
            "ja": "キーボードショートカット、Space レイアウト、自動ウィンドウ配置に対応する macOS のタイル型ウィンドウマネージャー。",
            "ko": "키보드 단축키, Spaces 레이아웃, 자동 창 배치를 지원하는 macOS 타일형 창 관리자.",
        },
        "extra": {
            "zh-CN": "需要 macOS 12 或更高版本。首次使用需在隐私与安全设置中授予辅助功能权限；还可关闭“根据最近使用情况自动重新排列空间”以获得稳定的空间导航。",
            "zh-TW": "需要 macOS 12 或更高版本。首次使用需在隱私與安全性設定中授予輔助使用權限；還可關閉「根據最近使用情況自動重新排列空間」以獲得穩定的空間導覽。",
            "en": "Requires macOS 12 or later. Grant Accessibility permission in Privacy & Security settings on first use; disabling Automatically rearrange Spaces based on most recent use makes Space navigation predictable.",
            "ja": "macOS 12 以降が必要です。初回使用時にプライバシーとセキュリティ設定でアクセシビリティ権限を許可してください。「最近の使用状況に基づいて Spaces を自動的に並べ替える」を無効にすると、Space ナビゲーションが安定します。",
            "ko": "macOS 12 이상이 필요합니다. 처음 사용할 때 개인정보 보호 및 보안 설정에서 손쉬운 사용 권한을 허용해야 하며, '최근 사용 기록에 따라 Spaces 자동 재정렬'을 끄면 Spaces 탐색 동작이 예측 가능해집니다.",
        },
    },
    "adrafinil": {
        "name": "Adrafinil", "publisher": "kagerou and contributors", "category": "system", "mark": "Ad", "accent": "#607d9b",
        "repo": "kageroumado/adrafinil", "license": "MIT", "licenseFile": "LICENSE",
        "homepage": "https://github.com/kageroumado/adrafinil", "cask": "adrafinil",
        "summary": {
            "zh-CN": "macOS 防睡眠与唤醒保持工具，支持应用进程、CLI、定时保持和温度保护。",
            "zh-TW": "macOS 防睡眠與喚醒保持工具，支援應用程式程序、CLI、定時保持和溫度保護。",
            "en": "A macOS sleep-blocking and wake-hold tool with process integration, CLI access, timed holds and thermal protection.",
            "ja": "プロセス連携、CLI、時間指定の保持、温度保護を備えた macOS のスリープ抑制・起床維持ツール。",
            "ko": "프로세스 연동, CLI, 시간 지정 유지 및 온도 보호를 제공하는 macOS 수면 방지·깨우기 유지 도구.",
        },
        "extra": {
            "zh-CN": "仅支持 Apple 芯片且需要 macOS 26 或更高版本。Cask 安装只放置应用；首次运行后需用户显式批准，才会注册用户 LaunchAgent、root LaunchDaemon 特权 helper 及可选 CLI/PATH 链接。",
            "zh-TW": "僅支援 Apple 晶片且需要 macOS 26 或更高版本。Cask 安裝只放置應用程式；首次執行後需使用者明確批准，才會註冊使用者 LaunchAgent、root LaunchDaemon 特權 helper 及可選 CLI/PATH 連結。",
            "en": "Apple silicon only; requires macOS 26 or later. The cask installation only places the app. After first launch, explicit user approval is required before registering the user LaunchAgent, root LaunchDaemon privileged helper and optional CLI/PATH links.",
            "ja": "Apple シリコンのみ対応で、macOS 26 以降が必要です。Cask のインストールはアプリの配置のみです。初回起動後、ユーザー LaunchAgent、root の LaunchDaemon 特権ヘルパー、任意の CLI/PATH リンクの登録にはユーザーの明示的な許可が必要です。",
            "ko": "Apple Silicon만 지원하며 macOS 26 이상이 필요합니다. Cask 설치는 앱 배치만 수행합니다. 처음 실행한 후 사용자 LaunchAgent, root LaunchDaemon 권한 헬퍼 및 선택적 CLI/PATH 링크를 등록하려면 사용자의 명시적 승인이 필요합니다.",
        },
    },
    "stats": {
        "name": "Stats", "publisher": "Serhiy Mytrovtsiy", "category": "system", "mark": "S", "accent": "#4a90d9",
        "repo": "exelban/stats", "license": "MIT", "licenseFile": "LICENSE",
        "homepage": "https://github.com/exelban/stats", "cask": "stats",
        "summary": {
            "zh-CN": "macOS 菜单栏系统监视器，显示 CPU、GPU、内存、磁盘、网络、电池和传感器信息。",
            "zh-TW": "macOS 選單列系統監視器，顯示 CPU、GPU、記憶體、磁碟、網路、電池和感測器資訊。",
            "en": "A macOS menu-bar system monitor for CPU, GPU, memory, disk, network, battery and sensor information.",
            "ja": "CPU、GPU、メモリ、ディスク、ネットワーク、バッテリー、センサー情報を表示する macOS メニューバーのシステムモニター。",
            "ko": "CPU, GPU, 메모리, 디스크, 네트워크, 배터리 및 센서 정보를 표시하는 macOS 메뉴 막대 시스템 모니터.",
        },
        "extra": {
            "zh-CN": "需要 macOS 12 或更高版本。部分菜单栏、传感器或风扇功能需要额外授权；本安装不启用远程监控脚本。",
            "zh-TW": "需要 macOS 12 或更高版本。部分選單列、感測器或風扇功能需要額外授權；此安裝不啟用遠端監控腳本。",
            "en": "Requires macOS 12 or later. Some menu-bar, sensor or fan features require additional authorization; this installation does not enable the remote-monitoring script.",
            "ja": "macOS 12 以降が必要です。メニューバー、センサー、ファンの一部機能には追加の許可が必要です。このインストールではリモート監視スクリプトを有効にしません。",
            "ko": "macOS 12 이상이 필요합니다. 일부 메뉴 막대, 센서 또는 팬 기능에는 추가 권한이 필요합니다. 이 설치는 원격 모니터링 스크립트를 활성화하지 않습니다.",
        },
    },
    "rectangle": {
        "name": "Rectangle", "publisher": "Ryan Hanson", "category": "system", "mark": "R", "accent": "#5795d4",
        "repo": "rxhanson/Rectangle", "license": "MIT", "licenseFile": "LICENSE",
        "homepage": "https://rectangleapp.com/", "cask": "rectangle",
        "summary": {
            "zh-CN": "macOS 窗口管理工具，使用快捷键或拖拽吸附调整窗口大小与位置。",
            "zh-TW": "macOS 視窗管理工具，使用快捷鍵或拖曳吸附調整視窗大小與位置。",
            "en": "A macOS window manager for resizing and positioning windows with shortcuts or drag-to-snap.",
            "ja": "ショートカットやドラッグ＆スナップでウィンドウのサイズと位置を調整する macOS ウィンドウマネージャー。",
            "ko": "단축키나 드래그 스냅으로 창 크기와 위치를 조정하는 macOS 창 관리 도구.",
        },
        "extra": {
            "zh-CN": "需要 macOS 14 或更高版本。移动其他应用窗口前需在 macOS 隐私与安全设置中授予辅助功能权限。",
            "zh-TW": "需要 macOS 14 或更高版本。移動其他應用程式視窗前需在 macOS 隱私與安全性設定中授予輔助使用權限。",
            "en": "Requires macOS 14 or later. Grant Accessibility permission in macOS Privacy & Security settings before controlling other apps' windows.",
            "ja": "macOS 14 以降が必要です。他のアプリのウィンドウを操作するには、macOS のプライバシーとセキュリティ設定でアクセシビリティ権限を許可してください。",
            "ko": "macOS 14 이상이 필요합니다. 다른 앱의 창을 제어하려면 macOS 개인정보 보호 및 보안 설정에서 손쉬운 사용 권한을 허용해야 합니다.",
        },
    },
    "maccy": {
        "name": "Maccy", "publisher": "Alex Rodionov", "category": "tools", "mark": "M", "accent": "#7095d3",
        "repo": "p0deje/Maccy", "license": "MIT", "licenseFile": "LICENSE",
        "homepage": "https://maccy.app/", "cask": "maccy",
        "summary": {
            "zh-CN": "轻量级 macOS 剪贴板管理器，可搜索并快速使用历史复制内容。",
            "zh-TW": "輕量級 macOS 剪貼簿管理器，可搜尋並快速使用歷史複製內容。",
            "en": "A lightweight macOS clipboard manager for searching and quickly reusing copied content.",
            "ja": "コピー履歴を検索してすばやく再利用できる軽量 macOS クリップボードマネージャー。",
            "ko": "복사 기록을 검색하고 빠르게 다시 사용할 수 있는 가벼운 macOS 클립보드 관리자.",
        },
        "extra": {
            "zh-CN": "需要 macOS 14 或更高版本。剪贴板历史可能包含敏感信息，请按需要配置排除应用和清除策略。",
            "zh-TW": "需要 macOS 14 或更高版本。剪貼簿歷史可能包含敏感資訊，請按需要設定排除應用程式和清除策略。",
            "en": "Requires macOS 14 or later. Clipboard history can contain sensitive data; configure application exclusions and clearing policies as needed.",
            "ja": "macOS 14 以降が必要です。クリップボード履歴には機密情報が含まれる場合があるため、必要に応じて除外アプリと消去ポリシーを設定してください。",
            "ko": "macOS 14 이상이 필요합니다. 클립보드 기록에 민감한 정보가 포함될 수 있으므로 필요에 따라 제외 앱과 삭제 정책을 설정하세요.",
        },
    },
    "motrix": {
        "name": "Motrix", "publisher": "Dr_rOot", "category": "media", "mark": "M", "accent": "#4a8ec2",
        "repo": "agalwood/Motrix", "license": "MIT", "licenseFile": "LICENSE",
        "homepage": "https://motrix.app/", "cask": "motrix",
        "summary": {
            "zh-CN": "桌面下载管理器，支持 HTTP、FTP、BitTorrent、磁力链接和断点续传。",
            "zh-TW": "桌面下載管理器，支援 HTTP、FTP、BitTorrent、磁力連結和斷點續傳。",
            "en": "A desktop download manager supporting HTTP, FTP, BitTorrent, magnet links and resumable downloads.",
            "ja": "HTTP、FTP、BitTorrent、マグネットリンク、ダウンロード再開に対応するデスクトップダウンロードマネージャー。",
            "ko": "HTTP, FTP, BitTorrent, 마그넷 링크 및 다운로드 재개를 지원하는 데스크톱 다운로드 관리자.",
        },
        "extra": {
            "zh-CN": "需要 macOS 13 或更高版本。升级会迁移 SQLite 数据库；请先备份配置和下载记录。第三方素材另见其许可证声明。",
            "zh-TW": "需要 macOS 13 或更高版本。升級會遷移 SQLite 資料庫；請先備份設定和下載記錄。第三方素材另見其授權聲明。",
            "en": "Requires macOS 13 or later. Upgrades migrate the SQLite database; back up your profile and download history first. Third-party assets have separate notices.",
            "ja": "macOS 13 以降が必要です。アップグレードで SQLite データベースを移行するため、先にプロフィールとダウンロード履歴をバックアップしてください。第三者素材には別の通知があります。",
            "ko": "macOS 13 이상이 필요합니다. 업그레이드 시 SQLite 데이터베이스를 마이그레이션하므로 먼저 프로필과 다운로드 기록을 백업하세요. 타사 자산에는 별도의 고지가 적용됩니다.",
        },
    },
    "netron": {
        "name": "Netron", "publisher": "Lutz Roeder", "category": "ai", "mark": "N", "accent": "#516f9f",
        "repo": "lutzroeder/netron", "license": "MIT", "licenseFile": "LICENSE",
        "homepage": "https://github.com/lutzroeder/netron", "cask": "netron",
        "summary": {
            "zh-CN": "神经网络与机器学习模型查看器，支持 ONNX、TensorFlow Lite、PyTorch 等格式。",
            "zh-TW": "神經網路與機器學習模型檢視器，支援 ONNX、TensorFlow Lite、PyTorch 等格式。",
            "en": "A neural-network and machine-learning model viewer supporting ONNX, TensorFlow Lite, PyTorch and other formats.",
            "ja": "ONNX、TensorFlow Lite、PyTorch などの形式に対応するニューラルネットワーク・機械学習モデルビューアー。",
            "ko": "ONNX, TensorFlow Lite, PyTorch 등의 형식을 지원하는 신경망 및 기계 학습 모델 뷰어.",
        },
        "extra": {
            "zh-CN": "需要 macOS 13 或更高版本。此变体安装本地桌面模型查看器，不启动浏览器服务。",
            "zh-TW": "需要 macOS 13 或更高版本。此變體安裝本機桌面模型檢視器，不啟動瀏覽器服務。",
            "en": "Requires macOS 13 or later. This variant installs the local desktop model viewer; it does not start a browser service.",
            "ja": "macOS 13 以降が必要です。このバリアントはローカルのデスクトップモデルビューアーをインストールし、ブラウザーサービスは開始しません。",
            "ko": "macOS 13 이상이 필요합니다. 이 변형은 로컬 데스크톱 모델 뷰어를 설치하며 브라우저 서비스를 시작하지 않습니다.",
        },
    },
    "vscodium": {
        "name": "VSCodium", "publisher": "VSCodium contributors", "category": "dev", "mark": "V", "accent": "#4f8ac9",
        "repo": "VSCodium/vscodium", "license": "MIT", "licenseFile": "LICENSE",
        "homepage": "https://vscodium.com/", "cask": "vscodium",
        "summary": {
            "zh-CN": "由社区从 VS Code 源码构建的自由开源编辑器，默认禁用 Microsoft 遥测。",
            "zh-TW": "由社群從 VS Code 原始碼建構的自由開源編輯器，預設停用 Microsoft 遙測。",
            "en": "Community-built, freely licensed VS Code binaries with Microsoft telemetry disabled by default.",
            "ja": "VS Code のソースからコミュニティが構築した自由に利用できるエディターで、Microsoft テレメトリはデフォルトで無効です。",
            "ko": "VS Code 소스에서 커뮤니티가 빌드한 자유 소프트웨어 편집기로, Microsoft 원격 분석이 기본적으로 비활성화되어 있습니다.",
        },
        "extra": {
            "zh-CN": "这是社区构建的 VSCodium，不是 Microsoft 官方 Visual Studio Code。扩展市场、账户和同步功能可能需要单独配置。",
            "zh-TW": "這是社群建構的 VSCodium，不是 Microsoft 官方 Visual Studio Code。擴充市集、帳戶和同步功能可能需要單獨設定。",
            "en": "This is community-built VSCodium, not Microsoft's official Visual Studio Code. Extension marketplace, account and sync features may require separate configuration.",
            "ja": "これはコミュニティが構築した VSCodium であり、Microsoft 公式の Visual Studio Code ではありません。拡張機能マーケットプレイス、アカウント、同期機能は個別の設定が必要な場合があります。",
            "ko": "이것은 커뮤니티가 빌드한 VSCodium이며 Microsoft 공식 Visual Studio Code가 아닙니다. 확장 마켓플레이스, 계정 및 동기화 기능은 별도 구성이 필요할 수 있습니다.",
        },
    },
    "wezterm": {
        "name": "WezTerm", "publisher": "Wez Furlong", "category": "dev", "mark": "W", "accent": "#536dbe",
        "repo": "wezterm/wezterm", "license": "MIT", "licenseFile": "LICENSE.md",
        "homepage": "https://wezterm.org/installation", "cask": "wezterm",
        "summary": {
            "zh-CN": "GPU 加速的跨平台终端模拟器与多路复用器，支持分屏、标签页和 Lua 配置。",
            "zh-TW": "GPU 加速的跨平台終端機模擬器與多工器，支援分割畫面、分頁和 Lua 設定。",
            "en": "A GPU-accelerated terminal emulator and multiplexer with split panes, tabs and Lua configuration.",
            "ja": "GPU アクセラレーション対応のターミナルエミュレーター兼マルチプレクサーで、分割ペイン、タブ、Lua 設定に対応します。",
            "ko": "분할 창, 탭 및 Lua 구성을 지원하는 GPU 가속 터미널 에뮬레이터 및 멀티플렉서.",
        },
        "extra": {
            "zh-CN": "使用 Homebrew 的稳定 cask，不使用 nightly 构建。随附字体按各自 SIL Open Font License 分发。",
            "zh-TW": "使用 Homebrew 的穩定 cask，不使用 nightly 建構。隨附字型按各自 SIL Open Font License 分發。",
            "en": "Uses Homebrew's stable cask, not a nightly build. Bundled fonts are distributed under their respective SIL Open Font Licenses.",
            "ja": "Homebrew の安定版 cask を使用し、nightly ビルドは使用しません。同梱フォントにはそれぞれの SIL Open Font License が適用されます。",
            "ko": "nightly 빌드가 아닌 Homebrew의 안정 cask를 사용합니다. 번들 글꼴에는 각각의 SIL Open Font License가 적용됩니다.",
        },
    },
    "lossless-cut": {
        "name": "LosslessCut", "publisher": "Mikael Finstad", "category": "media", "mark": "LC", "accent": "#3b83a8",
        "repo": "mifi/lossless-cut", "license": "GPL-2.0-or-later", "licenseFile": "LICENSE",
        "homepage": "https://losslesscut.app/", "cask": "losslesscut",
        "summary": {
            "zh-CN": "基于 FFmpeg 的无损音视频编辑器，可快速剪切、合并、提取轨道和重新封装。",
            "zh-TW": "基於 FFmpeg 的無損影音編輯器，可快速剪切、合併、擷取軌道和重新封裝。",
            "en": "An FFmpeg-based lossless media editor for fast cutting, merging, track extraction and remuxing.",
            "ja": "FFmpeg ベースの無損メディアエディターで、高速なカット、結合、トラック抽出、再 mux に対応します。",
            "ko": "FFmpeg 기반 무손실 미디어 편집기로 빠른 자르기, 병합, 트랙 추출 및 리먹싱을 지원합니다.",
        },
        "extra": {
            "zh-CN": "需要 macOS 12 或更高版本。无损剪切依赖源文件编码和关键帧；重新编码或导出覆盖原文件前请确认输出位置。",
            "zh-TW": "需要 macOS 12 或更高版本。無損剪切依賴來源檔案編碼和關鍵幀；重新編碼或匯出覆蓋原檔案前請確認輸出位置。",
            "en": "Requires macOS 12 or later. Lossless cutting depends on source codecs and keyframes; confirm output locations before re-encoding or overwriting source files.",
            "ja": "macOS 12 以降が必要です。無損カットはソースのコーデックとキーフレームに依存します。再エンコードやソースの上書き前に出力先を確認してください。",
            "ko": "macOS 12 이상이 필요합니다. 무손실 자르기는 소스 코덱과 키프레임에 따라 달라집니다. 재인코딩하거나 원본을 덮어쓰기 전에 출력 위치를 확인하세요.",
        },
    },
    "appflowy": {
        "name": "AppFlowy", "publisher": "AppFlowy", "category": "office", "mark": "A", "accent": "#7359a8",
        "repo": "AppFlowy-IO/AppFlowy", "license": "AGPL-3.0-only", "licenseFile": "LICENSE",
        "homepage": "https://www.appflowy.io/", "cask": "appflowy",
        "summary": {
            "zh-CN": "开源文档与项目工作空间，支持笔记、看板、数据库和本地数据管理。",
            "zh-TW": "開源文件與專案工作空間，支援筆記、看板、資料庫和本機資料管理。",
            "en": "An open-source document and project workspace with notes, kanban boards, databases and local data control.",
            "ja": "ノート、かんばんボード、データベース、ローカルデータ管理に対応するオープンソースの文書・プロジェクトワークスペース。",
            "ko": "노트, 칸반 보드, 데이터베이스 및 로컬 데이터 관리를 지원하는 오픈 소스 문서 및 프로젝트 작업 공간.",
        },
        "extra": {
            "zh-CN": "需要 macOS 12 或更高版本。本地功能与云端同步、协作、AI 服务不同；后几项可能需要账号、网络或订阅。",
            "zh-TW": "需要 macOS 12 或更高版本。本機功能與雲端同步、協作、AI 服務不同；後幾項可能需要帳號、網路或訂閱。",
            "en": "Requires macOS 12 or later. Local features are separate from cloud sync, collaboration and AI services, which may require accounts, network access or subscriptions.",
            "ja": "macOS 12 以降が必要です。ローカル機能とクラウド同期、共同作業、AI サービスは別で、後者にはアカウント、ネットワーク、サブスクリプションが必要な場合があります。",
            "ko": "macOS 12 이상이 필요합니다. 로컬 기능과 클라우드 동기화, 공동 작업 및 AI 서비스는 별개이며, 후자는 계정, 네트워크 또는 구독이 필요할 수 있습니다.",
        },
    },
    "cc-switch": {
        "name": "CC Switch", "publisher": "Jason Young", "category": "dev", "mark": "Cw", "accent": "#f97316",
        "repo": "farion1231/cc-switch", "license": "MIT", "licenseFile": "LICENSE", "licenseRef": "v4.0.34/LICENSE",
        "homepage": "https://ccswitch.io/", "cask": "cc-switch", "caskArchitectures": ["x86_64", "aarch64"],
        "winget": "farion1231.CC-Switch",
        "summary": {
            "zh-CN": "面向 Claude Code、Codex、Gemini CLI 等 AI 编程工具的图形配置、路由和用量管理应用。",
            "zh-TW": "面向 Claude Code、Codex、Gemini CLI 等 AI 編程工具的圖形設定、路由和用量管理應用程式。",
            "en": "A graphical configuration, routing and usage-management app for AI coding tools such as Claude Code, Codex and Gemini CLI.",
            "ja": "Claude Code、Codex、Gemini CLI などの AI コーディングツール向けの、グラフィカルな設定・ルーティング・使用量管理アプリ。",
            "ko": "Claude Code, Codex, Gemini CLI 등 AI 코딩 도구를 위한 그래픽 구성, 라우팅 및 사용량 관리 앱.",
        },
        "extra": {
            "zh-CN": "本条目对应稳定版 v4.0.34；GitHub release 为非草稿、非预发布，4.0.4 是 4.0 系列首个稳定版。Homebrew cask 的 DMG SHA-256 已与 GitHub 资产核对，镜像内只有 CC Switch.app、Applications 拖放链接和背景资源；cask 无 formula 依赖、冲突或 caveat，只复制 App，zap 路径仅在卸载清理时使用。主程序是含 x86_64 与 arm64 的 universal Mach-O，两个切片均启用 hardened runtime，标识符为 com.ccswitch.desktop，Developer ID Application 证书属于 Jason Young (R8UR22V2F9)，并含 s8ch v1 notarization ticket 容器与 g8tk trailer；Linux 本地解析不宣称在线 Gatekeeper 评估结果。需要 macOS 12 或更高版本。应用管理 Claude Code、Codex、Gemini CLI、OpenCode 等工具的用户配置，会读写 ~/.cc-switch 及各工具配置目录；本地路由默认关闭，默认监听 127.0.0.1:15721。ccswitch:// 导入先进入待确认队列；图标抓取仅允许默认端口的公网 HTTPS 并拒绝回环、内网和凭据 URL，普通外部链接则限制为 HTTP/HTTPS/mailto 且拒绝 javascript:、file:、data: 与自定义协议。终端、shell、osascript 与受管 CLI 安装/更新均由用户在界面触发；登录项和更新下载默认关闭，复制 cask 不会启用，Tauri 更新经 minisign 验签、暂存于内存且安装仍需用户确认。Info.plist 仅声明本地网络用途，未声明相机或麦克风用途。清单审查未发现常见第三方分析 SDK；应用本身提供本地用量/成本统计和可选同步，因此不表述为完全无遥测。",
            "zh-TW": "本條目對應穩定版 v4.0.34；GitHub release 為非草稿、非預先發布，4.0.4 是 4.0 系列首個穩定版。Homebrew cask 的 DMG SHA-256 已與 GitHub 資產核對，映像檔內只有 CC Switch.app、Applications 拖放連結和背景資源；cask 無 formula 相依性、衝突或 caveat，只複製 App，zap 路徑僅在解除安裝清理時使用。主程式是含 x86_64 與 arm64 的 universal Mach-O，兩個切片均啟用 hardened runtime，識別碼為 com.ccswitch.desktop，Developer ID Application 憑證屬於 Jason Young (R8UR22V2F9)，並含 s8ch v1 notarization ticket 容器與 g8tk trailer；Linux 本機解析不宣稱線上 Gatekeeper 評估結果。需要 macOS 12 或更新版本。應用程式管理 Claude Code、Codex、Gemini CLI、OpenCode 等工具的使用者設定，會讀寫 ~/.cc-switch 及各工具設定目錄；本機路由預設關閉，預設監聽 127.0.0.1:15721。ccswitch:// 匯入先進入待確認佇列；圖示擷取僅允許預設連接埠的公網 HTTPS 並拒絕回環、內網和憑證 URL，一般外部連結則限制為 HTTP/HTTPS/mailto 且拒絕 javascript:、file:、data: 與自訂協定。終端機、shell、osascript 與受管 CLI 安裝/更新皆由使用者在介面觸發；登入項目和更新下載預設關閉，複製 cask 不會啟用，Tauri 更新經 minisign 驗簽、暫存於記憶體且安裝仍需使用者確認。Info.plist 僅宣告本機網路用途，未宣告相機或麥克風用途。清單審查未發現常見第三方分析 SDK；應用本身提供本機用量/成本統計和可選同步，因此不表述為完全無遙測。",
            "en": "This entry corresponds to stable v4.0.34; the GitHub release is neither a draft nor a prerelease, and v4.0.4 is documented as the first stable 4.0 release. The Homebrew cask DMG SHA-256 matches the GitHub asset; the image contains only CC Switch.app, an Applications drag-install link and background assets. The cask has no formula dependency, conflict or caveat, only copies the app, and uses zap paths solely for uninstall cleanup. The primary executable is a universal Mach-O containing x86_64 and arm64; both slices use the hardened runtime, identifier com.ccswitch.desktop, a Developer ID Application certificate for Jason Young (R8UR22V2F9), an s8ch version-1 notarization ticket container and a g8tk trailer. Local Linux parsing does not claim an online Gatekeeper verdict. macOS 12 or newer is required. The app manages user configuration for Claude Code, Codex, Gemini CLI, OpenCode and similar tools, reading and writing ~/.cc-switch and tool-specific configuration directories. Local routing is disabled by default and defaults to loopback 127.0.0.1:15721. ccswitch:// imports enter a pending confirmation queue; icon fetching permits only public HTTPS URLs on default ports and rejects loopback, intranet and credential URLs, while ordinary external links are restricted to HTTP/HTTPS/mailto and reject javascript:, file:, data: and custom schemes. Terminal, shell, osascript and managed CLI install/update paths are user-triggered app features. Login-item creation and update download are off by default and are not enabled by copying the cask; Tauri updates are minisign-verified, retained in memory and installed only after user confirmation. Info.plist declares local-network usage only and no camera or microphone usage. Manifest review found no common third-party analytics SDK; the app still provides local usage/cost statistics and optional sync, so it is not described as having no telemetry at all.",
            "ja": "このエントリは安定版 v4.0.34 に対応します。GitHub リリースはドラフトでもプレリリースでもなく、v4.0.4 は 4.0 系初の安定リリースとして文書化されています。Homebrew cask の DMG SHA-256 は GitHub アセットと一致し、イメージには CC Switch.app、Applications へのドラッグインストール用リンク、背景素材だけが含まれます。cask に formula 依存、競合、caveat はなく、アプリのコピーだけを行い、zap パスはアンインストール時のクリーンアップ専用です。主要実行ファイルは x86_64 と arm64 を含む universal Mach-O で、両スライスとも Hardened Runtime を有効化し、識別子は com.ccswitch.desktop、Developer ID Application 証明書は Jason Young (R8UR22V2F9)、s8ch バージョン 1 の公証チケットコンテナと g8tk トレーラーを含みます。Linux 上のローカル解析では、オンラインの Gatekeeper 判定は主張しません。macOS 12 以降が必要です。アプリは Claude Code、Codex、Gemini CLI、OpenCode などのユーザー設定を管理し、~/.cc-switch および各ツールの設定ディレクトリを読み書きします。ローカルルーティングは既定で無効、リッスンアドレスは既定でループバック 127.0.0.1:15721 です。ccswitch:// インポートは確認待ちキューに入ります。アイコン取得は既定ポートのパブリック HTTPS だけを許可してループバック、イントラネット、認証情報付き URL を拒否し、通常の外部リンクは HTTP/HTTPS/mailto に制限され、javascript:、file:、data:、カスタムスキームは拒否されます。ターミナル、シェル、osascript、管理対象 CLI のインストール/更新は、すべてユーザーが UI から起動する機能です。ログイン項目と更新ダウンロードは既定でオフであり、cask のコピーでは有効化されません。Tauri 更新は minisign で検証され、メモリーに保持され、インストールはユーザー確認後に限られます。Info.plist にはローカルネットワーク用途だけが宣言され、カメラやマイクの用途文字列はありません。マニフェスト審査では一般的なサードパーティ分析 SDK は見つかりませんでしたが、アプリ自体はローカルの使用量/コスト統計と任意同期を備えるため、一切のテレメトリーがないとは表現しません。",
            "ko": "이 항목은 안정 v4.0.34에 해당합니다. GitHub 릴리스는 초안도 사전 릴리스도 아니며, v4.0.4는 4.0 계열의 첫 안정 릴리스로 문서화되어 있습니다. Homebrew cask DMG SHA-256은 GitHub 자산과 일치하고 이미지에는 CC Switch.app, Applications 드래그 설치 링크, 배경 자산만 포함됩니다. cask에는 formula 종속, 충돌, caveat가 없고 앱 복사만 수행하며 zap 경로는 제거 정리에만 사용됩니다. 주 실행 파일은 x86_64와 arm64를 포함하는 universal Mach-O입니다. 두 슬라이스 모두 Hardened Runtime을 사용하고 식별자는 com.ccswitch.desktop이며, Developer ID Application 인증서는 Jason Young (R8UR22V2F9)이고 s8ch 버전 1 공증 티켓 컨테이너와 g8tk 트레일러를 포함합니다. Linux 로컬 파싱은 온라인 Gatekeeper 평가 결과를 주장하지 않습니다. macOS 12 이상이 필요합니다. 앱은 Claude Code, Codex, Gemini CLI, OpenCode 등 도구의 사용자 구성을 관리하고 ~/.cc-switch 및 도구별 구성 디렉터리를 읽고 씁니다. 로컬 라우팅은 기본적으로 비활성화되고 기본 수신 주소는 루프백 127.0.0.1:15721입니다. ccswitch:// 가져오기는 확인 대기 큐에 들어갑니다. 아이콘 가져오기는 기본 포트의 공개 HTTPS만 허용하고 루프백, 인트라넷, 자격 증명 URL을 거부하며, 일반 외부 링크는 HTTP/HTTPS/mailto로 제한되고 javascript:, file:, data: 및 사용자 지정 스킴은 거부됩니다. 터미널, 셸, osascript, 관리 CLI 설치/업데이트 경로는 모두 사용자가 UI에서 트리거하는 기능입니다. 로그인 항목 생성과 업데이트 다운로드는 기본적으로 꺼져 있고 cask 복사로 활성화되지 않습니다. Tauri 업데이트는 minisign 서명을 검증하고 메모리에 보관하며 사용자 확인 후에만 설치됩니다. Info.plist에는 로컬 네트워크 사용만 선언되어 있고 카메라나 마이크 사용 문자열은 없습니다. 매니페스트 검토에서 일반적인 서드파티 분석 SDK는 발견되지 않았지만, 앱 자체는 로컬 사용량/비용 통계와 선택 동기화를 제공하므로 텔레메트리가 전혀 없다고 설명하지 않습니다.",
        },
        "windowsExtra": {
            "zh-CN": "Windows 路由使用精确 winget 包 farion1231.CC-Switch；当前评审清单 4.0.25 包含 x64 与 arm64 两个 per-user WiX MSI，InstallScope=perUser、InstallPrivileges=limited，默认装入 %LocalAppData%\\Programs\\CC Switch，不需要管理员。winget 是动态包管理器路由，因此其版本可滞后于已评审的 macOS cask v4.0.34。MSI 写入 HKCU 协议、开始菜单快捷方式和可选桌面快捷方式；交互式退出对话框的启动复选框只在 UI 安装路径出现，winget 静默安装不执行该启动动作，也不注册登录项。应用级自动更新默认关闭，更新包验签后暂存内存并等待用户确认。ccswitch:// 配置导入需要前端确认；本地路由默认关闭并使用 127.0.0.1:15721。读写 AI 编程 CLI 配置、启动终端或执行受管 CLI 安装脚本都来自用户操作，不是 winget 安装期动作。清单审查未发现常见第三方分析 SDK，但应用保留本地用量统计与可选同步。",
            "zh-TW": "Windows 路由使用精確 winget 套件 farion1231.CC-Switch；目前評審清單 4.0.25 包含 x64 與 arm64 兩個 per-user WiX MSI，InstallScope=perUser、InstallPrivileges=limited，預設安裝至 %LocalAppData%\\Programs\\CC Switch，不需要系統管理員。winget 是動態套件管理器路由，因此其版本可落後於已評審的 macOS cask v4.0.34。MSI 寫入 HKCU 協定、開始功能表捷徑和可選桌面捷徑；互動式結束對話框的啟動核取方塊只在 UI 安裝路徑出現，winget 靜默安裝不執行該啟動動作，也不註冊登入項目。應用層級自動更新預設關閉，更新包驗簽後暫存於記憶體並等待使用者確認。ccswitch:// 設定匯入需要前端確認；本機路由預設關閉並使用 127.0.0.1:15721。讀寫 AI 編程 CLI 設定、啟動終端機或執行受管 CLI 安裝指令碼都來自使用者操作，不是 winget 安裝期動作。清單審查未發現常見第三方分析 SDK，但應用保留本機用量統計與可選同步。",
            "en": "The Windows route uses the exact winget package farion1231.CC-Switch. The reviewed 4.0.25 manifest contains x64 and arm64 per-user WiX MSIs with InstallScope=perUser and InstallPrivileges=limited, installed by default under %LocalAppData%\\Programs\\CC Switch without administrator rights. Winget is a dynamic package-manager route, so its version can lag the reviewed macOS cask v4.0.34. The MSI writes an HKCU protocol handler, Start Menu shortcuts and an optional desktop shortcut. The launch checkbox on the interactive exit dialog belongs only to the UI installation path; a silent winget installation does not run that launch action and registers no login item. App-level automatic update download is off by default; verified update packages are retained in memory and installation waits for user confirmation. ccswitch:// configuration imports require frontend confirmation, while local routing is disabled by default and uses 127.0.0.1:15721. Reading or writing AI coding CLI configuration, opening terminals and running managed CLI install scripts are user-triggered features, not winget install-time actions. Manifest review found no common third-party analytics SDK, but the app retains local usage statistics and optional sync.",
            "ja": "Windows ルートは正確な winget パッケージ farion1231.CC-Switch を使用します。レビュー済みの 4.0.25 マニフェストには、x64 と arm64 の per-user WiX MSI が含まれ、InstallScope=perUser、InstallPrivileges=limited、既定インストール先は %LocalAppData%\\Programs\\CC Switch で、管理者権限は不要です。winget は動的なパッケージマネージャールートなので、そのバージョンはレビュー済み macOS cask v4.0.34 より古い場合があります。MSI は HKCU プロトコルハンドラー、スタートメニューショートカット、任意のデスクトップショートカットを書き込みます。対話インストールの終了ダイアログにある起動チェックボックスは UI 経路だけのもので、winget のサイレントインストールではその起動アクションを実行せず、ログイン項目も登録しません。アプリの自動更新ダウンロードは既定でオフであり、検証済み更新はメモリーに保持され、インストールはユーザー確認を待ちます。ccswitch:// 設定インポートにはフロントエンドの確認が必要で、ローカルルーティングは既定で無効、127.0.0.1:15721 を使用します。AI コーディング CLI 設定の読み書き、ターミナル起動、管理 CLI インストールスクリプトの実行は、winget のインストール時動作ではなく、ユーザーが起動する機能です。マニフェスト審査では一般的なサードパーティ分析 SDK は見つかりませんでしたが、アプリにはローカル使用量統計と任意同期があります。",
            "ko": "Windows 경로는 정확한 winget 패키지 farion1231.CC-Switch을 사용합니다. 검토된 4.0.25 매니페스트에는 x64 및 arm64 per-user WiX MSI가 포함되고 InstallScope=perUser, InstallPrivileges=limited이며 기본 위치는 %LocalAppData%\\Programs\\CC Switch이고 관리자 권한이 필요 없습니다. winget은 동적 패키지 관리자 경로이므로 버전이 검토된 macOS cask v4.0.34보다 뒤처질 수 있습니다. MSI는 HKCU 프로토콜 처리기, 시작 메뉴 바로 가기, 선택적 바탕 화면 바로 가기를 작성합니다. 대화형 종료 대화상자의 시작 체크박스는 UI 설치 경로에만 속하며 winget 자동 설치는 해당 시작 동작을 실행하지 않고 로그인 항목도 등록하지 않습니다. 앱 수준 자동 업데이트 다운로드는 기본적으로 꺼져 있고, 검증된 업데이트 패키지는 메모리에 보관되며 설치는 사용자 확인을 기다립니다. ccswitch:// 구성 가져오기는 프런트엔드 확인이 필요하고 로컬 라우팅은 기본적으로 비활성화되며 127.0.0.1:15721을 사용합니다. AI 코딩 CLI 구성 읽기/쓰기, 터미널 열기, 관리 CLI 설치 스크립트 실행은 winget 설치 시 동작이 아니라 사용자가 트리거하는 기능입니다. 매니페스트 검토에서 일반적인 서드파티 분석 SDK는 발견되지 않았지만 앱에는 로컬 사용량 통계와 선택 동기화가 유지됩니다.",
        },
    },
    "open-design": {
        "name": "Open Design", "publisher": "nexu-io", "category": "ai", "mark": "OD", "accent": "#7c5cff",
        "repo": "nexu-io/open-design", "license": "Apache-2.0", "licenseFile": "LICENSE", "licenseRef": "open-design-v0.27.1/LICENSE",
        "homepage": "https://open-design.ai/", "cask": "open-design", "caskArchitectures": ["x86_64", "aarch64"],
        "summary": {
            "zh-CN": "本地优先的协作式 AI 设计代理工作空间，借助本机或自选编码代理生成原型、幻灯片、图像和视频，并管理设计系统、插件与技能。",
            "zh-TW": "本地優先的協作式 AI 設計代理工作空間，藉由本機或自選編碼代理生成原型、簡報、影像和影片，並管理設計系統、外掛與技能。",
            "en": "A local-first collaborative AI design-agent workspace for prototypes, slides, images and video, using local or user-selected coding agents with design systems, plugins and skills.",
            "ja": "ローカルファーストの協調型 AI デザインエージェントワークスペースです。ローカルまたはユーザーが選択したコーディングエージェントを使い、プロトタイプ、スライド、画像、動画を作成し、デザインシステム、プラグイン、スキルを管理します。",
            "ko": "로컬 우선의 협업형 AI 디자인 에이전트 워크스페이스로, 로컬 또는 사용자가 선택한 코딩 에이전트로 프로토타입, 슬라이드, 이미지, 영상을 만들고 디자인 시스템, 플러그인, 스킬을 관리합니다.",
        },
        "extra": {
            "zh-CN": "本条目评审稳定标签 open-design-v0.27.1；GitHub release 为非草稿、非预发布，稳定通道标识为 io.open-design.desktop，稳定提升流程会拒绝草稿、预发布和回退版本。Homebrew cask open-design 当前未停用或弃用，无 formula 依赖、冲突、caveat、安装脚本或 postflight，只把 Open Design.app 复制到 /Applications，zap 路径仅用于卸载清理；需要 macOS 12 或更高版本。两个 DMG 是分别构建的瘦包：arm64 镜像内 19 个 Mach-O 全部为 arm64，x64 镜像内 19 个全部为 x86_64，架构判断来自 Mach-O 头而非文件名；cask 摘要与 GitHub 资产一致。全部 Mach-O 均含 LC_CODE_SIGNATURE、启用 hardened runtime、Team ID 为 236R69AWW2，Developer ID Application 叶子证书为 Wei Huang (236R69AWW2)。7 个可执行文件包含 XML 与 DER entitlement，两者内容一致，只有 com.apple.security.cs.allow-jit、com.apple.security.cs.allow-unsigned-executable-memory 和 com.apple.security.cs.disable-library-validation 三个 true 键。App 内含 s8ch v1 notarization ticket 容器和 g8tk trailer；g8tk 中 8 个 CodeDirectory hash 覆盖主要 framework 与 Helper，Linux 本地解析不宣称在线 Gatekeeper 评估结果。Info.plist 声明摄像头、麦克风、音频采集和蓝牙使用说明，并注册 opendesign 方案，但未发现屏幕录制使用说明；这些 TCC 字符串不等于安装时已授权，嵌入 entitlement 也没有相机、麦克风或 sandbox 键。打包版会通过内置 Electron Helper 的 Node 模式启动受监督的本地 daemon 与 web sidecar，使用临时本地端口、子进程环境 allowlist 和受限重启；默认数据、缓存、日志、runtime、更新和 Electron 会话位于 Electron userData/namespaces/<namespace>/ 下。外部 opendesign:// 只接受固定的 workspace/invite/continue 与无负载 workspace/open，邀请继续仅向本地 daemon 发送 nonce；内部 od://app 仅代理到本地 web sidecar，子窗口限制为 od、blob 与 about:blank，内嵌浏览器允许 HTTP(S)/about:blank 并拒绝 file、javascript、data 导航。打包版默认启用自定义更新器，稳定元数据位于 releases.open-design.ai，自动检查和自动下载开启、自动打开关闭，稳定通道每 6 小时轮询；cask 的 auto_updates 与该运行时行为一致。官方构建包含可在首跑横幅和 Settings → Privacy 中关闭的 PostHog 产品分析/质量追踪，以及在配置遥测目标时始终启用、经过脱敏且有边界的安全可靠性遥测；BYOK 密钥和生成文件内容不作为遥测收集，崩溃 minidump 默认留在本机，仅在用户主动导出支持包时提供。本条目只收录 macOS Homebrew cask；没有精确 winget ID 或 Flathub ref，0.27.1 也没有 Linux 预构建产物，源码运行不是受支持安装路由；同版 Windows x64 EXE 尚未通过本目录要求的签名 Windows 路由核验，因此不创建 Windows 变体。",
            "zh-TW": "本條目評審穩定標籤 open-design-v0.27.1；GitHub release 為非草稿、非預先發布，穩定通道識別碼為 io.open-design.desktop，穩定提升流程會拒絕草稿、預發布和回溯版本。Homebrew cask open-design 目前未停用或棄用，無 formula 相依性、衝突、caveat、安裝指令碼或 postflight，只把 Open Design.app 複製到 /Applications，zap 路徑僅用於解除安裝清理；需要 macOS 12 或更新版本。兩個 DMG 是分別建置的瘦包：arm64 映像檔內 19 個 Mach-O 全部為 arm64，x64 映像檔內 19 個全部為 x86_64，架構判斷來自 Mach-O 標頭而非檔名；cask 摘要與 GitHub 資產一致。所有 Mach-O 均含 LC_CODE_SIGNATURE、啟用 hardened runtime、Team ID 為 236R69AWW2，Developer ID Application 葉子憑證為 Wei Huang (236R69AWW2)。7 個可執行檔包含 XML 與 DER entitlement，兩者內容一致，只有 com.apple.security.cs.allow-jit、com.apple.security.cs.allow-unsigned-executable-memory 和 com.apple.security.cs.disable-library-validation 三個 true 鍵。App 內含 s8ch v1 notarization ticket 容器與 g8tk trailer；g8tk 中 8 個 CodeDirectory hash 覆蓋主要 framework 與 Helper，Linux 本機解析不宣稱線上 Gatekeeper 評估結果。Info.plist 宣告攝影機、麥克風、音訊擷取和藍牙使用說明，並註冊 opendesign 方案，但未發現螢幕錄製使用說明；這些 TCC 字串不等於安裝時已授權，嵌入 entitlement 也沒有攝影機、麥克風或 sandbox 鍵。打包版會透過內建 Electron Helper 的 Node 模式啟動受監督的本機 daemon 與 web sidecar，使用暫時本機連接埠、子程序環境 allowlist 與受限重啟；預設資料、快取、記錄、runtime、更新和 Electron 工作階段位於 Electron userData/namespaces/<namespace>/ 下。外部 opendesign:// 只接受固定的 workspace/invite/continue 與無承載 workspace/open，邀請繼續僅向本機 daemon 傳送 nonce；內部 od://app 僅代理到本機 web sidecar，子視窗限制為 od、blob 與 about:blank，內嵌瀏覽器允許 HTTP(S)/about:blank 並拒絕 file、javascript、data 導覽。打包版預設啟用自訂更新程式，穩定中繼資料位於 releases.open-design.ai，自動檢查和自動下載開啟、自動開啟關閉，穩定通道每 6 小時輪詢；cask 的 auto_updates 與該執行階段行為一致。官方建置包含可在首跑橫幅和 Settings → Privacy 中關閉的 PostHog 產品分析/品質追蹤，以及在設定遙測目標時始終啟用、經過脱敏且有邊界的安全可靠性遙測；BYOK 金鑰和生成檔案內容不作為遙測收集，崩潰 minidump 預設留在本機，僅在使用者主動匯出支援包時提供。本條目只收錄 macOS Homebrew cask；沒有精確 winget ID 或 Flathub ref，0.27.1 也沒有 Linux 預建構產物，原始碼執行不是受支援安裝路由；同版 Windows x64 EXE 尚未通過本目錄要求的簽章 Windows 路由核驗，因此不建立 Windows 變體。",
            "en": "This entry reviews the stable tag open-design-v0.27.1. The GitHub release is neither a draft nor a prerelease, the stable identity is io.open-design.desktop, and the stable-promotion workflow rejects drafts, prereleases and regressed versions. The active, non-deprecated Homebrew cask open-design has no formula dependency, conflict, caveat, installer script or postflight; it only copies Open Design.app to /Applications, while zap paths are uninstall cleanup. macOS 12 or newer is required. The two DMGs are separate thin builds: all 19 Mach-O files in the arm64 image are arm64 and all 19 in the x64 image are x86_64, based on Mach-O headers rather than filenames; cask digests match the GitHub assets. Every Mach-O has LC_CODE_SIGNATURE, hardened runtime, Team ID 236R69AWW2 and Developer ID Application leaf certificate Wei Huang (236R69AWW2). Seven executables carry identical XML and DER entitlements containing only com.apple.security.cs.allow-jit, com.apple.security.cs.allow-unsigned-executable-memory and com.apple.security.cs.disable-library-validation set to true. The app contains an s8ch version-1 notarization ticket container and a g8tk trailer; eight CodeDirectory hashes in g8tk cover the main frameworks and helpers. Local Linux parsing does not claim an online Gatekeeper verdict. Info.plist declares camera, microphone, audio-capture and Bluetooth usage strings and registers the opendesign scheme, but no screen-capture usage string was found. Those TCC strings are not installation-time grants, and the embedded entitlements contain no camera, microphone or sandbox key. Packaged builds start supervised local daemon and web sidecars through the bundled Electron Helper in Node mode, using ephemeral local ports, a child-process environment allowlist and bounded restarts. Default data, cache, logs, runtime, update and Electron session paths live under Electron userData/namespaces/<namespace>/. External opendesign:// links accept only the fixed workspace/invite/continue shape or payload-free workspace/open; invite continuation POSTs only the nonce to the local daemon. The internal od://app scheme is proxied only to the local web sidecar; child windows are limited to od, blob and about:blank, while the embedded browser allows HTTP(S)/about:blank and rejects file, javascript and data navigation. Packaged builds enable the custom updater by default, with stable metadata on releases.open-design.ai, auto-check and auto-download enabled, auto-open disabled and a six-hour stable poll interval; the cask's auto_updates setting is consistent with this runtime. Official builds include opt-out PostHog product analytics and quality traces via the first-run banner and Settings → Privacy, plus always-enabled bounded and scrubbed safety/reliability telemetry when a telemetry destination is configured. BYOK keys and generated artifact contents are not collected as telemetry; crash minidumps remain local unless the user initiates a support bundle. This entry contains only the macOS Homebrew cask. There is no exact winget ID or Flathub ref, no prebuilt Linux artifact in 0.27.1, and source-only execution is not a supported install route. The co-released Windows x64 EXE has not passed the catalog's signed Windows-route verification, so no Windows variant is created.",
            "ja": "このエントリは安定タグ open-design-v0.27.1 を審査しています。GitHub リリースはドラフトでもプレリリースでもなく、安定版 ID は io.open-design.desktop で、安定版昇格ワークフローはドラフト、プレリリース、バージョン退行を拒否します。有効で廃止されていない Homebrew cask open-design に formula 依存、競合、caveat、インストーラースクリプト、postflight はなく、Open Design.app を /Applications にコピーするだけです。zap パスはアンインストール時のクリーンアップ専用で、macOS 12 以降が必要です。2 つの DMG は別々の thin ビルドで、arm64 イメージの 19 個の Mach-O はすべて arm64、x64 イメージの 19 個はすべて x86_64 です。判定はファイル名ではなく Mach-O ヘッダーに基づき、cask のダイジェストは GitHub アセットと一致します。すべての Mach-O に LC_CODE_SIGNATURE と Hardened Runtime があり、Team ID は 236R69AWW2、Developer ID Application のリーフ証明書は Wei Huang (236R69AWW2) です。7 つの実行可能ファイルには XML と DER の同一の entitlement があり、true のキーは com.apple.security.cs.allow-jit、com.apple.security.cs.allow-unsigned-executable-memory、com.apple.security.cs.disable-library-validation の 3 つだけです。アプリには s8ch バージョン 1 の公証チケットコンテナと g8tk トレーラーが含まれ、g8tk 内の 8 個の CodeDirectory ハッシュが主要 framework と Helper を対象にしています。Linux 上のローカル解析ではオンラインの Gatekeeper 判定を主張しません。Info.plist にはカメラ、マイク、オーディオキャプチャ、Bluetooth の利用目的文字列と opendesign スキーム登録がありますが、画面収集の利用目的文字列はありません。これらの TCC 文字列はインストール時の許可を意味せず、埋め込み entitlement にもカメラ、マイク、sandbox キーはありません。パッケージ版は同梱 Electron Helper を Node モードで使い、監督下のローカル daemon と web sidecar を起動します。ローカルの一時ポート、子プロセス環境の allowlist、制限付き再起動を使用し、既定のデータ、キャッシュ、ログ、ランタイム、更新、Electron セッションは Electron userData/namespaces/<namespace>/ 配下です。外部 opendesign:// は固定の workspace/invite/continue 形式、またはペイロードを持たない workspace/open だけを受け入れ、招待の継続は nonce だけをローカル daemon に POST します。内部 od://app はローカル web sidecar だけにプロキシされ、子ウィンドウは od、blob、about:blank に制限され、埋め込みブラウザーは HTTP(S)/about:blank を許可して file、javascript、data ナビゲーションを拒否します。パッケージ版のカスタム更新機能は既定で有効で、安定メタデータは releases.open-design.ai、自動チェックと自動ダウンロードはオン、自動オープンはオフ、安定チャネルのポーリング間隔は 6 時間です。cask の auto_updates はこの動作と整合します。公式ビルドには、初回起動バナーと Settings → Privacy で無効にできる PostHog 製品分析・品質トレースと、テレメトリ送信先が設定されたビルドで常時有効な、境界付きでスクラブ済みの安全性・信頼性テレメトリがあります。BYOK キーと生成ファイルの内容はテレメトリとして収集されず、クラッシュ minidump はユーザーがサポートバンドルを開始しない限りローカルに残ります。このエントリは macOS Homebrew cask のみです。正確な winget ID と Flathub ref はなく、0.27.1 に Linux の事前ビルド成果物はなく、ソースからの実行はサポートされるインストール経路ではありません。同時リリースの Windows x64 EXE は、カタログが要求する署名済み Windows 経路の検証を終えていないため Windows 変体を作成しません。",
            "ko": "이 항목은 안정 태그 open-design-v0.27.1을 검토했습니다. GitHub 릴리스는 초안도 사전 릴리스도 아니고 안정 채널 ID는 io.open-design.desktop이며, 안정 승급 워크플로는 초안, 사전 릴리스, 버전 회귀를 거부합니다. 활성 상태이고 폐기되지 않은 Homebrew cask open-design에는 formula 종속, 충돌, caveat, 설치 스크립트, postflight가 없고 Open Design.app을 /Applications로 복사만 합니다. zap 경로는 제거 정리 전용이며 macOS 12 이상이 필요합니다. 두 DMG는 별도의 thin 빌드로, arm64 이미지의 Mach-O 19개는 모두 arm64이고 x64 이미지의 19개는 모두 x86_64입니다. 아키텍처는 파일 이름이 아니라 Mach-O 헤더로 판단했고 cask 다이제스트는 GitHub 자산과 일치합니다. 모든 Mach-O에 LC_CODE_SIGNATURE와 Hardened Runtime이 있고 Team ID는 236R69AWW2, Developer ID Application 리프 인증서는 Wei Huang (236R69AWW2)입니다. 실행 파일 7개에는 XML과 DER로 동일한 entitlement가 들어 있고 true 키는 com.apple.security.cs.allow-jit, com.apple.security.cs.allow-unsigned-executable-memory, com.apple.security.cs.disable-library-validation 세 개뿐입니다. 앱에는 s8ch 버전 1 공증 티켓 컨테이너와 g8tk 트레일러가 포함되고, g8tk의 CodeDirectory 해시 8개가 주요 framework와 Helper를 커버합니다. Linux 로컬 파싱은 온라인 Gatekeeper 평가 결과를 주장하지 않습니다. Info.plist에는 카메라, 마이크, 오디오 캡처, Bluetooth 사용 설명 문자열과 opendesign 스킴 등록이 있지만 화면 녹화 사용 설명은 없습니다. 이 TCC 문자열이 설치 시점 권한 부여를 의미하지는 않으며, 임베디드 entitlement에도 카메라, 마이크, sandbox 키가 없습니다. 패키지 빌드는 번들된 Electron Helper를 Node 모드로 사용해 감독되는 로컬 daemon과 web sidecar를 시작하고 임시 로컬 포트, 자식 프로세스 환경 allowlist, 제한된 재시작을 사용합니다. 기본 데이터, 캐시, 로그, 런타임, 업데이트, Electron 세션 경로는 Electron userData/namespaces/<namespace>/ 아래에 있습니다. 외부 opendesign://는 고정된 workspace/invite/continue 형식 또는 페이로드 없는 workspace/open만 허용하고, 초대 계속은 nonce만 로컬 daemon에 POST합니다. 내부 od://app 스킴은 로컬 web sidecar로만 프록시되고, 자식 창은 od, blob, about:blank로 제한되며 임베디드 브라우저는 HTTP(S)/about:blank를 허용하고 file, javascript, data 탐색을 거부합니다. 패키지 빌드의 사용자 지정 업데이터는 기본적으로 활성화되고 안정 메타데이터는 releases.open-design.ai에 있으며, 자동 확인과 자동 다운로드는 켜지고 자동 열기는 꺼지며 안정 채널 폴링 주기는 6시간입니다. cask의 auto_updates 설정은 이 런타임 동작과 일치합니다. 공식 빌드에는 첫 실행 배너와 Settings → Privacy에서 끌 수 있는 PostHog 제품 분석 및 품질 추적과, 원격 텔레메트리 대상이 구성된 빌드에서 항상 활성화되고 제한·마스킹된 안전 및 신뢰성 텔레메트리가 있습니다. BYOK 키와 생성한 아티팩트 내용은 텔레메트리로 수집되지 않고, 크래시 minidump는 사용자가 지원 번들 내보내기를 시작하기 전까지 로컬에 남습니다. 이 항목에는 macOS Homebrew cask만 포함됩니다. 정확한 winget ID나 Flathub ref가 없고 0.27.1에는 Linux 사전 빌드 자산이 없으며, 소스 실행은 지원되는 설치 경로가 아닙니다. 함께 릴리스된 Windows x64 EXE는 카탈로그 요구 사항인 서명 Windows 경로 검증을 통과하지 않았으므로 Windows 변형을 만들지 않습니다.",
        },
    },
    "cherry-studio": {
        "name": "Cherry Studio", "publisher": "CherryHQ", "category": "ai", "mark": "CS", "accent": "#d35870",
        "repo": "CherryHQ/cherry-studio", "license": "AGPL-3.0-only", "licenseFile": "LICENSE",
        "homepage": "https://www.cherry-ai.com/", "cask": "cherry-studio",
        "summary": {
            "zh-CN": "多模型 AI 桌面客户端，支持对话、知识库、图像生成与可配置自动化工具。",
            "zh-TW": "多模型 AI 桌面用戶端，支援對話、知識庫、圖像生成與可設定自動化工具。",
            "en": "A multi-model AI desktop client for conversations, knowledge bases, image generation and configurable automation tools.",
            "ja": "会話、ナレッジベース、画像生成、設定可能な自動化ツールに対応するマルチモデル AI デスクトップクライアント。",
            "ko": "대화, 지식 베이스, 이미지 생성 및 구성 가능한 자동화 도구를 지원하는 멀티 모델 AI 데스크톱 클라이언트.",
        },
        "extra": {
            "zh-CN": "需要 macOS 13 或更高版本。模型服务通常需要账号、API 密钥或付费；自动化工具可能访问文件、浏览器或执行命令，需逐项授权。",
            "zh-TW": "需要 macOS 13 或更高版本。模型服務通常需要帳號、API 金鑰或付費；自動化工具可能存取檔案、瀏覽器或執行命令，需逐項授權。",
            "en": "Requires macOS 13 or later. Model services generally require accounts, API keys or payment. Automation tools may access files or browsers or execute commands and require explicit authorization.",
            "ja": "macOS 13 以降が必要です。モデルサービスには通常、アカウント、API キー、料金が必要です。自動化ツールはファイルやブラウザーへのアクセス、コマンド実行を行う場合があり、個別の許可が必要です。",
            "ko": "macOS 13 이상이 필요합니다. 모델 서비스에는 일반적으로 계정, API 키 또는 결제가 필요합니다. 자동화 도구는 파일이나 브라우저에 접근하거나 명령을 실행할 수 있으므로 명시적 권한 부여가 필요합니다.",
        },
    },
    "freecad": {
        "name": "FreeCAD", "publisher": "FreeCAD contributors", "category": "education", "mark": "FC", "accent": "#5579a4",
        "repo": "FreeCAD/FreeCAD", "license": "LGPL-2.0-or-later", "licenseFile": "src/Doc/LICENSE.html",
        "licenseRef": "1.1.4/src/Doc/LICENSE.html",
        "homepage": "https://www.freecad.org/", "cask": "freecad",
        "summary": {
            "zh-CN": "开源参数化三维 CAD 建模工具，适用于机械设计、工程制图与可定制建模工作流。",
            "zh-TW": "開源參數化三維 CAD 建模工具，適用於機械設計、工程製圖與可自訂建模工作流程。",
            "en": "An open-source parametric 3D CAD modeler for mechanical design, engineering drawings and customizable workflows.",
            "ja": "機械設計、工学図面、カスタマイズ可能なワークフローに対応するオープンソースのパラメトリック 3D CAD モデラー。",
            "ko": "기계 설계, 엔지니어링 도면 및 사용자 정의 작업 흐름을 지원하는 오픈 소스 파라메트릭 3D CAD 모델러.",
        },
        "extra": {
            "zh-CN": "核心安装与使用不要求账号或订阅。应用按 LGPL2+ 分发；随附第三方库与素材另有独立许可证声明。",
            "zh-TW": "核心安裝與使用不要求帳號或訂閱。應用程式按 LGPL2+ 分發；隨附第三方函式庫與素材另有獨立授權聲明。",
            "en": "Core installation and use require no account or subscription. The application uses LGPL2+; bundled third-party libraries and assets have separate notices.",
            "ja": "基本的なインストールと使用にアカウントやサブスクリプションは不要です。アプリは LGPL2+ で配布され、同梱の第三者ライブラリと素材には別の通知があります。",
            "ko": "기본 설치와 사용에 계정이나 구독이 필요하지 않습니다. 애플리케이션은 LGPL2+로 배포되며, 번들 타사 라이브러리와 자산에는 별도의 고지가 적용됩니다.",
        },
    },
    "siyuan": {
        "name": "SiYuan", "publisher": "Yunnan Liandi Technology Co., Ltd.", "category": "office", "mark": "SY", "accent": "#5684ab",
        "repo": "siyuan-note/siyuan", "license": "AGPL-3.0-or-later", "licenseFile": "LICENSE",
        "licenseRef": "v3.8.10/LICENSE",
        "homepage": "https://b3log.org/siyuan/en/", "cask": "siyuan",
        "summary": {
            "zh-CN": "重视隐私的本地优先个人知识管理工具，支持块级引用、双向链接和文档组织。",
            "zh-TW": "重視隱私的本機優先個人知識管理工具，支援區塊級參照、雙向連結和文件組織。",
            "en": "A privacy-focused, local-first personal knowledge manager with block references, backlinks and document organization.",
            "ja": "ブロック参照、バックリンク、文書整理に対応するプライバシー重視のローカルファースト個人ナレッジマネージャー。",
            "ko": "블록 참조, 백링크 및 문서 구성을 지원하는 개인정보 보호 중심의 로컬 우선 개인 지식 관리 도구.",
        },
        "extra": {
            "zh-CN": "需要 macOS 12 或更高版本。本地基本功能免费；官方同步、会员功能需要账号/订阅，第三方 S3 或 WebDAV 存储需自行配置并可能付费。",
            "zh-TW": "需要 macOS 12 或更高版本。本機基本功能免費；官方同步、會員功能需要帳號／訂閱，第三方 S3 或 WebDAV 儲存需自行設定並可能付費。",
            "en": "Requires macOS 12 or later. Basic local features are free. Official sync and member features require an account/subscription; third-party S3 or WebDAV storage must be configured separately and may cost money.",
            "ja": "macOS 12 以降が必要です。基本的なローカル機能は無料です。公式同期とメンバー機能にはアカウント・サブスクリプションが必要で、第三者の S3 や WebDAV ストレージは別途設定・契約が必要です。",
            "ko": "macOS 12 이상이 필요합니다. 기본 로컬 기능은 비용 없이 사용할 수 있으며 공식 동기화와 멤버 기능에는 계정/구독이 필요합니다. 타사 S3 또는 WebDAV 저장소는 별도로 구성해야 하며 비용이 발생할 수 있습니다.",
        },
    },
    "keepassxc": {
        "name": "KeePassXC", "publisher": "KeePassXC Team", "category": "security", "mark": "K", "accent": "#6bac4d",
        "repo": "keepassxreboot/keepassxc", "license": "GPL-2.0-only OR GPL-3.0-only", "licenseFile": "COPYING",
        "homepage": "https://keepassxc.org/", "cask": "keepassxc", "winget": "KeePassXCTeam.KeePassXC", "wingetVersion": "2.8.0",
        "windowsRequiresAdmin": True,
        "windowsExtra": {
            "zh-CN": "Windows 使用机器范围 MSI，需要管理员安装，并按架构安装相应 Visual C++ 运行库。浏览器、SSH Agent 与自动输入集成需另行授权；安装不会解锁数据库。",
            "zh-TW": "Windows 使用機器範圍 MSI，需要管理員安裝，並按架構安裝相應 Visual C++ 執行階段程式庫。瀏覽器、SSH Agent 與自動輸入整合需另行授權；安裝不會解鎖資料庫。",
            "en": "Windows uses a machine-scope MSI requiring administrator installation and the architecture-matching Visual C++ runtime. Browser, SSH Agent and Auto-Type integrations need separate authorization; installation does not unlock databases.",
            "ja": "Windows では管理者によるマシンスコープ MSI インストールと、アーキテクチャに対応する Visual C++ ランタイムが必要です。ブラウザー、SSH Agent、自動入力連携は個別の許可が必要で、インストール時にデータベースを解除しません。",
            "ko": "Windows에서는 관리자 권한의 머신 범위 MSI 설치와 아키텍처에 맞는 Visual C++ 런타임이 필요합니다. 브라우저, SSH Agent 및 자동 입력 통합에는 별도의 권한이 필요하며 설치 시 데이터베이스를 잠금 해제하지 않습니다.",
        },
        "summary": {
            "zh-CN": "本地密码数据库管理器，支持自动输入、浏览器集成、硬件密钥与 TOTP。",
            "zh-TW": "本機密碼資料庫管理器，支援自動輸入、瀏覽器整合、硬體金鑰與 TOTP。",
            "en": "A local password-database manager with Auto-Type, browser integration, hardware keys and TOTP.",
            "ja": "自動入力、ブラウザー連携、ハードウェアキー、TOTP に対応するローカルパスワードデータベースマネージャー。",
            "ko": "자동 입력, 브라우저 통합, 하드웨어 키 및 TOTP를 지원하는 로컬 암호 데이터베이스 관리자.",
        },
        "extra": {
            "zh-CN": "macOS 需要 12 或更高版本。浏览器、SSH Agent、自动输入与安全密钥集成需另行配置授权；安装不会解锁数据库或处理凭据。",
            "zh-TW": "macOS 需要 12 或更高版本。瀏覽器、SSH Agent、自動輸入與安全金鑰整合需另行設定授權；安裝不會解鎖資料庫或處理憑證。",
            "en": "macOS requires version 12 or later. Browser, SSH Agent, Auto-Type and security-key integrations require separate configuration and authorization; installation does not unlock databases or process credentials.",
            "ja": "macOS 12 以降が必要です。ブラウザー、SSH Agent、自動入力、セキュリティキー連携には個別の設定と許可が必要です。インストール時にデータベースを解除したり認証情報を処理したりしません。",
            "ko": "macOS 12 이상이 필요합니다. 브라우저, SSH Agent, 자동 입력 및 보안 키 통합에는 별도의 구성과 권한이 필요합니다. 설치 시 데이터베이스를 잠금 해제하거나 자격 증명을 처리하지 않습니다.",
        },
    },
    "cryptomator": {
        "name": "Cryptomator", "publisher": "Skymatic GmbH", "category": "security", "mark": "C", "accent": "#49b286",
        "repo": "cryptomator/cryptomator", "license": "GPL-3.0-only", "licenseFile": "LICENSE.txt",
        "homepage": "https://cryptomator.org/", "cask": "cryptomator", "winget": "Cryptomator.Cryptomator", "wingetVersion": "1.19.6",
        "windowsRequiresAdmin": True,
        "windowsExtra": {
            "zh-CN": "Windows 仅提供 x86_64 机器范围 MSI，需要 Windows 10 1809 或更高版本和管理员安装。WinFsp 挂载模式需单独组件；安装不会自动解锁或挂载保险库。",
            "zh-TW": "Windows 僅提供 x86_64 機器範圍 MSI，需要 Windows 10 1809 或更高版本和管理員安裝。WinFsp 掛載模式需單獨元件；安裝不會自動解鎖或掛載保險庫。",
            "en": "Windows provides only an x86_64 machine-scope MSI, requiring Windows 10 1809 or later and administrator installation. WinFsp mounting needs a separate component; installation does not unlock or mount vaults.",
            "ja": "Windows では x86_64 のマシンスコープ MSI のみを提供し、Windows 10 1809 以降と管理者インストールが必要です。WinFsp マウントには別コンポーネントが必要で、インストール時にボールトを解除またはマウントしません。",
            "ko": "Windows에서는 x86_64 머신 범위 MSI만 제공하며 Windows 10 1809 이상과 관리자 설치가 필요합니다. WinFsp 마운트에는 별도 구성 요소가 필요하며 설치 시 볼트를 잠금 해제하거나 마운트하지 않습니다.",
        },
        "summary": {
            "zh-CN": "客户端文件加密工具，为云存储和本地文件创建可解锁的加密保险库。",
            "zh-TW": "用戶端檔案加密工具，為雲端儲存和本機檔案建立可解鎖的加密保險庫。",
            "en": "Client-side file encryption for creating unlockable encrypted vaults for cloud storage and local files.",
            "ja": "クラウドストレージとローカルファイル用のロック解除可能な暗号化ボールトを作成するクライアント側ファイル暗号化ツール。",
            "ko": "클우드 저장소와 로컬 파일용으로 잠금 해제 가능한 암호화 볼트를 생성하는 클라이언트 측 파일 암호화 도구.",
        },
        "extra": {
            "zh-CN": "挂载为虚拟驱动器时，Windows 的 WinFsp、macOS 的 macFUSE 或 Linux 的 FUSE3 可能需单独安装授权；安装不会自动解锁或挂载保险库。",
            "zh-TW": "掛載為虛擬磁碟機時，Windows 的 WinFsp、macOS 的 macFUSE 或 Linux 的 FUSE3 可能需單獨安裝授權；安裝不會自動解鎖或掛載保險庫。",
            "en": "Virtual-drive mounting may require separately installed and authorized WinFsp on Windows, macFUSE on macOS or FUSE3 on Linux. Installation does not unlock or mount vaults.",
            "ja": "仮想ドライブとしてマウントする場合、Windows の WinFsp、macOS の macFUSE、Linux の FUSE3 を個別にインストール・許可する必要があります。インストール時にボールトを解除またはマウントしません。",
            "ko": "가상 드라이브로 마운트하려면 Windows의 WinFsp, macOS의 macFUSE 또는 Linux의 FUSE3를 별도로 설치하고 승인해야 할 수 있습니다. 설치 시 볼트를 잠금 해제하거나 마운트하지 않습니다.",
        },
    },
    "utm": {
        "name": "UTM", "publisher": "UTM contributors", "category": "system", "mark": "U", "accent": "#527ac3",
        "repo": "utmapp/UTM", "license": "Apache-2.0", "licenseFile": "LICENSE", "licenseRef": "v5.0.8/LICENSE",
        "homepage": "https://mac.getutm.app/", "cask": "utm", "caskArchitectures": ["x86_64", "aarch64"],
        "summary": {
            "zh-CN": "macOS 虚拟机与系统模拟工具，支持 Apple Virtualization 和 QEMU 后端。",
            "zh-TW": "macOS 虛擬機器與系統模擬工具，支援 Apple Virtualization 和 QEMU 後端。",
            "en": "A macOS virtual-machine and system-emulation tool using Apple Virtualization and QEMU backends.",
            "ja": "Apple Virtualization と QEMU バックエンドを使用する macOS 仮想マシン・システムエミュレーションツール。",
            "ko": "Apple Virtualization 및 QEMU 백엔드를 사용하는 macOS 가상 머신 및 시스템 에뮬레이션 도구.",
        },
        "extra": {
            "zh-CN": "需要 macOS 12 或更高版本。Apple Virtualization 需要 Apple Silicon；QEMU 可用于两种架构。安装不会创建或启动虚拟机、下载系统镜像或直通 USB 设备。",
            "zh-TW": "需要 macOS 12 或更高版本。Apple Virtualization 需要 Apple Silicon；QEMU 可用於兩種架構。安裝不會建立或啟動虛擬機器、下載系統映像或直通 USB 裝置。",
            "en": "Requires macOS 12 or later. Apple Virtualization requires Apple Silicon; QEMU is available on both architectures. Installation does not create/start VMs, download guest images or pass through USB devices.",
            "ja": "macOS 12 以降が必要です。Apple Virtualization には Apple Silicon が必要で、QEMU は両アーキテクチャで利用できます。インストール時に VM の作成・起動、ゲストイメージのダウンロード、USB パススルーは行いません。",
            "ko": "macOS 12 이상이 필요합니다. Apple Virtualization에는 Apple Silicon이 필요하며 QEMU는 두 아키텍처에서 사용할 수 있습니다. 설치 시 VM을 생성/시작하거나 게스트 이미지를 다운로드하거나 USB 장치를 패스스루하지 않습니다.",
        },
    },
    "balenaetcher": {
        "name": "balenaEtcher", "publisher": "Balena", "category": "system", "mark": "BE", "accent": "#527c9b",
        "repo": "balena-io/etcher", "license": "Apache-2.0", "licenseFile": "LICENSE", "licenseRef": "v2.1.9/LICENSE",
        "homepage": "https://etcher.balena.io/", "cask": "balenaetcher", "caskArchitectures": ["x86_64", "aarch64"],
        "summary": {
            "zh-CN": "将操作系统镜像写入 SD 卡和 USB 驱动器的桌面工具，提供写入校验。",
            "zh-TW": "將作業系統映像寫入 SD 卡和 USB 磁碟機的桌面工具，提供寫入驗證。",
            "en": "A desktop tool for writing operating-system images to SD cards and USB drives with validation.",
            "ja": "OS イメージを SD カードや USB ドライブへ書き込み、検証するデスクトップツール。",
            "ko": "운영체제 이미지를 SD 카드와 USB 드라이브에 기록하고 검증하는 데스크톱 도구.",
        },
        "extra": {
            "zh-CN": "需要 macOS 13 或更高版本。写盘会提升权限并覆盖目标设备全部数据；安装不会启动应用、请求凭据、选择设备或开始写入。使用前必须核对设备并备份。",
            "zh-TW": "需要 macOS 13 或更高版本。寫盤會提升權限並覆蓋目標裝置全部資料；安裝不會啟動應用程式、請求憑證、選擇裝置或開始寫入。使用前必須核對裝置並備份。",
            "en": "Requires macOS 13 or later. Flashing elevates privileges and overwrites the selected device. Installation does not launch the app, supply credentials, select a device or start flashing. Verify the target and back up first.",
            "ja": "macOS 13 以降が必要です。書き込みには権限昇格が必要で、選択したデバイスを上書きします。インストール時にアプリの起動、認証情報の入力、デバイス選択、書き込み開始は行いません。事前に対象を確認してバックアップしてください。",
            "ko": "macOS 13 이상이 필요합니다. 플래싱은 권한을 상승시키고 선택한 장치를 덮어씁니다. 설치 시 앱을 실행하거나 자격 증명을 제공하거나 장치를 선택하거나 플래싱을 시작하지 않습니다. 대상을 확인하고 먼저 백업하세요.",
        },
    },
    "karabiner-elements": {
        "name": "Karabiner-Elements", "publisher": "Fumihiko Takayama", "category": "system", "mark": "KE", "accent": "#6480a6",
        "repo": "pqrs-org/Karabiner-Elements", "license": "Unlicense", "licenseFile": "LICENSE.md", "licenseRef": "v16.3.0/LICENSE.md",
        "homepage": "https://karabiner-elements.pqrs.org/", "cask": "karabiner-elements", "caskArchitectures": ["x86_64", "aarch64"],
        "requiresAdmin": True, "interactive": True,
        "summary": {
            "zh-CN": "macOS 键盘与指点设备自定义工具，支持复杂按键映射和设备级规则。",
            "zh-TW": "macOS 鍵盤與指向裝置自訂工具，支援複雜按鍵映射和裝置級規則。",
            "en": "A macOS keyboard and pointing-device customizer with complex key mappings and per-device rules.",
            "ja": "複雑なキーマッピングとデバイスごとのルールに対応する macOS キーボード・ポインティングデバイスカスタマイズツール。",
            "ko": "복잡한 키 매핑과 장치별 규칙을 지원하는 macOS 키보드 및 포인팅 장치 사용자 정의 도구.",
        },
        "extra": {
            "zh-CN": "管理员 PKG 安装，适用于 macOS 13 及更高版本的当前 cask。需手动批准后台服务、辅助功能、输入监控和驱动扩展；安装不代表驱动已激活或按键已重映射。",
            "zh-TW": "管理員 PKG 安裝，適用於 macOS 13 及更高版本的目前 cask。需手動批准背景服務、輔助使用、輸入監控和驅動程式擴充；安裝不代表驅動程式已啟用或按鍵已重新映射。",
            "en": "Administrator PKG installation for the current cask on macOS 13 or later. Manually approve background services, Accessibility, Input Monitoring and the Driver Extension. Installation does not activate the driver or remap keys.",
            "ja": "macOS 13 以降の現在の cask に対する管理者 PKG インストールです。バックグラウンドサービス、アクセシビリティ、入力監視、Driver Extension を手動で承認する必要があります。インストールしてもドライバーの有効化やキーの再割り当ては行われません。",
            "ko": "macOS 13 이상의 현재 cask에 대한 관리자 PKG 설치입니다. 백그라운드 서비스, 손쉬운 사용, 입력 모니터링 및 드라이버 확장을 수동으로 승인해야 합니다. 설치가 드라이버 활성화나 키 재매핑을 의미하지는 않습니다.",
        },
    },
    "monitorcontrol": {
        "name": "MonitorControl", "publisher": "MonitorControl contributors", "category": "system", "mark": "MC", "accent": "#6b8cc1",
        "repo": "MonitorControl/MonitorControl", "license": "MIT", "licenseFile": "License.txt", "licenseRef": "v4.4.0/License.txt",
        "homepage": "https://github.com/MonitorControl/MonitorControl", "cask": "monitorcontrol", "caskArchitectures": ["x86_64", "aarch64"],
        "summary": {
            "zh-CN": "macOS 外接显示器亮度和音量控制工具，支持 DDC 与软件调光。",
            "zh-TW": "macOS 外接顯示器亮度和音量控制工具，支援 DDC 與軟體調光。",
            "en": "A macOS external-display brightness and volume controller with DDC and software dimming.",
            "ja": "DDC とソフトウェア調光に対応する macOS 外部ディスプレイの輝度・音量コントローラー。",
            "ko": "DDC 및 소프트웨어 디밍을 지원하는 macOS 외부 디스플레이 밝기 및 음량 제어 도구.",
        },
        "extra": {
            "zh-CN": "仅使用原生键盘亮度/媒体键时需要可选的辅助功能权限。DDC 支持取决于 Mac、GPU 和连接方式；DisplayLink 可能只支持软件调光。",
            "zh-TW": "僅使用原生鍵盤亮度／媒體鍵時需要可選的輔助使用權限。DDC 支援取決於 Mac、GPU 和連線方式；DisplayLink 可能只支援軟體調光。",
            "en": "Accessibility permission is optional and only needed for native Apple keyboard brightness/media keys. DDC support depends on the Mac, GPU and connection; DisplayLink may support software dimming only.",
            "ja": "アクセシビリティ権限はオプションで、Apple 純正キーボードの輝度・メディアキー使用時のみ必要です。DDC 対応は Mac、GPU、接続方法に依存し、DisplayLink ではソフトウェア調光のみの場合があります。",
            "ko": "손쉬운 사용 권한은 선택 사항이며 Apple 기본 키보드의 밝기/미디어 키를 사용할 때만 필요합니다. DDC 지원은 Mac, GPU 및 연결 방식에 따라 다륾며 DisplayLink는 소프트웨어 디밍만 지원할 수 있습니다.",
        },
    },
    "insomnia": {
        "name": "Insomnia", "publisher": "Kong", "category": "dev", "mark": "I", "accent": "#6853bf",
        "repo": "Kong/insomnia", "license": "Apache-2.0", "licenseFile": "LICENSE", "licenseRef": "core@13.4.0/LICENSE",
        "homepage": "https://insomnia.rest/", "cask": "insomnia", "caskArchitectures": ["x86_64", "aarch64"],
        "summary": {
            "zh-CN": "桌面 API 设计与调试客户端，支持请求组织、环境、测试和本地或云端协作。",
            "zh-TW": "桌面 API 設計與除錯用戶端，支援請求組織、環境、測試和本機或雲端協作。",
            "en": "A desktop API design and debugging client for requests, environments, testing and local or cloud collaboration.",
            "ja": "リクエスト、環境、テスト、ローカルまたはクラウドでの共同作業に対応するデスクトップ API 設計・デバッグクライアント。",
            "ko": "요청, 환경, 테스트 및 로컬 또는 클라우드 공동 작업을 지원하는 데스크톱 API 설계 및 디버깅 클라이언트.",
        },
        "extra": {
            "zh-CN": "需要 macOS 12 或更高版本。此变体是 Insomnia 桌面版，不是 Inso CLI。Scratch Pad 可离线使用；多数协作、组织、SSO 等功能需要账号或付费计划。",
            "zh-TW": "需要 macOS 12 或更高版本。此變體是 Insomnia 桌面版，不是 Inso CLI。Scratch Pad 可離線使用；多數協作、組織、SSO 等功能需要帳號或付費方案。",
            "en": "Requires macOS 12 or later. This is the Insomnia desktop app, not the Inso CLI. Scratch Pad works offline; most collaboration, organization and SSO features require an account or paid plan.",
            "ja": "macOS 12 以降が必要です。これは Insomnia デスクトップアプリで、Inso CLI ではありません。Scratch Pad はオフラインで動作しますが、共同作業、組織、SSO の多くの機能にはアカウントまたは有料プランが必要です。",
            "ko": "macOS 12 이상이 필요합니다. 이것은 Inso CLI가 아닌 Insomnia 데스크톱 앱입니다. Scratch Pad는 오프라인에서 작동하지만 대부분의 공동 작업, 조직 및 SSO 기능에는 계정 또는 유료 요금제가 필요합니다.",
        },
    },
    "devtoys": {
        "name": "DevToys", "publisher": "DevToys contributors", "category": "dev", "mark": "DT", "accent": "#4b82b2",
        "repo": "DevToys-app/DevToys", "license": "MIT", "licenseFile": "LICENSE.md", "licenseRef": "v2.0.9.0/LICENSE.md",
        "homepage": "https://devtoys.app/", "cask": "devtoys", "caskArchitectures": ["x86_64", "aarch64"],
        "summary": {
            "zh-CN": "离线开发者工具箱，提供格式化、转换、编码解码、哈希和文本处理等工具。",
            "zh-TW": "離線開發者工具箱，提供格式化、轉換、編碼解碼、雜湊和文字處理等工具。",
            "en": "An offline developer toolbox with formatters, converters, encoders, decoders, hashing and text utilities.",
            "ja": "フォーマッター、変換、エンコード・デコード、ハッシュ、テキスト処理を備えるオフライン開発者ツールボックス。",
            "ko": "포맷터, 변환기, 인코더/디코더, 해시 및 텍스트 도구를 제공하는 오프라인 개발자 도구 상자.",
        },
        "extra": {
            "zh-CN": "需要 macOS 12 或更高版本。默认工具离线运行且不要求账号；可选第三方扩展有各自条款，应只从可信来源安装。",
            "zh-TW": "需要 macOS 12 或更高版本。預設工具離線執行且不要求帳號；可選第三方擴充有各自條款，應只從可信來源安裝。",
            "en": "Requires macOS 12 or later. Default tools run offline and require no account. Optional third-party extensions have separate terms and should only be installed from trusted sources.",
            "ja": "macOS 12 以降が必要です。標準ツールはオフラインで動作し、アカウントは不要です。オプションの第三者拡張機能には個別の条件があり、信頼できるソースからのみインストールしてください。",
            "ko": "macOS 12 이상이 필요합니다. 기본 도구는 오프라인에서 실행되며 계정이 필요하지 않습니다. 선택적 타사 확장에는 별도의 조건이 적용되므로 신뢰할 수 있는 소스에서만 설치하세요.",
        },
    },
    "mockoon": {
        "name": "Mockoon", "publisher": "Mockoon", "category": "dev", "mark": "M", "accent": "#538a9e",
        "repo": "mockoon/mockoon", "license": "MIT", "licenseFile": "LICENSE.md", "licenseRef": "v9.10.0/LICENSE.md",
        "homepage": "https://mockoon.com/", "cask": "mockoon", "caskArchitectures": ["x86_64", "aarch64"],
        "summary": {
            "zh-CN": "本地 API 模拟桌面工具，可设计 REST 端点、响应和代理行为，无需远程部署。",
            "zh-TW": "本機 API 模擬桌面工具，可設計 REST 端點、回應和代理行為，無需遠端部署。",
            "en": "A local API-mocking desktop tool for REST endpoints, responses and proxy behavior without remote deployment.",
            "ja": "REST エンドポイント、レスポンス、プロキシ動作をリモートデプロイなしで設計できるローカル API モックデスクトップツール。",
            "ko": "원격 배포 없이 REST 엔드포인트, 응답 및 프록시 동작을 설계할 수 있는 로컬 API 모킹 데스크톱 도구.",
        },
        "extra": {
            "zh-CN": "需要 macOS 13 或更高版本。本地模拟不要求账号；可选 Mockoon Pro 的同步和托管部署是单独付费服务。",
            "zh-TW": "需要 macOS 13 或更高版本。本機模擬不要求帳號；可選 Mockoon Pro 的同步和託管部署是單獨付費服務。",
            "en": "Requires macOS 13 or later. Local mocking requires no account. Optional Mockoon Pro synchronization and managed deployment are separate paid services.",
            "ja": "macOS 13 以降が必要です。ローカルモックにアカウントは不要です。オプションの Mockoon Pro 同期とマネージドデプロイは別の有料サービスです。",
            "ko": "macOS 13 이상이 필요합니다. 로컬 모킹에는 계정이 필요하지 않습니다. 선택적 Mockoon Pro 동기화 및 관리형 배포는 별도의 유료 서비스입니다.",
        },
    },
    "godot-engine": {
        "name": "Godot Engine", "publisher": "Godot contributors", "category": "dev", "mark": "G", "accent": "#4d8bb8",
        "repo": "godotengine/godot", "license": "MIT", "licenseFile": "LICENSE.txt", "licenseRef": "4.7.2-stable/LICENSE.txt",
        "homepage": "https://godotengine.org/", "cask": "godot", "caskArchitectures": ["x86_64", "aarch64"],
        "summary": {
            "zh-CN": "开源二维与三维游戏引擎，提供场景编辑、脚本、动画和跨平台导出工具。",
            "zh-TW": "開源二維與三維遊戲引擎，提供場景編輯、指令碼、動畫和跨平台匯出工具。",
            "en": "An open-source 2D and 3D game engine with scene editing, scripting, animation and cross-platform export.",
            "ja": "シーン編集、スクリプト、アニメーション、クロスプラットフォームエクスポートを備えるオープンソースの 2D/3D ゲームエンジン。",
            "ko": "장면 편집, 스크립팅, 애니메이션 및 크로스 플랫폼 저장 도구를 제공하는 오픈 소스 2D/3D 게임 엔진.",
        },
        "extra": {
            "zh-CN": "此变体是当前标准 Godot，不是带 .NET SDK 依赖的 Mono 版或旧版 3.x。安装与使用不要求账号；导出平台可能需要额外 SDK。",
            "zh-TW": "此變體是目前標準 Godot，不是帶 .NET SDK 依賴的 Mono 版或舊版 3.x。安裝與使用不要求帳號；匯出平台可能需要額外 SDK。",
            "en": "This is the current standard Godot, not the Mono build with its .NET SDK dependency or legacy 3.x. No account is required; target-platform exports may need additional SDKs.",
            "ja": "これは現在の標準 Godot で、.NET SDK が必要な Mono 版や旧 3.x ではありません。アカウントは不要ですが、対象プラットフォームへのエクスポートには追加 SDK が必要な場合があります。",
            "ko": "이것은 .NET SDK가 필요한 Mono 빌드나 레거시 3.x가 아닌 현재 표준 Godot입니다. 계정은 필요하지 않지만 대상 플랫폼용으로 저장하려면 추가 SDK가 필요할 수 있습니다.",
        },
    },
    "lapce": {
        "name": "Lapce", "publisher": "Lapce contributors", "category": "dev", "mark": "L", "accent": "#5782b6",
        "repo": "lapce/lapce", "license": "Apache-2.0", "licenseFile": "LICENSE", "licenseRef": "v0.4.6/LICENSE",
        "homepage": "https://lapce.dev/", "cask": "lapce", "caskArchitectures": ["x86_64", "aarch64"],
        "summary": {
            "zh-CN": "使用 Rust 编写的桌面代码编辑器，提供语法高亮、LSP 和可选远程开发。",
            "zh-TW": "使用 Rust 編寫的桌面程式碼編輯器，提供語法醒目提示、LSP 和可選遠端開發。",
            "en": "A Rust-based desktop code editor with syntax highlighting, LSP and optional remote development.",
            "ja": "構文強調、LSP、オプションのリモート開発に対応する Rust 製デスクトップコードエディター。",
            "ko": "구문 강조, LSP 및 선택적 원격 개발을 지원하는 Rust 기반 데스크톱 코드 편집기.",
        },
        "extra": {
            "zh-CN": "本地编辑不要求账号。可选远程开发通过 SSH 在远端下载并启动 lapce-proxy，需另行配置 SSH 认证；安装不会自动连接远程主机。",
            "zh-TW": "本機編輯不要求帳號。可選遠端開發透過 SSH 在遠端下載並啟動 lapce-proxy，需另行設定 SSH 認證；安裝不會自動連線遠端主機。",
            "en": "Local editing requires no account. Optional remote development downloads and starts lapce-proxy over SSH and requires separate SSH authentication; installation does not connect to remote hosts.",
            "ja": "ローカル編集にアカウントは不要です。オプションのリモート開発では SSH 経由で lapce-proxy をダウンロード・起動し、個別の SSH 認証が必要です。インストール時にリモートホストへ接続しません。",
            "ko": "로컬 편집에는 계정이 필요하지 않습니다. 선택적 원격 개발은 SSH를 통해 lapce-proxy를 다운로드하고 시작하며 별도의 SSH 인증이 필요합니다. 설치 시 원격 호스트에 연결하지 않습니다.",
        },
    },
    "kitty": {
        "name": "kitty", "publisher": "Kovid Goyal", "category": "dev", "mark": "K", "accent": "#6c7bab",
        "repo": "kovidgoyal/kitty", "license": "GPL-3.0-only", "licenseFile": "LICENSE", "licenseRef": "v0.50.0/LICENSE",
        "homepage": "https://sw.kovidgoyal.net/kitty/", "cask": "kitty", "caskArchitectures": ["x86_64", "aarch64"],
        "summary": {
            "zh-CN": "GPU 加速终端模拟器，支持分屏、标签页、图形协议和 kitten 扩展。",
            "zh-TW": "GPU 加速終端機模擬器，支援分割畫面、分頁、圖形協定和 kitten 擴充。",
            "en": "A GPU-accelerated terminal emulator with split panes, tabs, graphics protocol and kitten extensions.",
            "ja": "分割ペイン、タブ、グラフィックスプロトコル、kitten 拡張に対応する GPU アクセラレーションターミナルエミュレーター。",
            "ko": "분할 창, 탭, 그래픽 프로토콜 및 kitten 확장을 지원하는 GPU 가속 터미널 에뮬레이터.",
        },
        "extra": {
            "zh-CN": "需要 macOS 12 或更高版本。安装官方 kitty.app DMG，不使用同名无关 PyPI 包；cask 同时提供 kitty 与 kitten 命令入口。",
            "zh-TW": "需要 macOS 12 或更高版本。安裝官方 kitty.app DMG，不使用同名無關 PyPI 套件；cask 同時提供 kitty 與 kitten 命令入口。",
            "en": "Requires macOS 12 or later. Installs the official kitty.app DMG, not the unrelated same-name PyPI package; the cask also provides kitty and kitten command launchers.",
            "ja": "macOS 12 以降が必要です。同名の無関係な PyPI パッケージではなく公式 kitty.app DMG をインストールします。kitty と kitten のコマンドランチャーも提供されます。",
            "ko": "macOS 12 이상이 필요합니다. 이름만 같은 관련 없는 PyPI 패키지가 아닌 공식 kitty.app DMG를 설치하며 kitty 및 kitten 명령 실행기도 제공합니다.",
        },
    },
    "anki": {
        "name": "Anki", "publisher": "Anki contributors", "category": "education", "mark": "A", "accent": "#6687bc",
        "repo": "ankitects/anki", "license": "AGPL-3.0-or-later", "licenseFile": "LICENSE", "licenseRef": "26.09.4/LICENSE",
        "homepage": "https://apps.ankiweb.net/", "cask": "anki", "caskArchitectures": ["x86_64", "aarch64"],
        "summary": {
            "zh-CN": "间隔重复记忆与卡片学习工具，支持牌组、多媒体卡片和学习进度管理。",
            "zh-TW": "間隔重複記憶與卡片學習工具，支援牌組、多媒體卡片和學習進度管理。",
            "en": "A spaced-repetition flashcard application with decks, multimedia cards and learning-progress management.",
            "ja": "デッキ、マルチメディアカード、学習進捗管理に対応する間隔反復フラッシュカードアプリ。",
            "ko": "덱, 멀티미디어 카드 및 학습 진행 관리를 지원하는 간격 반복 플래시카드 애플리케이션.",
        },
        "extra": {
            "zh-CN": "当前 cask 面向 macOS 13 或更高版本，Intel 与 Apple Silicon 使用不同 DMG。本地学习不要求账号；可选 AnkiWeb 同步需免费账号和联网。",
            "zh-TW": "目前 cask 面向 macOS 13 或更高版本，Intel 與 Apple Silicon 使用不同 DMG。本機學習不要求帳號；可選 AnkiWeb 同步需免費帳號和連線。",
            "en": "The current cask targets macOS 13 or later, with separate Intel and Apple Silicon DMGs. Local study requires no account; optional AnkiWeb sync requires a free account and network access.",
            "ja": "現在の cask は macOS 13 以降が対象で、Intel と Apple Silicon 用に別の DMG があります。ローカル学習にアカウントは不要で、オプションの AnkiWeb 同期には無料アカウントとネットワークが必要です。",
            "ko": "현재 cask는 macOS 13 이상을 대상으로 하며 Intel과 Apple Silicon용 DMG가 다릅니다. 로컬 학습에는 계정이 필요하지 않으며 선택적 AnkiWeb 동기화는 비용 없이 계정을 만들어 사용할 수 있고 네트워크가 필요합니다.",
        },
    },
    "shotcut": {
        "name": "Shotcut", "publisher": "Meltytech", "category": "media", "mark": "S", "accent": "#547c98",
        "repo": "mltframework/shotcut", "license": "GPL-3.0-or-later", "licenseFile": "COPYING",
        "homepage": "https://www.shotcut.org/", "cask": "shotcut", "caskArchitectures": ["x86_64", "aarch64"],
        "summary": {
            "zh-CN": "跨平台视频编辑器，支持多轨时间线、音视频滤镜、转场和多种媒体格式。",
            "zh-TW": "跨平台影片編輯器，支援多軌時間線、影音濾鏡、轉場和多種媒體格式。",
            "en": "A cross-platform video editor with multitrack timelines, audio/video filters, transitions and broad format support.",
            "ja": "マルチトラックタイムライン、音声・映像フィルター、トランジション、幅広い形式に対応するクロスプラットフォーム動画エディター。",
            "ko": "멀티트랙 타임라인, 오디오/비디오 필터, 전환 및 다양한 형식을 지원하는 크로스 플랫폼 비디오 편집기.",
        },
        "extra": {
            "zh-CN": "需要 macOS 12 或更高版本。FFmpeg 提供格式支持；摄像头、麦克风、屏幕录制或网络串流仅在使用时需要相应权限。",
            "zh-TW": "需要 macOS 12 或更高版本。FFmpeg 提供格式支援；攝影機、麥克風、螢幕錄製或網路串流僅在使用時需要相應權限。",
            "en": "Requires macOS 12 or later. FFmpeg provides format support; camera, microphone, screen recording or network streaming require relevant permissions only when used.",
            "ja": "macOS 12 以降が必要です。FFmpeg が形式をサポートし、カメラ、マイク、画面録画、ネットワーク配信は使用時にのみ対応する権限が必要です。",
            "ko": "macOS 12 이상이 필요합니다. FFmpeg가 형식을 지원하며 영상 촬영 장치, 마이크, 화면 녹화 또는 네트워크 스트리밍은 사용할 때만 해당 권한이 필요합니다.",
        },
    },
    "musescore": {
        "name": "MuseScore Studio", "publisher": "Muse Group", "category": "media", "mark": "MS", "accent": "#3979b9",
        "repo": "musescore/MuseScore", "license": "GPL-3.0-only", "licenseFile": "LICENSE.txt",
        "homepage": "https://musescore.org/", "cask": "musescore", "caskArchitectures": ["x86_64", "aarch64"],
        "summary": {
            "zh-CN": "乐谱编辑与制谱软件，支持乐谱排版、播放、导入导出和本地保存。",
            "zh-TW": "樂譜編輯與製譜軟體，支援樂譜排版、播放、匯入匯出和本機儲存。",
            "en": "A music-notation editor for score layout, playback, import/export and local saving.",
            "ja": "楽譜のレイアウト、再生、インポート・エクスポート、ローカル保存に対応する楽譜作成エディター。",
            "ko": "악보 레이아웃, 재생, 가져오기, 다른 형식으로 저장 및 로컬 저장을 지원하는 악보 편집기.",
        },
        "extra": {
            "zh-CN": "本地制谱和保存不要求账号；发布或云保存到 MuseScore.com 需要账号，订阅服务另有条件。MuseSounds 在 macOS 需另装 Muse Hub。",
            "zh-TW": "本機製譜和儲存不要求帳號；發佈或雲端儲存到 MuseScore.com 需要帳號，訂閱服務另有條件。MuseSounds 在 macOS 需另裝 Muse Hub。",
            "en": "Local notation and saving require no account. Publishing or cloud saving to MuseScore.com requires an account and subscription tiers have separate terms. MuseSounds requires separate Muse Hub on macOS.",
            "ja": "ローカルでの楽譜作成と保存にアカウントは不要です。MuseScore.com への公開やクラウド保存にはアカウントが必要で、サブスクリプションには別の条件があります。MuseSounds には別途 Muse Hub が必要です。",
            "ko": "로컬 악보 작성과 저장에는 계정이 필요하지 않습니다. MuseScore.com에 게시하거나 클라우드에 저장하려면 계정이 필요하며 구독에는 별도의 조건이 적용됩니다. MuseSounds에는 별도의 Muse Hub가 필요합니다.",
        },
    },
    "zettlr": {
        "name": "Zettlr", "publisher": "Zettlr contributors", "category": "office", "mark": "Z", "accent": "#4f8c83",
        "repo": "Zettlr/Zettlr", "license": "GPL-3.0-only", "licenseFile": "LICENSE", "licenseRef": "v4.10.0/LICENSE",
        "homepage": "https://www.zettlr.com/", "cask": "zettlr", "caskArchitectures": ["x86_64", "aarch64"],
        "summary": {
            "zh-CN": "面向写作与知识管理的 Markdown 编辑器，支持文献引用、项目组织和文档导出。",
            "zh-TW": "面向寫作與知識管理的 Markdown 編輯器，支援文獻引用、專案組織和文件匯出。",
            "en": "A Markdown editor for writing and knowledge management with citations, project organization and document export.",
            "ja": "文献引用、プロジェクト整理、文書エクスポートに対応する執筆・ナレッジ管理向け Markdown エディター。",
            "ko": "문헌 인용, 프로젝트 구성 및 문서를 다른 형식으로 저장하는 기능을 지원하는 글쓰기 및 지식 관리용 Markdown 편집기.",
        },
        "extra": {
            "zh-CN": "需要 macOS 13 或更高版本。本地写作不要求账号或订阅；Pandoc 和 LaTeX 是相应导出流程的可选外部依赖。",
            "zh-TW": "需要 macOS 13 或更高版本。本機寫作不要求帳號或訂閱；Pandoc 和 LaTeX 是相應匯出流程的可選外部依賴。",
            "en": "Requires macOS 13 or later. Local writing requires no account or subscription. Pandoc and LaTeX are optional external dependencies for their export workflows.",
            "ja": "macOS 13 以降が必要です。ローカル執筆にアカウントやサブスクリプションは不要です。Pandoc と LaTeX は対応するエクスポート用のオプションの外部依存です。",
            "ko": "macOS 13 이상이 필요합니다. 로컬 글쓰기에는 계정이나 구독이 필요하지 않습니다. Pandoc과 LaTeX는 해당 형식으로 저장할 때 선택적으로 필요한 외부 도구입니다.",
        },
    },
    "notesnook": {
        "name": "Notesnook", "publisher": "Streetwriters", "category": "office", "mark": "N", "accent": "#5c8da9",
        "repo": "streetwriters/notesnook", "license": "GPL-3.0-or-later", "licenseFile": "LICENSE", "licenseRef": "v3.4.13/LICENSE",
        "homepage": "https://notesnook.com/", "cask": "notesnook", "caskArchitectures": ["x86_64", "aarch64"],
        "summary": {
            "zh-CN": "注重隐私的笔记应用，支持笔记本、标签、搜索、加密同步和本地编辑。",
            "zh-TW": "注重隱私的筆記應用程式，支援筆記本、標籤、搜尋、加密同步和本機編輯。",
            "en": "A privacy-focused notes app with notebooks, tags, search, encrypted sync and local editing.",
            "ja": "ノートブック、タグ、検索、暗号化同期、ローカル編集に対応するプライバシー重視のノートアプリ。",
            "ko": "노트북, 태그, 검색, 암호화 동기화 및 로컬 편집을 지원하는 개인정보 보호 중심 노트 앱.",
        },
        "extra": {
            "zh-CN": "本地笔记不要求账号；附件上传需登录，账号同步免费。更高存储空间、离线附件等功能可能需要付费计划。",
            "zh-TW": "本機筆記不要求帳號；附件上傳需登入，帳號同步免費。更高儲存空間、離線附件等功能可能需要付費方案。",
            "en": "Local notes require no account. Attachment upload requires login; account sync is free. Additional storage, offline attachments and other features may require a paid plan.",
            "ja": "ローカルノートにアカウントは不要です。添付ファイルのアップロードにはログインが必要で、アカウント同期は無料です。追加ストレージやオフライン添付などには有料プランが必要な場合があります。",
            "ko": "로컬 노트에는 계정이 필요하지 않습니다. 첨부 파일 업로드에는 로그인이 필요하며 계정 동기화는 비용 없이 사용할 수 있습니다. 추가 저장 공간, 오프라인 첨부 파일 등의 기능에는 유료 요금제가 필요할 수 있습니다.",
        },
    },
    "standard-notes": {
        "name": "Standard Notes", "publisher": "Standard Notes", "category": "office", "mark": "SN", "accent": "#5279a6",
        "repo": "standardnotes/app", "license": "AGPL-3.0-only", "licenseFile": "LICENSE", "licenseRef": "@standardnotes/desktop@3.202.11/LICENSE",
        "homepage": "https://standardnotes.com/", "cask": "standard-notes", "caskArchitectures": ["x86_64", "aarch64"],
        "summary": {
            "zh-CN": "支持端到端加密同步的笔记应用，可离线使用并通过扩展增加编辑功能。",
            "zh-TW": "支援端對端加密同步的筆記應用程式，可離線使用並透過擴充增加編輯功能。",
            "en": "A notes app with end-to-end encrypted sync, offline use and extensible editing features.",
            "ja": "エンドツーエンド暗号化同期、オフライン使用、拡張可能な編集機能に対応するノートアプリ。",
            "ko": "종단 간 암호화 동기화, 오프라인 사용 및 확장 가능한 편집 기능을 지원하는 노트 앱.",
        },
        "extra": {
            "zh-CN": "可不注册账号完全离线使用，但数据不会跨设备同步。免费账号提供加密同步；更多笔记类型、历史和加密存储可能需要付费订阅。",
            "zh-TW": "可不註冊帳號完全離線使用，但資料不會跨裝置同步。免費帳號提供加密同步；更多筆記類型、歷史和加密儲存可能需要付費訂閱。",
            "en": "Fully offline use works without an account, but data does not sync across devices. A free account provides encrypted sync; additional note types, history and encrypted storage may require a paid subscription.",
            "ja": "アカウントなしで完全オフライン利用が可能ですが、データはデバイス間で同期されません。無料アカウントで暗号化同期ができ、追加のノート形式、履歴、暗号化ストレージには有料プランが必要な場合があります。",
            "ko": "계정 없이 완전한 오프라인 사용이 가능하지만 데이터는 장치 간 동기화되지 않습니다. 비용 없이 만든 계정으로 암호화 동기화를 사용할 수 있으며 추가 노트 유형, 기록 및 암호화 저장소에는 유료 구독이 필요할 수 있습니다.",
        },
    },
    "tagspaces": {
        "name": "TagSpaces Lite", "publisher": "TagSpaces", "category": "office", "mark": "TS", "accent": "#65899b",
        "repo": "tagspaces/tagspaces", "license": "AGPL-3.0-only", "licenseFile": "LICENSE.txt", "licenseRef": "v6.14.3/LICENSE.txt",
        "homepage": "https://www.tagspaces.org/", "cask": "tagspaces", "caskArchitectures": ["x86_64", "aarch64"],
        "summary": {
            "zh-CN": "离线文件组织与标记工具，通过标签和预览管理本地文档、图片及其他文件。",
            "zh-TW": "離線檔案組織與標記工具，透過標籤和預覽管理本機文件、圖片及其他檔案。",
            "en": "An offline file organizer and tagging tool for local documents, images and other files with previews.",
            "ja": "タグとプレビューでローカルの文書、画像、その他のファイルを整理するオフラインファイル管理・タグ付けツール。",
            "ko": "태그와 미리 보기로 로컬 문서, 이미지 및 기타 파일을 관리하는 오프라인 파일 구성 및 태깅 도구.",
        },
        "extra": {
            "zh-CN": "需要 macOS 13 或更高版本。此变体是仅含 AGPL 核心的 Lite，不含 TagSpaces Pro 专有模块；本地使用不要求账号或云服务。",
            "zh-TW": "需要 macOS 13 或更高版本。此變體是僅含 AGPL 核心的 Lite，不含 TagSpaces Pro 專有模組；本機使用不要求帳號或雲端服務。",
            "en": "Requires macOS 13 or later. This is the Lite build containing only the AGPL core, not proprietary TagSpaces Pro modules. Local use requires no account or cloud service.",
            "ja": "macOS 13 以降が必要です。これは AGPL コアのみを含む Lite ビルドで、独自の TagSpaces Pro モジュールは含みません。ローカル使用にアカウントやクラウドサービスは不要です。",
            "ko": "macOS 13 이상이 필요합니다. 이것은 AGPL 코어만 포함하는 Lite 빌드이며 독점 TagSpaces Pro 모듈은 포함하지 않습니다. 로컬 사용에 계정이나 클라우드 서비스가 필요하지 않습니다.",
        },
    },
    "super-productivity": {
        "name": "Super Productivity", "publisher": "Johannes Millan", "category": "office", "mark": "SP", "accent": "#537fba",
        "repo": "super-productivity/super-productivity", "license": "MIT", "licenseFile": "LICENSE", "licenseRef": "v20.0.0/LICENSE",
        "homepage": "https://super-productivity.com/", "cask": "super-productivity", "caskArchitectures": ["x86_64", "aarch64"],
        "summary": {
            "zh-CN": "本地优先的任务与时间管理工具，支持待办事项、时间跟踪、日历和第三方项目集成。",
            "zh-TW": "本機優先的任務與時間管理工具，支援待辦事項、時間追蹤、日曆和第三方專案整合。",
            "en": "A local-first task and time manager with to-dos, time tracking, calendars and third-party project integrations.",
            "ja": "ToDo、時間追跡、カレンダー、第三者プロジェクト連携を備えるローカルファーストのタスク・時間管理ツール。",
            "ko": "할 일, 시간 추적, 캘린더 및 타사 프로젝트 통합을 지원하는 로컬 우선 작업 및 시간 관리 도구.",
        },
        "extra": {
            "zh-CN": "需要 macOS 13 或更高版本。核心离线功能不要求账号；第三方集成和同步会使用各自凭据并可能回写状态。SuperSync 仍是 beta，不作为核心依赖。",
            "zh-TW": "需要 macOS 13 或更高版本。核心離線功能不要求帳號；第三方整合和同步會使用各自憑證並可能回寫狀態。SuperSync 仍是 beta，不作為核心依賴。",
            "en": "Requires macOS 13 or later. Core offline features require no account. Third-party integrations and sync use their own credentials and may write status back. SuperSync remains beta and is not a core dependency.",
            "ja": "macOS 13 以降が必要です。コアのオフライン機能にアカウントは不要です。第三者連携と同期はそれぞれの認証情報を使用し、状態を書き戻す場合があります。SuperSync は beta で、コアの依存ではありません。",
            "ko": "macOS 13 이상이 필요합니다. 핵심 오프라인 기능에는 계정이 필요하지 않습니다. 타사 통합 및 동기화는 각 서비스의 자격 증명을 사용하며 상태를 다시 기록할 수 있습니다. SuperSync는 베타이며 핵심 종속성이 아닙니다.",
        },
    },
    "activitywatch": {
        "name": "ActivityWatch", "publisher": "ActivityWatch contributors", "category": "tools", "mark": "AW", "accent": "#58849c",
        "repo": "ActivityWatch/activitywatch", "license": "MPL-2.0", "licenseFile": "LICENSE.txt", "licenseRef": "v0.14.2/LICENSE.txt",
        "homepage": "https://activitywatch.net/", "cask": "activitywatch", "caskArchitectures": ["x86_64", "aarch64"],
        "summary": {
            "zh-CN": "本地活动时间跟踪工具，记录应用与窗口使用情况并提供时间线和分析界面。",
            "zh-TW": "本機活動時間追蹤工具，記錄應用程式與視窗使用情況並提供時間線和分析介面。",
            "en": "A local activity tracker recording application and window usage with timelines and analytics.",
            "ja": "アプリとウィンドウの使用状況を記録し、タイムラインと分析を提供するローカルアクティビティトラッカー。",
            "ko": "앱과 창 사용을 기록하고 타임라인과 분석을 제공하는 로컬 활동 추적 도구.",
        },
        "extra": {
            "zh-CN": "需要 macOS 12 或更高版本。窗口标题记录需辅助功能权限；aw-qt 会启动本地服务与监视器，默认本地 API 未认证，其他本地进程可能访问活动数据。可选 AI 摘要会发送活动概览。",
            "zh-TW": "需要 macOS 12 或更高版本。視窗標題記錄需輔助使用權限；aw-qt 會啟動本機服務與監視器，預設本機 API 未認證，其他本機處理程序可能存取活動資料。可選 AI 摘要會傳送活動概覽。",
            "en": "Requires macOS 12 or later. Window-title tracking needs Accessibility permission. aw-qt starts a local server and watchers; the default unauthenticated local API can expose activity data to other local processes. Optional AI summaries transmit an activity overview.",
            "ja": "macOS 12 以降が必要です。ウィンドウタイトルの記録にはアクセシビリティ権限が必要です。aw-qt はローカルサーバーと watcher を起動し、デフォルトの未認証ローカル API から他のローカルプロセスが活動データにアクセスできる場合があります。オプションの AI 要約は活動概要を送信します。",
            "ko": "macOS 12 이상이 필요합니다. 창 제목 추적에는 손쉬운 사용 권한이 필요합니다. aw-qt는 로컬 서버와 감시자를 시작하며 기본 인증되지 않은 로컬 API를 통해 다른 로컬 프로세스가 활동 데이터에 접근할 수 있습니다. 선택적 AI 요약은 활동 개요를 전송합니다.",
        },
    },
    "hammerspoon": {
        "name": "Hammerspoon", "publisher": "Hammerspoon contributors", "category": "system", "mark": "H", "accent": "#657fa8",
        "repo": "Hammerspoon/hammerspoon", "license": "MIT", "licenseFile": "LICENSE", "licenseRef": "1.1.1/LICENSE",
        "homepage": "https://www.hammerspoon.org/", "cask": "hammerspoon", "caskArchitectures": ["x86_64", "aarch64"],
        "summary": {
            "zh-CN": "macOS Lua 自动化工具，可控制窗口、应用、输入事件和系统功能。",
            "zh-TW": "macOS Lua 自動化工具，可控制視窗、應用程式、輸入事件和系統功能。",
            "en": "A macOS Lua automation tool for windows, applications, input events and system functions.",
            "ja": "ウィンドウ、アプリ、入力イベント、システム機能を制御する macOS Lua 自動化ツール。",
            "ko": "창, 앱, 입력 이벤트 및 시스템 기능을 제어하는 macOS Lua 자동화 도구.",
        },
        "extra": {
            "zh-CN": "需要 macOS 13 或更高版本和手动辅助功能授权。脚本/Spoons 可拦截输入、控制应用或执行命令，仅使用可信来源；崩溃上报默认开启，可在偏好设置关闭。",
            "zh-TW": "需要 macOS 13 或更高版本和手動輔助使用授權。指令碼／Spoons 可攔截輸入、控制應用程式或執行命令，僅使用可信來源；當機回報預設開啟，可在偏好設定關閉。",
            "en": "Requires macOS 13 or later and manually granted Accessibility access. Scripts and Spoons can intercept input, control apps or execute commands; use trusted sources only. Crash reporting is enabled by default and can be disabled in Preferences.",
            "ja": "macOS 13 以降と手動でのアクセシビリティ許可が必要です。スクリプトと Spoon は入力の傍受、アプリ操作、コマンド実行が可能なため、信頼できるソースのみ使用してください。クラッシュレポートはデフォルトで有効で、設定で無効にできます。",
            "ko": "macOS 13 이상과 수동 손쉬운 사용 권한이 필요합니다. 스크립트와 Spoon은 입력을 가로채거나 앱을 제어하거나 명령을 실행할 수 있으므로 신뢰할 수 있는 소스만 사용하세요. 충돌 보고는 기본적으로 활성화되어 있으며 설정에서 끌 수 있습니다.",
        },
    },
    "espanso": {
        "name": "Espanso", "publisher": "Federico Terzi", "category": "tools", "mark": "E", "accent": "#5c88a6",
        "repo": "espanso/espanso", "license": "GPL-3.0-only", "licenseFile": "LICENSE", "licenseRef": "v2.4.1/LICENSE",
        "homepage": "https://espanso.org/", "cask": "espanso", "caskArchitectures": ["x86_64", "aarch64"],
        "summary": {
            "zh-CN": "跨平台文本扩展工具，通过自定义触发词插入文本、剪贴板内容或命令输出。",
            "zh-TW": "跨平台文字擴充工具，透過自訂觸發詞插入文字、剪貼簿內容或命令輸出。",
            "en": "A cross-platform text expander inserting text, clipboard content or command output from custom triggers.",
            "ja": "カスタムトリガーからテキスト、クリップボード内容、コマンド出力を挿入するクロスプラットフォームテキスト展開ツール。",
            "ko": "사용자 정의 트리거로 텍스트, 클립보드 내용 또는 명령 출력을 삽입하는 크로스 플랫폼 텍스트 확장 도구.",
        },
        "extra": {
            "zh-CN": "需要辅助功能权限以识别系统范围输入。扩展可模拟键盘或暂时修改剪贴板；规则可执行命令，软件包需来自可信来源。后台服务与登录启动需由用户启用。",
            "zh-TW": "需要輔助使用權限以識別系統範圍輸入。擴充可模擬鍵盤或暫時修改剪貼簿；規則可執行命令，套件需來自可信來源。背景服務與登入啟動需由使用者啟用。",
            "en": "Accessibility permission is required to recognize system-wide typing. Expansion can simulate keys or temporarily modify the clipboard; rules can execute commands, so packages need trusted sources. Background service and login startup are user-enabled.",
            "ja": "システム全体の入力を認識するにはアクセシビリティ権限が必要です。展開はキー入力をシミュレートしたりクリップボードを一時変更したりします。ルールはコマンドを実行できるため、パッケージは信頼できるソースが必要です。バックグラウンドサービスとログイン起動はユーザーが有効にします。",
            "ko": "시스템 전체 입력을 인식하려면 손쉬운 사용 권한이 필요합니다. 확장은 키 입력을 시뮬레이션하거나 클립보드를 일시적으로 수정할 수 있으며 규칙은 명령을 실행할 수 있으므로 패키지는 신뢰할 수 있는 소스에서 가져와야 합니다. 백그라운드 서비스와 로그인 시작은 사용자가 활성화합니다.",
        },
    },
    "bruno": {
        "name": "Bruno", "publisher": "Bruno contributors", "category": "dev", "mark": "B", "accent": "#4f88aa",
        "repo": "usebruno/bruno", "license": "MIT", "licenseFile": "license.md", "licenseRef": "v4.3.0/license.md",
        "homepage": "https://www.usebruno.com/", "cask": "bruno", "caskArchitectures": ["x86_64", "aarch64"],
        "summary": {
            "zh-CN": "本地优先的 API 客户端，以文件形式管理请求集合并支持环境、测试和版本控制。",
            "zh-TW": "本機優先的 API 用戶端，以檔案形式管理請求集合並支援環境、測試和版本控制。",
            "en": "A local-first API client storing request collections as files with environments, tests and version control.",
            "ja": "リクエストコレクションをファイルで管理し、環境、テスト、バージョン管理に対応するローカルファーストの API クライアント。",
            "ko": "요청 컬렉션을 파일로 관리하고 환경, 테스트 및 버전 관리를 지원하는 로컬 우선 API 클라이언트.",
        },
        "extra": {
            "zh-CN": "macOS 使用独立 Intel/ARM64 DMG。开源桌面版可离线管理本地集合；发送请求会访问目标 API。云同步、团队协作等付费服务需另行账号，不是本地功能依赖。",
            "zh-TW": "macOS 使用獨立 Intel／ARM64 DMG。開源桌面版可離線管理本機集合；傳送請求會存取目標 API。雲端同步、團隊協作等付費服務需另行帳號，不是本機功能依賴。",
            "en": "macOS uses separate Intel and ARM64 DMGs. The open-source desktop manages local collections offline; sending requests accesses target APIs. Cloud sync and team services require separate accounts and are not local-feature dependencies.",
            "ja": "macOS では Intel と ARM64 用の別 DMG を使用します。オープンソース版はローカルコレクションをオフライン管理でき、リクエスト送信時に対象 API へアクセスします。クラウド同期やチームサービスには別アカウントが必要で、ローカル機能の依存ではありません。",
            "ko": "macOS에서는 Intel과 ARM64용 별도 DMG를 사용합니다. 오픈 소스 데스크톱은 로컬 컬렉션을 오프라인으로 관리하며 요청을 볼 때 대상 API에 접근합니다. 클라우드 동기화와 팀 서비스에는 별도 계정이 필요하지만 로컬 기능의 종속성은 아닙니다.",
        },
    },
    "zen-browser": {
        "name": "Zen Browser", "publisher": "Zen Browser contributors", "category": "browser", "mark": "Z", "accent": "#657daa",
        "repo": "zen-browser/desktop", "license": "MPL-2.0", "licenseFile": "LICENSE", "licenseRef": "1.24.2b/LICENSE",
        "homepage": "https://zen-browser.app/", "cask": "zen", "caskArchitectures": ["x86_64", "aarch64"],
        "summary": {
            "zh-CN": "基于 Firefox 的桌面浏览器，提供工作区、分栏视图和可定制界面。",
            "zh-TW": "基於 Firefox 的桌面瀏覽器，提供工作區、分割檢視和可自訂介面。",
            "en": "A Firefox-based desktop browser with workspaces, split views and a customizable interface.",
            "ja": "ワークスペース、分割ビュー、カスタマイズ可能なインターフェイスを備える Firefox ベースのデスクトップブラウザー。",
            "ko": "작업 공간, 분할 보기 및 사용자 정의 인터페이스를 제공하는 Firefox 기반 데스크톱 브라우저.",
        },
        "extra": {
            "zh-CN": "macOS 使用同一通用 DMG 支持 Intel 与 ARM64。此 1.24.2b 在上游 release 页明确标为 Stable Release，b 不是 beta。浏览和可选同步会访问相应服务。",
            "zh-TW": "macOS 使用同一通用 DMG 支援 Intel 與 ARM64。此 1.24.2b 在上游 release 頁明確標為 Stable Release，b 不是 beta。瀏覽和可選同步會存取相應服務。",
            "en": "One universal macOS DMG supports Intel and ARM64. Upstream explicitly labels 1.24.2b a Stable Release; the b suffix is not beta. Browsing and optional sync access their respective services.",
            "ja": "1 つのユニバーサル macOS DMG が Intel と ARM64 に対応します。1.24.2b は上流で Stable Release と明示され、b は beta ではありません。ブラウジングとオプションの同期はそれぞれのサービスにアクセスします。",
            "ko": "하나의 유니버설 macOS DMG가 Intel과 ARM64를 지원합니다. 1.24.2b는 업스트림에서 Stable Release로 명시되며 b 접미사는 베타가 아닙니다. 브라우징과 선택적 동기화는 해당 서비스에 접근합니다.",
        },
    },
    "spacedrive": {
        "name": "Spacedrive", "publisher": "Spacedrive", "category": "system", "mark": "SD", "accent": "#587da9",
        "repo": "spacedriveapp/spacedrive", "license": "Apache-2.0", "licenseFile": "LICENSE", "licenseRef": "0.4.3/LICENSE",
        "homepage": "https://www.spacedrive.com/", "cask": "spacedrive", "caskArchitectures": ["x86_64", "aarch64"],
        "summary": {
            "zh-CN": "跨平台文件管理器，以统一资料库组织和搜索本地及连接存储中的文件。",
            "zh-TW": "跨平台檔案管理器，以統一資料庫組織和搜尋本機及連接儲存中的檔案。",
            "en": "A cross-platform file manager organizing and searching local and connected storage through a unified library.",
            "ja": "統一ライブラリでローカルおよび接続ストレージ内のファイルを整理・検索するクロスプラットフォームファイルマネージャー。",
            "ko": "통합 라이브러리로 로컬 및 연결된 저장소의 파일을 구성하고 검색하는 크로스 플랫폼 파일 관리자.",
        },
        "extra": {
            "zh-CN": "macOS 使用独立 x86_64/aarch64 DMG。索引和预览会读取用户选择的位置；添加网络存储或使用可选同步时才访问对应服务。",
            "zh-TW": "macOS 使用獨立 x86_64／aarch64 DMG。索引和預覽會讀取使用者選擇的位置；新增網路儲存或使用可選同步時才存取對應服務。",
            "en": "macOS uses separate x86_64 and aarch64 DMGs. Indexing and previews read user-selected locations; network storage or optional sync accesses those services only when configured.",
            "ja": "macOS では x86_64 と aarch64 用の別 DMG を使用します。インデックスとプレビューはユーザーが選択した場所を読み取り、ネットワークストレージやオプション同期は設定時にのみアクセスします。",
            "ko": "macOS에서는 별도의 x86_64 및 aarch64 DMG를 사용합니다. 인덱싱과 미리 보기는 사용자가 선택한 위치를 읽으며 네트워크 저장소나 선택적 동기화는 구성할 때만 해당 서비스에 접근합니다.",
        },
    },
    "folo": {
        "name": "Folo", "publisher": "RSSNext", "category": "media", "mark": "F", "accent": "#5d85a9",
        "repo": "RSSNext/Folo", "license": "AGPL-3.0-only", "licenseFile": "LICENSE", "licenseRef": "desktop/v1.17.0/LICENSE",
        "homepage": "https://folo.is/", "cask": "folo", "caskArchitectures": ["x86_64", "aarch64"],
        "summary": {
            "zh-CN": "内容订阅与阅读桌面客户端，组织信息流、订阅源和稍后阅读内容。",
            "zh-TW": "內容訂閱與閱讀桌面用戶端，組織資訊流、訂閱源和稍後閱讀內容。",
            "en": "A desktop feed reader organizing timelines, subscriptions and read-later content.",
            "ja": "タイムライン、購読フィード、後で読むコンテンツを整理するデスクトップフィードリーダー。",
            "ko": "타임라인, 구독 피드 및 나중에 읽을 콘텐츠를 구성하는 데스크톱 피드 리더.",
        },
        "extra": {
            "zh-CN": "需要 macOS 13 或更高版本，Intel 与 ARM64 使用不同 DMG。订阅获取需要网络；登录和同步取决于所选服务，可能涉及账号或付费功能。",
            "zh-TW": "需要 macOS 13 或更高版本，Intel 與 ARM64 使用不同 DMG。訂閱擷取需要網路；登入和同步取決於所選服務，可能涉及帳號或付費功能。",
            "en": "Requires macOS 13 or later, with separate Intel and ARM64 DMGs. Feed retrieval needs network access; login and sync depend on the selected service and may involve accounts or paid features.",
            "ja": "macOS 13 以降が必要で、Intel と ARM64 用に別の DMG があります。フィード取得にはネットワークが必要で、ログインと同期は選択サービスに依存し、アカウントや有料機能が必要な場合があります。",
            "ko": "macOS 13 이상이 필요하며 Intel과 ARM64용 DMG가 다릅니다. 피드를 가져오려면 네트워크가 필요하며 로그인과 동기화는 선택한 서비스에 따라 계정이나 유료 기능이 필요할 수 있습니다.",
        },
    },
    "trilium": {
        "name": "Trilium Notes", "publisher": "TriliumNext contributors", "category": "office", "mark": "T", "accent": "#5a879f",
        "repo": "TriliumNext/Trilium", "license": "AGPL-3.0-or-later", "licenseFile": "LICENSE", "licenseRef": "v0.107.0/LICENSE",
        "homepage": "https://triliumnext.org/", "cask": "trilium-notes", "caskArchitectures": ["x86_64", "aarch64"],
        "summary": {
            "zh-CN": "分层知识库与笔记应用，支持树状组织、富文本、搜索、脚本和本地数据。",
            "zh-TW": "分層知識庫與筆記應用程式，支援樹狀組織、富文字、搜尋、指令碼和本機資料。",
            "en": "A hierarchical notes and knowledge-base app with rich text, search, scripting and local data.",
            "ja": "階層整理、リッチテキスト、検索、スクリプト、ローカルデータに対応するノート・ナレッジベースアプリ。",
            "ko": "계층 구조, 서식 있는 텍스트, 검색, 스크립팅 및 로컬 데이터를 지원하는 노트 및 지식 관리 앱.",
        },
        "extra": {
            "zh-CN": "需要 macOS 13 或更高版本，使用独立 Intel/ARM64 DMG。本地笔记不要求账号；可选同步服务器和脚本自动化需用户配置，脚本只应来自可信来源。",
            "zh-TW": "需要 macOS 13 或更高版本，使用獨立 Intel／ARM64 DMG。本機筆記不要求帳號；可選同步伺服器和指令碼自動化需使用者設定，指令碼只應來自可信來源。",
            "en": "Requires macOS 13 or later and uses separate Intel/ARM64 DMGs. Local notes need no account. Optional sync servers and scripting require configuration; scripts must come from trusted sources.",
            "ja": "macOS 13 以降が必要で、Intel と ARM64 用の別 DMG を使用します。ローカルノートにアカウントは不要です。オプションの同期サーバーとスクリプトには設定が必要で、信頼できるソースのスクリプトのみ使用してください。",
            "ko": "macOS 13 이상이 필요하며 별도의 Intel/ARM64 DMG를 사용합니다. 로컬 노트에는 계정이 필요하지 않습니다. 선택적 동기화 서버와 스크립팅은 구성이 필요하며 신뢰할 수 있는 소스의 스크립트만 사용해야 합니다.",
        },
    },
    "recordly": {
        "name": "Recordly", "publisher": "Recordly contributors", "category": "media", "mark": "R", "accent": "#6285a6",
        "repo": "webadderallorg/Recordly", "license": "AGPL-3.0-only", "licenseFile": "LICENSE.md", "licenseRef": "v1.4.0/LICENSE.md",
        "homepage": "https://github.com/webadderallorg/Recordly", "cask": "recordly", "caskArchitectures": ["x86_64", "aarch64"],
        "summary": {
            "zh-CN": "桌面录屏工具，支持屏幕录制、摄像头与麦克风输入及本地视频保存。",
            "zh-TW": "桌面螢幕錄製工具，支援螢幕錄製、攝影機與麥克風輸入及本機影片儲存。",
            "en": "A desktop screen recorder with camera and microphone input and local video saving.",
            "ja": "画面収録、カメラ・マイク入力、ローカル動画保存に対応するデスクトップスクリーンレコーダー。",
            "ko": "화면 녹화, 영상 촬영 장치 및 마이크 입력, 로컬 비디오 저장을 지원하는 데스크톱 화면 녹화 도구.",
        },
        "extra": {
            "zh-CN": "需要 macOS 12 或更高版本，Intel 与 ARM64 使用不同 DMG。录屏需屏幕录制权限；摄像头和麦克风仅启用时需要权限。录制内容保存在本地，上传取决于用户操作。",
            "zh-TW": "需要 macOS 12 或更高版本，Intel 與 ARM64 使用不同 DMG。錄屏需螢幕錄製權限；攝影機和麥克風僅啟用時需要權限。錄製內容儲存在本機，上傳取決於使用者操作。",
            "en": "Requires macOS 12 or later, with separate Intel and ARM64 DMGs. Recording needs Screen Recording permission; camera and microphone permissions apply only when enabled. Videos stay local unless the user uploads them.",
            "ja": "macOS 12 以降が必要で、Intel と ARM64 用に別の DMG があります。画面収録には画面収録権限が必要で、カメラとマイクは有効時にのみ権限が必要です。動画はユーザーがアップロードしない限りローカルに保存されます。",
            "ko": "macOS 12 이상이 필요하며 Intel과 ARM64용 DMG가 다릅니다. 화면 녹화에는 화면 기록 권한이 필요하고 영상 촬영 장치와 마이크는 활성화할 때만 권한이 필요합니다. 사용자가 업로드하지 않는 한 동영상은 로컬에 저장됩니다.",
        },
    },
    "another-redis-desktop-manager": {
        "name": "Another Redis Desktop Manager", "publisher": "qishibo", "category": "dev", "mark": "AR", "accent": "#5e86a8",
        "repo": "qishibo/AnotherRedisDesktopManager", "license": "MIT", "licenseFile": "LICENSE", "licenseRef": "v1.7.4/LICENSE",
        "homepage": "https://github.com/qishibo/AnotherRedisDesktopManager", "cask": "another-redis-desktop-manager", "caskArchitectures": ["x86_64", "aarch64"],
        "summary": {
            "zh-CN": "Redis 桌面管理客户端，支持连接管理、键值浏览、编辑和命令执行。",
            "zh-TW": "Redis 桌面管理用戶端，支援連線管理、鍵值瀏覽、編輯和命令執行。",
            "en": "A Redis desktop client for connection management, key browsing, editing and command execution.",
            "ja": "接続管理、キー参照、編集、コマンド実行に対応する Redis デスクトップクライアント。",
            "ko": "연결 관리, 키 탐색, 편집 및 명령 실행을 지원하는 Redis 데스크톱 클라이언트.",
        },
        "extra": {
            "zh-CN": "macOS 使用独立 Intel/ARM64 DMG。本地应用不要求账号；连接 Redis 会使用用户配置的主机、凭据和 TLS，执行命令或编辑键会修改目标数据库。",
            "zh-TW": "macOS 使用獨立 Intel／ARM64 DMG。本機應用程式不要求帳號；連接 Redis 會使用使用者設定的主機、憑證和 TLS，執行命令或編輯鍵會修改目標資料庫。",
            "en": "macOS uses separate Intel/ARM64 DMGs. The local app needs no account; Redis connections use configured hosts, credentials and TLS. Commands and key edits modify the target database.",
            "ja": "macOS では Intel と ARM64 用の別 DMG を使用します。ローカルアプリにアカウントは不要で、Redis 接続には設定したホスト、認証情報、TLS を使用します。コマンド実行とキー編集は対象データベースを変更します。",
            "ko": "macOS에서는 별도의 Intel/ARM64 DMG를 사용합니다. 로컬 앱에는 계정이 필요하지 않으며 Redis 연결에는 구성된 호스트, 자격 증명 및 TLS를 사용합니다. 명령 실행과 키 편집은 대상 데이터베이스를 수정합니다.",
        },
    },
    "vorssaint-utils": {
        "name": "Vorssaint", "publisher": "Vorssaint", "category": "tools", "mark": "V", "accent": "#6389a6",
        "repo": "vorssaint/vorssaint-utils", "license": "GPL-3.0-or-later", "licenseFile": "LICENSE", "licenseRef": "v3.4.4/LICENSE",
        "homepage": "https://github.com/vorssaint/vorssaint-utils", "cask": "vorssaint", "caskArchitectures": ["aarch64"],
        "summary": {
            "zh-CN": "Apple Silicon Mac 实用工具集，提供系统信息与常用桌面辅助功能。",
            "zh-TW": "Apple Silicon Mac 實用工具集，提供系統資訊與常用桌面輔助功能。",
            "en": "An Apple Silicon Mac utility collection with system information and desktop helpers.",
            "ja": "システム情報とデスクトップ補助機能を備える Apple Silicon Mac 用ユーティリティ集。",
            "ko": "시스템 정보와 데스크톱 보조 기능을 제공하는 Apple Silicon Mac 유틸리티 모음.",
        },
        "extra": {
            "zh-CN": "仅提供原生 ARM64 构建，需要 macOS 14 或更高版本；不声称支持 Intel 或通过 Rosetta 运行。具体系统权限取决于启用的工具。",
            "zh-TW": "僅提供原生 ARM64 建置，需要 macOS 14 或更高版本；不聲稱支援 Intel 或透過 Rosetta 執行。具體系統權限取決於啟用的工具。",
            "en": "Only a native ARM64 build is provided, requiring macOS 14 or later. Intel and Rosetta operation are not claimed. System permissions depend on the enabled tools.",
            "ja": "ネイティブ ARM64 ビルドのみで、macOS 14 以降が必要です。Intel や Rosetta での動作は示しません。システム権限は有効にするツールによります。",
            "ko": "네이티브 ARM64 빌드만 제공하며 macOS 14 이상이 필요합니다. Intel이나 Rosetta 실행은 지원한다고 명시하지 않습니다. 시스템 권한은 활성화한 도구에 따라 다릅니다.",
        },
    },
    "gdevelop": {
        "name": "GDevelop", "publisher": "GDevelop contributors", "category": "dev", "mark": "GD", "accent": "#5885a6",
        "repo": "4ian/GDevelop", "license": "MIT", "licenseFile": "LICENSE.md", "licenseRef": "v5.6.287/LICENSE.md",
        "homepage": "https://gdevelop.io/", "cask": "gdevelop", "caskArchitectures": ["x86_64", "aarch64"],
        "summary": {
            "zh-CN": "可视化二维游戏开发 IDE，使用事件、场景和扩展创建并导出游戏。",
            "zh-TW": "視覺化二維遊戲開發 IDE，使用事件、場景和擴充建立並匯出遊戲。",
            "en": "A visual 2D game-development IDE using events, scenes and extensions to create and export games.",
            "ja": "イベント、シーン、拡張でゲームを作成・出力するビジュアル 2D ゲーム開発 IDE。",
            "ko": "이벤트, 장면 및 확장을 사용해 게임을 만들고 저장하는 시각적 2D 게임 개발 IDE.",
        },
        "extra": {
            "zh-CN": "macOS 使用通用 DMG，覆盖 Intel 与 ARM64。MIT 许可适用于 Core/GDJS/newIDE/Extensions；模板、云构建、发布与账号服务可能另有条件，不是本地 IDE 核心依赖。",
            "zh-TW": "macOS 使用通用 DMG，涵蓋 Intel 與 ARM64。MIT 授權適用於 Core／GDJS／newIDE／Extensions；範本、雲端建置、發佈與帳號服務可能另有條件，不是本機 IDE 核心依賴。",
            "en": "The universal macOS DMG covers Intel and ARM64. MIT applies to Core, GDJS, newIDE and Extensions. Templates, cloud builds, publishing and account services have separate terms and are not core local-IDE dependencies.",
            "ja": "ユニバーサル macOS DMG が Intel と ARM64 に対応します。MIT は Core、GDJS、newIDE、Extensions に適用されます。テンプレート、クラウドビルド、公開、アカウントサービスには別条件があり、ローカル IDE コアの依存ではありません。",
            "ko": "유니버설 macOS DMG가 Intel과 ARM64를 지원합니다. MIT는 Core, GDJS, newIDE 및 Extensions에 적용됩니다. 템플릿, 클라우드 빌드, 게시 및 계정 서비스에는 별도 조건이 적용되며 로컬 IDE 핵심의 종속성은 아닙니다.",
        },
    },
    "wox": {
        "name": "Wox", "publisher": "Wox launcher contributors", "category": "tools", "mark": "W", "accent": "#6486a8",
        "repo": "Wox-launcher/Wox", "license": "GPL-3.0-only", "licenseFile": "LICENSE", "licenseRef": "v2.5.0/LICENSE",
        "homepage": "https://github.com/Wox-launcher/Wox", "cask": "wox", "caskArchitectures": ["x86_64", "aarch64"],
        "summary": {
            "zh-CN": "跨平台桌面启动器，通过搜索框启动应用、查找内容并运行插件。",
            "zh-TW": "跨平台桌面啟動器，透過搜尋框啟動應用程式、尋找內容並執行外掛。",
            "en": "A cross-platform launcher for applications, content search and plugins through a search box.",
            "ja": "検索ボックスからアプリ起動、コンテンツ検索、プラグイン実行を行うクロスプラットフォームランチャー。",
            "ko": "검색 상자에서 앱 실행, 콘텐츠 검색 및 플러그인 실행을 지원하는 크로스 플랫폼 런처.",
        },
        "extra": {
            "zh-CN": "需要 macOS 12 或更高版本，使用独立 Intel/ARM64 DMG。全局快捷键和插件可能需要辅助功能等权限；插件可执行代码，应仅安装可信来源。",
            "zh-TW": "需要 macOS 12 或更高版本，使用獨立 Intel／ARM64 DMG。全域快速鍵和外掛可能需要輔助使用等權限；外掛可執行程式碼，應僅安裝可信來源。",
            "en": "Requires macOS 12 or later, with separate Intel/ARM64 DMGs. Global shortcuts and plugins may need Accessibility or other permissions. Plugins can execute code and must come from trusted sources.",
            "ja": "macOS 12 以降が必要で、Intel と ARM64 用に別の DMG があります。グローバルショートカットとプラグインにはアクセシビリティ等の権限が必要な場合があります。プラグインはコードを実行できるため信頼できるソースのみ使用してください。",
            "ko": "macOS 12 이상이 필요하며 별도의 Intel/ARM64 DMG를 사용합니다. 전역 단축키와 플러그인에는 손쉬운 사용 등의 권한이 필요할 수 있습니다. 플러그인은 코드를 실행할 수 있으므로 신뢰할 수 있는 소스만 설치해야 합니다.",
        },
    },
    "picgo": {
        "name": "PicGo", "publisher": "Molunerfinn", "category": "media", "mark": "P", "accent": "#5b89a7",
        "repo": "Molunerfinn/PicGo", "license": "MIT", "licenseFile": "LICENSE", "licenseRef": "v3.0.4/LICENSE",
        "homepage": "https://picgo.github.io/PicGo-Doc/", "cask": "picgo", "caskArchitectures": ["x86_64", "aarch64"],
        "summary": {
            "zh-CN": "图片上传与图床管理工具，支持剪贴板图片、插件和多种上传服务。",
            "zh-TW": "圖片上傳與圖床管理工具，支援剪貼簿圖片、外掛和多種上傳服務。",
            "en": "An image-upload and hosting client with clipboard images, plugins and multiple upload services.",
            "ja": "クリップボード画像、プラグイン、複数のアップロードサービスに対応する画像アップロード・ホスティングツール。",
            "ko": "클립보드 이미지, 플러그인 및 다양한 업로드 서비스를 지원하는 이미지 업로드 및 호스팅 도구.",
        },
        "extra": {
            "zh-CN": "需要 macOS 12 或更高版本，使用独立 Intel/ARM64 DMG。上传会向用户配置的图床发送图片及凭据；可选 PicGo Cloud 和插件服务需另行配置。",
            "zh-TW": "需要 macOS 12 或更高版本，使用獨立 Intel／ARM64 DMG。上傳會向使用者設定的圖床傳送圖片及憑證；可選 PicGo Cloud 和外掛服務需另行設定。",
            "en": "Requires macOS 12 or later, with separate Intel/ARM64 DMGs. Uploading sends images and credentials to user-configured hosts. Optional PicGo Cloud and plugin services need separate configuration.",
            "ja": "macOS 12 以降が必要で、Intel と ARM64 用に別の DMG があります。アップロードは設定したホストへ画像と認証情報を送信します。オプションの PicGo Cloud とプラグインサービスには個別設定が必要です。",
            "ko": "macOS 12 이상이 필요하며 별도의 Intel/ARM64 DMG를 사용합니다. 업로드는 사용자가 구성한 호스트로 이미지와 자격 증명을 전송합니다. 선택적 PicGo Cloud와 플러그인 서비스는 별도 구성이 필요합니다.",
        },
    },
    "switchhosts": {
        "name": "SwitchHosts", "publisher": "oldj", "category": "dev", "mark": "SH", "accent": "#6386a5",
        "repo": "oldj/SwitchHosts", "license": "Apache-2.0", "licenseFile": "LICENSE", "licenseRef": "v5.0.3/LICENSE",
        "homepage": "https://switchhosts.vercel.app/", "cask": "switchhosts", "caskArchitectures": ["x86_64", "aarch64"],
        "summary": {
            "zh-CN": "hosts 文件管理工具，支持多套配置、切换和本地规则编辑。",
            "zh-TW": "hosts 檔案管理工具，支援多套設定、切換和本機規則編輯。",
            "en": "A hosts-file manager with multiple configurations, switching and local rule editing.",
            "ja": "複数設定、切り替え、ローカルルール編集に対応する hosts ファイル管理ツール。",
            "ko": "여러 구성, 전환 및 로컬 규칙 편집을 지원하는 hosts 파일 관리 도구.",
        },
        "extra": {
            "zh-CN": "macOS 使用独立 Intel/ARM64 DMG。安装不会自动修改 hosts；用户启用配置时才请求系统授权并改变域名解析。远程配置需联网，其内容由用户选择。",
            "zh-TW": "macOS 使用獨立 Intel／ARM64 DMG。安裝不會自動修改 hosts；使用者啟用設定時才請求系統授權並改變網域名稱解析。遠端設定需連線，其內容由使用者選擇。",
            "en": "macOS uses separate Intel/ARM64 DMGs. Installation does not modify hosts automatically; enabling a configuration requests system authorization and changes name resolution. Remote configurations require network access and user-selected content.",
            "ja": "macOS では Intel と ARM64 用の別 DMG を使用します。インストール時に hosts を自動変更しません。設定を有効にするとシステム許可を求め、名前解決を変更します。リモート設定にはネットワークが必要で、内容はユーザーが選択します。",
            "ko": "macOS에서는 별도의 Intel/ARM64 DMG를 사용합니다. 설치 시 hosts를 자동으로 수정하지 않으며 구성을 활성화할 때 시스템 권한을 요청하고 이름 확인을 변경합니다. 원격 구성에는 네트워크가 필요하며 내용은 사용자가 선택합니다.",
        },
    },
    "readest": {
        "name": "Readest", "publisher": "Readest contributors", "category": "education", "mark": "R", "accent": "#5e88a7",
        "repo": "readest/readest", "license": "AGPL-3.0-or-later", "licenseFile": "LICENSE", "licenseRef": "v0.12.20/LICENSE",
        "homepage": "https://readest.com/", "cask": "readest", "caskArchitectures": ["x86_64", "aarch64"],
        "summary": {
            "zh-CN": "跨平台电子书阅读器，支持书库、排版设置、标注与阅读进度。",
            "zh-TW": "跨平台電子書閱讀器，支援書庫、排版設定、標註與閱讀進度。",
            "en": "A cross-platform ebook reader with a library, layout settings, annotations and reading progress.",
            "ja": "ライブラリ、レイアウト設定、注釈、読書進捗に対応するクロスプラットフォーム電子書籍リーダー。",
            "ko": "서재, 레이아웃 설정, 주석 및 읽기 진행을 지원하는 크로스 플랫폼 전자책 리더.",
        },
        "extra": {
            "zh-CN": "需要 macOS 12 或更高版本，通用 DMG 覆盖 Intel 与 ARM64。本地阅读不要求账号；可选云同步、在线书源或 AI 功能会访问对应服务，账号和费用取决于配置。",
            "zh-TW": "需要 macOS 12 或更高版本，通用 DMG 涵蓋 Intel 與 ARM64。本機閱讀不要求帳號；可選雲端同步、線上書源或 AI 功能會存取對應服務，帳號和費用取決於設定。",
            "en": "Requires macOS 12 or later. The universal DMG covers Intel and ARM64. Local reading needs no account; optional sync, online sources or AI features access their configured services, with account and cost requirements depending on configuration.",
            "ja": "macOS 12 以降が必要で、ユニバーサル DMG が Intel と ARM64 に対応します。ローカル読書にアカウントは不要です。オプションの同期、オンライン書源、AI 機能は設定したサービスにアクセスし、アカウントや料金は設定によります。",
            "ko": "macOS 12 이상이 필요하며 유니버설 DMG가 Intel과 ARM64를 지원합니다. 로컬 읽기에는 계정이 필요하지 않습니다. 선택적 동기화, 온라인 서점 또는 AI 기능은 구성된 서비스에 접근하며 계정과 비용은 구성에 따라 다릅니다.",
        },
    },
    "lutris": {
        "name": "Lutris", "publisher": "Lutris contributors", "category": "media", "mark": "L", "accent": "#6688a7",
        "repo": "lutris/lutris", "license": "GPL-3.0-or-later", "licenseFile": "LICENSE", "licenseRef": "master/LICENSE",
        "homepage": "https://lutris.net/", "flathub": "net.lutris.Lutris",
        "summary": {
            "zh-CN": "Linux 游戏库与启动管理器，集中配置本地游戏、模拟器和兼容运行环境。",
            "zh-TW": "Linux 遊戲庫與啟動管理器，集中設定本機遊戲、模擬器和相容執行環境。",
            "en": "A Linux game library and launcher for local games, emulators and compatibility runners.",
            "ja": "ローカルゲーム、エミュレーター、互換ランナーを一元設定する Linux ゲームライブラリ・ランチャー。",
            "ko": "로컬 게임, 에뮬레이터 및 호환 러너를 통합 관리하는 Linux 게임 라이브러리 및 런처.",
        },
        "extra": {
            "zh-CN": "当前 Flathub 应用 ref 仅验证 x86_64。安装 Lutris 不会自动安装 Wine、runner、游戏或登录商店；这些需用户另行配置，可能涉及第三方账号、下载和各自许可。",
            "zh-TW": "目前 Flathub 應用程式 ref 僅驗證 x86_64。安裝 Lutris 不會自動安裝 Wine、runner、遊戲或登入商店；這些需使用者另行設定，可能涉及第三方帳號、下載和各自授權。",
            "en": "Only the x86_64 application ref is currently verified on Flathub. Installing Lutris does not install Wine, runners or games, or sign into stores. Those require separate user configuration and may involve third-party accounts, downloads and licenses.",
            "ja": "現在 Flathub で確認できるアプリ ref は x86_64 のみです。Lutris のインストールで Wine、ランナー、ゲームの導入やストアへのログインは行われません。これらは個別設定が必要で、サードパーティのアカウント、ダウンロード、ライセンスが関係する場合があります。",
            "ko": "현재 Flathub에서는 x86_64 애플리케이션 ref만 확인했습니다. Lutris 설치 시 Wine, 러너 또는 게임이 설치되거나 스토어에 로그인되지 않습니다. 이는 사용자가 별도로 구성해야 하며 타사 계정, 다운로드 및 라이선스가 필요할 수 있습니다.",
        },
    },
    "easyeffects": {
        "name": "Easy Effects", "publisher": "Wellington Wallace and contributors", "category": "media", "mark": "EE", "accent": "#6289a8",
        "repo": "wwmm/easyeffects", "license": "GPL-3.0-or-later", "licenseFile": "LICENSE", "licenseRef": "master/LICENSE",
        "homepage": "https://github.com/wwmm/easyeffects", "flathub": "com.github.wwmm.easyeffects",
        "summary": {
            "zh-CN": "PipeWire 音频效果处理器，为输入和输出音频流配置均衡器、压缩器等效果。",
            "zh-TW": "PipeWire 音訊效果處理器，為輸入和輸出音訊串流設定等化器、壓縮器等效果。",
            "en": "A PipeWire audio-effects processor for input and output streams, including equalizers and compressors.",
            "ja": "入出力オーディオストリームにイコライザーやコンプレッサー等を設定する PipeWire エフェクトプロセッサー。",
            "ko": "입출력 오디오 스트림에 이퀄라이저와 컴프레서 등의 효과를 설정하는 PipeWire 오디오 효과 처리기.",
        },
        "extra": {
            "zh-CN": "当前 Flathub 应用 ref 仅验证 x86_64。运行需要可用的 PipeWire 音频服务；启用效果会改变所选音频流的处理，安装本身不调整系统音频配置。",
            "zh-TW": "目前 Flathub 應用程式 ref 僅驗證 x86_64。執行需要可用的 PipeWire 音訊服務；啟用效果會改變所選音訊串流的處理，安裝本身不調整系統音訊設定。",
            "en": "Only the x86_64 application ref is currently verified on Flathub. A working PipeWire service is required. Enabled effects change processing for selected streams; installation itself does not alter system audio configuration.",
            "ja": "現在 Flathub で確認できるアプリ ref は x86_64 のみです。動作には PipeWire サービスが必要です。有効にしたエフェクトは選択したストリームの処理を変更しますが、インストール自体はシステム音声設定を変更しません。",
            "ko": "현재 Flathub에서는 x86_64 애플리케이션 ref만 확인했습니다. 실행에는 PipeWire 서비스가 필요합니다. 활성화한 효과는 선택한 스트림의 처리를 변경하지만 설치 자체가 시스템 오디오 구성을 바꾸지는 않습니다.",
        },
    },
    "feishin": {
        "name": "Feishin", "publisher": "jeffvli and contributors", "category": "media", "mark": "F", "accent": "#5f87a7",
        "repo": "jeffvli/feishin", "license": "GPL-3.0-only", "licenseFile": "LICENSE", "licenseRef": "development/LICENSE",
        "homepage": "https://github.com/jeffvli/feishin", "flathub": "org.jeffvli.feishin",
        "summary": {
            "zh-CN": "自托管音乐服务桌面客户端，支持 Subsonic、Navidrome、Jellyfin 等音乐库。",
            "zh-TW": "自託管音樂服務桌面用戶端，支援 Subsonic、Navidrome、Jellyfin 等音樂庫。",
            "en": "A desktop client for self-hosted music services including Subsonic, Navidrome and Jellyfin libraries.",
            "ja": "Subsonic、Navidrome、Jellyfin 等のライブラリに対応するセルフホスト音楽サービス用デスクトップクライアント。",
            "ko": "Subsonic, Navidrome, Jellyfin 등의 라이브러리를 지원하는 자체 호스팅 음악 서비스 데스크톱 클라이언트.",
        },
        "extra": {
            "zh-CN": "当前 Flathub 应用 ref 仅验证 x86_64。使用需另行提供兼容音乐服务器及账号；音乐会从用户配置的服务器读取，安装不提供音乐内容或托管服务。",
            "zh-TW": "目前 Flathub 應用程式 ref 僅驗證 x86_64。使用需另行提供相容音樂伺服器及帳號；音樂會從使用者設定的伺服器讀取，安裝不提供音樂內容或託管服務。",
            "en": "Only the x86_64 application ref is currently verified on Flathub. A compatible music server and account are required separately. Music is read from the configured server; installation provides neither content nor hosting.",
            "ja": "現在 Flathub で確認できるアプリ ref は x86_64 のみです。互換音楽サーバーとアカウントを別途用意する必要があります。音楽は設定したサーバーから読み込み、インストールでコンテンツやホスティングは提供されません。",
            "ko": "현재 Flathub에서는 x86_64 애플리케이션 ref만 확인했습니다. 호환 음악 서버와 계정이 별도로 필요합니다. 음악은 구성된 서버에서 읽으며 설치 시 콘텐츠나 호스팅을 제공하지 않습니다.",
        },
    },
    "solaar": {
        "name": "Solaar", "publisher": "Solaar contributors", "category": "system", "mark": "S", "accent": "#6387a5",
        "repo": "pwr-Solaar/Solaar", "license": "GPL-2.0-only", "licenseFile": "LICENSE.txt", "licenseRef": "master/LICENSE.txt",
        "homepage": "https://pwr-solaar.github.io/Solaar/", "flathub": "io.github.pwr_solaar.solaar",
        "summary": {
            "zh-CN": "Linux Logitech 设备管理器，查看状态、管理接收器配对并调整支持的设备功能。",
            "zh-TW": "Linux Logitech 裝置管理器，檢視狀態、管理接收器配對並調整支援的裝置功能。",
            "en": "A Linux Logitech device manager for status, receiver pairing and supported device settings.",
            "ja": "状態表示、レシーバーのペアリング、対応デバイス設定を行う Linux 用 Logitech デバイスマネージャー。",
            "ko": "상태 확인, 수신기 페어링 및 지원되는 장치 설정을 관리하는 Linux용 Logitech 장치 관리자.",
        },
        "extra": {
            "zh-CN": "当前 Flathub 应用 ref 仅验证 x86_64。设备访问需要系统 udev/Flatpak 权限；配对、取消配对或修改功能会改变设备状态，安装不自动执行这些操作。",
            "zh-TW": "目前 Flathub 應用程式 ref 僅驗證 x86_64。裝置存取需要系統 udev／Flatpak 權限；配對、取消配對或修改功能會改變裝置狀態，安裝不自動執行這些操作。",
            "en": "Only the x86_64 application ref is currently verified on Flathub. Device access requires system udev/Flatpak permissions. Pairing, unpairing and feature changes modify device state; installation performs none of these automatically.",
            "ja": "現在 Flathub で確認できるアプリ ref は x86_64 のみです。デバイスアクセスにはシステムの udev／Flatpak 権限が必要です。ペアリング、解除、機能変更はデバイス状態を変更しますが、インストール時に自動実行されません。",
            "ko": "현재 Flathub에서는 x86_64 애플리케이션 ref만 확인했습니다. 장치 접근에는 시스템 udev/Flatpak 권한이 필요합니다. 페어링, 해제 및 기능 변경은 장치 상태를 바꾸지만 설치 시 자동으로 수행되지 않습니다.",
        },
    },
    "fsearch": {
        "name": "FSearch", "publisher": "Christian Boxdörfer and contributors", "category": "tools", "mark": "FS", "accent": "#6388a5",
        "repo": "cboxdoerfer/fsearch", "license": "GPL-2.0-or-later", "licenseFile": "src/fsearch.c", "licenseRef": "master/src/fsearch.c",
        "homepage": "https://github.com/cboxdoerfer/fsearch", "flathub": "io.github.cboxdoerfer.FSearch",
        "summary": {
            "zh-CN": "Linux 文件搜索工具，通过可配置索引快速按名称查找本地文件。",
            "zh-TW": "Linux 檔案搜尋工具，透過可設定索引快速依名稱尋找本機檔案。",
            "en": "A Linux file-search utility that uses configurable indexes for fast local filename searches.",
            "ja": "設定可能なインデックスでローカルファイル名を高速検索する Linux 用ファイル検索ユーティリティ。",
            "ko": "구성 가능한 인덱스로 로컬 파일 이름을 빠르게 검색하는 Linux용 파일 검색 도구.",
        },
        "extra": {
            "zh-CN": "当前 Flathub 应用 ref 仅验证 x86_64。首次搜索需建立文件索引；索引范围和更新由用户配置，可能读取所选目录的文件名和元数据。",
            "zh-TW": "目前 Flathub 應用程式 ref 僅驗證 x86_64。首次搜尋需建立檔案索引；索引範圍和更新由使用者設定，可能讀取所選目錄的檔名和中繼資料。",
            "en": "Only the x86_64 application ref is currently verified on Flathub. Initial searches require an index. Its scope and updates are user-configured and may read filenames and metadata in selected directories.",
            "ja": "現在 Flathub で確認できるアプリ ref は x86_64 のみです。初回検索にはインデックス作成が必要です。対象範囲と更新はユーザー設定で、選択したディレクトリのファイル名やメタデータを読み取ります。",
            "ko": "현재 Flathub에서는 x86_64 애플리케이션 ref만 확인했습니다. 첫 검색에는 인덱스 생성이 필요합니다. 범위와 업데이트는 사용자가 구성하며 선택한 디렉터리의 파일 이름과 메타데이터를 읽을 수 있습니다.",
        },
    },
    "rawtherapee": {
        "name": "RawTherapee", "publisher": "RawTherapee contributors", "category": "media", "mark": "RT", "accent": "#6789a5",
        "repo": "RawTherapee/RawTherapee", "license": "GPL-3.0-or-later", "licenseFile": "rtengine/improccoordinator.cc", "licenseRef": "dev/rtengine/improccoordinator.cc",
        "homepage": "https://www.rawtherapee.com/", "flathub": "com.rawtherapee.RawTherapee",
        "summary": {
            "zh-CN": "RAW 照片显影与编辑器，提供非破坏性调整、批处理和色彩管理。",
            "zh-TW": "RAW 照片顯影與編輯器，提供非破壞性調整、批次處理和色彩管理。",
            "en": "A RAW photo developer and editor with non-destructive adjustments, batch processing and color management.",
            "ja": "非破壊調整、バッチ処理、カラーマネジメントを備える RAW 写真現像・編集アプリケーション。",
            "ko": "비파괴 조정, 일괄 처리 및 색상 관리를 제공하는 RAW 사진 현상 및 편집기.",
        },
        "extra": {
            "zh-CN": "当前 Flathub 应用 ref 仅验证 x86_64。处理配置和导出会写入用户选择的位置；安装不导入照片或修改现有图像。",
            "zh-TW": "目前 Flathub 應用程式 ref 僅驗證 x86_64。處理設定和匯出會寫入使用者選擇的位置；安裝不匯入照片或修改現有圖像。",
            "en": "Only the x86_64 application ref is currently verified on Flathub. Processing profiles and exports are written to user-selected locations. Installation neither imports photos nor modifies existing images.",
            "ja": "現在 Flathub で確認できるアプリ ref は x86_64 のみです。処理プロファイルと書き出しはユーザーが選択した場所に保存されます。インストールで写真の読み込みや既存画像の変更は行いません。",
            "ko": "현재 Flathub에서는 x86_64 애플리케이션 ref만 확인했습니다. 처리 프로필과 \ub0b4\ubcf4\ub0b4\uae30 파일은 선택한 위치에 저장됩니다. 설치 시 사진을 가져오거나 기존 이미지를 수정하지 않습니다.",
        },
    },
    "strawberry": {
        "name": "Strawberry Music Player", "publisher": "Strawberry contributors", "category": "media", "mark": "SM", "accent": "#6686a4",
        "repo": "strawberrymusicplayer/strawberry", "license": "GPL-3.0-or-later", "licenseFile": "src/main.cpp", "licenseRef": "master/src/main.cpp",
        "homepage": "https://www.strawberrymusicplayer.org/", "flathub": "org.strawberrymusicplayer.strawberry",
        "summary": {
            "zh-CN": "本地音乐收藏管理器和播放器，支持音频 CD、播放列表及可选流媒体服务。",
            "zh-TW": "本機音樂收藏管理器和播放器，支援音訊 CD、播放清單及可選串流媒體服務。",
            "en": "A local music collection manager and player with audio CD, playlist and optional streaming-service support.",
            "ja": "オーディオ CD、プレイリスト、オプションのストリーミングサービスに対応するローカル音楽コレクション管理・再生アプリ。",
            "ko": "오디오 CD, 재생 목록 및 선택적 스트리밍 서비스를 지원하는 로컬 음악 컬렉션 관리 및 재생 앱.",
        },
        "extra": {
            "zh-CN": "当前 Flathub 应用 ref 仅验证 x86_64。在线元数据和流媒体功能需网络及相应服务；部分服务可能需要账号或订阅，安装不提供这些内容。",
            "zh-TW": "目前 Flathub 應用程式 ref 僅驗證 x86_64。線上中繼資料和串流媒體功能需網路及相應服務；部分服務可能需要帳號或訂閱，安裝不提供這些內容。",
            "en": "Only the x86_64 application ref is currently verified on Flathub. Online metadata and streaming require network access and their respective services; some may require accounts or subscriptions. Installation provides neither.",
            "ja": "現在 Flathub で確認できるアプリ ref は x86_64 のみです。オンラインメタデータとストリーミングにはネットワークと各サービスが必要で、アカウントやサブスクリプションが必要な場合があります。インストールでこれらは提供されません。",
            "ko": "현재 Flathub에서는 x86_64 애플리케이션 ref만 확인했습니다. 온라인 메타데이터와 스트리밍에는 네트워크와 해당 서비스가 필요하며 일부는 계정이나 구독이 필요할 수 있습니다. 설치 시 이를 제공하지 않습니다.",
        },
    },
    "warzone2100": {
        "name": "Warzone 2100", "publisher": "Warzone 2100 Project", "category": "media", "mark": "WZ", "accent": "#6887a3",
        "repo": "Warzone2100/warzone2100", "license": "GPL-2.0-or-later", "licenseFile": "src/main.cpp", "licenseRef": "master/src/main.cpp",
        "homepage": "https://wz2100.net/", "flathub": "net.wz2100.wz2100",
        "summary": {
            "zh-CN": "开源即时战略游戏，包含战役、单人遭遇战、多人游戏和科技研究。",
            "zh-TW": "開源即時戰略遊戲，包含戰役、單人遭遇戰、多人遊戲和科技研究。",
            "en": "An open-source real-time strategy game with campaigns, skirmishes, multiplayer and technology research.",
            "ja": "キャンペーン、シングルプレイのスカーミッシュ、マルチプレイ、技術研究を備えるオープンソース RTS。",
            "ko": "캠페인, 싱글 스커미시, 멀티플레이 및 기술 연구를 제공하는 오픈 소스 실시간 전략 게임.",
        },
        "extra": {
            "zh-CN": "当前 Flathub 应用 ref 仅验证 x86_64。多人游戏会连接所选服务器并交换游戏数据；附加地图或模组由用户选择，可能涉及各自内容和许可。",
            "zh-TW": "目前 Flathub 應用程式 ref 僅驗證 x86_64。多人遊戲會連線所選伺服器並交換遊戲資料；附加地圖或模組由使用者選擇，可能涉及各自內容和授權。",
            "en": "Only the x86_64 application ref is currently verified on Flathub. Multiplayer connects to selected servers and exchanges game data. Additional maps or mods are user-selected and may have their own content and licenses.",
            "ja": "現在 Flathub で確認できるアプリ ref は x86_64 のみです。マルチプレイは選択したサーバーへ接続してゲームデータを交換します。追加マップや MOD はユーザーが選択し、独自のコンテンツやライセンスが適用される場合があります。",
            "ko": "현재 Flathub에서는 x86_64 애플리케이션 ref만 확인했습니다. 멀티플레이는 선택한 서버에 연결해 게임 데이터를 교환합니다. 추가 지도나 모드는 사용자가 선택하며 별도의 콘텐츠와 라이선스가 적용될 수 있습니다.",
        },
    },
    "antimicrox": {
        "name": "AntiMicroX", "publisher": "AntiMicroX contributors", "category": "system", "mark": "AX", "accent": "#6486a4",
        "repo": "AntiMicroX/antimicrox", "license": "GPL-3.0-or-later", "licenseFile": "src/main.cpp", "licenseRef": "master/src/main.cpp",
        "homepage": "https://github.com/AntiMicroX/antimicrox", "flathub": "io.github.antimicrox.antimicrox",
        "summary": {
            "zh-CN": "Linux 游戏手柄映射工具，将手柄按钮和轴映射为键盘、鼠标等输入。",
            "zh-TW": "Linux 遊戲手把映射工具，將手把按鈕和軸映射為鍵盤、滑鼠等輸入。",
            "en": "A Linux gamepad mapper that translates buttons and axes into keyboard, mouse and other inputs.",
            "ja": "ゲームパッドのボタンや軸をキーボード、マウス等の入力へ変換する Linux 用マッピングツール。",
            "ko": "게임패드 버튼과 축을 키보드, 마우스 등의 입력으로 변환하는 Linux용 매핑 도구.",
        },
        "extra": {
            "zh-CN": "当前 Flathub 应用 ref 仅验证 x86_64。启用映射会生成模拟输入并影响其他应用；设备访问需系统权限，安装不自动启用配置。",
            "zh-TW": "目前 Flathub 應用程式 ref 僅驗證 x86_64。啟用映射會產生模擬輸入並影響其他應用程式；裝置存取需系統權限，安裝不自動啟用設定。",
            "en": "Only the x86_64 application ref is currently verified on Flathub. Active mappings generate emulated input that affects other applications. Device access requires system permissions; installation enables no profile automatically.",
            "ja": "現在 Flathub で確認できるアプリ ref は x86_64 のみです。マッピングを有効にすると他のアプリへ影響するエミュレート入力を生成します。デバイスアクセスにはシステム権限が必要で、インストール時にプロファイルは自動有効化されません。",
            "ko": "현재 Flathub에서는 x86_64 애플리케이션 ref만 확인했습니다. 매핑을 활성화하면 다른 앱에 영향을 주는 가상 입력이 생성됩니다. 장치 접근에는 시스템 권한이 필요하며 설치 시 프로필이 자동으로 활성화되지 않습니다.",
        },
    },
    "kooha": {
        "name": "Kooha", "publisher": "SeaDve and contributors", "category": "media", "mark": "Ko", "accent": "#6387a4",
        "repo": "SeaDve/Kooha", "license": "GPL-3.0-or-later", "licenseFile": "Cargo.toml", "licenseRef": "main/Cargo.toml",
        "homepage": "https://github.com/SeaDve/Kooha", "flathub": "io.github.seadve.Kooha",
        "summary": {
            "zh-CN": "Linux 屏幕录制应用，可录制选定区域、系统音频和麦克风输入。",
            "zh-TW": "Linux 螢幕錄製應用程式，可錄製選定區域、系統音訊和麥克風輸入。",
            "en": "A Linux screen recorder for selected areas, system audio and microphone input.",
            "ja": "選択領域、システム音声、マイク入力を録画できる Linux 用スクリーンレコーダー。",
            "ko": "선택 영역, 시스템 오디오 및 마이크 입력을 녹화하는 Linux 화면 녹화기.",
        },
        "extra": {
            "zh-CN": "当前 Flathub 应用 ref 仅验证 x86_64。录制需要屏幕/音频 portal 权限；录制文件写入用户选择的位置，安装不自动开始录制。",
            "zh-TW": "目前 Flathub 應用程式 ref 僅驗證 x86_64。錄製需要螢幕／音訊 portal 權限；錄製檔案寫入使用者選擇的位置，安裝不自動開始錄製。",
            "en": "Only the x86_64 application ref is currently verified on Flathub. Recording requires screen/audio portal permission. Recordings are written to a user-selected location; installation does not start recording.",
            "ja": "現在 Flathub で確認できるアプリ ref は x86_64 のみです。録画には画面／音声ポータル権限が必要です。録画ファイルは選択した場所に保存され、インストール時に録画は開始されません。",
            "ko": "현재 Flathub에서는 x86_64 애플리케이션 ref만 확인했습니다. 녹화에는 화면/오디오 포털 권한이 필요합니다. 파일은 선택한 위치에 저장되며 설치 시 녹화가 시작되지 않습니다.",
        },
    },
    "legcord": {
        "name": "Legcord", "publisher": "Legcord contributors", "category": "remote", "mark": "Le", "accent": "#6686a5",
        "repo": "Legcord/Legcord", "license": "OSL-3.0", "licenseFile": "license.txt", "licenseRef": "dev/license.txt",
        "homepage": "https://legcord.app/", "flathub": "app.legcord.Legcord",
        "summary": {
            "zh-CN": "第三方 Discord 桌面客户端，提供主题、插件和客户端行为定制。",
            "zh-TW": "第三方 Discord 桌面用戶端，提供主題、外掛和用戶端行為自訂。",
            "en": "A third-party Discord desktop client with themes, plugins and client customization.",
            "ja": "テーマ、プラグイン、クライアントカスタマイズを備えるサードパーティ製 Discord デスクトップクライアント。",
            "ko": "테마, 플러그인 및 클라이언트 사용자 지정을 제공하는 서드파티 Discord 데스크톱 클라이언트.",
        },
        "extra": {
            "zh-CN": "当前 Flathub 应用 ref 仅验证 x86_64。使用需 Discord 账号并连接其服务；这是第三方客户端，插件/主题可能执行额外代码或访问服务，需用户自行审查。",
            "zh-TW": "目前 Flathub 應用程式 ref 僅驗證 x86_64。使用需 Discord 帳號並連線其服務；這是第三方用戶端，外掛／主題可能執行額外程式碼或存取服務，需使用者自行審查。",
            "en": "Only the x86_64 application ref is currently verified on Flathub. A Discord account and its service are required. This is a third-party client; plugins and themes may run additional code or access services and need separate review.",
            "ja": "現在 Flathub で確認できるアプリ ref は x86_64 のみです。Discord アカウントとサービス接続が必要なサードパーティ製クライアントです。プラグインやテーマは追加コード実行やサービスアクセスを行う場合があり、個別確認が必要です。",
            "ko": "현재 Flathub에서는 x86_64 애플리케이션 ref만 확인했습니다. Discord 계정과 서비스 연결이 필요한 서드파티 클라이언트입니다. 플러그인과 테마는 추가 코드를 실행하거나 서비스에 접근할 수 있으므로 별도 검토가 필요합니다.",
        },
    },
    "seer": {
        "name": "Seer", "publisher": "Ernie Pasveer and contributors", "category": "dev", "mark": "Se", "accent": "#6387a5",
        "repo": "epasveer/seer", "license": "GPL-3.0-or-later", "licenseFile": "src/SeerMainWindow.cpp", "licenseRef": "HEAD/src/SeerMainWindow.cpp",
        "homepage": "https://github.com/epasveer/seer", "flathub": "io.github.epasveer.seer",
        "summary": {
            "zh-CN": "GDB 图形调试前端，查看源码、断点、寄存器、内存和程序执行状态。",
            "zh-TW": "GDB 圖形除錯前端，檢視原始碼、中斷點、暫存器、記憶體和程式執行狀態。",
            "en": "A graphical GDB frontend for source, breakpoints, registers, memory and execution state.",
            "ja": "ソース、ブレークポイント、レジスター、メモリー、実行状態を表示する GDB グラフィカルフロントエンド。",
            "ko": "소스, 중단점, 레지스터, 메모리 및 실행 상태를 보여 주는 GDB 그래픽 프런트엔드.",
        },
        "extra": {
            "zh-CN": "当前 Flathub 应用 ref 仅验证 x86_64。调试会启动或附加目标程序并可修改其状态；需本机 GDB 和适当进程访问权限，安装不自动调试程序。",
            "zh-TW": "目前 Flathub 應用程式 ref 僅驗證 x86_64。除錯會啟動或附加目標程式並可修改其狀態；需本機 GDB 和適當處理程序存取權限，安裝不自動除錯程式。",
            "en": "Only the x86_64 application ref is currently verified on Flathub. Debugging launches or attaches to targets and can alter their state. Local GDB and process-access permission are required; installation starts no debug session.",
            "ja": "現在 Flathub で確認できるアプリ ref は x86_64 のみです。デバッグでは対象の起動やアタッチ、状態変更を行います。ローカル GDB とプロセスアクセス権限が必要で、インストール時にデバッグは開始されません。",
            "ko": "현재 Flathub에서는 x86_64 애플리케이션 ref만 확인했습니다. 디버깅은 대상을 시작하거나 연결하고 상태를 변경할 수 있습니다. 로컬 GDB와 프로세스 접근 권한이 필요하며 설치 시 디버깅이 시작되지 않습니다.",
        },
    },
    "lrcget": {
        "name": "LRCGET", "publisher": "LRCLIB contributors", "category": "media", "mark": "LR", "accent": "#6487a4",
        "repo": "tranxuanthang/lrcget", "license": "MIT", "licenseFile": "src-tauri/Cargo.toml", "licenseRef": "HEAD/src-tauri/Cargo.toml",
        "homepage": "https://lrclib.net/", "flathub": "net.lrclib.lrcget",
        "summary": {
            "zh-CN": "本地音乐同步歌词工具，从 LRCLIB 检索、下载并嵌入或保存歌词。",
            "zh-TW": "本機音樂同步歌詞工具，從 LRCLIB 檢索、下載並嵌入或儲存歌詞。",
            "en": "A synchronized-lyrics tool for local music that retrieves and saves or embeds lyrics from LRCLIB.",
            "ja": "LRCLIB から同期歌詞を検索・取得し、保存または埋め込むローカル音楽向けツール。",
            "ko": "LRCLIB에서 동기화 가사를 검색·다운로드하여 저장하거나 삽입하는 로컬 음악 도구.",
        },
        "extra": {
            "zh-CN": "当前 Flathub 应用 ref 仅验证 x86_64。歌词检索会连接 LRCLIB 并发送曲目元数据；嵌入歌词会修改所选音频文件，安装不扫描或修改音乐库。",
            "zh-TW": "目前 Flathub 應用程式 ref 僅驗證 x86_64。歌詞檢索會連線 LRCLIB 並傳送曲目中繼資料；嵌入歌詞會修改所選音訊檔案，安裝不掃描或修改音樂庫。",
            "en": "Only the x86_64 application ref is currently verified on Flathub. Searches contact LRCLIB with track metadata. Embedding lyrics modifies selected audio files; installation neither scans nor changes a music library.",
            "ja": "現在 Flathub で確認できるアプリ ref は x86_64 のみです。検索時に曲のメタデータを LRCLIB へ送信します。歌詞の埋め込みは選択した音声ファイルを変更しますが、インストール時にライブラリのスキャンや変更は行いません。",
            "ko": "현재 Flathub에서는 x86_64 애플리케이션 ref만 확인했습니다. 검색 시 곡 메타데이터가 LRCLIB로 전송됩니다. 가사 삽입은 선택한 오디오 파일을 수정하지만 설치 시 음악 라이브러리를 검색하거나 변경하지 않습니다.",
        },
    },
    "nomacs": {
        "name": "nomacs Image Lounge", "publisher": "nomacs contributors", "category": "media", "mark": "Ni", "accent": "#6787a3",
        "repo": "nomacs/nomacs", "license": "GPL-3.0-or-later", "licenseFile": "ImageLounge/src/main.cpp", "licenseRef": "master/ImageLounge/src/main.cpp",
        "homepage": "https://nomacs.org/", "flathub": "org.nomacs.ImageLounge",
        "summary": {
            "zh-CN": "跨平台图片查看器，支持多种格式、基本编辑、元数据和图像比较。",
            "zh-TW": "跨平台圖片檢視器，支援多種格式、基本編輯、中繼資料和圖像比較。",
            "en": "A cross-platform image viewer with broad format support, basic editing, metadata and image comparison.",
            "ja": "多形式対応、基本編集、メタデータ表示、画像比較を備えるクロスプラットフォーム画像ビューアー。",
            "ko": "다양한 형식, 기본 편집, 메타데이터 및 이미지 비교를 지원하는 크로스 플랫폼 이미지 뷰어.",
        },
        "extra": {
            "zh-CN": "当前 Flathub 应用 ref 仅验证 x86_64。编辑和保存会修改或写出用户选择的图像；安装不导入图片或更改文件关联。",
            "zh-TW": "目前 Flathub 應用程式 ref 僅驗證 x86_64。編輯和儲存會修改或寫出使用者選擇的圖像；安裝不匯入圖片或更改檔案關聯。",
            "en": "Only the x86_64 application ref is currently verified on Flathub. Editing and saving modify or write user-selected images. Installation imports no images and changes no file associations.",
            "ja": "現在 Flathub で確認できるアプリ ref は x86_64 のみです。編集・保存は選択した画像を変更または書き出します。インストールで画像の読み込みやファイル関連付けの変更は行いません。",
            "ko": "현재 Flathub에서는 x86_64 애플리케이션 ref만 확인했습니다. 편집 및 저장은 선택한 이미지를 수정하거나 새로 저장합니다. 설치 시 이미지를 가져오거나 파일 연결을 변경하지 않습니다.",
        },
    },
    "media-downloader": {
        "name": "Media Downloader", "publisher": "Francis Banyikwa and contributors", "category": "media", "mark": "MD", "accent": "#6687a3",
        "repo": "mhogomchungu/media-downloader", "license": "GPL-2.0-or-later", "licenseFile": "src/main.cpp", "licenseRef": "main/src/main.cpp",
        "homepage": "https://mhogomchungu.github.io/media-downloader/", "flathub": "io.github.mhogomchungu.media-downloader",
        "summary": {
            "zh-CN": "媒体下载器图形前端，通过受支持下载引擎获取在线音视频和播放列表。",
            "zh-TW": "媒體下載器圖形前端，透過受支援下載引擎取得線上影音和播放清單。",
            "en": "A graphical media downloader that uses supported download engines for online audio, video and playlists.",
            "ja": "対応ダウンロードエンジンでオンライン音声・動画・プレイリストを取得する GUI メディアダウンローダー。",
            "ko": "지원되는 다운로드 엔진으로 온라인 오디오, 동영상 및 재생 목록을 받는 그래픽 미디어 다운로더.",
        },
        "extra": {
            "zh-CN": "当前 Flathub 应用 ref 仅验证 x86_64。下载会访问用户指定服务并写入文件；外部引擎、网站条款和内容版权由用户另行确认，安装不下载媒体。",
            "zh-TW": "目前 Flathub 應用程式 ref 僅驗證 x86_64。下載會存取使用者指定服務並寫入檔案；外部引擎、網站條款和內容版權由使用者另行確認，安裝不下載媒體。",
            "en": "Only the x86_64 application ref is currently verified on Flathub. Downloads contact user-selected services and write files. External engines, site terms and content rights require separate consideration; installation downloads no media.",
            "ja": "現在 Flathub で確認できるアプリ ref は x86_64 のみです。ダウンロードは指定サービスへ接続してファイルを保存します。外部エンジン、サイト規約、コンテンツの権利は個別確認が必要で、インストール時にメディアは取得しません。",
            "ko": "현재 Flathub에서는 x86_64 애플리케이션 ref만 확인했습니다. 다운로드는 선택한 서비스에 접근해 파일을 저장합니다. 외부 엔진, 사이트 약관 및 콘텐츠 권리는 별도 확인이 필요하며 설치 시 미디어를 받지 않습니다.",
        },
    },
    "saber": {
        "name": "Saber", "publisher": "Adil Hanney and contributors", "category": "office", "mark": "Sa", "accent": "#6487a4",
        "repo": "saber-notes/saber", "license": "GPL-3.0-only", "licenseFile": "LICENSE", "licenseRef": "main/LICENSE",
        "homepage": "https://github.com/saber-notes/saber", "flathub": "com.adilhanney.saber",
        "summary": {
            "zh-CN": "手写笔记应用，支持压感笔、画布、文档整理和本地或同步使用。",
            "zh-TW": "手寫筆記應用程式，支援壓感筆、畫布、文件整理和本機或同步使用。",
            "en": "A handwritten-notes app with stylus input, canvases, document organization and local or synchronized use.",
            "ja": "スタイラス入力、キャンバス、文書整理、ローカルまたは同期利用に対応する手書きノートアプリ。",
            "ko": "스타일러스 입력, 캔버스, 문서 정리 및 로컬 또는 동기화 사용을 지원하는 필기 노트 앱.",
        },
        "extra": {
            "zh-CN": "当前 Flathub 应用 ref 仅验证 x86_64。笔记保存在用户配置的位置；可选同步需要网络及对应服务/账号，安装不自动上传笔记。",
            "zh-TW": "目前 Flathub 應用程式 ref 僅驗證 x86_64。筆記儲存在使用者設定的位置；可選同步需要網路及對應服務／帳號，安裝不自動上傳筆記。",
            "en": "Only the x86_64 application ref is currently verified on Flathub. Notes are stored in the configured location. Optional synchronization requires network access and the corresponding service/account; installation uploads no notes.",
            "ja": "現在 Flathub で確認できるアプリ ref は x86_64 のみです。ノートは設定した場所に保存されます。オプションの同期にはネットワークと対応サービス／アカウントが必要で、インストール時にノートは送信されません。",
            "ko": "현재 Flathub에서는 x86_64 애플리케이션 ref만 확인했습니다. 노트는 구성된 위치에 저장됩니다. 선택적 동기화에는 네트워크와 해당 서비스/계정이 필요하며 설치 시 노트를 업로드하지 않습니다.",
        },
    },
    "ghostwriter": {
        "name": "ghostwriter", "publisher": "KDE and ghostwriter contributors", "category": "office", "mark": "Gw", "accent": "#6487a5",
        "repo": "KDE/ghostwriter", "sourceRepository": "https://invent.kde.org/office/ghostwriter", "license": "GPL-3.0-or-later", "licenseUrl": "https://invent.kde.org/office/ghostwriter/-/raw/master/LICENSES/GPL-3.0-or-later.txt",
        "homepage": "https://ghostwriter.kde.org/", "flathub": "org.kde.ghostwriter",
        "summary": {
            "zh-CN": "KDE Markdown 编辑器，提供专注写作、实时预览、主题和文档导航。",
            "zh-TW": "KDE Markdown 編輯器，提供專注寫作、即時預覽、主題和文件導覽。",
            "en": "A KDE Markdown editor with distraction-free writing, live preview, themes and document navigation.",
            "ja": "集中執筆、ライブプレビュー、テーマ、文書ナビゲーションを備える KDE Markdown エディター。",
            "ko": "집중 작성, 실시간 미리 보기, 테마 및 문서 탐색을 제공하는 KDE Markdown 편집기.",
        },
        "extra": {
            "zh-CN": "当前 Flathub 应用 ref 仅验证 x86_64。编辑和保存会写入用户选择的 Markdown 文档；预览插件和导出可能使用本地工具，安装不打开或修改文档。",
            "zh-TW": "目前 Flathub 應用程式 ref 僅驗證 x86_64。編輯和儲存會寫入使用者選擇的 Markdown 文件；預覽外掛和匯出可能使用本機工具，安裝不開啟或修改文件。",
            "en": "Only the x86_64 application ref is currently verified on Flathub. Editing saves user-selected Markdown documents. Preview plugins and exports may use local tools; installation opens or modifies no documents.",
            "ja": "現在 Flathub で確認できるアプリ ref は x86_64 のみです。編集・保存は選択した Markdown 文書へ書き込みます。プレビューやエクスポートはローカルツールを使う場合がありますが、インストール時に文書は変更されません。",
            "ko": "현재 Flathub에서는 x86_64 애플리케이션 ref만 확인했습니다. 편집 및 저장은 선택한 Markdown 문서에 기록합니다. 미리 보기와 파일 생성에는 로컬 도구가 사용될 수 있으며 설치 시 문서를 열거나 수정하지 않습니다.",
        },
    },
    "blanket": {
        "name": "Blanket", "publisher": "Rafael Mardojai CM and contributors", "category": "media", "mark": "Bl", "accent": "#6687a4",
        "repo": "rafaelmardojai/blanket", "license": "GPL-3.0-or-later", "licenseFile": "blanket/main.py", "licenseRef": "master/blanket/main.py",
        "homepage": "https://apps.gnome.org/Blanket/", "flathub": "com.rafaelmardojai.Blanket",
        "summary": {
            "zh-CN": "环境声音播放器，组合雨声、风声等背景音并调节各音轨音量。",
            "zh-TW": "環境聲音播放器，組合雨聲、風聲等背景音並調節各音軌音量。",
            "en": "An ambient-sound player that mixes rain, wind and other background tracks with individual volume controls.",
            "ja": "雨や風などの環境音を組み合わせ、トラックごとの音量を調整できるアンビエントサウンドプレーヤー。",
            "ko": "빗소리, 바람 소리 등 배경음을 조합하고 트랙별 음량을 조절하는 앰비언트 사운드 플레이어.",
        },
        "extra": {
            "zh-CN": "当前 Flathub 应用 ref 仅验证 x86_64。随附声音素材有独立许可说明；自定义音频由用户选择并保留其原有权利和许可。",
            "zh-TW": "目前 Flathub 應用程式 ref 僅驗證 x86_64。隨附聲音素材有獨立授權說明；自訂音訊由使用者選擇並保留其原有權利和授權。",
            "en": "Only the x86_64 application ref is currently verified on Flathub. Bundled sounds have separate license notices. User-selected custom audio retains its own rights and licensing.",
            "ja": "現在 Flathub で確認できるアプリ ref は x86_64 のみです。同梱サウンドには個別のライセンス表示があります。ユーザー追加の音声には元の権利とライセンスが適用されます。",
            "ko": "현재 Flathub에서는 x86_64 애플리케이션 ref만 확인했습니다. 번들 사운드에는 별도의 라이선스 고지가 있습니다. 사용자가 추가한 오디오에는 기존 권리와 라이선스가 적용됩니다.",
        },
    },
    "gittyup": {
        "name": "Gittyup", "publisher": "Murmele and contributors", "category": "dev", "mark": "Gu", "accent": "#6587a4",
        "repo": "Murmele/Gittyup", "license": "MIT", "licenseFile": "LICENSE.md", "licenseRef": "master/LICENSE.md",
        "homepage": "https://murmele.github.io/Gittyup/", "flathub": "com.github.Murmele.Gittyup",
        "summary": {
            "zh-CN": "图形 Git 客户端，查看历史、管理分支、暂存更改和解决合并冲突。",
            "zh-TW": "圖形 Git 用戶端，檢視歷史、管理分支、暫存變更和解決合併衝突。",
            "en": "A graphical Git client for history, branches, staged changes and merge-conflict resolution.",
            "ja": "履歴、ブランチ、ステージ変更、マージ競合解決を扱うグラフィカル Git クライアント。",
            "ko": "기록, 브랜치, 스테이징 변경 및 병합 충돌 해결을 위한 그래픽 Git 클라이언트.",
        },
        "extra": {
            "zh-CN": "当前 Flathub 应用 ref 仅验证 x86_64。提交、改写历史、拉取和推送会修改本地或远程仓库；凭据由用户配置，安装不自动操作仓库。",
            "zh-TW": "目前 Flathub 應用程式 ref 僅驗證 x86_64。提交、改寫歷史、拉取和推送會修改本機或遠端倉庫；憑證由使用者設定，安裝不自動操作倉庫。",
            "en": "Only the x86_64 application ref is currently verified on Flathub. Commits, history rewrites, pulls and pushes modify local or remote repositories. Credentials are user-configured; installation performs no repository operation.",
            "ja": "現在 Flathub で確認できるアプリ ref は x86_64 のみです。コミット、履歴書き換え、プル、プッシュはローカルまたはリモートリポジトリを変更します。認証情報はユーザーが設定し、インストール時にリポジトリ操作は行いません。",
            "ko": "현재 Flathub에서는 x86_64 애플리케이션 ref만 확인했습니다. 커밋, 기록 재작성, 풀 및 푸시는 로컬 또는 원격 저장소를 변경합니다. 자격 증명은 사용자가 구성하며 설치 시 저장소 작업을 수행하지 않습니다.",
        },
    },
    "openmw": {
        "name": "OpenMW", "publisher": "OpenMW contributors", "category": "media", "mark": "OM", "accent": "#6687a3",
        "repo": "OpenMW/openmw", "sourceRepository": "https://gitlab.com/OpenMW/openmw", "license": "GPL-3.0-or-later", "licenseUrl": "https://gitlab.com/OpenMW/openmw/-/raw/master/LICENSE",
        "homepage": "https://openmw.org/", "flathub": "org.openmw.OpenMW",
        "summary": {
            "zh-CN": "开源 Morrowind 游戏引擎重实现，提供启动器、游戏引擎和内容文件管理。",
            "zh-TW": "開源 Morrowind 遊戲引擎重實作，提供啟動器、遊戲引擎和內容檔案管理。",
            "en": "An open-source Morrowind engine reimplementation with a launcher, game engine and content-file management.",
            "ja": "ランチャー、ゲームエンジン、コンテンツファイル管理を備えるオープンソース Morrowind エンジン再実装。",
            "ko": "런처, 게임 엔진 및 콘텐츠 파일 관리를 제공하는 오픈 소스 Morrowind 엔진 재구현.",
        },
        "extra": {
            "zh-CN": "当前 Flathub 应用 ref 仅验证 x86_64。运行游戏需用户合法取得的原版 Morrowind 数据文件；安装不提供受版权保护的游戏内容或模组。",
            "zh-TW": "目前 Flathub 應用程式 ref 僅驗證 x86_64。執行遊戲需使用者合法取得的原版 Morrowind 資料檔案；安裝不提供受版權保護的遊戲內容或模組。",
            "en": "Only the x86_64 application ref is currently verified on Flathub. Playing requires legally obtained original Morrowind data files. Installation supplies neither copyrighted game content nor mods.",
            "ja": "現在 Flathub で確認できるアプリ ref は x86_64 のみです。プレイには正規に入手した Morrowind オリジナルデータが必要です。インストールで著作権のあるゲーム内容や MOD は提供されません。",
            "ko": "현재 Flathub에서는 x86_64 애플리케이션 ref만 확인했습니다. 플레이하려면 합법적으로 구한 원본 Morrowind 데이터 파일이 필요합니다. 설치 시 저작권이 있는 게임 콘텐츠나 모드는 제공되지 않습니다.",
        },
    },
    "ytmdesktop": {
        "name": "YouTube Music Desktop App", "publisher": "YTMD contributors", "category": "media", "mark": "YM", "accent": "#6787a3",
        "repo": "ytmdesktop/ytmdesktop", "license": "GPL-3.0-only", "licenseFile": "package.json", "licenseRef": "master/package.json",
        "homepage": "https://ytmdesktop.app/", "flathub": "app.ytmdesktop.ytmdesktop",
        "summary": {
            "zh-CN": "YouTube Music 桌面客户端，提供媒体键、播放控制和桌面集成。",
            "zh-TW": "YouTube Music 桌面用戶端，提供媒體鍵、播放控制和桌面整合。",
            "en": "A YouTube Music desktop client with media keys, playback controls and desktop integration.",
            "ja": "メディアキー、再生コントロール、デスクトップ統合を備える YouTube Music デスクトップクライアント。",
            "ko": "미디어 키, 재생 제어 및 데스크톱 통합을 제공하는 YouTube Music 데스크톱 클라이언트.",
        },
        "extra": {
            "zh-CN": "当前 Flathub 应用 ref 仅验证 x86_64。播放需连接 YouTube Music，部分功能需账号或订阅；服务内容和条款由 YouTube 管理，安装不登录账号。",
            "zh-TW": "目前 Flathub 應用程式 ref 僅驗證 x86_64。播放需連線 YouTube Music，部分功能需帳號或訂閱；服務內容和條款由 YouTube 管理，安裝不登入帳號。",
            "en": "Only the x86_64 application ref is currently verified on Flathub. Playback requires YouTube Music; some features require an account or subscription. YouTube controls service content and terms; installation signs in to no account.",
            "ja": "現在 Flathub で確認できるアプリ ref は x86_64 のみです。再生には YouTube Music 接続が必要で、一部機能にはアカウントやサブスクリプションが必要です。サービス内容と規約は YouTube が管理し、インストール時にログインは行いません。",
            "ko": "현재 Flathub에서는 x86_64 애플리케이션 ref만 확인했습니다. 재생에는 YouTube Music 연결이 필요하며 일부 기능은 계정 또는 구독이 필요합니다. 서비스 콘텐츠와 약관은 YouTube가 관리하며 설치 시 로그인하지 않습니다.",
        },
    },
    "stretchly": {
        "name": "Stretchly", "publisher": "Jan Hovancik and contributors", "category": "office", "mark": "St", "accent": "#6487a4",
        "repo": "hovancik/stretchly", "license": "BSD-2-Clause", "licenseFile": "LICENSE", "licenseRef": "master/LICENSE",
        "homepage": "https://hovancik.net/stretchly/", "flathub": "net.hovancik.Stretchly",
        "summary": {
            "zh-CN": "休息提醒应用，按可配置计划提示短暂休息和较长休息。",
            "zh-TW": "休息提醒應用程式，依可設定計畫提示短暫休息和較長休息。",
            "en": "A break reminder that prompts short and longer breaks on a configurable schedule.",
            "ja": "設定可能なスケジュールで短い休憩と長い休憩を通知する休憩リマインダー。",
            "ko": "설정 가능한 일정에 따라 짧은 휴식과 긴 휴식을 알려 주는 휴식 알림 앱.",
        },
        "extra": {
            "zh-CN": "当前 Flathub 应用 ref 仅验证 x86_64。提醒可显示全屏休息界面并影响当前操作；开机启动和休息计划由用户设置，安装不自动跳过提醒。",
            "zh-TW": "目前 Flathub 應用程式 ref 僅驗證 x86_64。提醒可顯示全螢幕休息介面並影響目前操作；開機啟動和休息計畫由使用者設定，安裝不自動跳過提醒。",
            "en": "Only the x86_64 application ref is currently verified on Flathub. Reminders can show full-screen breaks and interrupt current activity. Autostart and schedules are user-controlled; installation does not skip breaks.",
            "ja": "現在 Flathub で確認できるアプリ ref は x86_64 のみです。全画面の休憩表示で操作中の作業が中断される場合があります。自動起動とスケジュールはユーザー設定で、インストール時に休憩はスキップされません。",
            "ko": "현재 Flathub에서는 x86_64 애플리케이션 ref만 확인했습니다. 전체 화면 휴식 알림이 현재 작업을 방해할 수 있습니다. 자동 시작과 일정은 사용자가 설정하며 설치 시 휴식을 걸러내지 않습니다.",
        },
    },
    "librecad": {
        "name": "LibreCAD", "publisher": "LibreCAD contributors", "category": "dev", "mark": "LC", "accent": "#6387a4",
        "repo": "LibreCAD/LibreCAD", "license": "GPL-2.0-only", "licenseFile": "LICENSE", "licenseRef": "master/LICENSE",
        "homepage": "https://librecad.org/", "flathub": "org.librecad.librecad",
        "summary": {
            "zh-CN": "二维 CAD 绘图应用，用于技术图纸、图层、尺寸标注和 DXF 文档。",
            "zh-TW": "二維 CAD 繪圖應用程式，用於技術圖紙、圖層、尺寸標註和 DXF 文件。",
            "en": "A 2D CAD application for technical drawings, layers, dimensions and DXF documents.",
            "ja": "技術図面、レイヤー、寸法、DXF 文書に対応する 2D CAD アプリケーション。",
            "ko": "기술 도면, 레이어, 치수 및 DXF 문서를 위한 2D CAD 애플리케이션.",
        },
        "extra": {
            "zh-CN": "当前 Flathub 应用 ref 仅验证 x86_64。保存或导出会写入用户选择的图纸；格式兼容性取决于文件内容和导出选项，安装不修改现有图纸。",
            "zh-TW": "目前 Flathub 應用程式 ref 僅驗證 x86_64。儲存或匯出會寫入使用者選擇的圖紙；格式相容性取決於檔案內容和匯出選項，安裝不修改現有圖紙。",
            "en": "Only the x86_64 application ref is currently verified on Flathub. Saving and exporting write selected drawings. Format compatibility depends on document content and export options; installation changes no existing drawings.",
            "ja": "現在 Flathub で確認できるアプリ ref は x86_64 のみです。保存・エクスポートは選択した図面へ書き込みます。形式互換性は内容と設定に依存し、インストールで既存図面は変更されません。",
            "ko": "현재 Flathub에서는 x86_64 애플리케이션 ref만 확인했습니다. 저장 및 파일 생성은 선택한 도면에 기록합니다. 형식 호환성은 문서 내용과 설정에 따라 달라지며 설치 시 기존 도면을 변경하지 않습니다.",
        },
    },
    "ludusavi": {
        "name": "Ludusavi", "publisher": "Matthew Kennerly and contributors", "category": "media", "mark": "Lu", "accent": "#6687a4",
        "repo": "mtkennerly/ludusavi", "license": "MIT", "licenseFile": "LICENSE", "licenseRef": "master/LICENSE",
        "homepage": "https://github.com/mtkennerly/ludusavi", "flathub": "com.github.mtkennerly.ludusavi",
        "summary": {
            "zh-CN": "PC 游戏存档备份工具，扫描已支持游戏并备份、预览和恢复存档。",
            "zh-TW": "PC 遊戲存檔備份工具，掃描已支援遊戲並備份、預覽和還原存檔。",
            "en": "A PC game-save backup tool that scans supported games and backs up, previews and restores saves.",
            "ja": "対応 PC ゲームをスキャンし、セーブデータのバックアップ、プレビュー、復元を行うツール。",
            "ko": "지원되는 PC 게임을 검색하고 세이브 파일을 백업, 미리 보기 및 복원하는 도구.",
        },
        "extra": {
            "zh-CN": "当前 Flathub 应用 ref 仅验证 x86_64。扫描会读取游戏目录；恢复会覆盖所选存档，操作前应检查预览和备份。安装不自动扫描、备份或恢复。",
            "zh-TW": "目前 Flathub 應用程式 ref 僅驗證 x86_64。掃描會讀取遊戲目錄；還原會覆蓋所選存檔，操作前應檢查預覽和備份。安裝不自動掃描、備份或還原。",
            "en": "Only the x86_64 application ref is currently verified on Flathub. Scans read game directories; restores overwrite selected saves. Review previews and backups before restoring. Installation performs no scan, backup or restore.",
            "ja": "現在 Flathub で確認できるアプリ ref は x86_64 のみです。スキャンはゲームディレクトリを読み、復元は選択したセーブを上書きします。復元前にプレビューとバックアップを確認してください。インストール時に自動実行はされません。",
            "ko": "현재 Flathub에서는 x86_64 애플리케이션 ref만 확인했습니다. 검색은 게임 디렉터리를 읽고 복원은 선택한 세이브를 덮어씁니다. 복원 전에 미리 보기와 백업을 확인하세요. 설치 시 검색, 백업 또는 복원을 수행하지 않습니다.",
        },
    },
    "piper": {
        "name": "Piper", "publisher": "libratbag contributors", "category": "system", "mark": "Pi", "accent": "#6487a4",
        "repo": "libratbag/piper", "license": "GPL-2.0-or-later", "licenseFile": "COPYING", "licenseRef": "master/COPYING",
        "homepage": "https://github.com/libratbag/piper", "flathub": "org.freedesktop.Piper",
        "summary": {
            "zh-CN": "libratbag 图形配置前端，用于受支持游戏鼠标的按键、DPI 和 LED 设置。",
            "zh-TW": "libratbag 圖形設定前端，用於受支援遊戲滑鼠的按鍵、DPI 和 LED 設定。",
            "en": "A graphical libratbag frontend for buttons, DPI and LED settings on supported gaming mice.",
            "ja": "対応ゲーミングマウスのボタン、DPI、LED を設定する libratbag グラフィカルフロントエンド。",
            "ko": "지원되는 게이밍 마우스의 버튼, DPI 및 LED를 설정하는 libratbag 그래픽 프런트엔드.",
        },
        "extra": {
            "zh-CN": "当前 Flathub 应用 ref 仅验证 x86_64。需系统安装并运行 ratbagd 及设备访问权限；写入配置会改变设备行为，安装不修改鼠标设置。",
            "zh-TW": "目前 Flathub 應用程式 ref 僅驗證 x86_64。需系統安裝並執行 ratbagd 及裝置存取權限；寫入設定會改變裝置行為，安裝不修改滑鼠設定。",
            "en": "Only the x86_64 application ref is currently verified on Flathub. System ratbagd and device-access permission are required. Applying settings changes device behavior; installation changes no mouse settings.",
            "ja": "現在 Flathub で確認できるアプリ ref は x86_64 のみです。システムの ratbagd とデバイスアクセス権限が必要です。設定適用でデバイスの動作が変わりますが、インストール時にマウス設定は変更されません。",
            "ko": "현재 Flathub에서는 x86_64 애플리케이션 ref만 확인했습니다. 시스템 ratbagd와 장치 접근 권한이 필요합니다. 설정 적용은 장치 동작을 변경하지만 설치 시 마우스 설정을 바꾸지 않습니다.",
        },
    },
    "pdf-arranger": {
        "name": "PDF Arranger", "publisher": "PDF Arranger contributors", "category": "office", "mark": "PA", "accent": "#6687a4",
        "repo": "pdfarranger/pdfarranger", "license": "GPL-3.0-or-later", "licenseFile": "COPYING", "licenseRef": "master/COPYING",
        "homepage": "https://github.com/pdfarranger/pdfarranger", "flathub": "com.github.jeromerobert.pdfarranger",
        "summary": {
            "zh-CN": "PDF 页面整理工具，支持合并、拆分、旋转、裁剪和重新排列页面。",
            "zh-TW": "PDF 頁面整理工具，支援合併、拆分、旋轉、裁剪和重新排列頁面。",
            "en": "A PDF page organizer for merging, splitting, rotating, cropping and rearranging pages.",
            "ja": "PDF ページの結合、分割、回転、切り抜き、並べ替えを行うページ整理ツール。",
            "ko": "PDF 페이지를 병합, 분할, 회전, 자르기 및 재배열하는 페이지 정리 도구.",
        },
        "extra": {
            "zh-CN": "当前 Flathub 应用 ref 仅验证 x86_64。保存会创建或覆盖用户选择的 PDF；加密/特殊文档能力取决于内容和支持库，安装不修改文档。",
            "zh-TW": "目前 Flathub 應用程式 ref 僅驗證 x86_64。儲存會建立或覆蓋使用者選擇的 PDF；加密／特殊文件能力取決於內容和支援函式庫，安裝不修改文件。",
            "en": "Only the x86_64 application ref is currently verified on Flathub. Saving creates or overwrites a selected PDF. Encrypted and unusual-document support depends on content and supporting libraries; installation changes no documents.",
            "ja": "現在 Flathub で確認できるアプリ ref は x86_64 のみです。保存は選択した PDF を作成または上書きします。暗号化・特殊文書の対応は内容とライブラリに依存し、インストールで文書は変更されません。",
            "ko": "현재 Flathub에서는 x86_64 애플리케이션 ref만 확인했습니다. 저장은 선택한 PDF를 만들거나 덮어씁니다. 암호화 및 특수 문서 지원은 내용과 지원 라이브러리에 따라 달라지며 설치 시 문서를 변경하지 않습니다.",
        },
    },
    "jellyfin-desktop": {
        "name": "Jellyfin Desktop", "publisher": "Jellyfin contributors", "category": "media", "mark": "JD", "accent": "#6687a4",
        "repo": "jellyfin/jellyfin-desktop", "license": "GPL-2.0-only", "licenseFile": "LICENSE", "licenseRef": "master/LICENSE",
        "homepage": "https://github.com/jellyfin/jellyfin-desktop", "flathub": "org.jellyfin.JellyfinDesktop",
        "summary": {
            "zh-CN": "Jellyfin 媒体服务器桌面客户端，整合网页界面与本地媒体播放。",
            "zh-TW": "Jellyfin 媒體伺服器桌面用戶端，整合網頁介面與本機媒體播放。",
            "en": "A Jellyfin media-server desktop client combining the web interface with local media playback.",
            "ja": "Web インターフェースとローカルメディア再生を統合する Jellyfin メディアサーバー用デスクトップクライアント。",
            "ko": "웹 인터페이스와 로컬 미디어 재생을 결합한 Jellyfin 미디어 서버용 데스크톱 클라이언트.",
        },
        "extra": {
            "zh-CN": "当前 Flathub 应用 ref 仅验证 x86_64。使用需用户配置可访问的 Jellyfin 服务器；服务器地址和凭据由用户输入，安装不配置服务器或账号。",
            "zh-TW": "目前 Flathub 應用程式 ref 僅驗證 x86_64。使用需使用者設定可存取的 Jellyfin 伺服器；伺服器位址和憑證由使用者輸入，安裝不設定伺服器或帳號。",
            "en": "Only the x86_64 application ref is currently verified on Flathub. A reachable user-configured Jellyfin server is required. Server addresses and credentials are user-supplied; installation configures neither.",
            "ja": "現在 Flathub で確認できるアプリ ref は x86_64 のみです。接続可能な Jellyfin サーバーの設定が必要です。アドレスと認証情報はユーザーが入力し、インストールでサーバーやアカウントは設定されません。",
            "ko": "현재 Flathub에서는 x86_64 애플리케이션 ref만 확인했습니다. 접근 가능한 Jellyfin 서버를 사용자가 구성해야 합니다. 서버 주소와 자격 증명은 사용자가 입력하며 설치 시 서버나 계정을 설정하지 않습니다.",
        },
    },
    "qownnotes": {
        "name": "QOwnNotes", "publisher": "Patrizio Bekerle and contributors", "category": "office", "mark": "QO", "accent": "#6487a4",
        "repo": "pbek/QOwnNotes", "license": "GPL-2.0-or-later", "licenseFile": "LICENSE", "licenseRef": "main/LICENSE",
        "homepage": "https://www.qownnotes.org/", "flathub": "org.qownnotes.QOwnNotes",
        "summary": {
            "zh-CN": "纯文本 Markdown 笔记应用，支持本地文件、标签、待办和可选 Nextcloud 集成。",
            "zh-TW": "純文字 Markdown 筆記應用程式，支援本機檔案、標籤、待辦和可選 Nextcloud 整合。",
            "en": "A plain-text Markdown notes app with local files, tags, tasks and optional Nextcloud integration.",
            "ja": "ローカルファイル、タグ、タスク、オプションの Nextcloud 統合を備えるプレーンテキスト Markdown ノートアプリ。",
            "ko": "로컬 파일, 태그, 작업 및 선택적 Nextcloud 통합을 지원하는 일반 텍스트 Markdown 노트 앱.",
        },
        "extra": {
            "zh-CN": "当前 Flathub 应用 ref 仅验证 x86_64。笔记以文件保存；可选 Nextcloud/同步功能会连接用户配置的服务并传输笔记，安装不启用同步。",
            "zh-TW": "目前 Flathub 應用程式 ref 僅驗證 x86_64。筆記以檔案儲存；可選 Nextcloud／同步功能會連線使用者設定的服務並傳輸筆記，安裝不啟用同步。",
            "en": "Only the x86_64 application ref is currently verified on Flathub. Notes are stored as files. Optional Nextcloud/synchronization features contact configured services and transfer notes; installation enables no synchronization.",
            "ja": "現在 Flathub で確認できるアプリ ref は x86_64 のみです。ノートはファイルとして保存されます。オプションの Nextcloud／同期は設定したサービスへノートを送信しますが、インストール時に同期は有効化されません。",
            "ko": "현재 Flathub에서는 x86_64 애플리케이션 ref만 확인했습니다. 노트는 파일로 저장됩니다. 선택적 Nextcloud/동기화 기능은 구성된 서비스와 노트를 주고받으며 설치 시 동기화를 활성화하지 않습니다.",
        },
    },
    "wayback": {
        "name": "wayback", "publisher": "wabarc and contributors", "category": "tools", "mark": "Wb", "accent": "#6587a4",
        "repo": "wabarc/wayback", "license": "GPL-3.0-only", "licenseFile": "LICENSE", "licenseRef": "main/LICENSE",
        "homepage": "https://github.com/wabarc/wayback", "flathub": "org.eu.wabarc.wayback",
        "summary": {
            "zh-CN": "网页归档命令行工具，将指定页面提交到一个或多个公共归档服务。",
            "zh-TW": "網頁封存命令列工具，將指定頁面提交到一個或多個公共封存服務。",
            "en": "A command-line web archiver that submits selected pages to one or more public archiving services.",
            "ja": "指定した Web ページを一つ以上の公開アーカイブサービスへ送信するコマンドラインアーカイバー。",
            "ko": "선택한 웹 페이지를 하나 이상의 공개 아카이브 서비스에 제출하는 명령줄 아카이버.",
        },
        "extra": {
            "zh-CN": "当前 Flathub 应用 ref 仅验证 x86_64。归档会把 URL 及可访问页面内容发送给外部服务，可能形成公开且难以删除的副本；仅提交有权公开的内容。",
            "zh-TW": "目前 Flathub 應用程式 ref 僅驗證 x86_64。封存會將 URL 及可存取頁面內容傳送給外部服務，可能形成公開且難以刪除的副本；僅提交有權公開的內容。",
            "en": "Only the x86_64 application ref is currently verified on Flathub. Archiving sends URLs and accessible page content to external services and may create public, hard-to-delete copies. Submit only content you may publish.",
            "ja": "現在 Flathub で確認できるアプリ ref は x86_64 のみです。URL とアクセス可能なページ内容を外部サービスへ送信し、公開され削除困難な複製が作られる場合があります。公開権限のある内容だけを送信してください。",
            "ko": "현재 Flathub에서는 x86_64 애플리케이션 ref만 확인했습니다. 아카이빙은 URL과 접근 가능한 페이지 내용을 외부 서비스에 전송하며 공개되고 삭제하기 어려운 사본이 생길 수 있습니다. 공개 권한이 있는 내용만 제출하세요.",
        },
    },
    "cava": {
        "name": "Cava", "publisher": "Karl Stavestrand and contributors", "category": "media", "mark": "Ca", "accent": "#6687a4",
        "repo": "karlstav/cava", "license": "MIT", "licenseFile": "LICENSE", "licenseRef": "master/LICENSE",
        "homepage": "https://github.com/karlstav/cava", "formula": "cava",
        "summary": {
            "zh-CN": "终端音频可视化工具，将实时音频输入显示为可配置的频谱。",
            "zh-TW": "終端音訊視覺化工具，將即時音訊輸入顯示為可設定頻譜。",
            "en": "A terminal audio visualizer that renders live audio input as a configurable spectrum.",
            "ja": "ライブ音声入力を設定可能なスペクトルとして表示するターミナルオーディオビジュアライザー。",
            "ko": "실시간 오디오 입력을 설정 가능한 스펙트럼으로 표시하는 터미널 오디오 시각화 도구.",
        },
        "extra": {
            "zh-CN": "Homebrew formula 仅提供本目录已验证的 macOS 架构，并含 Intel macOS bottle。使用需可访问的音频输入或支持的播放后端，安装不开始捕获音频。",
            "zh-TW": "Homebrew formula 僅提供本目錄已驗證的 macOS 架構，並含 Intel macOS bottle。使用需可存取的音訊輸入或支援的播放後端，安裝不開始擷取音訊。",
            "en": "The Homebrew formula provides the macOS architectures verified for this catalog, including an Intel-macOS bottle. Accessible audio input or a supported playback backend is required; installation starts no audio capture.",
            "ja": "Homebrew formula はこのカタログで確認済みの macOS アーキテクチャを提供し、Intel macOS bottle もあります。音声入力または対応再生バックエンドが必要で、インストール時に音声キャプチャは開始されません。",
            "ko": "Homebrew formula는 이 카탈로그에서 확인된 macOS 아키텍처를 제공하며 Intel macOS bottle도 포함합니다. 접근 가능한 오디오 입력 또는 지원되는 재생 백엔드가 필요하며 설치 시 오디오 캡처를 시작하지 않습니다.",
        },
    },
    "taskwarrior": {
        "name": "Taskwarrior", "publisher": "Gothenburg Bit Factory and contributors", "category": "office", "mark": "Tw", "accent": "#6587a4",
        "repo": "GothenburgBitFactory/taskwarrior", "license": "MIT", "licenseFile": "LICENSE", "licenseRef": "develop/LICENSE",
        "homepage": "https://taskwarrior.org/", "formula": "task",
        "summary": {
            "zh-CN": "命令行任务管理器，支持项目、标签、优先级、到期日和本地任务数据。",
            "zh-TW": "命令列任務管理器，支援專案、標籤、優先順序、到期日和本機任務資料。",
            "en": "A command-line task manager with projects, tags, priorities, due dates and local task data.",
            "ja": "プロジェクト、タグ、優先度、期限、ローカルタスクデータを扱うコマンドラインタスクマネージャー。",
            "ko": "프로젝트, 태그, 우선순위, 마감일 및 로컬 작업 데이터를 지원하는 명령줄 작업 관리자.",
        },
        "extra": {
            "zh-CN": "Homebrew 包名为 task，仅提供本目录已验证的 macOS 架构。任务写入本地数据目录；可选同步需另行配置服务，安装不创建任务或启用同步。",
            "zh-TW": "Homebrew 套件名為 task，僅提供本目錄已驗證的 macOS 架構。任務寫入本機資料目錄；可選同步需另行設定服務，安裝不建立任務或啟用同步。",
            "en": "The Homebrew package is named task and provides only the macOS architectures verified for this catalog. Tasks are written to a local data directory. Optional sync requires separate service configuration; installation creates no tasks and enables no sync.",
            "ja": "Homebrew パッケージ名は task で、このカタログで確認済みの macOS アーキテクチャのみを提供します。タスクはローカルデータに保存されます。同期には別途設定が必要で、インストール時にタスク作成や同期有効化は行いません。",
            "ko": "Homebrew 패키지 이름은 task이며 이 카탈로그에서 확인된 macOS 아키텍처만 제공합니다. 작업은 로컬 데이터 디렉터리에 저장됩니다. 선택적 동기화에는 별도 서비스 구성이 필요하며 설치 시 작업을 만들거나 동기화를 켜지 않습니다.",
        },
    },
    "dua-cli": {
        "name": "Dua CLI", "publisher": "Sebastian Thiel and contributors", "category": "system", "mark": "Du", "accent": "#6487a4",
        "repo": "Byron/dua-cli", "license": "MIT", "licenseFile": "LICENSE", "licenseRef": "main/LICENSE",
        "homepage": "https://github.com/Byron/dua-cli", "formula": "dua-cli",
        "linuxReleases": [
            {"architecture": "x86_64", "format": "tar_gz", "asset": "dua-v*-x86_64-unknown-linux-musl.tar.gz", "executable": "dua", "stripComponents": 1, "linkName": "dua", "detectProgram": "dua"},
            {"architecture": "aarch64", "format": "tar_gz", "asset": "dua-v*-aarch64-unknown-linux-musl.tar.gz", "executable": "dua", "stripComponents": 1, "linkName": "dua", "detectProgram": "dua"},
        ],
        "linuxExtra": {
            "zh-CN": "Linux 使用上游 musl 静态归档，不经过 Homebrew。删除模式会永久移除所选文件，运行前检查路径，安装不扫描或删除文件。",
            "zh-TW": "Linux 使用上游 musl 靜態封存檔，不透過 Homebrew。刪除模式會永久移除所選檔案，執行前檢查路徑，安裝不掃描或刪除檔案。",
            "en": "Linux uses the upstream musl static archive, not Homebrew. Deletion mode permanently removes selected files. Check paths before use; installation scans or deletes nothing.",
            "ja": "Linux では Homebrew を使わず、上流の musl 静的アーカイブを使用します。削除モードは選択ファイルを完全に削除します。事前にパスを確認してください。インストール時にスキャンや削除は行いません。",
            "ko": "Linux에서는 Homebrew가 아니라 업스트림 musl 정적 아카이브를 사용합니다. 삭제 모드는 선택한 파일을 영구 삭제합니다. 실행 전 경로를 확인하세요. 설치 시 검색하거나 삭제하지 않습니다.",
        },
        "summary": {
            "zh-CN": "终端磁盘用量分析器，交互浏览目录大小并可选删除条目。",
            "zh-TW": "終端磁碟用量分析器，互動瀏覽目錄大小並可選刪除項目。",
            "en": "A terminal disk-usage analyzer for interactive directory-size browsing and optional entry deletion.",
            "ja": "ディレクトリサイズを対話的に表示し、必要に応じて項目を削除できるターミナルディスク使用量分析ツール。",
            "ko": "디렉터리 크기를 대화형으로 탐색하고 선택적으로 항목을 삭제하는 터미널 디스크 사용량 분석기.",
        },
        "extra": {
            "zh-CN": "Homebrew 仅提供本目录已验证的 macOS arm64 bottle，未声明 Intel macOS。删除模式会永久移除所选文件，运行前检查路径，安装不扫描或删除文件。",
            "zh-TW": "Homebrew 僅提供本目錄已驗證的 macOS arm64 bottle，未宣告 Intel macOS。刪除模式會永久移除所選檔案，執行前檢查路徑，安裝不掃描或刪除檔案。",
            "en": "Homebrew provides only the macOS arm64 bottle verified for this catalog; Intel macOS is not declared. Deletion mode permanently removes selected files. Check paths before use; installation scans or deletes nothing.",
            "ja": "Homebrew はこのカタログで確認済みの macOS arm64 bottle のみを提供し、Intel macOS は対象外です。削除モードは選択ファイルを完全に削除します。事前にパスを確認してください。インストール時にスキャンや削除は行いません。",
            "ko": "Homebrew는 이 카탈로그에서 확인된 macOS arm64 bottle만 제공하며 Intel macOS는 선언하지 않습니다. 삭제 모드는 선택한 파일을 영구 삭제합니다. 실행 전 경로를 확인하세요. 설치 시 검색하거나 삭제하지 않습니다.",
        },
    },
    "gowall": {
        "name": "Gowall", "publisher": "Achno and contributors", "category": "media", "mark": "Go", "accent": "#6687a4",
        "repo": "Achno/gowall", "license": "MIT", "licenseFile": "LICENSE", "licenseRef": "main/LICENSE",
        "homepage": "https://github.com/Achno/gowall", "formula": "gowall",
        "linuxReleases": [
            {"architecture": "x86_64", "format": "tar_gz", "asset": "gowall-amd64-linux.tar.gz", "executable": "gowall", "stripComponents": 0, "linkName": "gowall", "detectProgram": "gowall"},
        ],
        "linuxExtra": {
            "zh-CN": "Linux 仅验证 x86_64 上游 GitHub Release 归档；当前稳定版没有 Linux ARM64 资产。转换会写出或覆盖用户指定图像，安装不处理图片或更改壁纸。",
            "zh-TW": "Linux 僅驗證 x86_64 上游 GitHub Release 封存檔；目前穩定版沒有 Linux ARM64 資產。轉換會寫出或覆蓋使用者指定圖像，安裝不處理圖片或更改桌布。",
            "en": "Only the x86_64 upstream Linux GitHub Release archive is verified; the current stable release has no Linux ARM64 asset. Conversion writes or overwrites selected images. Installation processes no images and changes no wallpaper.",
            "ja": "Linux では x86_64 の上流 GitHub Release アーカイブのみを検証済みです。現在の安定版に Linux ARM64 資産はありません。変換は指定画像を出力または上書きしますが、インストール時に画像処理や壁紙変更は行いません。",
            "ko": "Linux에서는 x86_64 업스트림 GitHub Release 아카이브만 확인했으며 현재 안정 릴리스에는 Linux ARM64 자산이 없습니다. 변환은 지정한 이미지를 쓰거나 덮어씁니다. 설치 시 이미지를 처리하거나 배경 화면을 바꾸지 않습니다.",
        },
        "summary": {
            "zh-CN": "命令行图像转换工具，用于壁纸转换、调色板、缩放和批量图像处理。",
            "zh-TW": "命令列圖像轉換工具，用於桌布轉換、調色盤、縮放和批次圖像處理。",
            "en": "A command-line image converter for wallpapers, palettes, resizing and batch image processing.",
            "ja": "壁紙変換、パレット、リサイズ、バッチ画像処理を行うコマンドライン画像コンバーター。",
            "ko": "배경 화면 변환, 팔레트, 크기 조정 및 일괄 이미지 처리를 위한 명령줄 이미지 변환기.",
        },
        "extra": {
            "zh-CN": "Homebrew 仅提供本目录已验证的 macOS arm64 bottle，未声明 Intel macOS。转换会写出或覆盖用户指定图像，安装不处理图片或更改壁纸。",
            "zh-TW": "Homebrew 僅提供本目錄已驗證的 macOS arm64 bottle，未宣告 Intel macOS。轉換會寫出或覆蓋使用者指定圖像，安裝不處理圖片或更改桌布。",
            "en": "Homebrew provides only the macOS arm64 bottle verified for this catalog; Intel macOS is not declared. Conversion writes or overwrites selected images. Installation processes no images and changes no wallpaper.",
            "ja": "Homebrew はこのカタログで確認済みの macOS arm64 bottle のみを提供し、Intel macOS は対象外です。変換は指定画像を出力または上書きしますが、インストール時に画像処理や壁紙変更は行いません。",
            "ko": "Homebrew는 이 카탈로그에서 확인된 macOS arm64 bottle만 제공하며 Intel macOS는 선언하지 않습니다. 변환은 지정한 이미지를 쓰거나 덮어씁니다. 설치 시 이미지를 처리하거나 배경 화면을 바꾸지 않습니다.",
        },
    },
    "termusic": {
        "name": "Termusic", "publisher": "tramhao and contributors", "category": "media", "mark": "Te", "accent": "#6587a4",
        "repo": "tramhao/termusic", "license": "MIT AND GPL-3.0-or-later", "licenseFile": "LICENSE_MIT", "licenseRef": "master/LICENSE_MIT",
        "homepage": "https://github.com/tramhao/termusic", "formula": "termusic",
        "summary": {
            "zh-CN": "终端音乐与播客播放器，支持本地媒体库、播放列表和可选服务端。",
            "zh-TW": "終端音樂與播客播放器，支援本機媒體庫、播放清單和可選伺服端。",
            "en": "A terminal music and podcast player with a local library, playlists and an optional server component.",
            "ja": "ローカルライブラリ、プレイリスト、オプションのサーバー機能を備えるターミナル音楽・ポッドキャストプレーヤー。",
            "ko": "로컬 라이브러리, 재생 목록 및 선택적 서버 구성 요소를 갖춘 터미널 음악·팟캐스트 플레이어.",
        },
        "extra": {
            "zh-CN": "Homebrew formula 仅提供本目录已验证的 macOS 架构，并含 Intel macOS bottle。组合许可含 MIT 与 GPL-3.0-or-later；播客功能联网，安装不扫描媒体或启动服务。",
            "zh-TW": "Homebrew formula 僅提供本目錄已驗證的 macOS 架構，並含 Intel macOS bottle。組合授權含 MIT 與 GPL-3.0-or-later；播客功能連網，安裝不掃描媒體或啟動服務。",
            "en": "The Homebrew formula provides only the macOS architectures verified for this catalog, including an Intel-macOS bottle. The combined grant includes MIT and GPL-3.0-or-later. Podcasts use the network; installation scans no media and starts no server.",
            "ja": "Homebrew formula はこのカタログで確認済みの macOS アーキテクチャのみを提供し、Intel macOS bottle もあります。組合せライセンスは MIT と GPL-3.0-or-later を含みます。ポッドキャストは通信を行いますが、インストール時にメディアスキャンやサーバー起動は行いません。",
            "ko": "Homebrew formula는 이 카탈로그에서 확인된 macOS 아키텍처만 제공하며 Intel macOS bottle도 포함합니다. 결합 라이선스에는 MIT와 GPL-3.0-or-later가 포함됩니다. 팟캐스트는 네트워크를 사용하지만 설치 시 미디어를 검색하거나 서버를 시작하지 않습니다.",
        },
    },
    "slides": {
        "name": "Slides", "publisher": "Maas Lalani and contributors", "category": "office", "mark": "Sl", "accent": "#6587a4",
        "repo": "maaslalani/slides", "license": "MIT", "licenseFile": "LICENSE", "licenseRef": "main/LICENSE",
        "homepage": "https://maaslalani.com/slides/", "formula": "slides",
        "summary": {
            "zh-CN": "终端演示工具，使用 Markdown 编写幻灯片并在终端中展示。",
            "zh-TW": "終端簡報工具，使用 Markdown 編寫投影片並在終端機中展示。",
            "en": "A terminal presentation tool for writing slides in Markdown and presenting them in the terminal.",
            "ja": "Markdown でスライドを作成し、ターミナルで発表するためのターミナルプレゼンテーションツール。",
            "ko": "Markdown으로 슬라이드를 작성하고 터미널에서 발표하는 터미널 프레젠테이션 도구.",
        },
        "extra": {
            "zh-CN": "Homebrew formula 提供本目录已验证的 Apple Silicon 与 Intel macOS bottle。使用时打开用户指定的本地 Markdown 演示文件；安装不打开演示、不修改文件。",
            "zh-TW": "Homebrew formula 提供本目錄已驗證的 Apple Silicon 與 Intel macOS bottle。使用時開啟使用者指定的本機 Markdown 簡報檔；安裝不開啟簡報、不修改檔案。",
            "en": "The Homebrew formula provides the Apple Silicon and Intel macOS bottles verified for this catalog. Usage opens a user-selected local Markdown presentation; installation opens no presentation and modifies no files.",
            "ja": "Homebrew formula は、このカタログで確認済みの Apple Silicon と Intel macOS bottle を提供します。使用時にユーザーが選択したローカル Markdown プレゼンテーションを開きます。インストール時にプレゼンテーションを開かず、ファイルも変更しません。",
            "ko": "Homebrew formula는 이 카탈로그에서 확인된 Apple Silicon 및 Intel macOS bottle을 제공합니다. 사용 시 사용자가 선택한 로컬 Markdown 프레젠테이션을 열며, 설치 시 프레젠테이션을 열거나 파일을 수정하지 않습니다.",
        },
    },
    "micro": {
        "name": "Micro", "publisher": "Zachary Yedidia and contributors", "category": "dev", "mark": "Mi", "accent": "#6587a4",
        "repo": "micro-editor/micro", "license": "MIT", "licenseFile": "LICENSE", "licenseRef": "master/LICENSE",
        "homepage": "https://github.com/micro-editor/micro", "formula": "micro",
        "summary": {
            "zh-CN": "现代终端文本编辑器，提供常用快捷键、语法高亮、多缓冲和插件支持。",
            "zh-TW": "現代終端機文字編輯器，提供常用快捷鍵、語法突顯、多緩衝區和外掛支援。",
            "en": "A modern terminal text editor with familiar shortcuts, syntax highlighting, multiple buffers and plugins.",
            "ja": "使い慣れたショートカット、構文ハイライト、複数バッファー、プラグインを備えたモダンなターミナルテキストエディター。",
            "ko": "익숙한 단축키, 구문 강조, 다중 버퍼 및 플러그인을 지원하는 현대적인 터미널 텍스트 편집기.",
        },
        "extra": {
            "zh-CN": "Homebrew formula 提供本目录已验证的 Apple Silicon 与 Intel macOS bottle。仅在用户保存时修改所打开的文件；安装不启动编辑器、不读取或写入文档。",
            "zh-TW": "Homebrew formula 提供本目錄已驗證的 Apple Silicon 與 Intel macOS bottle。僅在使用者儲存時修改所開啟的檔案；安裝不啟動編輯器、不讀取或寫入文件。",
            "en": "The Homebrew formula provides the Apple Silicon and Intel macOS bottles verified for this catalog. Files are changed only when the user saves; installation starts no editor and reads or writes no documents.",
            "ja": "Homebrew formula は、このカタログで確認済みの Apple Silicon と Intel macOS bottle を提供します。ファイルが変更されるのはユーザーが保存した場合のみで、インストール時にエディターは起動せず、ドキュメントの読み書きも行いません。",
            "ko": "Homebrew formula는 이 카탈로그에서 확인된 Apple Silicon 및 Intel macOS bottle을 제공합니다. 사용자가 저장할 때만 연 파일을 변경하며, 설치 시 편집기를 시작하거나 문서를 읽고 쓰지 않습니다.",
        },
    },
    "glow": {
        "name": "Glow", "publisher": "Charmbracelet, Inc. and contributors", "category": "office", "mark": "Gl", "accent": "#6587a4",
        "repo": "charmbracelet/glow", "license": "MIT", "licenseFile": "LICENSE", "licenseRef": "main/LICENSE",
        "homepage": "https://github.com/charmbracelet/glow", "formula": "glow",
        "summary": {
            "zh-CN": "命令行 Markdown 阅读与渲染工具，可在终端中格式化展示 Markdown 输入。",
            "zh-TW": "命令列 Markdown 閱讀與渲染工具，可在終端機中格式化展示 Markdown 輸入。",
            "en": "A command-line Markdown reader and renderer that formats Markdown input in the terminal.",
            "ja": "ターミナルで Markdown 入力を整形表示するコマンドライン Markdown リーダー兼レンダラー。",
            "ko": "터미널에서 Markdown 입력을 서식에 맞춰 표시하는 명령줄 Markdown 리더 및 렌더러.",
        },
        "extra": {
            "zh-CN": "Homebrew formula 提供本目录已验证的 Apple Silicon 与 Intel macOS bottle。渲染只读取用户指定的 Markdown 输入；安装不打开文档、不修改内容。",
            "zh-TW": "Homebrew formula 提供本目錄已驗證的 Apple Silicon 與 Intel macOS bottle。渲染只讀取使用者指定的 Markdown 輸入；安裝不開啟文件、不修改內容。",
            "en": "The Homebrew formula provides the Apple Silicon and Intel macOS bottles verified for this catalog. Rendering reads only the Markdown input selected by the user; installation opens no document and modifies no content.",
            "ja": "Homebrew formula は、このカタログで確認済みの Apple Silicon と Intel macOS bottle を提供します。レンダリングはユーザーが指定した Markdown 入力のみを読み取り、インストール時にドキュメントを開かず内容も変更しません。",
            "ko": "Homebrew formula는 이 카탈로그에서 확인된 Apple Silicon 및 Intel macOS bottle을 제공합니다. 렌더링은 사용자가 지정한 Markdown 입력만 읽으며, 설치 시 문서를 열거나 내용을 수정하지 않습니다.",
        },
    },
    "restic": {
        "name": "restic", "publisher": "Alexander Neumann and restic contributors", "category": "system", "mark": "Re", "accent": "#6587a4",
        "repo": "restic/restic", "license": "BSD-2-Clause", "licenseFile": "LICENSE", "licenseRef": "master/LICENSE",
        "homepage": "https://restic.net/", "formula": "restic",
        "summary": {
            "zh-CN": "快速、安全的命令行备份程序，支持去重、加密、校验和本地或远程备份仓库。",
            "zh-TW": "快速、安全的命令列備份程式，支援去重、加密、校驗和本機或遠端備份儲存庫。",
            "en": "A fast, secure command-line backup program with deduplication, encryption, verification and local or remote backup repositories.",
            "ja": "重複排除、暗号化、検証、ローカルまたはリモートのバックアップリポジトリを備えた、高速で安全なコマンドラインバックアッププログラム。",
            "ko": "중복 제거, 암호화, 검증, 로컬 또는 원격 백업 리포지토리를 지원하는 빠르고 안전한 명령줄 백업 프로그램.",
        },
        "extra": {
            "zh-CN": "Homebrew formula 提供本目录已验证的 Apple Silicon 与 Intel macOS bottle。备份、恢复或仓库访问仅在用户显式执行命令时发生；安装不创建备份、不连接仓库。",
            "zh-TW": "Homebrew formula 提供本目錄已驗證的 Apple Silicon 與 Intel macOS bottle。備份、還原或儲存庫存取僅在使用者明確執行命令時發生；安裝不建立備份、不連線儲存庫。",
            "en": "The Homebrew formula provides the Apple Silicon and Intel macOS bottles verified for this catalog. Backup, restore or repository access occurs only when the user runs an explicit command; installation creates no backup and connects to no repository.",
            "ja": "Homebrew formula は、このカタログで確認済みの Apple Silicon と Intel macOS bottle を提供します。バックアップ、復元、リポジトリへの接続はユーザーが明示的にコマンドを実行した場合のみ発生し、インストール時にバックアップは作成されずリポジトリにも接続しません。",
            "ko": "Homebrew formula는 이 카탈로그에서 확인된 Apple Silicon 및 Intel macOS bottle을 제공합니다. 백업, 복원 또는 리포지토리 접근은 사용자가 명시적으로 명령을 실행할 때만 발생하며, 설치 시 백업을 만들거나 리포지토리에 연결하지 않습니다.",
        },
    },
    "lazydocker": {
        "name": "Lazydocker", "publisher": "Jesse Duffield and contributors", "category": "dev", "mark": "Ld", "accent": "#6587a4",
        "repo": "jesseduffield/lazydocker", "license": "MIT", "licenseFile": "LICENSE", "licenseRef": "master/LICENSE",
        "homepage": "https://github.com/jesseduffield/lazydocker", "formula": "lazydocker",
        "summary": {
            "zh-CN": "Docker 终端管理界面，用于查看容器、镜像、卷、日志并执行常用 Docker 操作。",
            "zh-TW": "Docker 終端機管理介面，用於檢視容器、映像檔、磁碟區、記錄並執行常用 Docker 操作。",
            "en": "A terminal UI for Docker to inspect containers, images, volumes and logs and run common Docker operations.",
            "ja": "コンテナー、イメージ、ボリューム、ログを確認し、一般的な Docker 操作を実行するためのターミナル UI。",
            "ko": "컨테이너, 이미지, 볼륨, 로그를 살펴보고 일반적인 Docker 작업을 실행하는 터미널 UI.",
        },
        "extra": {
            "zh-CN": "Homebrew formula 提供本目录已验证的 Apple Silicon 与 Intel macOS bottle。使用时需要可访问的 Docker 守护进程和上下文；安装不启动 Docker、不更改容器或镜像。",
            "zh-TW": "Homebrew formula 提供本目錄已驗證的 Apple Silicon 與 Intel macOS bottle。使用時需要可存取的 Docker 守護程序和內容；安裝不啟動 Docker、不變更容器或映像檔。",
            "en": "The Homebrew formula provides the Apple Silicon and Intel macOS bottles verified for this catalog. Usage requires an accessible Docker daemon and context; installation starts no Docker engine and changes no containers or images.",
            "ja": "Homebrew formula は、このカタログで確認済みの Apple Silicon と Intel macOS bottle を提供します。使用にはアクセス可能な Docker デーモンとコンテキストが必要で、インストール時に Docker は起動せず、コンテナーやイメージも変更しません。",
            "ko": "Homebrew formula는 이 카탈로그에서 확인된 Apple Silicon 및 Intel macOS bottle을 제공합니다. 사용하려면 접근 가능한 Docker 데몬과 컨텍스트가 필요하며, 설치 시 Docker를 시작하거나 컨테이너 또는 이미지를 변경하지 않습니다.",
        },
    },

    "helix": {
        "name": "Helix", "publisher": "Blaž Hrastnik and contributors", "category": "dev", "mark": "Hx", "accent": "#6587a4",
        "repo": "helix-editor/helix", "license": "MPL-2.0", "licenseFile": "LICENSE", "licenseRef": "master/LICENSE",
        "homepage": "https://helix-editor.com", "formula": "helix",
        "summary": {
            "zh-CN": "模态终端文本编辑器，内置多光标、语法高亮、语言服务器与树形结构解析支持。",
            "zh-TW": "模態終端機文字編輯器，內建多游標、語法突顯、語言伺服器與樹狀結構解析支援。",
            "en": "A modal terminal text editor with built-in multiple cursors, syntax highlighting, language servers and tree-sitter support.",
            "ja": "複数カーソル、構文ハイライト、言語サーバー、Tree-sitter を組み込んだモーダルなターミナルテキストエディター。",
            "ko": "다중 커서, 구문 강조, 언어 서버 및 Tree-sitter 지원을 내장한 모달 터미널 텍스트 편집기.",
        },
        "extra": {
            "zh-CN": "Homebrew 仅提供本目录已验证的 macOS arm64 bottle，未声明 Intel macOS。仅在用户保存时修改所打开的文件；安装不启动编辑器、不读取或写入文档。",
            "zh-TW": "Homebrew 僅提供本目錄已驗證的 macOS arm64 bottle，未宣告 Intel macOS。僅在使用者儲存時修改所開啟的檔案；安裝不啟動編輯器、不讀取或寫入文件。",
            "en": "Homebrew provides only the macOS arm64 bottle verified for this catalog; Intel macOS is not declared. Files are changed only when the user saves; installation starts no editor and reads or writes no documents.",
            "ja": "Homebrew はこのカタログで確認済みの macOS arm64 bottle のみを提供し、Intel macOS は対象外です。ファイルが変更されるのはユーザーが保存した場合のみで、インストール時にエディターは起動せず、ドキュメントの読み書きも行いません。",
            "ko": "Homebrew는 이 카탈로그에서 확인된 macOS arm64 bottle만 제공하며 Intel macOS는 선언하지 않습니다. 사용자가 저장할 때만 연 파일을 변경하며, 설치 시 편집기를 시작하거나 문서를 읽고 쓰지 않습니다.",
        },
    },
    "nushell": {
        "name": "Nushell", "publisher": "The Nushell Project Developers", "category": "dev", "mark": "Nu", "accent": "#6587a4",
        "repo": "nushell/nushell", "license": "MIT", "licenseFile": "LICENSE", "licenseRef": "main/LICENSE",
        "homepage": "https://www.nushell.sh", "formula": "nushell",
        "summary": {
            "zh-CN": "结构化命令行 Shell，使用表格、管道和插件处理系统数据与外部命令输出。",
            "zh-TW": "結構化命令列 Shell，使用表格、管道和外掛處理系統資料與外部命令輸出。",
            "en": "A structured command-line shell that uses tables, pipelines and plugins to work with system data and external command output.",
            "ja": "テーブル、パイプライン、プラグインを使ってシステムデータや外部コマンドの出力を扱う構造化コマンドラインシェル。",
            "ko": "테이블, 파이프라인 및 플러그인으로 시스템 데이터와 외부 명령 출력을 다루는 구조화된 명령줄 셸.",
        },
        "extra": {
            "zh-CN": "Homebrew 仅提供本目录已验证的 macOS arm64 bottle，未声明 Intel macOS；formula 含 openssl@4 运行时依赖，并使用 macOS 提供的 curl。文件与网络访问仅在用户启动 nu 并执行命令时发生；安装不更改默认 Shell、不执行命令。",
            "zh-TW": "Homebrew 僅提供本目錄已驗證的 macOS arm64 bottle，未宣告 Intel macOS；formula 含 openssl@4 執行階段相依性，並使用 macOS 提供的 curl。檔案與網路存取僅在使用者啟動 nu 並執行命令時發生；安裝不變更預設 Shell、不執行命令。",
            "en": "Homebrew provides only the macOS arm64 bottle verified for this catalog; Intel macOS is not declared. The formula has an openssl@4 runtime dependency and uses curl provided by macOS. File and network access occurs only when the user starts nu and runs commands; installation changes no default shell and executes no commands.",
            "ja": "Homebrew はこのカタログで確認済みの macOS arm64 bottle のみを提供し、Intel macOS は対象外です。formula には実行時依存関係として openssl@4 があり、macOS が提供する curl を使用します。ファイルやネットワークへのアクセスはユーザーが nu を起動してコマンドを実行した場合のみ発生し、インストール時に既定シェルを変更せずコマンドも実行しません。",
            "ko": "Homebrew는 이 카탈로그에서 확인된 macOS arm64 bottle만 제공하며 Intel macOS는 선언하지 않습니다. formula에는 openssl@4 런타임 종속성이 있고 macOS가 제공하는 curl을 사용합니다. 파일 및 네트워크 접근은 사용자가 nu를 시작하고 명령을 실행할 때만 발생하며, 설치 시 기본 셸을 바꾸거나 명령을 실행하지 않습니다.",
        },
    },
    "croc": {
        "name": "Croc", "publisher": "Zack and contributors", "category": "tools", "mark": "Cr", "accent": "#6587a4",
        "repo": "schollz/croc", "license": "MIT", "licenseFile": "LICENSE", "licenseRef": "main/LICENSE",
        "homepage": "https://github.com/schollz/croc", "formula": "croc",
        "summary": {
            "zh-CN": "端到端加密的命令行文件与文件夹传输工具，支持中继、断点续传和多文件传输。",
            "zh-TW": "端對端加密的命令列檔案與資料夾傳輸工具，支援中繼、斷點續傳和多檔案傳輸。",
            "en": "An end-to-end encrypted command-line tool for transferring files and folders, with relay support, resumable transfers and multiple files.",
            "ja": "中継、再開可能な転送、複数ファイル転送をサポートする、エンドツーエンド暗号化のコマンドラインファイル・フォルダー転送ツール。",
            "ko": "중계, 재개 가능한 전송 및 다중 파일 전송을 지원하는 엔드투엔드 암호화 명령줄 파일·폴더 전송 도구.",
        },
        "extra": {
            "zh-CN": "Homebrew 仅提供本目录已验证的 macOS arm64 bottle，未声明 Intel macOS。发送或接收仅在用户显式运行命令时发生，会访问所选文件并建立网络连接；安装不传输文件、不启动常驻服务。",
            "zh-TW": "Homebrew 僅提供本目錄已驗證的 macOS arm64 bottle，未宣告 Intel macOS。傳送或接收僅在使用者明確執行命令時發生，會存取所選檔案並建立網路連線；安裝不傳輸檔案、不啟動常駐服務。",
            "en": "Homebrew provides only the macOS arm64 bottle verified for this catalog; Intel macOS is not declared. Sending or receiving occurs only when the user runs an explicit command, accesses selected files and opens network connections; installation transfers no files and starts no persistent service.",
            "ja": "Homebrew はこのカタログで確認済みの macOS arm64 bottle のみを提供し、Intel macOS は対象外です。送信または受信はユーザーが明示的にコマンドを実行した場合のみ発生し、選択したファイルへのアクセスとネットワーク接続を行います。インストール時にファイル転送や常駐サービスの起動は行いません。",
            "ko": "Homebrew는 이 카탈로그에서 확인된 macOS arm64 bottle만 제공하며 Intel macOS는 선언하지 않습니다. 전송 또는 수신은 사용자가 명시적으로 명령을 실행할 때만 발생하고 선택한 파일에 접근하며 네트워크 연결을 만듭니다. 설치 시 파일을 전송하거나 상시 서비스를 시작하지 않습니다.",
        },
    },
    "zellij": {
        "name": "Zellij", "publisher": "Aram Drevekenin and contributors", "category": "dev", "mark": "Ze", "accent": "#6587a4",
        "repo": "zellij-org/zellij", "license": "MIT", "licenseFile": "LICENSE.md", "licenseRef": "main/LICENSE.md",
        "homepage": "https://zellij.dev", "formula": "zellij",
        "summary": {
            "zh-CN": "可插件扩展的终端工作区与复用器，提供窗格、标签页、布局和会话管理。",
            "zh-TW": "可外掛擴充的終端機工作區與複用器，提供窗格、分頁、版面和工作階段管理。",
            "en": "A pluggable terminal workspace and multiplexer with panes, tabs, layouts and session management.",
            "ja": "ペイン、タブ、レイアウト、セッション管理を備えた、プラグイン拡張可能なターミナルワークスペース兼マルチプレクサー。",
            "ko": "플러그인으로 확장 가능한 터미널 작업 공간 및 멀티플렉서로, 창, 탭, 레이아웃과 세션 관리를 제공합니다.",
        },
        "extra": {
            "zh-CN": "Homebrew 仅提供本目录已验证的 macOS arm64 bottle，未声明 Intel macOS；formula 未声明第三方 formula 依赖，但使用 macOS 提供的 curl。会话、窗格和布局仅在用户启动 zellij 后创建，运行状态写入用户配置目录。formula 另声明可选的 brew services 入口 zellij web，只有用户显式启用才运行；安装不注册或启动服务、不修改终端配置。",
            "zh-TW": "Homebrew 僅提供本目錄已驗證的 macOS arm64 bottle，未宣告 Intel macOS；formula 未宣告第三方 formula 相依性，但使用 macOS 提供的 curl。工作階段、窗格和版面僅在使用者啟動 zellij 後建立，執行狀態寫入使用者設定目錄。formula 另宣告可選的 brew services 入口 zellij web，只有使用者明確啟用才執行；安裝不註冊或啟動服務、不修改終端機設定。",
            "en": "Homebrew provides only the macOS arm64 bottle verified for this catalog; Intel macOS is not declared. The formula declares no third-party formula dependencies but uses curl provided by macOS. Sessions, panes and layouts are created only after the user starts zellij, with runtime state written under the user configuration directory. The formula also declares an optional brew services entry for zellij web, which runs only when the user explicitly enables it; installation registers or starts no service and changes no terminal configuration.",
            "ja": "Homebrew はこのカタログで確認済みの macOS arm64 bottle のみを提供し、Intel macOS は対象外です。formula はサードパーティーの formula 依存関係を宣言していませんが、macOS が提供する curl を使用します。セッション、ペイン、レイアウトはユーザーが zellij を起動した後にのみ作成され、実行状態はユーザー設定ディレクトリに書き込まれます。formula には brew services のオプション入口 zellij web も宣言されていますが、実行されるのはユーザーが明示的に有効化した場合のみです。インストール時にサービスの登録や起動、ターミナル設定の変更は行いません。",
            "ko": "Homebrew는 이 카탈로그에서 확인된 macOS arm64 bottle만 제공하며 Intel macOS는 선언하지 않습니다. formula는 타사 formula 종속성을 선언하지 않지만 macOS가 제공하는 curl을 사용합니다. 세션, 창 및 레이아웃은 사용자가 zellij를 시작한 후에만 만들어지고 런타임 상태는 사용자 구성 디렉터리에 기록됩니다. formula에는 brew services의 선택적 zellij web 진입점도 선언되어 있지만 사용자가 명시적으로 활성화할 때만 실행됩니다. 설치 시 서비스를 등록하거나 시작하지 않으며 터미널 구성을 수정하지 않습니다.",
        },
    },

    "amical": {
        "name": "Amical", "publisher": "Naomi Chopra, Haritabh Singh and contributors", "category": "ai", "mark": "Ac", "accent": "#6f76c8",
        "repo": "amicalhq/amical", "license": "MIT", "licenseFile": "LICENSE", "licenseRef": "main/LICENSE",
        "homepage": "https://amical.ai/", "cask": "amical", "caskArchitectures": ["x86_64", "aarch64"],
        "summary": {
            "zh-CN": "本地优先的 AI 听写与语音转文字桌面应用，使用开源模型并支持离线处理。",
            "zh-TW": "本地優先的 AI 聽寫與語音轉文字桌面應用程式，使用開源模型並支援離線處理。",
            "en": "A local-first AI dictation and speech-to-text desktop app using open-source models with offline processing.",
            "ja": "オープンソースモデルを使用し、オフライン処理に対応するローカルファーストの AI ディクテーション・音声テキスト化デスクトップアプリ。",
            "ko": "오픈 소스 모델을 사용하고 오프라인 처리를 지원하는 로컬 우선 AI 받아쓰기 및 음성-텍스트 데스크톱 앱.",
        },
        "extra": {
            "zh-CN": "需要 macOS 13 或更高版本；Intel 与 Apple Silicon 使用不同 DMG，Homebrew cask 只复制 Amical.app 到应用程序目录。cask 声明 auto_updates: true，应用可能自行管理更新。应用包声明麦克风用途字符串用于录音转写，并声明摄像头用途字符串；权限只在系统提示且用户授权后使用。安装不启动应用、不请求权限、不录音或下载模型。",
            "zh-TW": "需要 macOS 13 或更高版本；Intel 與 Apple Silicon 使用不同 DMG，Homebrew cask 只複製 Amical.app 到應用程式目錄。cask 宣告 auto_updates: true，應用可能自行管理更新。應用套件宣告麥克風用途字串用於錄音轉寫，並宣告攝影機用途字串；權限只在系統提示且使用者授權後使用。安裝不啟動應用程式、不請求權限、不錄音或下載模型。",
            "en": "Requires macOS 13 or later. Intel and Apple Silicon use separate DMGs; the Homebrew cask only copies Amical.app into Applications. The cask declares auto_updates: true, so the app may manage its own updates. The bundle declares a microphone usage string for recording transcription and a camera usage string; permissions are used only after a system prompt and user authorization. Installation launches no app, requests no permissions, records no audio and downloads no models.",
            "ja": "macOS 13 以降が必要です。Intel と Apple Silicon では別々の DMG を使用し、Homebrew cask は Amical.app をアプリケーションフォルダへコピーするだけです。cask は auto_updates: true を宣言しているため、アプリが更新を独自管理する場合があります。バンドルには録音・文字起こし用のマイク用途文字列とカメラ用途文字列が宣言されており、権限はシステムのプロンプトでユーザーが許可した場合にのみ使用されます。インストール時にアプリの起動、権限要求、録音、モデルのダウンロードは行われません。",
            "ko": "macOS 13 이상이 필요합니다. Intel과 Apple Silicon은 별도 DMG를 사용하며 Homebrew cask는 Amical.app을 응용 프로그램 폴더로 복사만 합니다. cask는 auto_updates: true를 선언하므로 앱이 업데이트를 자체 관리할 수 있습니다. 번들에는 녹음 전사용 마이크 사용 문자열과 카메라 사용 문자열이 선언되어 있고, 권한은 시스템 프롬프트에서 사용자가 허용한 경우에만 사용됩니다. 설치 시 앱을 시작하거나 권한을 요청하거나 오디오를 녹음하거나 모델을 다운로드하지 않습니다.",
        },
    },
    "beutl": {
        "name": "Beutl", "publisher": "Yuto Terada and contributors", "category": "media", "mark": "Bt", "accent": "#537fa3",
        "repo": "b-editor/beutl", "license": "MIT", "licenseFile": "LICENSE", "licenseRef": "main/LICENSE",
        "homepage": "https://beutl.beditor.net/", "cask": "beutl", "caskArchitectures": ["aarch64"],
        "summary": {
            "zh-CN": "跨平台视频编辑与合成桌面软件，提供基于时间线的媒体制作工作流。",
            "zh-TW": "跨平台影片編輯與合成桌面軟體，提供基於時間軸的媒體製作工作流程。",
            "en": "A cross-platform desktop video editing and compositing application with a timeline-based media production workflow.",
            "ja": "タイムラインベースのメディア制作ワークフローを備えた、クロスプラットフォームのデスクトップ動画編集・コンポジットアプリケーション。",
            "ko": "타임라인 기반 미디어 제작 워크플로를 제공하는 크로스 플랫폼 데스크톱 비디오 편집 및 합성 애플리케이션.",
        },
        "extra": {
            "zh-CN": "需要 macOS 12 或更高版本。本目录仅声明已验证的 Apple Silicon Homebrew 路由；虽然 Beutl.app 含 x86_64 Mach-O，但强制依赖 ffmpeg@6 没有 Intel macOS bottle，因此不声明 x86_64，也不推断 Rosetta 或源码构建可用。cask 会安装 GPL-3.0-or-later 的 ffmpeg@6；其 arm64 bottle 已验证 digest，ffmpeg、ffprobe、ffplay 均为 arm64 Mach-O，94 个运行时 formula 均有 Apple Silicon 或架构无关 bottle。安装只复制 Beutl.app，不启动应用或打开项目。",
            "zh-TW": "需要 macOS 12 或更高版本。本目錄僅宣告已驗證的 Apple Silicon Homebrew 路由；雖然 Beutl.app 含 x86_64 Mach-O，但強制相依 ffmpeg@6 沒有 Intel macOS bottle，因此不宣告 x86_64，也不推斷 Rosetta 或原始碼建置可用。cask 會安裝 GPL-3.0-or-later 的 ffmpeg@6；其 arm64 bottle 已驗證 digest，ffmpeg、ffprobe、ffplay 均為 arm64 Mach-O，94 個執行階段 formula 均有 Apple Silicon 或架構無關 bottle。安裝只複製 Beutl.app，不啟動應用程式或開啟專案。",
            "en": "Requires macOS 12 or later. This catalog declares only the verified Apple Silicon Homebrew route. Although Beutl.app contains an x86_64 Mach-O, the required ffmpeg@6 formula has no Intel macOS bottle, so x86_64 is not declared and Rosetta or source-build operation is not inferred. The cask installs ffmpeg@6 under GPL-3.0-or-later; its arm64 bottle digest is verified, ffmpeg, ffprobe and ffplay are arm64 Mach-O files, and all 94 runtime formulas have Apple Silicon or architecture-independent bottles. Installation only copies Beutl.app and launches no app or project.",
            "ja": "macOS 12 以降が必要です。このカタログでは検証済みの Apple Silicon 向け Homebrew ルートのみを宣言します。Beutl.app に x86_64 Mach-O が含まれていても、必須依存の ffmpeg@6 に Intel macOS bottle がないため x86_64 は宣言せず、Rosetta やソースビルドでの動作も推測しません。cask は GPL-3.0-or-later の ffmpeg@6 をインストールします。arm64 bottle のダイジェストは検証済みで、ffmpeg、ffprobe、ffplay はすべて arm64 Mach-O、94 個の実行時 formula には Apple Silicon またはアーキテクチャ非依存 bottle があります。インストールでは Beutl.app をコピーするだけで、アプリやプロジェクトは起動しません。",
            "ko": "macOS 12 이상이 필요합니다. 이 카탈로그는 검증된 Apple Silicon Homebrew 경로만 선언합니다. Beutl.app에 x86_64 Mach-O가 포함되어 있지만 필수 종속성인 ffmpeg@6에 Intel macOS bottle이 없으므로 x86_64를 선언하지 않고 Rosetta나 소스 빌드 동작을 추정하지 않습니다. cask는 GPL-3.0-or-later 라이선스의 ffmpeg@6를 설치합니다. arm64 bottle 다이제스트가 검증되었고 ffmpeg, ffprobe, ffplay는 모두 arm64 Mach-O이며 94개 런타임 formula는 모두 Apple Silicon 또는 아키텍처 독립 bottle을 제공합니다. 설치는 Beutl.app 복사만 수행하고 앱이나 프로젝트를 시작하지 않습니다.",
        },
    },

    "buzz": {
        "name": "Buzz", "publisher": "Chidi Williams and contributors", "category": "ai", "mark": "Bz", "accent": "#5f83b6",
        "repo": "chidiwilliams/buzz", "license": "MIT", "licenseFile": "LICENSE", "licenseRef": "main/LICENSE",
        "homepage": "https://github.com/chidiwilliams/buzz", "cask": "buzz", "caskArchitectures": ["x86_64", "aarch64"],
        "summary": {
            "zh-CN": "本地离线音频转录与翻译桌面应用，基于 Whisper，支持媒体文件、麦克风实时转录和字幕导出。",
            "zh-TW": "本地離線音訊轉錄與翻譯桌面應用程式，基於 Whisper，支援媒體檔案、麥克風即時轉錄和字幕匯出。",
            "en": "A local desktop audio transcription and translation app powered by Whisper, with media files, live microphone transcription and subtitle export.",
            "ja": "Whisper を活用し、メディアファイル、マイクのリアルタイム文字起こし、字幕エクスポートに対応するローカルの音声文字起こし・翻訳デスクトップアプリ。",
            "ko": "Whisper 기반의 로컬 오디오 전사 및 번역 데스크톱 앱으로, 미디어 파일, 마이크 실시간 전사, 자막 내보내기를 지원합니다.",
        },
        "extra": {
            "zh-CN": "本条目对应稳定版 v1.4.5；Apple Silicon 与 Intel 使用不同 DMG，两者 SHA-256 摘要与主程序 Mach-O 架构均已验证。cask 无 formula 依赖且只复制 Buzz.app 到应用程序目录，并声明 auto_updates: true，应用可能自行管理更新；cask 与 Info.plist 均未声明最低 macOS 版本。上游 README 说明 1.4.5 是最后一个支持 Intel Mac 的版本，因此 x86_64 仅代表此已验证版本，不推断未来版本。应用包声明麦克风用途字符串，实时转录只在用户启动并授权后使用麦克风；本地文件转录可离线执行，YouTube 链接或 Hugging Face Transformer 模型可能需要联网或额外模型。安装不启动应用、不请求麦克风权限、不录音、不访问媒体或下载模型。",
            "zh-TW": "本條目對應穩定版 v1.4.5；Apple Silicon 與 Intel 使用不同 DMG，兩者 SHA-256 摘要與主程式 Mach-O 架構均已驗證。cask 無 formula 相依性且只複製 Buzz.app 到應用程式目錄，並宣告 auto_updates: true，應用可能自行管理更新；cask 與 Info.plist 均未宣告最低 macOS 版本。上游 README 說明 1.4.5 是最後一個支援 Intel Mac 的版本，因此 x86_64 僅代表此已驗證版本，不推斷未來版本。應用套件宣告麥克風用途字串，即時轉錄只在使用者啟動並授權後使用麥克風；本機檔案轉錄可離線執行，YouTube 連結或 Hugging Face Transformer 模型可能需要連網或額外模型。安裝不啟動應用程式、不請求麥克風權限、不錄音、不存取媒體或下載模型。",
            "en": "This entry corresponds to stable v1.4.5. Apple Silicon and Intel use separate DMGs; both SHA-256 digests and primary Mach-O architectures are verified. The cask has no formula dependency and only copies Buzz.app into Applications. It declares auto_updates: true, so the app may manage its own updates; neither the cask nor Info.plist declares a minimum macOS version. The upstream README states that 1.4.5 is the last release supporting Intel Macs, so x86_64 applies only to this verified release and is not inferred for future releases. The bundle declares a microphone usage string; live transcription uses the microphone only after the user starts it and grants permission. Local-file transcription can run offline, while YouTube links or Hugging Face Transformer models may require network access or additional models. Installation launches no app, requests no microphone permission, records no audio, accesses no media and downloads no models.",
            "ja": "このエントリは安定版 v1.4.5 に対応します。Apple Silicon と Intel は別々の DMG を使用し、両方の SHA-256 ダイジェストと主要 Mach-O のアーキテクチャを検証済みです。cask に formula 依存関係はなく、Buzz.app をアプリケーションフォルダへコピーするだけです。auto_updates: true が宣言されているため、アプリが更新を独自管理する場合があります。cask と Info.plist のいずれにも最低 macOS バージョンの宣言はありません。上流 README では 1.4.5 が Intel Mac をサポートする最後のリリースと説明されているため、x86_64 はこの検証済みリリースにのみ適用され、将来のリリースには推測しません。バンドルにはマイク用途文字列が宣言されており、リアルタイム文字起こしはユーザーが開始して許可した後にのみマイクを使用します。ローカルファイルの文字起こしはオフラインで実行できますが、YouTube リンクや Hugging Face Transformer モデルはネットワーク接続または追加モデルが必要になる場合があります。インストール時にアプリの起動、マイク権限の要求、録音、メディアへのアクセス、モデルのダウンロードは行われません。",
            "ko": "이 항목은 안정 v1.4.5에 해당합니다. Apple Silicon과 Intel은 별도 DMG를 사용하며 두 SHA-256 다이제스트와 기본 Mach-O 아키텍처를 모두 검증했습니다. cask에는 formula 종속성이 없고 Buzz.app을 응용 프로그램 폴더로 복사만 합니다. auto_updates: true를 선언하므로 앱이 업데이트를 자체 관리할 수 있으며, cask와 Info.plist 어디에도 최소 macOS 버전이 선언되어 있지 않습니다. 업스트림 README는 1.4.5가 Intel Mac을 지원하는 마지막 릴리스라고 설명하므로 x86_64는 이 검증된 릴리스에만 적용되고 향후 릴리스에 대해 추정하지 않습니다. 번들에는 마이크 사용 문자열이 선언되어 있고 실시간 전사는 사용자가 시작하고 권한을 부여한 후에만 마이크를 사용합니다. 로컬 파일 전사는 오프라인으로 실행할 수 있지만 YouTube 링크나 Hugging Face Transformer 모델은 네트워크 접근 또는 추가 모델이 필요할 수 있습니다. 설치 시 앱을 시작하거나 마이크 권한을 요청하거나 오디오를 녹음하거나 미디어에 접근하거나 모델을 다운로드하지 않습니다.",
        },
    },
    "hidden": {
        "name": "Hidden Bar",
        "publisher": "Dwarves Foundation",
        "category": "system",
        "mark": "HB",
        "accent": "#6387a4",
        "repo": "consolelabs/hidden",
        "license": "MIT",
        "licenseFile": "LICENSE",
        "licenseRef": "v1.13.0/LICENSE",
        "homepage": "https://github.com/consolelabs/hidden",
        "cask": "hiddenbar",
        "caskArchitectures": ["x86_64", "aarch64"],
        "summary": {
            "zh-CN": "macOS 菜单栏图标隐藏工具，可折叠、搜索和分组管理其他应用的菜单栏项目。",
            "zh-TW": "macOS 選單列圖示隱藏工具，可摺疊、搜尋和分組管理其他應用程式的選單列項目。",
            "en": "A macOS menu-bar utility for hiding, revealing, searching and organizing other apps' menu-bar items.",
            "ja": "ほかのアプリのメニューバー項目を非表示、表示、検索、整理できる macOS メニューバーユーティリティ。",
            "ko": "다른 앱의 메뉴 막대 항목을 숨기고 표시하며 검색·정리할 수 있는 macOS 메뉴 막대 유틸리티.",
        },
        "extra": {
            "zh-CN": "本条目对应稳定版 v1.13.0；GitHub 标签发布为非草稿、非预发布，/releases/latest 解析到同一发布 ID 446937244，appcast 条目没有 beta 或 dev channel。Homebrew cask hiddenbar 处于有效状态，无 formula 依赖、冲突或 caveat；安装只将 Hidden Bar.app 复制到 /Applications，不启动应用、不请求权限、不注册登录项；uninstall/zap 仅退出应用、反授权旧 LauncherApplication 记录并清理卸载数据。ZIP SHA-256 a811717d84faba478ea66caa224f3360a9e5d844a7a50fa09009f32d78131a44 同时匹配 cask 与 GitHub 资产。按 Mach-O 头验证，6 个唯一真实 Mach-O 均为含 x86_64 与 arm64 的 fat 文件，共 12 个切片；所有切片都包含 LC_CODE_SIGNATURE、启用 hardened runtime，签名者均为 Developer ID Application: Dwarves Foundation Company Limited (W777S7V8TN)。应用包内可静态解析到 s8ch v1 stapled ticket 容器和 g8tk trailer；这只是本地证据，不在 Linux 上声称在线 Gatekeeper 验收结果。最低系统为 macOS 13；主程序是 LSUIElement，bundle ID 为 com.dwarvesv.minimalbar，直接构建没有沙盒 entitlement，嵌入的 entitlement 字典为空。读取和隐藏菜单栏项目所需的 Accessibility 是首次启动并使用相关功能后的用户授权，缺失时才打开系统设置，安装阶段不会提示；Info.plist 没有摄像头、麦克风、屏幕录制或 AppleEvents 用途字符串，产品和源码均说明只读取菜单栏项目，不读取屏幕内容。登录项默认关闭，用户启用后通过 SMAppService.mainApp 注册主程序；旧 com.dwarvesv.LauncherApplication helper 已不随包提供，只会在升级时一次性反授权。GitHub/Homebrew 直接构建包含 Sparkle 2.10.1，通过 HTTPS 获取 GitHub appcast 并以 EdDSA 公钥 C7MH2NS46vKD4nNh9i4BmFzXLSAbP2jM5qZcaPw9u3w= 校验，系统画像发送保持关闭且不发送个人信息；Homebrew 安装本身不检查或下载更新，应用启动后才初始化 Sparkle 的计划或手动更新机制。未发现受支持的上游 Linux 或 Windows 包及制品，因此仅声明 macOS brew cask 路由。",
            "zh-TW": "本條目對應穩定版 v1.13.0；GitHub 標籤發布為非草稿、非預先發布，/releases/latest 解析到同一發布 ID 446937244，appcast 條目沒有 beta 或 dev channel。Homebrew cask hiddenbar 處於有效狀態，無 formula 相依性、衝突或 caveat；安裝只將 Hidden Bar.app 複製到 /Applications，不啟動應用程式、不請求權限、不註冊登入項目；uninstall/zap 僅結束應用程式、反授權舊 LauncherApplication 記錄並清理解除安裝資料。ZIP SHA-256 a811717d84faba478ea66caa224f3360a9e5d844a7a50fa09009f32d78131a44 同時符合 cask 與 GitHub 資產。依 Mach-O 標頭驗證，6 個唯一真實 Mach-O 均為含 x86_64 與 arm64 的 fat 檔案，共 12 個切片；所有切片都包含 LC_CODE_SIGNATURE、啟用 hardened runtime，簽署者均為 Developer ID Application: Dwarves Foundation Company Limited (W777S7V8TN)。應用程式包內可靜態解析到 s8ch v1 stapled ticket 容器和 g8tk trailer；這只是本機證據，不在 Linux 上宣稱線上 Gatekeeper 驗收結果。最低系統為 macOS 13；主程式是 LSUIElement，bundle ID 為 com.dwarvesv.minimalbar，直接建置沒有沙盒 entitlement，嵌入的 entitlement 字典為空。讀取和隱藏選單列項目所需的 Accessibility 是首次啟動並使用相關功能後的使用者授權，缺失時才開啟系統設定，安裝階段不會提示；Info.plist 沒有攝影機、麥克風、螢幕錄製或 AppleEvents 用途字串，產品和原始碼均說明只讀取選單列項目，不讀取螢幕內容。登入項目預設關閉，使用者啟用後透過 SMAppService.mainApp 註冊主程式；舊 com.dwarvesv.LauncherApplication helper 已不隨包提供，只會在升級時一次性反授權。GitHub/Homebrew 直接建置包含 Sparkle 2.10.1，透過 HTTPS 取得 GitHub appcast 並以 EdDSA 公開金鑰 C7MH2NS46vKD4nNh9i4BmFzXLSAbP2jM5qZcaPw9u3w= 校驗，系統設定檔傳送保持關閉且不傳送個人資訊；Homebrew 安裝本身不檢查或下載更新，應用程式啟動後才初始化 Sparkle 的計劃或手動更新機制。未發現受支援的上游 Linux 或 Windows 套件及製品，因此僅宣告 macOS brew cask 路由。",
            "en": "This entry corresponds to stable v1.13.0. The GitHub tag release is non-draft and non-prerelease, /releases/latest resolves to the same release ID 446937244, and the appcast items have no beta or dev channel. The active Homebrew cask hiddenbar has no formula dependency, conflict or caveat. Installation only copies Hidden Bar.app into /Applications; it does not launch the app, request permissions or register a login item. Uninstall/zap only quits the app, deauthorizes the legacy LauncherApplication record and removes uninstall cleanup data. The ZIP SHA-256 a811717d84faba478ea66caa224f3360a9e5d844a7a50fa09009f32d78131a44 matches both the cask and GitHub asset. Mach-O headers prove that all 6 unique real Mach-O files are fat binaries containing x86_64 and arm64, for 12 slices total. Every slice contains LC_CODE_SIGNATURE, enables hardened runtime and is signed by Developer ID Application: Dwarves Foundation Company Limited (W777S7V8TN). The bundle contains a statically parseable s8ch v1 stapled-ticket container and g8tk trailer; this is local evidence, not an online Gatekeeper verdict obtained from Linux. It requires macOS 13 or newer. The main app is an LSUIElement with bundle ID com.dwarvesv.minimalbar; the direct build has no sandbox entitlement and its embedded entitlement dictionaries are empty. Accessibility for reading and hiding menu-bar items is a user grant after first launch and use, and System Settings opens only when that permission is missing; installation itself does not prompt. The Info.plist has no camera, microphone, screen-recording or AppleEvents usage strings, and both the product and source state that it reads menu-bar items but never screen contents. Start-at-login is off by default; when the user enables it, SMAppService.mainApp registers the main app. The legacy com.dwarvesv.LauncherApplication helper is no longer bundled and is only deauthorized once as migration cleanup. The GitHub/Homebrew direct build includes Sparkle 2.10.1, fetches a GitHub appcast over HTTPS, verifies it against EdDSA public key C7MH2NS46vKD4nNh9i4BmFzXLSAbP2jM5qZcaPw9u3w=, keeps system profiling disabled and sends no personal information. Homebrew installation itself checks for or downloads no update; Sparkle's scheduled or manual update behavior is initialized only after the app launches. No supported upstream Linux or Windows package or artifact exists, so only the macOS brew cask route is declared.",
            "ja": "このエントリは安定版 v1.13.0 に対応します。GitHub タグリリースはドラフトでもプレリリースでもなく、/releases/latest は同じリリース ID 446937244 を返し、appcast 項目に beta や dev チャネルはありません。有効な Homebrew cask hiddenbar に formula 依存関係、競合、caveat はありません。インストールは Hidden Bar.app を /Applications にコピーするだけで、アプリの起動、権限要求、ログイン項目の登録は行いません。uninstall/zap はアプリの終了、従来の LauncherApplication レコードの認証取り消し、アンインストール用クリーンアップデータの削除だけを行います。ZIP の SHA-256 a811717d84faba478ea66caa224f3360a9e5d844a7a50fa09009f32d78131a44 は cask と GitHub アセットの両方に一致します。Mach-O ヘッダーにより、6 個の一意な実 Mach-O はすべて x86_64 と arm64 を含む fat バイナリで、合計 12 スライスであることを確認済みです。すべてのスライスに LC_CODE_SIGNATURE があり、hardened runtime が有効で、署名者は Developer ID Application: Dwarves Foundation Company Limited (W777S7V8TN) です。アプリバンドルからは s8ch v1 stapled ticket コンテナと g8tk trailer を静的解析できます。これはローカル証拠であり、Linux からオンラインの Gatekeeper 判定を主張するものではありません。要件は macOS 13 以降です。メインアプリは LSUIElement、バンドル ID は com.dwarvesv.minimalbar で、直接ビルドにはサンドボックス entitlement がなく、埋め込み entitlement 辞書は空です。メニューバー項目の読み取りと非表示に必要な Accessibility は、初回起動後に機能を使う際のユーザー許可であり、権限がない場合にだけシステム設定を開きます。インストール時には要求しません。Info.plist にカメラ、マイク、画面収録、AppleEvents の用途文字列はなく、製品とソースはメニューバー項目を読むだけで画面内容は読まないと明記しています。ログイン時起動は既定で無効で、ユーザーが有効にした場合のみ SMAppService.mainApp がメインアプリを登録します。従来の com.dwarvesv.LauncherApplication ヘルパーは同梱されておらず、アップグレード時の移行クリーンアップで一度だけ認証を取り消します。GitHub/Homebrew の直接ビルドには Sparkle 2.10.1 が含まれ、HTTPS で GitHub appcast を取得し、EdDSA 公開鍵 C7MH2NS46vKD4nNh9i4BmFzXLSAbP2jM5qZcaPw9u3w= で検証します。システムプロファイル送信は無効で、個人情報も送信しません。Homebrew のインストール自体が更新確認やダウンロードを行うことはなく、Sparkle の定期または手動更新機能が初期化されるのはアプリ起動後です。サポートされる上流の Linux または Windows パッケージや成果物はないため、macOS の brew cask ルートのみを宣言します。",
            "ko": "이 항목은 안정 v1.13.0에 해당합니다. GitHub 태그 릴리스는 초안도 사전 릴리스도 아니고 /releases/latest는 동일한 릴리스 ID 446937244를 반환하며 appcast 항목에는 beta나 dev 채널이 없습니다. 활성 Homebrew cask hiddenbar에는 formula 종속성, 충돌, caveat가 없습니다. 설치는 Hidden Bar.app을 /Applications로 복사만 하고 앱 시작, 권한 요청, 로그인 항목 등록을 하지 않습니다. uninstall/zap은 앱 종료, 기존 LauncherApplication 기록 인증 해제, 제거 정리 데이터 삭제만 수행합니다. ZIP SHA-256 a811717d84faba478ea66caa224f3360a9e5d844a7a50fa09009f32d78131a44는 cask와 GitHub 자산 모두에 일치합니다. Mach-O 헤더 검증 결과 고유한 실제 Mach-O 6개는 모두 x86_64와 arm64를 포함하는 fat 바이너리이고 총 슬라이스는 12개입니다. 모든 슬라이스는 LC_CODE_SIGNATURE를 포함하고 hardened runtime이 활성화되어 있으며 Developer ID Application: Dwarves Foundation Company Limited (W777S7V8TN)으로 서명되었습니다. 앱 번들에서는 s8ch v1 stapled ticket 컨테이너와 g8tk trailer를 정적으로 분석할 수 있습니다. 이는 로컬 증거일 뿐 Linux에서 온라인 Gatekeeper 판정을 주장하지 않습니다. macOS 13 이상이 필요합니다. 주 앱은 LSUIElement이고 번들 ID는 com.dwarvesv.minimalbar이며 직접 빌드에는 샌드박스 entitlement가 없고 임베드된 entitlement 사전은 비어 있습니다. 메뉴 막대 항목을 읽고 숨기는 Accessibility는 첫 실행 후 관련 기능을 사용할 때 사용자가 부여하는 권한이며, 권한이 없을 때만 시스템 설정을 엽니다. 설치 단계에서는 프롬프트가 없습니다. Info.plist에는 카메라, 마이크, 화면 녹화, AppleEvents 사용 문자열이 없고 제품과 소스는 메뉴 막대 항목만 읽으며 화면 내용은 읽지 않는다고 명시합니다. 로그인 시 시작은 기본적으로 꺼져 있고 사용자가 활성화하면 SMAppService.mainApp이 주 앱을 등록합니다. 기존 com.dwarvesv.LauncherApplication 헬퍼는 더 이상 번들에 포함되지 않으며 업그레이드 마이그레이션 정리에서 한 번만 인증이 해제됩니다. GitHub/Homebrew 직접 빌드는 Sparkle 2.10.1을 포함하고 HTTPS로 GitHub appcast를 가져온 뒤 EdDSA 공개 키 C7MH2NS46vKD4nNh9i4BmFzXLSAbP2jM5qZcaPw9u3w=로 검증합니다. 시스템 프로파일 전송은 비활성화되어 있고 개인정보를 보내지 않습니다. Homebrew 설치 자체는 업데이트 확인이나 다운로드를 수행하지 않으며, Sparkle의 예약 또는 수동 업데이트 동작은 앱이 시작한 뒤에야 초기화됩니다. 지원되는 상류 Linux 또는 Windows 패키지나 산출물이 없으므로 macOS brew cask 경로만 선언합니다.",
        },
    },
    "keycastr": {
        "name": "KeyCastr", "publisher": "KeyCastr contributors", "category": "system", "mark": "Kc", "accent": "#6d83a6",
        "repo": "keycastr/keycastr", "license": "BSD-3-Clause", "licenseFile": "LICENSE.md", "licenseRef": "main/LICENSE.md",
        "homepage": "https://github.com/keycastr/keycastr", "cask": "keycastr", "caskArchitectures": ["x86_64", "aarch64"],
        "summary": {
            "zh-CN": "开源按键可视化工具，在录屏、演示或协作时显示键盘输入和可选的鼠标点击。",
            "zh-TW": "開源按鍵視覺化工具，在錄屏、演示或協作時顯示鍵盤輸入和可選的滑鼠點擊。",
            "en": "An open-source keystroke visualizer for showing keyboard input and optional mouse clicks during screencasts, presentations and collaboration.",
            "ja": "画面収録、プレゼンテーション、共同作業でキー入力と任意のマウスクリックを表示する、オープンソースのキーストローク可視化ツール。",
            "ko": "화면 녹화, 발표, 협업 중 키보드 입력과 선택적 마우스 클릭을 표시하는 오픈 소스 키 입력 시각화 도구.",
        },
        "extra": {
            "zh-CN": "本条目对应稳定版 v0.11.1。Homebrew cask 无 formula 依赖、冲突或 caveat，只将 KeyCastr.app 复制到应用程序目录，zap 仅在卸载清理时移除 HTTPStorage 和偏好设置。ZIP 的 SHA-256 摘要已与 cask 核对，归档顶层只有 KeyCastr.app；13 个符号链接均为框架内部相对路径，无绝对路径或越界目标。主程序及全部 11 个 Mach-O 均为含 x86_64 与 arm64 的 universal binary，Info.plist 与 Sparkle appcast 均声明最低 macOS 10.13。应用是 LSUIElement 后台界面工具，读取按键或鼠标事件需要用户首次运行后在系统设置授予输入监控或辅助功能权限；Info.plist 没有摄像头、麦克风等用途字符串。cask 声明 auto_updates: true，应用内置 Sparkle，更新源为 keycastr.github.io/appcast.xml；README 说明 Sparkle 是其唯一网络机制。安装不启动应用、不请求权限、不捕获输入、不检查或下载更新。",
            "zh-TW": "本條目對應穩定版 v0.11.1。Homebrew cask 無 formula 相依性、衝突或 caveat，只將 KeyCastr.app 複製到應用程式目錄，zap 僅在解除安裝清理時移除 HTTPStorage 和偏好設定。ZIP 的 SHA-256 摘要已與 cask 核對，封存檔頂層只有 KeyCastr.app；13 個符號連結均為框架內部相對路徑，無絕對路徑或越界目標。主程式及全部 11 個 Mach-O 均為含 x86_64 與 arm64 的 universal binary，Info.plist 與 Sparkle appcast 均宣告最低 macOS 10.13。應用是 LSUIElement 背景介面工具，讀取按鍵或滑鼠事件需要使用者首次執行後在系統設定授予輸入監控或輔助功能權限；Info.plist 沒有攝影機、麥克風等用途字串。cask 宣告 auto_updates: true，應用內建 Sparkle，更新來源為 keycastr.github.io/appcast.xml；README 說明 Sparkle 是其唯一網路機制。安裝不啟動應用程式、不請求權限、不擷取輸入、不檢查或下載更新。",
            "en": "This entry corresponds to stable v0.11.1. The Homebrew cask has no formula dependency, conflict or caveat; it only copies KeyCastr.app into Applications, and zap removes HTTP storage and preferences only during uninstall cleanup. The ZIP SHA-256 digest matches the cask, and the archive contains only KeyCastr.app at its top level. All 13 symlinks are framework-internal relative paths, with no absolute or archive-outside targets. The primary executable and all 11 Mach-O files are universal binaries containing x86_64 and arm64; both Info.plist and the Sparkle appcast declare macOS 10.13 as the minimum system version. The app is an LSUIElement background-interface utility. Reading keystroke or mouse events requires Input Monitoring or Accessibility permission granted by the user after first launch; the plist contains no camera, microphone or other usage strings. The cask declares auto_updates: true, and the bundled Sparkle updater uses keycastr.github.io/appcast.xml; the README states that Sparkle is its only networking mechanism. Installation launches no app, requests no permission, captures no input and checks for or downloads no update.",
            "ja": "このエントリは安定版 v0.11.1 に対応します。Homebrew cask に formula 依存関係、競合、caveat はなく、KeyCastr.app をアプリケーションフォルダへコピーするだけです。zap はアンインストール時のクリーンアップでのみ HTTP ストレージと設定を削除します。ZIP の SHA-256 ダイジェストは cask と一致し、アーカイブの最上位には KeyCastr.app のみが含まれます。13 個のシンボリックリンクはすべてフレームワーク内部の相対パスで、絶対パスやアーカイブ外を指すターゲットはありません。主要実行ファイルと全 11 個の Mach-O は x86_64 と arm64 を含む universal binary で、Info.plist と Sparkle appcast の両方が最低システムバージョンとして macOS 10.13 を宣言しています。アプリは LSUIElement のバックグラウンド UI ユーティリティです。キー入力やマウスイベントの読み取りには、初回起動後にユーザーがシステム設定で入力監視またはアクセシビリティ権限を許可する必要があり、plist にカメラ、マイクなどの用途文字列はありません。cask は auto_updates: true を宣言し、同梱の Sparkle アップデーターは keycastr.github.io/appcast.xml を使用します。README では Sparkle が唯一のネットワーク機構と説明されています。インストール時にアプリの起動、権限要求、入力キャプチャ、更新確認や更新のダウンロードは行われません。",
            "ko": "이 항목은 안정 v0.11.1에 해당합니다. Homebrew cask에는 formula 종속성, 충돌, caveat가 없고 KeyCastr.app을 응용 프로그램 폴더로 복사만 하며, zap은 제거 정리 시에만 HTTP 저장소와 환경설정을 삭제합니다. ZIP SHA-256 다이제스트는 cask와 일치하고 아카이브 최상위에는 KeyCastr.app만 있습니다. 13개 심볼릭 링크는 모두 프레임워크 내부 상대 경로이며 절대 경로나 아카이브 외부 대상은 없습니다. 기본 실행 파일과 11개 Mach-O 파일은 모두 x86_64와 arm64를 포함하는 universal binary이고 Info.plist와 Sparkle appcast 모두 최소 시스템 버전으로 macOS 10.13을 선언합니다. 앱은 LSUIElement 백그라운드 인터페이스 유틸리티입니다. 키 입력이나 마우스 이벤트를 읽으려면 첫 실행 후 사용자가 시스템 설정에서 입력 모니터링 또는 손쉬운 사용 권한을 부여해야 하며, plist에는 카메라, 마이크 등 사용 문자열이 없습니다. cask는 auto_updates: true를 선언하고 번들된 Sparkle 업데이터는 keycastr.github.io/appcast.xml을 사용합니다. README는 Sparkle이 유일한 네트워크 메커니즘이라고 설명합니다. 설치 시 앱을 시작하거나 권한을 요청하거나 입력을 캡처하거나 업데이트를 확인·다운로드하지 않습니다.",
        },
    },
    "flycut": {
        "name": "Flycut", "publisher": "General Arcade and contributors", "category": "tools", "mark": "Fc", "accent": "#6886a4",
        "repo": "TermiT/Flycut", "license": "MIT", "licenseFile": "license.txt", "licenseRef": "master/license.txt",
        "homepage": "https://github.com/TermiT/Flycut", "cask": "flycut", "caskArchitectures": ["x86_64", "aarch64"],
        "summary": {
            "zh-CN": "简洁的开源 macOS 剪贴板管理器，记录复制历史，并通过快捷键搜索、选择和重新粘贴文本。",
            "zh-TW": "簡潔的開源 macOS 剪貼簿管理器，記錄複製歷史，並透過快捷鍵搜尋、選擇和重新貼上文字。",
            "en": "A clean open-source macOS clipboard manager that keeps copy history and lets you search, select and paste text again with hotkeys.",
            "ja": "コピー履歴を保存し、ホットキーで検索・選択して再貼り付けできる、シンプルなオープンソースの macOS クリップボードマネージャー。",
            "ko": "복사 기록을 저장하고 단축키로 검색·선택하여 다시 붙여넣을 수 있는 깔끔한 오픈 소스 macOS 클립보드 관리자.",
        },
        "extra": {
            "zh-CN": "本条目对应稳定版 1.9.6，GitHub 的 /releases/tags/1.9.6 与 /releases/latest 返回同一非草稿、非预发布对象。Homebrew cask 无 formula 依赖、冲突或 caveat，只将 Flycut.app 复制到应用程序目录，zap 仅在卸载清理时移除偏好设置。ZIP 的 SHA-256 摘要已与 cask 核对，归档顶层只有 Flycut.app；3 个符号链接均为框架内部相对路径，无绝对路径或越界目标。主程序、CloudKit framework 和 LoginItem helper 共 3 个 Mach-O 均为含 x86_64 与 arm64 的 universal binary，Info.plist 声明最低 macOS 10.10。应用是 LSUIElement 后台工具；首次启动后，向其他应用粘贴历史内容需要用户在系统设置授予辅助功能权限，plist 的 NSAppleEventsUsageDescription 仅说明用于粘贴，源码通过 CGEvent 合成 Cmd-V。安装不会启动应用、请求权限、注册登录项或读取剪贴板。登录启动默认关闭，只能由用户在偏好设置中启用；iCloud/CloudKit 设置与剪贴记录同步也默认关闭，用户启用后才会访问 Apple CloudKit。源码和工程没有 Sparkle、appcast 或通用 URL 加载代码，cask 未声明自动更新。剪贴板历史可能包含敏感信息，可暂停跟踪、清除历史或配置密码项跳过。",
            "zh-TW": "本條目對應穩定版 1.9.6，GitHub 的 /releases/tags/1.9.6 與 /releases/latest 傳回同一非草稿、非預先發布物件。Homebrew cask 無 formula 相依性、衝突或 caveat，只將 Flycut.app 複製到應用程式目錄，zap 僅在解除安裝清理時移除偏好設定。ZIP 的 SHA-256 摘要已與 cask 核對，封存檔頂層只有 Flycut.app；3 個符號連結均為框架內部相對路徑，無絕對路徑或越界目標。主程式、CloudKit framework 和 LoginItem helper 共 3 個 Mach-O 均為含 x86_64 與 arm64 的 universal binary，Info.plist 宣告最低 macOS 10.10。應用是 LSUIElement 背景工具；首次啟動後，向其他應用程式貼上歷史內容需要使用者在系統設定授予輔助使用權限，plist 的 NSAppleEventsUsageDescription 僅說明用於貼上，原始碼透過 CGEvent 合成 Cmd-V。安裝不會啟動應用程式、請求權限、註冊登入項目或讀取剪貼簿。登入啟動預設關閉，只能由使用者在偏好設定中啟用；iCloud/CloudKit 設定與剪貼記錄同步也預設關閉，使用者啟用後才會存取 Apple CloudKit。原始碼和專案沒有 Sparkle、appcast 或通用 URL 載入程式碼，cask 未宣告自動更新。剪貼簿歷史可能包含敏感資訊，可暫停追蹤、清除歷史或設定密碼項目略過。",
            "en": "This entry corresponds to stable 1.9.6; GitHub's /releases/tags/1.9.6 and /releases/latest return the same non-draft, non-prerelease object. The Homebrew cask has no formula dependency, conflict or caveat; it only copies Flycut.app into Applications, and zap removes preferences only during uninstall cleanup. The ZIP SHA-256 digest matches the cask, the archive contains only Flycut.app at its top level, and all 3 symlinks are framework-internal relative paths with no absolute or archive-outside targets. The primary executable, CloudKit framework and LoginItem helper—all 3 Mach-O files—are universal binaries containing x86_64 and arm64; Info.plist declares macOS 10.10 as the minimum. The app is an LSUIElement background utility. After first launch, pasting history into other apps requires the user to grant Accessibility in System Settings; NSAppleEventsUsageDescription states only that Apple events are used for pasting, while source code synthesizes Cmd-V through CGEvent. Installation does not launch the app, request permission, register a login item or read the clipboard. Launch-at-login is off by default and can only be enabled by the user in preferences; iCloud/CloudKit settings and clippings sync are also off by default and use Apple CloudKit only after the user enables them. The source and project contain no Sparkle, appcast or generic URL-loading code, and the cask does not declare automatic updates. Clipboard history can contain sensitive data; tracking can be paused, history cleared, and password items configured to be skipped.",
            "ja": "このエントリは安定版 1.9.6 に対応し、GitHub の /releases/tags/1.9.6 と /releases/latest は同一のドラフトでもプレリリースでもないオブジェクトを返します。Homebrew cask に formula 依存関係、競合、caveat はなく、Flycut.app をアプリケーションフォルダへコピーするだけです。zap はアンインストール時のクリーンアップでのみ環境設定を削除します。ZIP の SHA-256 ダイジェストは cask と一致し、アーカイブ最上位には Flycut.app のみが含まれます。3 個のシンボリックリンクはすべてフレームワーク内部の相対パスで、絶対パスやアーカイブ外を指すターゲットはありません。主要実行ファイル、CloudKit フレームワーク、LoginItem helper の全 3 個の Mach-O は x86_64 と arm64 を含む universal binary で、Info.plist の最低システム要件は macOS 10.10 です。アプリは LSUIElement のバックグラウンドユーティリティです。初回起動後、ほかのアプリへ履歴を貼り付けるには、ユーザーがシステム設定でアクセシビリティ権限を許可する必要があります。NSAppleEventsUsageDescription は貼り付け用途だけを示し、ソースコードは CGEvent で Cmd-V を合成します。インストール時にアプリの起動、権限要求、ログイン項目の登録、クリップボードの読み取りは行われません。ログイン時起動は既定で無効で、ユーザーが環境設定で有効にした場合のみ登録されます。iCloud/CloudKit の設定とクリッピング同期も既定で無効で、ユーザーが有効にした後にのみ Apple CloudKit へアクセスします。ソースとプロジェクトに Sparkle、appcast、汎用 URL 読み込みコードはなく、cask も自動更新を宣言していません。クリップボード履歴には機密情報が含まれる可能性があるため、追跡の一時停止、履歴の消去、パスワード項目のスキップ設定ができます。",
            "ko": "이 항목은 안정 1.9.6에 해당하며 GitHub의 /releases/tags/1.9.6과 /releases/latest는 초안도 사전 릴리스도 아닌 동일한 객체를 반환합니다. Homebrew cask에는 formula 종속성, 충돌, caveat가 없고 Flycut.app을 응용 프로그램 폴더로 복사만 하며, zap은 제거 정리 시에만 환경설정을 삭제합니다. ZIP SHA-256 다이제스트는 cask와 일치하고 아카이브 최상위에는 Flycut.app만 있습니다. 3개 심볼릭 링크는 모두 프레임워크 내부 상대 경로이며 절대 경로나 아카이브 외부 대상은 없습니다. 주 실행 파일, CloudKit 프레임워크, LoginItem helper 등 Mach-O 3개는 모두 x86_64와 arm64를 포함하는 universal binary이고 Info.plist 최소 시스템 버전은 macOS 10.10입니다. 앱은 LSUIElement 백그라운드 유틸리티입니다. 첫 실행 후 다른 앱에 기록을 붙여넣으려면 사용자가 시스템 설정에서 손쉬운 사용 권한을 부여해야 합니다. NSAppleEventsUsageDescription은 붙여넣기 용도만 설명하고 소스 코드는 CGEvent로 Cmd-V를 합성합니다. 설치 시 앱을 시작하거나 권한을 요청하거나 로그인 항목을 등록하거나 클립보드를 읽지 않습니다. 로그인 시 시작은 기본적으로 꺼져 있고 사용자가 환경설정에서 활성화할 때만 등록됩니다. iCloud/CloudKit 설정 및 클리핑 동기화도 기본적으로 꺼져 있으며 사용자가 활성화한 후에만 Apple CloudKit에 접근합니다. 소스와 프로젝트에 Sparkle, appcast, 일반 URL 로딩 코드가 없고 cask도 자동 업데이트를 선언하지 않습니다. 클립보드 기록에는 민감한 정보가 포함될 수 있으므로 추적 일시정지, 기록 삭제, 비밀번호 항목 건너뛰기를 구성할 수 있습니다.",
        },
    },
    "wailbrew": {
        "name": "WailBrew", "publisher": "Nico Wickersheim and contributors", "category": "dev", "mark": "Wl", "accent": "#6387a4",
        "repo": "wickenico/WailBrew", "license": "MIT", "licenseFile": "LICENSE", "licenseRef": "main/LICENSE",
        "homepage": "https://github.com/wickenico/WailBrew", "cask": "wailbrew", "caskArchitectures": ["x86_64", "aarch64"],
        "summary": {
            "zh-CN": "开源 Homebrew 图形界面，使用 Wails、Go 和 React 管理 formula、cask、tap、服务、清理和 Brewfile。",
            "zh-TW": "開源 Homebrew 圖形介面，使用 Wails、Go 和 React 管理 formula、cask、tap、服務、清理和 Brewfile。",
            "en": "An open-source graphical Homebrew interface built with Wails, Go and React for formulae, casks, taps, services, cleanup and Brewfiles.",
            "ja": "Wails、Go、React で構築された、formula、cask、tap、サービス、クリーンアップ、Brewfile を管理するオープンソースの Homebrew GUI。",
            "ko": "Wails, Go, React로 구축된 오픈 소스 Homebrew 그래픽 인터페이스로 formula, cask, tap, 서비스, 정리 및 Brewfile을 관리합니다.",
        },
        "extra": {
            "zh-CN": "本条目对应稳定版 v0.14.3，GitHub 标签发布与 latest 指向同一非草稿、非预发布对象；ZIP 摘要同时匹配 cask 和 GitHub 资产摘要，归档顶层只有 WailBrew.app，无符号链接或越界路径。唯一真实 Mach-O 是含 x86_64 与 arm64 的 universal binary；cask 元数据声明支持 macOS 11 Big Sur 及更新版本。cask 无 formula 依赖、冲突或 caveat，安装只复制 WailBrew.app，不启动应用、不请求权限、不注册登录项；zap 路径仅在卸载清理时使用。WailBrew 是已有 Homebrew 的前端，不会替用户安装 Homebrew。启动后会执行 brew update 及已装 formula、cask、tap、outdated、leaves 查询；安装、卸载、升级、tap/untap/trust、服务、doctor、cleanup 和 Brewfile 操作都来自显式 GUI 动作，生产命令使用 argv 数组而不是 shell 拼接。brew services start 只会把用户选择的 Homebrew 服务注册到 launchd，不会注册 WailBrew 自身。需要 sudo 时才通过模式 0700 的临时 askpass 脚本和 osascript 提示；NoQuarantine 默认关闭，启用后才会用 xattr 移除隔离属性，AutoRelaunch 默认开启但只影响升级中的 cask 应用。配置默认位于 ~/.config/wailbrew/config.json，并兼容 ~/.wailbrew/config.json 与 WAILBREW_CONFIG_FILE。Info.plist 无摄像头、麦克风等用途字符串，签名 SuperBlob 无 entitlement 槽；两个架构切片均开启 hardened runtime，嵌入式 CMS 叶证书主体为 Developer ID Application: Nico Wickersheim (2MC8SWF35Z)，但 Linux 无法运行 macOS spctl 或 stapler 在线评估。更新检查只运行 brew info --cask --json=v2 wailbrew，没有 Sparkle、appcast 或直接 GitHub API 自更新；发布页只能由用户手动打开。包详情会按需请求 formulae.brew.sh 公共安装统计，brew vulns 仅在用户安全扫描时访问 OSV.dev，代理测试和外部链接也都是显式操作；源码未发现生产遥测依赖。",
            "zh-TW": "本條目對應穩定版 v0.14.3，GitHub 標籤發布與 latest 指向同一非草稿、非預先發布物件；ZIP 摘要同時符合 cask 和 GitHub 資產摘要，封存檔頂層只有 WailBrew.app，無符號連結或越界路徑。唯一真實 Mach-O 是含 x86_64 與 arm64 的 universal binary；cask 中繼資料宣告支援 macOS 11 Big Sur 與更新版本。cask 無 formula 相依性、衝突或 caveat，安裝只複製 WailBrew.app，不啟動應用程式、不請求權限、不註冊登入項目；zap 路徑僅在解除安裝清理時使用。WailBrew 是既有 Homebrew 的前端，不會替使用者安裝 Homebrew。啟動後會執行 brew update 及已安裝 formula、cask、tap、outdated、leaves 查詢；安裝、解除安裝、升級、tap/untap/trust、服務、doctor、cleanup 和 Brewfile 操作都來自明確 GUI 動作，生產命令使用 argv 陣列而不是 shell 串接。brew services start 只會把使用者選擇的 Homebrew 服務註冊到 launchd，不會註冊 WailBrew 本身。需要 sudo 時才透過模式 0700 的暫存 askpass 指令碼和 osascript 提示；NoQuarantine 預設關閉，啟用後才會用 xattr 移除隔離屬性，AutoRelaunch 預設開啟但只影響升級中的 cask 應用程式。設定預設位於 ~/.config/wailbrew/config.json，並相容 ~/.wailbrew/config.json 與 WAILBREW_CONFIG_FILE。Info.plist 無攝影機、麥克風等用途字串，簽章 SuperBlob 無 entitlement 槽；兩個架構切片均開啟 hardened runtime，內嵌 CMS 葉憑證主體為 Developer ID Application: Nico Wickersheim (2MC8SWF35Z)，但 Linux 無法執行 macOS spctl 或 stapler 線上評估。更新檢查只執行 brew info --cask --json=v2 wailbrew，沒有 Sparkle、appcast 或直接 GitHub API 自更新；發布頁只能由使用者手動開啟。套件詳情會依要求請求 formulae.brew.sh 公共安裝統計，brew vulns 僅在使用者安全掃描時存取 OSV.dev，代理測試和外部連結也都是明確操作；原始碼未發現生產遙測相依性。",
            "en": "This entry corresponds to stable v0.14.3; the GitHub tag release and latest endpoint resolve to the same non-draft, non-prerelease object. The ZIP digest matches both the cask and GitHub asset digest, and the archive contains only WailBrew.app at its top level, with no symlinks or archive-outside paths. Its only real Mach-O is a universal binary containing x86_64 and arm64; cask metadata declares macOS 11 Big Sur and newer. The cask has no formula dependency, conflict or caveat. Installation only copies WailBrew.app; it does not launch the app, request permissions or register a login item, and zap paths are used only during uninstall cleanup. WailBrew is a front-end for an existing Homebrew installation and does not install Homebrew itself. After launch it runs brew update plus installed formula, cask, tap, outdated and leaves queries; install, uninstall, upgrade, tap/untap/trust, service, doctor, cleanup and Brewfile actions are explicit GUI operations, and production commands use argv arrays rather than shell concatenation. brew services start registers only the selected Homebrew service with launchd, not WailBrew itself. A mode-0700 temporary askpass script and osascript prompt are used only when sudo is required. NoQuarantine defaults off and uses xattr only when enabled; AutoRelaunch defaults on but affects only cask apps during upgrade. Config defaults to ~/.config/wailbrew/config.json, with legacy ~/.wailbrew/config.json and WAILBREW_CONFIG_FILE support. Info.plist has no camera, microphone or other usage strings, and the signature SuperBlob has no entitlement slot. Both architecture slices enable the hardened runtime; the embedded CMS leaf subject is Developer ID Application: Nico Wickersheim (2MC8SWF35Z), while Linux cannot perform macOS spctl or stapler online assessment. Update checks run only brew info --cask --json=v2 wailbrew; there is no Sparkle, appcast or direct GitHub API self-updater, and the release page opens only through manual user action. Package details fetch public formulae.brew.sh install statistics on demand, brew vulns reaches OSV.dev only during an explicit security scan, proxy tests and external links are explicit actions, and no production telemetry dependency was found in source.",
            "ja": "このエントリは安定版 v0.14.3 に対応し、GitHub のタグリリースと latest エンドポイントはドラフトでもプレリリースでもない同一オブジェクトを返します。ZIP ダイジェストは cask と GitHub アセットの両方に一致し、アーカイブ最上位には WailBrew.app のみが含まれ、シンボリックリンクやアーカイブ外を指すパスはありません。唯一の実 Mach-O は x86_64 と arm64 を含む universal binary で、cask メタデータは macOS 11 Big Sur 以降を宣言しています。cask に formula 依存関係、競合、caveat はなく、インストールは WailBrew.app のコピーだけで、アプリの起動、権限要求、ログイン項目の登録は行いません。zap パスはアンインストール時のクリーンアップでのみ使用されます。WailBrew は既存 Homebrew のフロントエンドであり、Homebrew 自体をインストールしません。起動後は brew update に加え、インストール済み formula、cask、tap、outdated、leaves のクエリを実行します。インストール、アンインストール、アップグレード、tap/untap/trust、サービス、doctor、cleanup、Brewfile の操作はすべて明示的な GUI 操作で、本番コマンドはシェル連結ではなく argv 配列を使用します。brew services start は選択した Homebrew サービスだけを launchd に登録し、WailBrew 自体は登録しません。sudo が必要な場合に限り、モード 0700 の一時 askpass スクリプトと osascript プロンプトを使用します。NoQuarantine は既定で無効で、有効時のみ xattr で隔離属性を削除します。AutoRelaunch は既定で有効ですが、アップグレード中の cask アプリにのみ影響します。設定は既定で ~/.config/wailbrew/config.json にあり、従来の ~/.wailbrew/config.json と WAILBREW_CONFIG_FILE もサポートします。Info.plist にカメラ、マイクなどの用途文字列はなく、署名 SuperBlob に entitlement スロットもありません。両アーキテクチャスライスで hardened runtime が有効で、埋め込み CMS のリーフ主体は Developer ID Application: Nico Wickersheim (2MC8SWF35Z) ですが、Linux では macOS の spctl や stapler オンライン評価は実行できません。更新確認は brew info --cask --json=v2 wailbrew を実行するだけで、Sparkle、appcast、直接 GitHub API を使う自己更新機能はなく、リリースページは手動操作でのみ開きます。パッケージ詳細は必要時に formulae.brew.sh の公開インストール統計を取得し、brew vulns は明示的なセキュリティスキャン時のみ OSV.dev に到達します。プロキシテストと外部リンクも明示的操作であり、ソースから本番テレメトリ依存は見つかりませんでした。",
            "ko": "이 항목은 안정 v0.14.3에 해당하며 GitHub 태그 릴리스와 latest 엔드포인트는 초안도 사전 릴리스도 아닌 동일한 객체를 반환합니다. ZIP 다이제스트는 cask와 GitHub 자산 다이제스트 모두에 일치하고 아카이브 최상위에는 WailBrew.app만 있으며 심볼릭 링크나 아카이브 외부 경로가 없습니다. 유일한 실제 Mach-O는 x86_64와 arm64를 포함하는 universal binary이고 cask 메타데이터는 macOS 11 Big Sur 이상을 선언합니다. cask에는 formula 종속성, 충돌, caveat가 없고 설치는 WailBrew.app 복사만 수행하며 앱 시작, 권한 요청, 로그인 항목 등록을 하지 않습니다. zap 경로는 제거 정리 시에만 사용됩니다. WailBrew는 기존 Homebrew의 프런트엔드이며 Homebrew 자체를 설치하지 않습니다. 실행 후 brew update와 설치된 formula, cask, tap, outdated, leaves 쿼리를 실행합니다. 설치, 제거, 업그레이드, tap/untap/trust, 서비스, doctor, cleanup, Brewfile 작업은 모두 명시적 GUI 동작이고 프로덕션 명령은 셸 연결이 아닌 argv 배열을 사용합니다. brew services start는 선택한 Homebrew 서비스만 launchd에 등록하며 WailBrew 자체를 등록하지 않습니다. sudo가 필요할 때만 모드 0700 임시 askpass 스크립트와 osascript 프롬프트를 사용합니다. NoQuarantine은 기본적으로 꺼져 있고 활성화할 때만 xattr으로 격리 속성을 제거합니다. AutoRelaunch는 기본적으로 켜져 있지만 업그레이드 중인 cask 앱에만 영향을 줍니다. 설정은 기본적으로 ~/.config/wailbrew/config.json에 저장되고 기존 ~/.wailbrew/config.json 및 WAILBREW_CONFIG_FILE을 지원합니다. Info.plist에는 카메라, 마이크 등 사용 문자열이 없고 서명 SuperBlob에는 entitlement 슬롯이 없습니다. 두 아키텍처 슬라이스 모두 hardened runtime이 활성화되어 있고 내장 CMS 리프 주체는 Developer ID Application: Nico Wickersheim (2MC8SWF35Z)이지만 Linux에서는 macOS spctl이나 stapler 온라인 평가를 실행할 수 없습니다. 업데이트 확인은 brew info --cask --json=v2 wailbrew만 실행하고 Sparkle, appcast, 직접 GitHub API 자동 업데이트는 없으며 릴리스 페이지는 사용자의 수동 동작으로만 열립니다. 패키지 세부 정보는 요청 시 formulae.brew.sh 공개 설치 통계를 가져오고 brew vulns는 명시적 보안 검사 시에만 OSV.dev에 접근하며 프록시 테스트와 외부 링크도 명시적 동작입니다. 소스에서 프로덕션 텔레메트리 의존성은 발견되지 않았습니다.",
        },
    },
}

def main() -> None:
    excluded_in_reviewed = EXCLUDED_APP_IDS.intersection(REVIEWED)
    if excluded_in_reviewed:
        raise ValueError(f"Excluded app IDs must not have reviewed catalog entries: {sorted(excluded_in_reviewed)}")
    for category_path in (ROOT / "catalog").glob("*.json"):
        if category_path.name == "index.json":
            continue
        category = json.loads(category_path.read_text())
        retained_apps = [app for app in category["apps"] if app["id"] not in EXCLUDED_APP_IDS]
        if len(retained_apps) != len(category["apps"]):
            category["apps"] = retained_apps
            category_path.write_text(json.dumps(category, ensure_ascii=False, indent=2) + "\n")
    for app_id, source in REVIEWED.items():
        app = dict(source)
        variants = []
        if "winget" in app:
            _, installer = winget(app["winget"])
            arches = []
            for entry in installer["Installers"]:
                if entry["Architecture"] == "x64":
                    arches.append("x86_64")
                elif entry["Architecture"] == "arm64":
                    arches.append("aarch64")
            arches = list(dict.fromkeys(arches))
            if arches:
                variants.append(package_variant(app_id, "windows", "winget", app["winget"], arches, False, app.get("windowsExtra", app["extra"])))
        if "formula" in app:
            package = FORMULAS[app["formula"]]
            macos = formula_arches(app_id, package, "macos")
            if macos:
                variants.append(package_variant(app_id, "macos", "brew", app["formula"], macos, False, app["extra"]))
        if "cask" in app:
            package = CASKS[app["cask"]]
            if package["deprecated"] or package["disabled"]:
                raise ValueError(f"Unavailable cask for {app_id}")
            variants.append(package_variant(app_id, "macos", "brew", app["cask"], brew_cask_arches(package), True, app["extra"]))
        if "flathub" in app:
            for arch in flathub_arches(app_id, app["flathub"]):
                variants.append(package_variant(app_id, "linux", "flatpak", app["flathub"], [arch], False, app.get("linuxExtra", app["extra"])))
        for release in app.get("linuxReleases", []):
            variants.append(release_variant(app_id, app["repo"], release, app.get("linuxExtra", app["extra"])))
        if "sourceRepository" not in app:
            app["sourceRepository"] = f"https://github.com/{app['repo']}"
        if "licenseUrl" not in app:
            app["licenseUrl"] = f"https://raw.githubusercontent.com/{app['repo']}/{app.get('licenseRef', 'HEAD/' + app['licenseFile'])}"
        app.update({"id": app_id, "variants": variants})
        for key in ("repo", "licenseFile", "licenseRef", "formula", "cask", "caskArchitectures", "winget", "wingetVersion", "flathub", "linuxReleases", "extra", "linuxExtra", "windowsExtra", "requiresAdmin", "interactive", "linuxRequiresAdmin", "linuxInteractive", "windowsRequiresAdmin", "windowsInteractive"):
            app.pop(key, None)
        path = ROOT / "catalog" / f"{app['category']}.json"
        category = json.loads(path.read_text())
        previous = next((existing for existing in category["apps"] if existing["id"] == app_id), None)
        if previous is not None:
            previous_variants = {variant["id"] for variant in previous["variants"]}
            generated_variants = {variant["id"] for variant in app["variants"]}
            removed_variants = previous_variants - generated_variants
            allowed_removals = REMOVED_LINUX_BREW_VARIANTS.get(app_id, {})
            if not removed_variants <= set(allowed_removals):
                raise ValueError(f"Refusing to remove existing variant IDs for {app_id}: {sorted(removed_variants - set(allowed_removals))}")
        category["apps"] = [existing for existing in category["apps"] if existing["id"] != app_id]
        category["apps"].append(app)
        category["apps"].sort(key=lambda existing: existing["id"])
        path.write_text(json.dumps(category, ensure_ascii=False, indent=2) + "\n")
    index = ROOT / "catalog" / "index.json"
    document = json.loads(index.read_text())
    document["catalogVersion"] = "2026.11.18-9"
    index.write_text(json.dumps(document, ensure_ascii=False, indent=2) + "\n")
    print(f"Added {len(REVIEWED)} reviewed applications")


if __name__ == "__main__":
    main()
