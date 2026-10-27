//! Pure parsers for apk's human-readable output.

use crate::backends::types::{ListRow, PackageDetail, SearchRow};

pub fn parse_search(_stdout: &str) -> Vec<SearchRow> {
    Vec::new()
}

pub fn parse_list(_stdout: &str) -> Vec<ListRow> {
    Vec::new()
}

pub fn parse_upgrades(_stdout: &str) -> Vec<ListRow> {
    Vec::new()
}

pub fn parse_show(_stdout: &str) -> PackageDetail {
    PackageDetail::default()
}
