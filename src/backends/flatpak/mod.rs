//! The flatpak backend: remotes as sources, apps via `--columns=` listings.
//!
//! flatpak gettext-localizes the `Key: value` labels of `info`, so every
//! spawn pins `LC_ALL=C`; the tab-separated `--columns=` listings are
//! locale-stable already. Update enumeration is best-effort by design:
//! `remote-ls --updates` reports unreachable remotes on stderr yet can still
//! exit 0, and whatever stdout did arrive is still worth parsing.
//!
//! There is no pkexec wrapper: flatpak raises its own polkit prompt when a
//! system-wide change needs one.

pub mod parse;

use std::path::PathBuf;

use super::exec::{self, Output};
use super::types::{
    BackendError, InstallOptions, ListRow, PackageDetail, SOURCE_ALL, SearchFilters, SearchRow,
};
use super::{BackendKind, Capabilities, PackageBackend};
use crate::model::Source;

/// The remote installs fall back to when the settings name none.
const DEFAULT_REMOTE: &str = "flathub";

/// A resolved flatpak executable.
#[derive(Debug, Clone)]
pub struct Flatpak {
    exe: PathBuf,
}

impl Flatpak {
    /// Wrap an explicit executable path.
    pub fn new(exe: impl Into<PathBuf>) -> Self {
        Self { exe: exe.into() }
    }

    /// Locate `flatpak` on `PATH`.
    pub fn discover() -> Result<Self, BackendError> {
        let path = std::env::var_os("PATH").ok_or(BackendError::NotFound("flatpak"))?;
        std::env::split_paths(&path)
            .map(|dir| dir.join("flatpak"))
            .find(|candidate| candidate.is_file())
            .map(Self::new)
            .ok_or(BackendError::NotFound("flatpak"))
    }

    /// Honor an operator-supplied path, falling back to discovery.
    ///
    /// An empty configured path means "auto-detect".
    pub fn resolve(configured: &str) -> Result<Self, BackendError> {
        let trimmed = configured.trim();
        if trimmed.is_empty() {
            return Self::discover();
        }
        let path = PathBuf::from(trimmed);
        if path.is_file() {
            Ok(Self::new(path))
        } else {
            Err(BackendError::InvalidPath("flatpak", path))
        }
    }
}

/// Map the UI's "all sources" sentinel to "no remote filter".
fn normalize_source(source: Option<&str>) -> Option<&str> {
    source.filter(|s| !s.is_empty() && *s != SOURCE_ALL)
}

impl PackageBackend for Flatpak {
    fn kind(&self) -> BackendKind {
        BackendKind::Flatpak
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities {
            source_edit: false,
            export: false,
            install_location: false,
            silent: false,
            agreements: false,
            search_filters: false,
            // One `flatpak update --assumeyes` covers every ref in a single
            // transaction (and a single polkit authorization).
            batch_upgrade: true,
            ..Capabilities::default()
        }
    }

    fn run(&self, args: &[&str]) -> Result<Output, BackendError> {
        exec::run_process(&self.exe, &[("LC_ALL", "C")], args)
    }

    fn version(&self) -> Result<String, BackendError> {
        let out = self.run(&["--version"])?;
        Ok(out.stdout.trim().to_owned())
    }

    /// flatpak exit codes are small integers; print them as such.
    fn format_exit_code(&self, code: u32) -> String {
        code.to_string()
    }

    /// An empty query runs `search ""`, which lists the whole catalog into
    /// the paged search view.
    fn empty_query_searches_catalog(&self) -> bool {
        true
    }

    /// `search --columns=application,name,version,remotes,description <query>`
    ///
    /// The query is pushed even when empty (`search ""` lists the whole
    /// catalog; a missing TEXT argument exits 1 instead). There are no
    /// per-remote, filter or count flags; the source is applied client-side
    /// in [`Flatpak::search_packages`] and the rest is ignored.
    fn search_args(
        &self,
        query: &str,
        _source: Option<&str>,
        _filters: SearchFilters,
        _count: Option<u32>,
    ) -> Vec<String> {
        vec![
            "search".to_owned(),
            "--columns=application,name,version,remotes,description".to_owned(),
            query.to_owned(),
        ]
    }

    /// `list --app --columns=application,name,version,origin`
    fn list_args(&self, _source: Option<&str>) -> Vec<String> {
        vec![
            "list".to_owned(),
            "--app".to_owned(),
            "--columns=application,name,version,origin".to_owned(),
        ]
    }

    /// `remote-ls --updates --app --columns=application,name,version,origin`
    fn upgrades_args(&self) -> Vec<String> {
        vec![
            "remote-ls".to_owned(),
            "--updates".to_owned(),
            "--app".to_owned(),
            "--columns=application,name,version,origin".to_owned(),
        ]
    }

    /// `info <id>` — installed apps only; the remote fallback lives in
    /// [`Flatpak::show_package`].
    fn show_args(&self, id: &str, _source: Option<&str>) -> Vec<String> {
        vec!["info".to_owned(), id.to_owned()]
    }

    /// `install --assumeyes <remote> <id>`
    fn install_args(&self, id: &str, options: &InstallOptions) -> Vec<String> {
        let remote = normalize_source(options.source.as_deref()).unwrap_or(DEFAULT_REMOTE);
        vec![
            "install".to_owned(),
            "--assumeyes".to_owned(),
            remote.to_owned(),
            id.to_owned(),
        ]
    }

    /// `uninstall --assumeyes <id>`
    fn uninstall_args(&self, id: &str) -> Vec<String> {
        vec![
            "uninstall".to_owned(),
            "--assumeyes".to_owned(),
            id.to_owned(),
        ]
    }

    /// `update --assumeyes <id>`
    fn upgrade_args(&self, id: &str) -> Vec<String> {
        vec!["update".to_owned(), "--assumeyes".to_owned(), id.to_owned()]
    }

    /// `update --assumeyes`
    fn upgrade_all_args(&self) -> Vec<String> {
        vec!["update".to_owned(), "--assumeyes".to_owned()]
    }

    /// `remote-add --if-not-exists <name> <url>`
    fn source_add_args(&self, name: &str, url: &str) -> Vec<String> {
        vec![
            "remote-add".to_owned(),
            "--if-not-exists".to_owned(),
            name.to_owned(),
            url.to_owned(),
        ]
    }

    /// flatpak remotes have no in-place edit.
    fn source_edit_args(&self, _name: &str, _explicit: Option<bool>) -> Option<Vec<String>> {
        None
    }

    /// `remote-delete <name>`
    fn source_remove_args(&self, name: &str) -> Vec<String> {
        vec!["remote-delete".to_owned(), name.to_owned()]
    }

    /// `update --appstream [<remote>]` — refreshes the appstream metadata
    /// that `search` reads.
    fn source_update_args(&self, name: Option<&str>) -> Vec<String> {
        let mut args = vec!["update".to_owned(), "--appstream".to_owned()];
        if let Some(name) = name.filter(|n| !n.is_empty()) {
            args.push(name.to_owned());
        }
        args
    }

    fn parse_search(&self, stdout: &str) -> Vec<SearchRow> {
        parse::parse_search(stdout)
    }

    fn parse_list(&self, stdout: &str) -> Vec<ListRow> {
        parse::parse_list(stdout)
    }

    fn parse_upgrades(&self, stdout: &str) -> Vec<ListRow> {
        parse::parse_updates(stdout)
    }

    fn parse_show(&self, stdout: &str) -> PackageDetail {
        parse::parse_show(stdout)
    }

    /// A named source filters the `remotes` column client-side — it is a
    /// comma list, and the row matches when the remote is one of its entries.
    fn search_packages(
        &self,
        query: &str,
        source: Option<&str>,
        filters: SearchFilters,
        count: Option<u32>,
    ) -> Vec<SearchRow> {
        let args = self.search_args(query, source, filters, count);
        let refs: Vec<&str> = args.iter().map(String::as_str).collect();
        let rows = self
            .run(&refs)
            .map(|out| self.parse_search(&out.stdout))
            .unwrap_or_default();
        let Some(want) = normalize_source(source) else {
            return rows;
        };
        rows.into_iter()
            .filter(|row| row.source.split(',').any(|remote| remote.trim() == want))
            .collect()
    }

    /// `info` covers installed apps only; anything else falls back to
    /// `remote-info` against the named remote (flathub by default).
    fn show_package(&self, id: &str, source: Option<&str>) -> Option<PackageDetail> {
        let present =
            |detail: &PackageDetail| !detail.version.is_empty() || !detail.description.is_empty();

        let args = self.show_args(id, source);
        let refs: Vec<&str> = args.iter().map(String::as_str).collect();
        if let Ok(out) = self.run(&refs)
            && out.status == 0
        {
            let detail = self.parse_show(&out.stdout);
            if present(&detail) {
                return Some(detail);
            }
        }

        let remote = normalize_source(source).unwrap_or(DEFAULT_REMOTE);
        let args = ["remote-info", remote, id];
        self.run(&args)
            .ok()
            .filter(|out| out.status == 0)
            .map(|out| self.parse_show(&out.stdout))
            .filter(present)
    }

    /// `remotes --columns=name,url` — every remote is trusted and explicit;
    /// flatpak tracks neither a package count nor a per-remote timestamp.
    fn list_sources(&self) -> Result<Vec<Source>, BackendError> {
        let out = self.run(&["remotes", "--columns=name,url"])?;
        Ok(parse::parse_remotes(&out.stdout)
            .into_iter()
            .map(|row| Source {
                name: row.name,
                url: row.argument,
                ty: "flatpak".to_owned(),
                trusted: true,
                explicit: true,
                ..Source::default()
            })
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backends::types::InstallOptions;

    fn flatpak() -> Flatpak {
        Flatpak::new("flatpak")
    }

    #[test]
    fn install_defaults_to_flathub() {
        let args = flatpak().install_args("org.mozilla.firefox", &InstallOptions::default());
        assert_eq!(
            args,
            vec!["install", "--assumeyes", "flathub", "org.mozilla.firefox"]
        );
    }

    #[test]
    fn install_uses_the_named_remote_and_skips_all() {
        let options = InstallOptions {
            source: Some("gnome-nightly".into()),
            ..InstallOptions::default()
        };
        let args = flatpak().install_args("org.gnome.Epiphany.Devel", &options);
        assert_eq!(
            args,
            vec![
                "install",
                "--assumeyes",
                "gnome-nightly",
                "org.gnome.Epiphany.Devel"
            ]
        );

        let options = InstallOptions {
            source: Some(SOURCE_ALL.into()),
            ..InstallOptions::default()
        };
        let args = flatpak().install_args("org.mozilla.firefox", &options);
        assert_eq!(args[2], "flathub");
    }

    #[test]
    fn upgrade_all_updates_everything() {
        assert_eq!(flatpak().upgrade_all_args(), vec!["update", "--assumeyes"]);
        assert_eq!(
            flatpak().upgrade_args("com.dbxio.dbx"),
            vec!["update", "--assumeyes", "com.dbxio.dbx"]
        );
        assert_eq!(
            flatpak().uninstall_args("com.dbxio.dbx"),
            vec!["uninstall", "--assumeyes", "com.dbxio.dbx"]
        );
    }

    #[test]
    fn remotes_have_no_edit_but_add_remove_sync() {
        let b = flatpak();
        assert_eq!(b.source_edit_args("flathub", Some(true)), None);
        assert_eq!(
            b.source_add_args("kdeapps", "https://distribute.kde.org/kdeapps.flatpakrepo"),
            vec![
                "remote-add",
                "--if-not-exists",
                "kdeapps",
                "https://distribute.kde.org/kdeapps.flatpakrepo"
            ]
        );
        assert_eq!(
            b.source_remove_args("kdeapps"),
            vec!["remote-delete", "kdeapps"]
        );
        assert_eq!(
            b.source_update_args(Some("flathub")),
            vec!["update", "--appstream", "flathub"]
        );
        assert_eq!(b.source_update_args(None), vec!["update", "--appstream"]);
    }

    #[test]
    fn an_empty_query_searches_the_whole_catalog() {
        let b = flatpak();
        assert!(b.empty_query_searches_catalog());
        let args = b.search_args("", None, SearchFilters::default(), None);
        assert_eq!(
            args,
            vec![
                "search",
                "--columns=application,name,version,remotes,description",
                ""
            ]
        );
    }

    #[test]
    fn search_carries_no_source_flag() {
        let args = flatpak().search_args(
            "firefox",
            Some("flathub"),
            SearchFilters::default(),
            Some(10),
        );
        assert_eq!(
            args,
            vec![
                "search",
                "--columns=application,name,version,remotes,description",
                "firefox"
            ]
        );
    }

    #[test]
    fn exit_codes_format_as_plain_integers() {
        assert_eq!(flatpak().format_exit_code(1), "1");
        assert_eq!(flatpak().format_exit_code(255), "255");
    }

    #[test]
    fn capabilities_hide_the_winget_only_surface() {
        let caps = flatpak().capabilities();
        assert!(caps.sources_view);
        assert!(caps.source_add);
        assert!(!caps.source_edit);
        assert!(!caps.export);
        assert!(!caps.install_location);
        assert!(!caps.silent);
        assert!(!caps.agreements);
        assert!(!caps.search_filters);
        assert!(caps.batch_upgrade);
    }

    #[test]
    fn an_invalid_configured_path_is_rejected() {
        let result = Flatpak::resolve("/definitely/not/a/flatpak");
        assert!(matches!(
            result,
            Err(BackendError::InvalidPath("flatpak", _))
        ));
    }
}
