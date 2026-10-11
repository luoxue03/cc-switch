//! Device-local, live-value protection for Codex TOML. No protected values are stored.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use toml_edit::{DocumentMut, Item, Table, TableLike};

use crate::error::AppError;
use crate::live::engine::{self, DeviceStore, LiveFile};
use crate::live::patch::{Guarded, LivePatch, LiveWriteError, WholeFile};

const RULES_FILE: &str = "codex-config-protection.json";
pub(crate) const MANUAL_EDIT: &str = "codex-manual-config-edit";
type Key = Vec<String>;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Rules {
    pub paths: Vec<Key>,
}

impl Default for Rules {
    fn default() -> Self {
        Self {
            paths: vec![
                vec!["model_provider".into()],
                vec!["openai_base_url".into()],
            ],
        }
    }
}

fn rules_path() -> PathBuf {
    DeviceStore::for_device().file(RULES_FILE)
}

/// Called before startup recovery. Old automation never runs before defaults exist.
pub(crate) fn initialize() -> Result<(), AppError> {
    if !rules_path().exists() {
        crate::config::write_json_file(&rules_path(), &Rules::default())?;
    }
    load_rules()?;
    Ok(())
}

pub fn load_rules() -> Result<Rules, AppError> {
    match engine::read_current(&rules_path())? {
        None => Ok(Rules { paths: vec![] }),
        Some(bytes) => {
            let rules: Rules = serde_json::from_slice(&bytes).map_err(|_| {
                AppError::Config(
                    "Codex protection settings are invalid; no configuration was changed".into(),
                )
            })?;
            validate_rules(rules)
        }
    }
}

fn validate_rules(mut rules: Rules) -> Result<Rules, AppError> {
    if rules.paths.len() > 4096
        || rules
            .paths
            .iter()
            .any(|key| key.is_empty() || key.len() > 64 || key.iter().any(|part| part.len() > 4096))
    {
        return Err(AppError::Config(
            "Invalid Codex protection field paths".into(),
        ));
    }
    rules.paths.sort();
    rules.paths.dedup();
    Ok(rules)
}

pub fn save_rules(paths: Vec<Key>, expected_revision: &str) -> Result<String, AppError> {
    let _guard = engine::lock_app("codex");
    let existing = load_rules()?;
    if revision(&existing) != expected_revision {
        return Err(AppError::Conflict(
            "Codex protection options changed; refresh before saving".into(),
        ));
    }
    let next = validate_rules(Rules { paths })?;
    crate::config::write_json_file(&rules_path(), &next)?;
    Ok(revision(&next))
}

fn revision(rules: &Rules) -> String {
    engine::sha256_hex(&serde_json::to_vec(rules).expect("serialize protection paths"))
}

pub(crate) fn is_config(path: &Path) -> bool {
    if path.file_name().and_then(|name| name.to_str()) != Some("config.toml") {
        return false;
    }
    let config = crate::codex_config::get_codex_config_path();
    path == config
        || matches!(
            (std::fs::canonicalize(path), std::fs::canonicalize(config)),
            (Ok(path), Ok(config)) if path == config
        )
}

pub(crate) fn field_label(key: &[String]) -> String {
    key.iter()
        .map(|s| {
            if !s.is_empty()
                && s.bytes()
                    .all(|c| c.is_ascii_alphanumeric() || c == b'_' || c == b'-')
            {
                s.clone()
            } else {
                serde_json::to_string(s).expect("key string")
            }
        })
        .collect::<Vec<_>>()
        .join(".")
}

fn item_at<'a>(doc: &'a DocumentMut, key: &[String]) -> Option<&'a Item> {
    let (last, parents) = key.split_last()?;
    let mut table: &dyn TableLike = doc.as_table();
    for parent in parents {
        table = table.get(parent)?.as_table_like()?;
    }
    table.get(last)
}

fn restore(doc: &mut DocumentMut, key: &[String], value: Option<&Item>) -> Result<(), String> {
    let (last, parents) = key.split_last().ok_or("empty path")?;
    let mut table: &mut dyn TableLike = doc.as_table_mut();
    for parent in parents {
        if !table.contains_key(parent) {
            if value.is_none() {
                return Ok(());
            }
            let mut added = Table::new();
            added.set_implicit(true);
            table.insert(parent, Item::Table(added));
        }
        let item = table.get_mut(parent).ok_or("missing parent")?;
        if item.as_table_like().is_none() && value.is_none() {
            return Ok(());
        }
        table = item.as_table_like_mut().ok_or_else(|| field_label(key))?;
    }
    if let Some(value) = value {
        table.insert(last, value.clone());
    } else {
        table.remove(last);
    }
    Ok(())
}

fn semantic_item(item: Option<&Item>) -> Option<toml::Value> {
    item.and_then(|item| {
        let mut doc = DocumentMut::new();
        doc.insert("value", item.clone());
        doc.to_string()
            .parse::<toml::Value>()
            .ok()?
            .get("value")
            .cloned()
    })
}

#[derive(Debug, PartialEq)]
struct Connection {
    selector: String,
    url: String,
    wire: String,
    auth: Vec<Option<toml::Value>>,
}

fn connection(doc: &DocumentMut) -> Connection {
    let profile = doc.get("profile").and_then(Item::as_str).and_then(|name| {
        doc.get("profiles")?
            .as_table_like()?
            .get(name)?
            .as_table_like()
    });
    let top = |key: &str| profile.and_then(|p| p.get(key)).or_else(|| doc.get(key));
    let selector = top("model_provider")
        .and_then(Item::as_str)
        .unwrap_or("openai")
        .to_string();
    let table = doc
        .get("model_providers")
        .and_then(Item::as_table_like)
        .and_then(|p| p.get(&selector))
        .and_then(Item::as_table_like);
    let url = if selector == "openai" {
        top("openai_base_url")
            .and_then(Item::as_str)
            .unwrap_or("https://api.openai.com/v1")
    } else {
        table
            .and_then(|t| t.get("base_url"))
            .and_then(Item::as_str)
            .unwrap_or("")
    };
    Connection {
        selector,
        url: url.trim_end_matches('/').to_string(),
        wire: table
            .and_then(|t| t.get("wire_api"))
            .and_then(Item::as_str)
            .unwrap_or("responses")
            .to_string(),
        auth: [
            "experimental_bearer_token",
            "env_key",
            "http_headers",
            "env_http_headers",
            "query_params",
            "auth",
            "requires_openai_auth",
        ]
        .iter()
        .map(|key| semantic_item(table.and_then(|t| t.get(key)).or_else(|| top(key))))
        .collect(),
    }
}

fn loopback_url(url: &str) -> bool {
    url::Url::parse(url)
        .ok()
        .is_some_and(|u| matches!(u.host_str(), Some("127.0.0.1" | "localhost" | "[::1]")))
}

fn same_connection(expected: &Connection, actual: &Connection) -> bool {
    expected == actual
        || (loopback_url(&expected.url)
            && expected.url == actual.url
            && expected.wire == actual.wire)
}

/// Preserve current leaves (including absence), then check the resulting connection.
pub(crate) fn protect_with_rules(
    path: &Path,
    pre: Option<&[u8]>,
    proposed: Option<Vec<u8>>,
    rules: &Rules,
) -> Result<Option<Vec<u8>>, LiveWriteError> {
    if rules.paths.is_empty() || pre == proposed.as_deref() {
        return Ok(proposed);
    }
    let before = crate::live::patch::toml::parse(path, pre)?;
    let mut after = crate::live::patch::toml::parse(path, proposed.as_deref())?;
    let expected = connection(&after);
    let mut changed = Vec::new();
    for key in &rules.paths {
        // A protected ancestor owns its entire value; restoring descendants is redundant.
        if rules
            .paths
            .iter()
            .any(|parent| parent.len() < key.len() && key.starts_with(parent))
        {
            continue;
        }
        let value = item_at(&before, key);
        if semantic_item(value) != semantic_item(item_at(&after, key)) {
            changed.push(field_label(key));
        }
        restore(&mut after, key, value).map_err(|_| LiveWriteError::Protected {
            path: path.to_path_buf(),
            keys: vec![field_label(key)],
            reason: "A parent table would be replaced; adjust it manually or unprotect the field"
                .into(),
        })?;
    }
    if !changed.is_empty() && !same_connection(&expected, &connection(&after)) {
        return Err(LiveWriteError::Protected { path: path.to_path_buf(), keys: changed,
            reason: "Protected connection fields do not reach the requested provider/proxy. Edit Codex TOML manually or unprotect these fields before retrying".into() });
    }
    let text = after.to_string();
    if text == before.to_string() {
        return Ok(pre.map(<[u8]>::to_vec));
    }
    Ok(Some(text.into_bytes()))
}

pub(crate) fn protect_auto(
    path: &Path,
    pre: Option<&[u8]>,
    proposed: Option<Vec<u8>>,
) -> Result<Option<Vec<u8>>, LiveWriteError> {
    if !is_config(path) {
        return Ok(proposed);
    }
    let rules = load_rules().map_err(|err| LiveWriteError::Protected {
        path: path.to_path_buf(),
        keys: vec![],
        reason: err.to_string(),
    })?;
    protect_with_rules(path, pre, proposed, &rules)
}

/// Recovery must not commit a provider switch against an externally changed route.
pub(crate) fn validate_recovery_connection(
    path: &Path,
    current: Option<&[u8]>,
    planned: Option<&[u8]>,
) -> Result<(), LiveWriteError> {
    let rules = load_rules().map_err(|err| LiveWriteError::Protected {
        path: path.to_path_buf(),
        keys: vec![],
        reason: err.to_string(),
    })?;
    if rules.paths.is_empty() {
        return Ok(());
    }
    let current = crate::live::patch::toml::parse(path, current)?;
    let planned = crate::live::patch::toml::parse(path, planned)?;
    if !same_connection(&connection(&planned), &connection(&current)) {
        return Err(LiveWriteError::Protected {
            path: path.to_path_buf(),
            keys: rules.paths.iter().map(|key| field_label(key)).collect(),
            reason: "The connection changed outside CC Switch during an interrupted operation; the provider switch has not been committed. Resolve the connection or protection options before retrying".into(),
        });
    }
    Ok(())
}

/// Legacy writers also pass through protection. Refuse stale writes rather than
/// recomputing a full-file overwrite against data produced by an external editor.
pub(crate) fn legacy_write(path: &Path, data: &[u8], mode: Option<u32>) -> Result<(), AppError> {
    let before = engine::read_current(path)?;
    let after = protect_auto(path, before.as_deref(), Some(data.to_vec()))?;
    if before == after {
        return Ok(());
    }
    let bytes = after.unwrap_or_default();
    let staged = crate::config::stage_write(path, &bytes, mode, true)?;
    if engine::read_current(path)? != before {
        let _ = std::fs::remove_file(staged.tmp_path());
        return Err(AppError::Conflict(
            "Codex TOML changed during the write; retry from the latest contents".into(),
        ));
    }
    staged.commit()
}

/// Used by automatic write projections before route validation; editor previews
/// render independently and never restore protected values into provider drafts.
pub(crate) fn protect_projection(
    path: &Path,
    before: &DocumentMut,
    after: &mut DocumentMut,
) -> Result<bool, LiveWriteError> {
    if !is_config(path) {
        return Ok(false);
    }
    let rules = load_rules().map_err(|err| LiveWriteError::Protected {
        path: path.to_path_buf(),
        keys: vec![],
        reason: err.to_string(),
    })?;
    if rules.paths.is_empty() {
        return Ok(false);
    }
    let mut intended = after.clone();
    intended.remove("profile");
    let intended_connection = connection(&intended);
    let bytes = protect_with_rules(
        path,
        Some(before.to_string().as_bytes()),
        Some(after.to_string().into_bytes()),
        &rules,
    )?;
    *after = crate::live::patch::toml::parse(path, bytes.as_deref())?;
    if !same_connection(&intended_connection, &connection(after)) {
        return Err(LiveWriteError::Protected {
            path: path.to_path_buf(), keys: rules.paths.iter().map(|p| field_label(p)).collect(),
            reason: "An active profile overrides the requested connection; adjust the profile manually before retrying".into(),
        });
    }
    Ok(true)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Field {
    pub path: Key,
    pub name: String,
    pub value: String,
    pub present: bool,
    pub sensitive: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub config_path: String,
    pub config_revision: Option<String>,
    pub rules_revision: String,
    pub protected_paths: Vec<Key>,
    pub fields: Vec<Field>,
    pub warning: Option<String>,
}

fn sensitive_key(key: &[String]) -> bool {
    key.iter().any(|part| {
        let part = part.to_ascii_lowercase();
        [
            "token",
            "secret",
            "password",
            "authorization",
            "api_key",
            "apikey",
            "credential",
            "headers",
            "env",
            "query_params",
        ]
        .iter()
        .any(|needle| part.contains(needle))
    })
}

fn collect_paths(table: &dyn TableLike, prefix: &[String], paths: &mut BTreeSet<Key>) {
    for (key, value) in table.iter() {
        let mut path = prefix.to_vec();
        path.push(key.to_string());
        if let Some(child) = value.as_table_like().filter(|t| !t.is_empty()) {
            collect_paths(child, &path, paths);
        } else {
            paths.insert(path);
        }
    }
}

pub fn snapshot() -> Result<Snapshot, AppError> {
    let _guard = engine::lock_app("codex");
    initialize()?;
    let rules = load_rules()?;
    let path = crate::codex_config::get_codex_config_path();
    let bytes = engine::read_current(&path)?;
    let parsed = crate::live::patch::toml::parse(&path, bytes.as_deref());
    let (doc, warning) = match parsed {
        Ok(doc) => (doc, None),
        Err(_) => (DocumentMut::new(), Some("Codex TOML 无法解析，自动写入已阻止；请在下方编辑器中修复 / Invalid TOML; automatic writes are blocked".into())),
    };
    let mut paths = BTreeSet::new();
    collect_paths(doc.as_table(), &[], &mut paths);
    paths.extend(rules.paths.iter().cloned());
    paths.extend(Rules::default().paths);
    let fields = paths
        .into_iter()
        .map(|key| {
            let value = item_at(&doc, &key);
            let sensitive = sensitive_key(&key)
                || value.is_some_and(|v| {
                    let text = v.to_string().to_ascii_lowercase();
                    text.contains("secret")
                        || text.contains("token")
                        || text.contains("password")
                        || text.contains("sk-")
                });
            Field {
                name: field_label(&key),
                path: key,
                present: value.is_some(),
                sensitive,
                value: if sensitive {
                    "[hidden]".into()
                } else {
                    value
                        .map(|v| v.to_string().chars().take(512).collect())
                        .unwrap_or_default()
                },
            }
        })
        .collect();
    Ok(Snapshot {
        config_path: path.display().to_string(),
        config_revision: engine::digest(bytes.as_deref()),
        rules_revision: revision(&rules),
        protected_paths: rules.paths,
        fields,
        warning,
    })
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Editor {
    pub config_path: String,
    pub text: String,
    pub revision: Option<String>,
}

pub fn read_editor() -> Result<Editor, AppError> {
    let path = crate::codex_config::get_codex_config_path();
    let bytes = engine::read_current(&path)?;
    let text = String::from_utf8(bytes.clone().unwrap_or_default())
        .map_err(|_| AppError::Config("Codex TOML is not UTF-8".into()))?;
    Ok(Editor {
        config_path: path.display().to_string(),
        text,
        revision: engine::digest(bytes.as_deref()),
    })
}

/// Only this explicit editor command can bypass automatic protection.
struct ManualPatch(Guarded);
impl LivePatch for ManualPatch {
    fn apply(&self, path: &Path, pre: Option<&[u8]>) -> Result<Vec<u8>, LiveWriteError> {
        self.0.apply(path, pre)
    }
    fn is_explicit_codex_edit(&self) -> bool {
        true
    }
}

pub fn save_editor(
    db: &crate::database::Database,
    path: &str,
    text: String,
    expected: Option<String>,
) -> Result<(), AppError> {
    let write = crate::mode::operation::AppWrite::begin(db, "codex")?;
    let current_path = crate::codex_config::get_codex_config_path();
    if current_path != Path::new(path) {
        return Err(AppError::Conflict(
            "Codex configuration directory changed; reload the editor".into(),
        ));
    }
    crate::live::patch::toml::parse(&current_path, Some(text.as_bytes()))?;
    let patch = ManualPatch(Guarded {
        expected_pre: expected,
        then: WholeFile::Write(text.into_bytes()),
    });
    write.run(
        MANUAL_EDIT,
        &[crate::mode::operation::FileChange {
            file: LiveFile::private(current_path),
            patch: &patch,
        }],
        Default::default(),
    )?;
    Ok(())
}

pub(crate) fn check_proxy_address_change(
    old: &str,
    old_port: u16,
    address: &str,
    port: u16,
) -> Result<(), String> {
    if old == address && old_port == port {
        return Ok(());
    }
    let rules = load_rules().map_err(|e| e.to_string())?;
    if rules.paths.is_empty() {
        return Ok(());
    }
    let path = crate::codex_config::get_codex_config_path();
    let bytes = engine::read_current(&path).map_err(|e| e.to_string())?;
    let doc =
        crate::live::patch::toml::parse(&path, bytes.as_deref()).map_err(|e| e.to_string())?;
    let connection = connection(&doc);
    if !loopback_url(&connection.url) {
        return Ok(());
    }
    let Some(url) = url::Url::parse(&connection.url).ok() else {
        return Ok(());
    };
    if url.port_or_known_default() != Some(old_port) {
        return Ok(());
    }
    let key = if connection.selector == "openai" {
        doc.get("profile")
            .and_then(Item::as_str)
            .and_then(|name| {
                item_at(
                    &doc,
                    &["profiles".into(), name.into(), "openai_base_url".into()],
                )?;
                Some(vec![
                    "profiles".into(),
                    name.into(),
                    "openai_base_url".into(),
                ])
            })
            .unwrap_or_else(|| vec!["openai_base_url".into()])
    } else {
        vec![
            "model_providers".into(),
            connection.selector.clone(),
            "base_url".into(),
        ]
    };
    let mut proposed = doc.clone();
    let new_url = format!(
        "{}{}",
        crate::services::proxy::proxy_origin(address, port),
        url.path()
    );
    restore(&mut proposed, &key, Some(&toml_edit::value(new_url)))?;
    protect_with_rules(&path, bytes.as_deref(), Some(proposed.to_string().into_bytes()), &rules)
        .map_err(|_| format!("Codex 配置项保护：受保护的连接地址仍依赖本地代理端口 {old_port}。请先手动调整地址或取消保护，再修改监听地址/端口 / Protected Codex URL still uses port {old_port}; no proxy settings were changed"))?;
    Ok(())
}

pub(crate) fn proxy_dependency_warning() -> Option<String> {
    let rules = load_rules().ok()?;
    if rules.paths.is_empty() {
        return None;
    }
    let path = crate::codex_config::get_codex_config_path();
    let bytes = engine::read_current(&path).ok()?;
    let doc = crate::live::patch::toml::parse(&path, bytes.as_deref()).ok()?;
    loopback_url(&connection(&doc).url).then(|| "Codex 的配置仍连接本地代理。停止服务后 Codex 将无法请求；受保护配置不会自动改成直连 / Codex still depends on the local proxy".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::live::patch::toml::TomlPatch;
    use crate::live::patch::KeyPath;
    use crate::mode::operation::{self, failpoint, FileChange};
    use serial_test::serial;

    fn rules(keys: &[&[&str]]) -> Rules {
        Rules {
            paths: keys
                .iter()
                .map(|key| key.iter().map(|s| s.to_string()).collect())
                .collect(),
        }
    }
    fn protected(before: &str, after: &str, rules: &Rules) -> Result<String, LiveWriteError> {
        protect_with_rules(
            Path::new("config.toml"),
            Some(before.as_bytes()),
            Some(after.as_bytes().to_vec()),
            rules,
        )
        .map(|b| String::from_utf8(b.unwrap_or_default()).unwrap())
    }
    #[test]
    fn preserves_latest_values_absence_nested_keys_comments_and_unprotected_edits() {
        let before = "model = 'old' # keep this\n[features]\nflag = true\n[my.\"literal.key\"]\nvalue = [1, 2]\n";
        let after = "model = 'new'\nreview_model = 'inserted'\n[features]\nflag = false\nextra = 42\n[my.\"literal.key\"]\nvalue = [3]\n";
        let rules = rules(&[
            &["model"],
            &["review_model"],
            &["my", "literal.key", "value"],
        ]);
        let out = protected(before, after, &rules).unwrap();
        assert!(out.contains("'old' # keep this"));
        assert!(!out.contains("review_model"));
        assert!(out.contains("flag = false"));
        assert!(out.contains("[1, 2]"));
        let external = before.replace("'old'", "'external'");
        assert!(protected(&external, after, &rules)
            .unwrap()
            .contains("'external'"));
    }
    #[test]
    fn whole_tables_and_arrays_are_protected_and_parent_shape_conflicts_are_explicit() {
        let before = "[custom]\nchild=1\n[[entries]]\nx=2\n";
        let out = protected(
            before,
            "model='new'\n",
            &rules(&[&["custom"], &["entries"]]),
        )
        .unwrap();
        let doc: toml::Value = out.parse().unwrap();
        assert_eq!(doc["custom"]["child"].as_integer(), Some(1));
        assert_eq!(doc["entries"][0]["x"].as_integer(), Some(2));
        assert!(protected(before, "custom=5\n", &rules(&[&["custom", "child"]])).is_err());
    }
    #[test]
    fn connection_protection_allows_same_proxy_but_rejects_direct_or_port_change() {
        let before =
            "model_provider='openai'\nopenai_base_url='http://127.0.0.1:15721/v1'\nmodel='old'\n";
        let proposal = "model_provider='custom'\nmodel='new'\n[model_providers.custom]\nbase_url='http://127.0.0.1:15721/v1'\nwire_api='responses'\nexperimental_bearer_token='placeholder'\n";
        let out = protected(before, proposal, &Rules::default()).unwrap();
        let doc: toml::Value = out.parse().unwrap();
        assert_eq!(doc["model_provider"].as_str(), Some("openai"));
        assert!(out.contains("model='new'"));
        assert!(protected(
            before,
            &proposal.replace("15721", "15722"),
            &Rules::default()
        )
        .is_err());
        assert!(protected(before, "model='direct'", &Rules::default()).is_err());
        assert!(protected("model_provider='openai'\n", proposal, &Rules::default()).is_err());
    }
    #[test]
    fn protected_credentials_cannot_silently_keep_the_wrong_direct_provider() {
        let before = "model_provider='custom'\n[model_providers.custom]\nbase_url='https://api.example/v1'\nexperimental_bearer_token='private-a'\n";
        let err = protected(
            before,
            &before.replace("private-a", "private-b"),
            &rules(&[&["model_providers", "custom", "experimental_bearer_token"]]),
        )
        .unwrap_err();
        assert!(!err.to_string().contains("private-a"));
        assert!(!err.to_string().contains("private-b"));
    }
    #[test]
    fn empty_selection_restores_old_behavior_and_broken_toml_fails_closed() {
        assert_eq!(
            protected("model='old'", "model='new'", &Rules { paths: vec![] }).unwrap(),
            "model='new'"
        );
        assert!(protected("model=[", "model='new'", &Rules::default()).is_err());
        let out = protected(
            "model_catalog_json='mine.json'",
            "model_catalog_json='generated.json'",
            &rules(&[&["model_catalog_json"]]),
        )
        .unwrap();
        assert!(out.contains("mine.json"));
    }

    struct Home {
        _dir: tempfile::TempDir,
        previous: Option<std::ffi::OsString>,
    }
    impl Home {
        fn new() -> Self {
            let dir = tempfile::tempdir().unwrap();
            let previous = std::env::var_os("CC_SWITCH_TEST_HOME");
            std::env::set_var("CC_SWITCH_TEST_HOME", dir.path());
            crate::settings::reload_settings().unwrap();
            Self {
                _dir: dir,
                previous,
            }
        }
        fn seed(&self, text: &str) -> PathBuf {
            let path = crate::codex_config::get_codex_config_path();
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, text).unwrap();
            path
        }
    }
    impl Drop for Home {
        fn drop(&mut self) {
            failpoint::crash_at(None);
            failpoint::on_before_publish(None);
            match &self.previous {
                Some(home) => std::env::set_var("CC_SWITCH_TEST_HOME", home),
                None => std::env::remove_var("CC_SWITCH_TEST_HOME"),
            }
            crate::settings::reload_settings().unwrap();
        }
    }
    #[test]
    #[serial]
    fn initializing_and_saving_rules_never_rewrites_config_and_clear_persists() {
        let home = Home::new();
        let path = home.seed("model_provider='openai' # personal\n");
        let before = std::fs::read(&path).unwrap();
        let stamp = std::fs::metadata(&path).unwrap().modified().unwrap();
        initialize().unwrap();
        assert_eq!(load_rules().unwrap().paths.len(), 2);
        let snapshot = snapshot().unwrap();
        save_rules(vec![], &snapshot.rules_revision).unwrap();
        initialize().unwrap();
        assert!(load_rules().unwrap().paths.is_empty());
        assert_eq!(std::fs::read(&path).unwrap(), before);
        assert_eq!(std::fs::metadata(&path).unwrap().modified().unwrap(), stamp);
        assert!(save_rules(vec![vec!["model".into()]], &snapshot.rules_revision).is_err());
    }
    #[test]
    #[serial]
    fn snapshot_includes_quoted_custom_paths_arrays_missing_and_masks_credentials() {
        let home = Home::new();
        home.seed(
            "model='one'\n[custom.\"a.b\"]\nvalue=2\napi_key='secret-value'\n[[entries]]\nx=1\n",
        );
        let snapshot = snapshot().unwrap();
        assert!(snapshot
            .fields
            .iter()
            .any(|f| f.path == vec!["custom", "a.b", "value"]));
        assert!(snapshot.fields.iter().any(|f| f.path == vec!["entries"]));
        assert!(snapshot
            .fields
            .iter()
            .any(|f| f.name == "openai_base_url" && !f.present));
        assert!(!serde_json::to_string(&snapshot)
            .unwrap()
            .contains("secret-value"));
    }
    #[test]
    #[serial]
    fn explicit_editor_edits_protected_fields_and_rejects_stale_or_wrong_directory_save() {
        let home = Home::new();
        let path = home.seed("model_provider='openai'\n");
        initialize().unwrap();
        let db = crate::database::Database::memory().unwrap();
        let editor = read_editor().unwrap();
        save_editor(
            &db,
            &editor.config_path,
            "model_provider='manual'\n".into(),
            editor.revision.clone(),
        )
        .unwrap();
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "model_provider='manual'\n"
        );
        assert!(save_editor(
            &db,
            &editor.config_path,
            "model_provider='stale'".into(),
            editor.revision
        )
        .is_err());
        let current = read_editor().unwrap();
        assert!(save_editor(
            &db,
            "/different/config.toml",
            "model='wrong'".into(),
            current.revision
        )
        .is_err());
        let current = read_editor().unwrap();
        assert!(save_editor(
            &db,
            &current.config_path,
            "model=[".into(),
            current.revision
        )
        .is_err());
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "model_provider='manual'\n"
        );
    }
    #[test]
    #[serial]
    fn engine_conflict_does_not_publish_auth_catalog_or_commit_switch() {
        let home = Home::new();
        let config = home.seed("model_provider='openai'\n");
        initialize().unwrap();
        let auth = config.with_file_name("auth.json");
        std::fs::write(&auth, "old-auth").unwrap();
        let guard = engine::lock_app("codex");
        let store = DeviceStore::for_device();
        let auth_patch = WholeFile::Write(b"new-auth".to_vec());
        let patch = WholeFile::Write(b"model_provider='custom'\n[model_providers.custom]\nbase_url='https://relay.example/v1'".to_vec());
        let changed = std::cell::Cell::new(false);
        let result = operation::run(
            &store,
            &guard,
            "switch",
            &[
                FileChange {
                    file: LiveFile::private(&auth),
                    patch: &auth_patch,
                },
                FileChange {
                    file: LiveFile::private(&config),
                    patch: &patch,
                },
            ],
            Default::default(),
            &|_| {
                changed.set(true);
                Ok(())
            },
        );
        assert!(result.is_err());
        assert!(!changed.get());
        assert_eq!(std::fs::read_to_string(&auth).unwrap(), "old-auth");
        assert_eq!(
            std::fs::read_to_string(&config).unwrap(),
            "model_provider='openai'\n"
        );
    }
    #[test]
    #[serial]
    fn engine_replan_keeps_external_latest_protected_value_and_updates_other_fields() {
        let home = Home::new();
        let path = home.seed("model='first'\nother=1\n");
        crate::config::write_json_file(&rules_path(), &rules(&[&["model"]])).unwrap();
        let guard = engine::lock_app("codex");
        let store = DeviceStore::for_device();
        let patch = TomlPatch {
            set: vec![
                (KeyPath::new(&["model"]), toml_edit::value("auto")),
                (KeyPath::new(&["other"]), toml_edit::value(2)),
            ],
            ..Default::default()
        };
        let mut called = false;
        failpoint::on_before_publish(Some(Box::new(move |_, path| {
            if !called {
                std::fs::write(path, "model='external'\nother=1\n").unwrap();
                called = true;
            }
        })));
        operation::run(
            &store,
            &guard,
            "apply",
            &[FileChange {
                file: LiveFile::private(&path),
                patch: &patch,
            }],
            Default::default(),
            &|_| Ok(()),
        )
        .unwrap();
        failpoint::on_before_publish(None);
        let out = std::fs::read_to_string(path).unwrap();
        assert!(out.contains("'external'"));
        assert!(out.contains("other=2"));
    }
    #[test]
    #[serial]
    fn legacy_writes_and_deletion_preserve_protected_values() {
        let home = Home::new();
        let path = home.seed("model='manual'\nother=1\n");
        crate::config::write_json_file(&rules_path(), &rules(&[&["model"]])).unwrap();
        crate::config::write_text_file(&path, "model='auto'\nother=2\n").unwrap();
        assert!(std::fs::read_to_string(&path).unwrap().contains("'manual'"));
        crate::config::delete_file(&path).unwrap();
        assert!(std::fs::read_to_string(&path).unwrap().contains("'manual'"));
    }
    #[test]
    #[serial]
    fn port_changes_are_blocked_before_service_config_mutation() {
        let home = Home::new();
        home.seed("model_provider='openai'\nopenai_base_url='http://127.0.0.1:15721/v1'\n");
        initialize().unwrap();
        assert!(check_proxy_address_change("127.0.0.1", 15721, "127.0.0.1", 15722).is_err());
        assert!(check_proxy_address_change("127.0.0.1", 15721, "127.0.0.1", 15721).is_ok());
        assert!(proxy_dependency_warning().is_some());
        home.seed("model_provider='custom'\n[model_providers.custom]\nbase_url='http://127.0.0.1:15721/v1'\n");
        // Protecting the selector alone must not freeze an unprotected table URL.
        assert!(check_proxy_address_change("127.0.0.1", 15721, "127.0.0.1", 15722).is_ok());
    }
    #[test]
    #[serial]
    fn corrupt_protection_rules_fail_closed() {
        let home = Home::new();
        let path = home.seed("model='manual'");
        initialize().unwrap();
        std::fs::write(rules_path(), "broken").unwrap();
        assert!(crate::config::write_text_file(&path, "model='auto'").is_err());
        assert_eq!(std::fs::read_to_string(path).unwrap(), "model='manual'");
    }
    #[test]
    #[serial]
    fn recovering_pending_write_respects_new_protection_without_reverting_current_content() {
        let home = Home::new();
        let path = home.seed("model='manual'\nother=1\n");
        let store = DeviceStore::for_device();
        let guard = engine::lock_app("codex");
        let patch = WholeFile::Write(b"model='auto'\nother=2\n".to_vec());
        failpoint::crash_at(Some("marked"));
        assert!(operation::run(
            &store,
            &guard,
            "apply",
            &[FileChange {
                file: LiveFile::private(&path),
                patch: &patch
            }],
            Default::default(),
            &|_| Ok(())
        )
        .is_err());
        failpoint::crash_at(None);
        crate::config::write_json_file(&rules_path(), &rules(&[&["model"]])).unwrap();
        operation::recover(&store, &guard, &|_| Ok(())).unwrap();
        let out = std::fs::read_to_string(path).unwrap();
        assert!(out.contains("'manual'"));
        assert!(out.contains("other=2"));
    }

    #[test]
    #[serial]
    fn projection_keeps_same_proxy_and_catalog_updates_but_rejects_active_profile_conflicts() {
        use crate::live::project::codex::{proxy_route_table, CodexConfigPatch, RouteWrite};
        let home = Home::new();
        let text = "model_provider='openai'\nopenai_base_url='http://127.0.0.1:15721/v1'\nmodel='old'\nmodel_catalog_json='personal.json'\n";
        let path = home.seed(text);
        let rules = rules(&[
            &["model_provider"],
            &["openai_base_url"],
            &["model_catalog_json"],
        ]);
        crate::config::write_json_file(&rules_path(), &rules).unwrap();
        let patch = CodexConfigPatch {
            top: vec![("model".into(), "new".into())],
            nested: vec![],
            exclusive: vec![],
            outgoing: vec![],
            route: RouteWrite::Custom(proxy_route_table(
                "CC Switch",
                "http://127.0.0.1:15721/v1",
                false,
            )),
            catalog: true,
            retired: vec![],
        };
        let mut doc = text.parse::<DocumentMut>().unwrap();
        patch.apply_to(&path, &mut doc).unwrap();
        assert_eq!(doc["model_provider"].as_str(), Some("openai"));
        assert_eq!(doc["model"].as_str(), Some("new"));
        assert_eq!(doc["model_catalog_json"].as_str(), Some("personal.json"));
        // The pointer rule does not freeze the referenced JSON file.
        let catalog = path.parent().unwrap().join("personal.json");
        crate::config::write_text_file(&catalog, "{\"models\":[]}").unwrap();
        crate::config::write_text_file(&catalog, "{\"models\":[\"new\"]}").unwrap();
        assert!(std::fs::read_to_string(catalog).unwrap().contains("new"));
        let conflict = format!(
            "profile='work'\n{text}\n[profiles.work]\nopenai_base_url='https://other.example/v1'\n"
        );
        let mut doc = conflict.parse::<DocumentMut>().unwrap();
        assert!(patch.apply_to(&path, &mut doc).is_err());
        // A profile that still reaches the intended local endpoint is valid.
        let mut doc = conflict
            .replace("https://other.example/v1", "http://127.0.0.1:15721/v1")
            .parse::<DocumentMut>()
            .unwrap();
        patch.apply_to(&path, &mut doc).unwrap();
    }

    #[test]
    #[serial]
    fn recovery_does_not_commit_a_switch_after_external_connection_drift() {
        let home = Home::new();
        let before =
            "model_provider='openai'\nopenai_base_url='http://127.0.0.1:15721/v1'\nmodel='old'\n";
        let path = home.seed(before);
        initialize().unwrap();
        let store = DeviceStore::for_device();
        let guard = engine::lock_app("codex");
        let patch = WholeFile::Write(before.replace("'old'", "'new'").into_bytes());
        failpoint::crash_at(Some("marked"));
        assert!(operation::run(
            &store,
            &guard,
            "switch",
            &[FileChange {
                file: LiveFile::private(&path),
                patch: &patch,
            }],
            Default::default(),
            &|_| Ok(())
        )
        .is_err());
        failpoint::crash_at(None);
        let external = before.replace("15721", "15722");
        std::fs::write(&path, &external).unwrap();
        let committed = std::cell::Cell::new(false);
        assert!(operation::recover(&store, &guard, &|_| {
            committed.set(true);
            Ok(())
        })
        .is_err());
        assert!(!committed.get());
        assert_eq!(std::fs::read_to_string(path).unwrap(), external);
    }

    #[test]
    #[serial]
    fn pending_manual_edit_can_recover_and_unpublished_auto_write_is_discarded() {
        let home = Home::new();
        let path = home.seed("model='manual'\n");
        let store = DeviceStore::for_device();
        let guard = engine::lock_app("codex");
        let patch = WholeFile::Write(b"model='automatic'\n".to_vec());
        failpoint::crash_at(Some("pending"));
        assert!(operation::run(
            &store,
            &guard,
            "apply",
            &[FileChange {
                file: LiveFile::private(&path),
                patch: &patch,
            }],
            Default::default(),
            &|_| Ok(())
        )
        .is_err());
        failpoint::crash_at(None);
        crate::config::write_json_file(&rules_path(), &rules(&[&["model"]])).unwrap();
        assert!(matches!(
            operation::recover(&store, &guard, &|_| panic!("unpublished target")),
            Ok(Some(operation::RecoveryOutcome::Discarded))
        ));
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "model='manual'\n");
        let manual = ManualPatch(Guarded {
            expected_pre: engine::digest(Some(b"model='manual'\n")),
            then: WholeFile::Write(b"model='explicit'\n".to_vec()),
        });
        failpoint::crash_at(Some("marked"));
        assert!(operation::run(
            &store,
            &guard,
            MANUAL_EDIT,
            &[FileChange {
                file: LiveFile::private(&path),
                patch: &manual,
            }],
            Default::default(),
            &|_| Ok(())
        )
        .is_err());
        failpoint::crash_at(None);
        operation::recover(&store, &guard, &|_| Ok(())).unwrap();
        assert_eq!(std::fs::read_to_string(path).unwrap(), "model='explicit'\n");
    }
}
