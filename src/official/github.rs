//! GitHub Releases adapter.
//!
//! Only the documented latest-release API and repository-owned asset URLs are
//! accepted. Release notes and other untrusted strings never become commands.

use std::time::Duration;

use serde::Deserialize;

use super::resolver::{
    HostContext, NetworkOptions, OfficialReleasePackage, ResolvedOfficialInstall,
    asset_pattern_matches,
};
use super::{OfficialError, OfficialVariant, validate_file_name};

const MAX_RELEASE_RESPONSE: u64 = 4 * 1024 * 1024;

#[derive(Clone, Deserialize)]
struct GithubRelease {
    tag_name: String,
    draft: bool,
    prerelease: bool,
    assets: Vec<GithubAsset>,
}

#[derive(Clone, Deserialize)]
struct GithubAsset {
    name: String,
    browser_download_url: String,
    size: u64,
    digest: Option<String>,
    state: String,
}

pub(super) fn resolve(
    repository: &str,
    packages: &[OfficialReleasePackage],
    variant: &OfficialVariant,
    host: HostContext,
    network: NetworkOptions<'_>,
) -> Result<ResolvedOfficialInstall, OfficialError> {
    let endpoint = if network.allow_http {
        std::env::var("PAKKIT_OFFICIAL_GITHUB_API_URL")
            .ok()
            .filter(|value| !value.trim().is_empty())
            .map(|base| {
                format!(
                    "{}/repos/{repository}/releases/latest",
                    base.trim_end_matches('/')
                )
            })
            .unwrap_or_else(|| format!("https://api.github.com/repos/{repository}/releases/latest"))
    } else {
        format!("https://api.github.com/repos/{repository}/releases/latest")
    };
    resolve_endpoint(repository, packages, variant, host, network, &endpoint)
}

fn fetch_release(
    endpoint: &str,
    network: NetworkOptions<'_>,
) -> Result<GithubRelease, OfficialError> {
    let client = super::agent(Duration::from_secs(30), network.proxy, network.allow_http)?;
    let headers = [
        ("Accept", "application/vnd.github+json".to_owned()),
        ("X-GitHub-Api-Version", "2022-11-28".to_owned()),
        (
            "User-Agent",
            concat!("pakkit/", env!("CARGO_PKG_VERSION")).to_owned(),
        ),
    ];
    #[cfg(not(test))]
    let headers = headers.to_vec();
    #[cfg(test)]
    let mut headers = headers.to_vec();
    #[cfg(test)]
    if let Ok(token) = std::env::var("GITHUB_TOKEN")
        && !token.is_empty()
    {
        headers.push(("Authorization", format!("Bearer {token}")));
    }
    let headers = headers
        .iter()
        .map(|(name, value)| (*name, value.as_str()))
        .collect::<Vec<_>>();
    let response = super::get_following(&client, endpoint, &headers, network.allow_http)
        .map_err(OfficialError::ResolveRequest)?;
    if !response.status().is_success() {
        return Err(OfficialError::ResolveRequest(format!(
            "GitHub API 返回 {}",
            response.status()
        )));
    }
    let mut body = response.into_body();
    let mut reader = body.with_config().limit(MAX_RELEASE_RESPONSE).reader();
    serde_json::from_reader(&mut reader)
        .map_err(|error| OfficialError::Resolve(format!("GitHub release 响应无效: {error}")))
}

fn resolve_endpoint(
    repository: &str,
    packages: &[OfficialReleasePackage],
    variant: &OfficialVariant,
    host: HostContext,
    network: NetworkOptions<'_>,
    endpoint: &str,
) -> Result<ResolvedOfficialInstall, OfficialError> {
    let release = fetch_release(endpoint, network)?;
    select_asset_with_options(
        repository,
        packages,
        variant,
        host,
        release,
        network.allow_http,
    )
}

#[cfg(test)]
pub fn verify_release_hosts(
    repository: &str,
    packages: &[OfficialReleasePackage],
    variant: &OfficialVariant,
    hosts: &[HostContext],
    network: NetworkOptions<'_>,
) -> Result<Vec<ResolvedOfficialInstall>, OfficialError> {
    let endpoint = format!("https://api.github.com/repos/{repository}/releases/latest");
    let release = fetch_release(&endpoint, network)?;
    hosts
        .iter()
        .map(|host| {
            select_asset_with_options(repository, packages, variant, *host, release.clone(), false)
        })
        .collect()
}

fn select_asset_with_options(
    repository: &str,
    packages: &[OfficialReleasePackage],
    variant: &OfficialVariant,
    host: HostContext,
    release: GithubRelease,
    development: bool,
) -> Result<ResolvedOfficialInstall, OfficialError> {
    if repository.trim().is_empty() {
        return Err(OfficialError::Resolve("GitHub 仓库为空".to_owned()));
    }
    if host.os != variant.os || !variant.architectures.contains(&host.arch) {
        return Err(OfficialError::Resolve(
            "当前系统与目录变体不匹配".to_owned(),
        ));
    }
    if release.draft || release.prerelease || !valid_tag(&release.tag_name) {
        return Err(OfficialError::Resolve(
            "GitHub 返回的版本不是可用的稳定版".to_owned(),
        ));
    }
    let package = packages
        .iter()
        .find(|package| host.supports(package) && package.assets.contains_key(&host.arch))
        .ok_or_else(|| OfficialError::Resolve("当前系统没有受支持的安装包格式".to_owned()))?;
    let pattern = &package.assets[&host.arch];
    let mut matches = release
        .assets
        .iter()
        .filter(|asset| asset_pattern_matches(pattern, &asset.name));
    let asset = matches
        .next()
        .ok_or_else(|| OfficialError::Resolve(format!("没有资产匹配 {pattern}")))?;
    if matches.next().is_some() {
        return Err(OfficialError::Resolve(format!(
            "多个资产匹配 {pattern}，已停止以避免选错安装包"
        )));
    }
    if asset.state != "uploaded" {
        return Err(OfficialError::Resolve("GitHub 资产尚未上传完成".to_owned()));
    }
    validate_file_name(&asset.name)?;
    if !package.format.matches_file(&asset.name) {
        return Err(OfficialError::Resolve(
            "资产扩展名与安装格式不符".to_owned(),
        ));
    }
    validate_asset_url(repository, &release.tag_name, asset, development)?;
    let sha256 = parse_digest(asset.digest.as_deref())?;
    Ok(ResolvedOfficialInstall {
        variant_id: variant.id.clone(),
        version: Some(release.tag_name),
        file_name: asset.name.clone(),
        resolved_url: asset.browser_download_url.clone(),
        size: Some(asset.size),
        sha256,
        format: package.format,
        install: package.install.clone(),
        install_action: package.install.action(),
        expected_signer: package.expected_signer.clone(),
        requires_admin: package.install.action()
            == super::resolver::OfficialInstallAction::InstallPackage
            || variant.requires_admin,
    })
}

fn valid_tag(tag: &str) -> bool {
    !tag.is_empty()
        && tag.len() <= 255
        && !tag.contains(['/', '\\', '?', '#'])
        && !tag.chars().any(char::is_control)
}

fn validate_asset_url(
    repository: &str,
    tag: &str,
    asset: &GithubAsset,
    development: bool,
) -> Result<(), OfficialError> {
    let actual = url::Url::parse(&asset.browser_download_url)
        .map_err(|error| OfficialError::Resolve(format!("资产 URL 无效: {error}")))?;
    let expected = url::Url::parse(&format!(
        "https://github.com/{repository}/releases/download/{tag}/{}",
        asset.name
    ))
    .map_err(|error| OfficialError::Resolve(format!("资产 URL 无效: {error}")))?;
    let development_loopback = development
        && actual.scheme() == "http"
        && actual.host_str().is_some_and(|host| {
            host.eq_ignore_ascii_case("localhost")
                || host
                    .parse::<std::net::IpAddr>()
                    .is_ok_and(|ip| ip.is_loopback())
        });
    if actual != expected && !development_loopback {
        return Err(OfficialError::Resolve(
            "资产 URL 不属于所选仓库和版本".to_owned(),
        ));
    }
    Ok(())
}

fn parse_digest(digest: Option<&str>) -> Result<String, OfficialError> {
    let value = digest
        .and_then(|value| value.strip_prefix("sha256:"))
        .ok_or_else(|| OfficialError::Resolve("GitHub 未提供 SHA-256 digest".to_owned()))?;
    if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(OfficialError::Resolve(
            "GitHub SHA-256 digest 格式无效".to_owned(),
        ));
    }
    // Keep one canonical representation for both display and verification.
    Ok(value.to_ascii_lowercase())
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;
    fn select_asset(
        repository: &str,
        packages: &[OfficialReleasePackage],
        variant: &OfficialVariant,
        host: HostContext,
        release: GithubRelease,
    ) -> Result<ResolvedOfficialInstall, OfficialError> {
        select_asset_with_options(repository, packages, variant, host, release, false)
    }

    use crate::official::resolver::{
        OfficialInstallAction, OfficialInstallConfig, OfficialPackageFormat, OfficialReleasePackage,
    };
    use crate::official::{OfficialArch, OfficialMethod, OfficialOs};

    fn fixture() -> (OfficialVariant, Vec<OfficialReleasePackage>, GithubRelease) {
        let packages = vec![OfficialReleasePackage {
            format: OfficialPackageFormat::Deb,
            manager: Some(super::super::resolver::LinuxPackageManager::Apt),
            assets: BTreeMap::from([
                (OfficialArch::X86_64, "OpenIDEA-*-amd64.deb".to_owned()),
                (OfficialArch::Aarch64, "OpenIDEA-*-arm64.deb".to_owned()),
            ]),
            expected_signer: None,
            install: OfficialInstallConfig::Package,
        }];
        let variant = OfficialVariant {
            id: "openidea-linux".to_owned(),
            os: OfficialOs::Linux,
            architectures: vec![OfficialArch::X86_64, OfficialArch::Aarch64],
            method: OfficialMethod::GithubRelease {
                repository: "owner/repo".to_owned(),
                packages: packages.clone(),
            },
            requires_admin: true,
            interactive: false,
            note: crate::official::LocalizedText {
                zh_cn: String::new(),
                zh_tw: String::new(),
                en: String::new(),
                ja: String::new(),
                ko: String::new(),
            },
            source_url: "https://github.com/owner/repo".to_owned(),
            detection: None,
        };
        let release = GithubRelease {
            tag_name: "v1.2.3".to_owned(),
            draft: false,
            prerelease: false,
            assets: vec![GithubAsset {
                name: "OpenIDEA-v1.2.3-amd64.deb".to_owned(),
                browser_download_url: "https://github.com/owner/repo/releases/download/v1.2.3/OpenIDEA-v1.2.3-amd64.deb".to_owned(),
                size: 42,
                digest: Some(format!("sha256:{}", "a".repeat(64))),
                state: "uploaded".to_owned(),
            }],
        };
        (variant, packages, release)
    }

    fn host() -> HostContext {
        HostContext::new(
            OfficialOs::Linux,
            OfficialArch::X86_64,
            Some(super::super::resolver::LinuxPackageManager::Apt),
        )
    }

    #[test]
    fn selects_one_stable_asset_and_preserves_metadata() {
        let (variant, source, release) = fixture();
        let resolved = select_asset("owner/repo", &source, &variant, host(), release)
            .expect("valid stable asset");
        assert_eq!(resolved.version.as_deref(), Some("v1.2.3"));
        assert_eq!(resolved.size, Some(42));
        assert_eq!(resolved.sha256, "a".repeat(64));
        assert_eq!(
            resolved.install_action,
            OfficialInstallAction::InstallPackage
        );
        assert!(resolved.requires_admin);
    }

    #[test]
    fn ambiguity_prerelease_and_unofficial_urls_fail_closed() {
        let (variant, source, mut release) = fixture();
        let duplicate = release.assets[0].browser_download_url.clone();
        release.assets.push(GithubAsset {
            name: "OpenIDEA-other-amd64.deb".to_owned(),
            browser_download_url: duplicate,
            size: 42,
            digest: Some(format!("sha256:{}", "a".repeat(64))),
            state: "uploaded".to_owned(),
        });
        assert!(select_asset("owner/repo", &source, &variant, host(), release).is_err());

        let (_, _, mut release) = fixture();
        release.prerelease = true;
        assert!(select_asset("owner/repo", &source, &variant, host(), release).is_err());

        let (_, _, mut release) = fixture();
        release.assets[0].browser_download_url = "https://evil.example/installer.deb".to_owned();
        assert!(select_asset("owner/repo", &source, &variant, host(), release).is_err());
    }

    #[test]
    fn development_assets_accept_only_loopback() {
        let (variant, source, mut release) = fixture();
        release.assets[0].browser_download_url =
            "http://127.0.0.1:8080/OpenIDEA-v1.2.3-amd64.deb".to_owned();
        assert!(
            select_asset_with_options(
                "owner/repo",
                &source,
                &variant,
                host(),
                release.clone(),
                true,
            )
            .is_ok()
        );
        assert!(select_asset("owner/repo", &source, &variant, host(), release.clone()).is_err());
        release.assets[0].browser_download_url =
            "http://192.0.2.1/OpenIDEA-v1.2.3-amd64.deb".to_owned();
        assert!(
            select_asset_with_options("owner/repo", &source, &variant, host(), release, true)
                .is_err()
        );
    }

    #[test]
    fn missing_or_invalid_digest_blocks_automatic_execution() {
        let (variant, source, mut release) = fixture();
        release.assets[0].digest = None;
        assert!(select_asset("owner/repo", &source, &variant, host(), release).is_err());

        let (_, _, mut release) = fixture();
        release.assets[0].digest = Some("sha256:short".to_owned());
        assert!(select_asset("owner/repo", &source, &variant, host(), release).is_err());
    }

    fn serve(status: &'static str, body: Vec<u8>) -> (String, std::thread::JoinHandle<()>) {
        use std::io::{Read as _, Write as _};

        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("local listener");
        let address = listener.local_addr().expect("local address");
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("local request");
            let mut request = [0_u8; 2048];
            let _ = stream.read(&mut request).expect("read request");
            let header = format!(
                "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            stream
                .write_all(header.as_bytes())
                .expect("response header");
            stream.write_all(&body).expect("response body");
        });
        (format!("http://{address}/releases/latest"), server)
    }

    #[test]
    fn malformed_oversized_and_rate_limited_api_responses_fail_closed() {
        for (status, body) in [
            ("403 Forbidden", b"rate limited".to_vec()),
            ("200 OK", b"not json".to_vec()),
            ("200 OK", vec![b' '; (MAX_RELEASE_RESPONSE + 1) as usize]),
        ] {
            let (endpoint, server) = serve(status, body);
            let (variant, source, _) = fixture();
            let result = resolve_endpoint(
                "owner/repo",
                &source,
                &variant,
                host(),
                NetworkOptions {
                    proxy: "",
                    allow_http: true,
                },
                &endpoint,
            );
            server.join().expect("server thread");
            assert!(result.is_err(), "status {status} must fail closed");
        }
    }
}
