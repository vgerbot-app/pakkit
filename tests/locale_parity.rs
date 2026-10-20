//! 各语言词条文件的 key 集合必须与基准 zh-CN 完全一致。

// This integration test only needs serde_yaml; the other dependencies are
// available to every test target and would each trigger a warning here.
#![allow(unused_crate_dependencies)]

use std::collections::BTreeSet;

fn flatten(value: &serde_yaml::Value, prefix: &str, out: &mut BTreeSet<String>) {
    if let serde_yaml::Value::Mapping(map) = value {
        for (k, v) in map {
            let Some(key) = k.as_str() else { continue };
            if key == "_version" {
                continue;
            }
            let path = if prefix.is_empty() {
                key.to_owned()
            } else {
                format!("{prefix}.{key}")
            };
            flatten(v, &path, out);
        }
    } else {
        out.insert(prefix.to_owned());
    }
}

fn keys(locale: &str) -> BTreeSet<String> {
    let path = format!("{}/locales/{locale}.yml", env!("CARGO_MANIFEST_DIR"));
    let text = std::fs::read_to_string(&path).expect("read locale file");
    let value: serde_yaml::Value = serde_yaml::from_str(&text).expect("parse locale yaml");
    let mut out = BTreeSet::new();
    flatten(&value, "", &mut out);
    out
}

#[test]
fn all_locales_have_identical_keys() {
    let base = keys("zh-CN");
    assert!(!base.is_empty(), "zh-CN.yml should not be empty");
    for locale in ["zh-TW", "en", "ja", "ko"] {
        let other = keys(locale);
        let missing: Vec<_> = base.difference(&other).collect();
        let extra: Vec<_> = other.difference(&base).collect();
        assert!(
            missing.is_empty() && extra.is_empty(),
            "{locale}.yml key mismatch — missing: {missing:?}, extra: {extra:?}"
        );
    }
}
