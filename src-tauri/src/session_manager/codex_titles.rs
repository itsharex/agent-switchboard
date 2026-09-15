//! Codex thread titles live outside the JSONL transcript. A rename lands in
//! `session_index.jsonl` (one `{id, thread_name}` line per update, last one
//! wins) and, on newer Codex builds, in the `threads` table of the state
//! database. Codex can move that database with `sqlite_home` in
//! `config.toml` or the `CODEX_SQLITE_HOME` variable, so every candidate
//! location is consulted. All reads are read-only and tolerate a missing or
//! locked file: a title lookup that fails simply leaves the parsed title.

use std::collections::HashMap;
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::time::Duration;

const SESSION_INDEX_FILE: &str = "session_index.jsonl";
/// Codex bumps this name across releases; keep the one source of truth here.
const STATE_DB_FILE: &str = "state_5.sqlite";
const SQLITE_HOME_ENV: &str = "CODEX_SQLITE_HOME";

/// Titles keyed by thread id for one Codex root. The state database is
/// consulted after the index so an explicit rename there wins.
pub(super) fn load(codex_root: &Path) -> HashMap<String, String> {
    let config_text = fs::read_to_string(codex_root.join("config.toml")).unwrap_or_default();
    let sqlite_home = std::env::var_os(SQLITE_HOME_ENV).map(PathBuf::from);
    load_from(
        &codex_root.join(SESSION_INDEX_FILE),
        &state_db_paths(codex_root, &config_text, sqlite_home.as_deref()),
    )
}

fn load_from(index: &Path, databases: &[PathBuf]) -> HashMap<String, String> {
    let mut titles = from_session_index(index);
    for database in databases {
        titles.extend(from_state_db(database));
    }
    titles
}

/// The root database plus, when Codex keeps SQLite state elsewhere, that
/// copy too. `config.toml` takes precedence over the environment, matching
/// the Codex resolver.
fn state_db_paths(
    codex_root: &Path,
    config_text: &str,
    sqlite_home_env: Option<&Path>,
) -> Vec<PathBuf> {
    let mut paths = vec![codex_root.join(STATE_DB_FILE)];
    let configured = config_text
        .parse::<toml_edit::DocumentMut>()
        .ok()
        .and_then(|document| document.get("sqlite_home")?.as_str().map(str::trim).map(String::from))
        .filter(|value| !value.is_empty())
        .map(|value| expand_home(&value))
        .or_else(|| {
            sqlite_home_env
                .filter(|path| !path.as_os_str().is_empty())
                .map(Path::to_path_buf)
        });
    if let Some(home) = configured {
        let candidate = home.join(STATE_DB_FILE);
        if !paths.contains(&candidate) {
            paths.push(candidate);
        }
    }
    paths
}

fn expand_home(raw: &str) -> PathBuf {
    let home = || crate::local_state::user_home_dir().ok();
    if raw == "~" {
        return home().unwrap_or_else(|| PathBuf::from(raw));
    }
    for prefix in ["~/", "~\\"] {
        if let Some(rest) = raw.strip_prefix(prefix) {
            return home()
                .map(|home| home.join(rest))
                .unwrap_or_else(|| PathBuf::from(raw));
        }
    }
    PathBuf::from(raw)
}

fn from_session_index(path: &Path) -> HashMap<String, String> {
    let Ok(file) = fs::File::open(path) else {
        return HashMap::new();
    };
    let mut titles = HashMap::new();
    for line in BufReader::new(file).lines().map_while(Result::ok) {
        let Ok(value) = serde_json::from_str::<serde_json::Value>(&line) else {
            continue;
        };
        let id = value.get("id").and_then(|id| id.as_str()).map(str::trim);
        let name = value
            .get("thread_name")
            .and_then(|name| name.as_str())
            .map(str::trim);
        if let (Some(id), Some(name)) = (id, name) {
            if super::parser::valid_session_id(id) && !name.is_empty() {
                titles.insert(id.to_string(), name.to_string());
            }
        }
    }
    titles
}

/// Mirrors Codex's own distinct-title rule: a title equal to the first user
/// message is not a rename. The comparison stays in SQL so the unbounded
/// first message is never loaded. Codex holds this database open while it
/// runs, so a short busy timeout keeps a concurrent write from dropping every
/// title.
fn from_state_db(path: &Path) -> HashMap<String, String> {
    if !path.is_file() {
        return HashMap::new();
    }
    let flags = rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX;
    let Ok(connection) = rusqlite::Connection::open_with_flags(path, flags) else {
        return HashMap::new();
    };
    if connection.busy_timeout(Duration::from_secs(2)).is_err() {
        return HashMap::new();
    }
    let Ok(mut statement) = connection.prepare(
        "SELECT id, title FROM threads WHERE title <> '' \
         AND (first_user_message IS NULL OR TRIM(title) <> TRIM(first_user_message))",
    ) else {
        return HashMap::new();
    };
    let Ok(rows) = statement.query_map([], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
    }) else {
        return HashMap::new();
    };
    rows.map_while(Result::ok)
        .filter(|(id, title)| super::parser::valid_session_id(id) && !title.trim().is_empty())
        .map(|(id, title)| (id, title.trim().to_string()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn threads_db(path: &Path, rows: &[(&str, &str, Option<&str>)]) {
        let connection = rusqlite::Connection::open(path).unwrap();
        connection
            .execute(
                "CREATE TABLE threads (id TEXT PRIMARY KEY, title TEXT NOT NULL, first_user_message TEXT)",
                [],
            )
            .unwrap();
        for (id, title, first) in rows {
            connection
                .execute(
                    "INSERT INTO threads (id, title, first_user_message) VALUES (?1, ?2, ?3)",
                    rusqlite::params![id, title, first],
                )
                .unwrap();
        }
    }

    #[test]
    fn session_index_keeps_the_latest_non_empty_name_per_thread() {
        let temp = tempdir().unwrap();
        let index = temp.path().join(SESSION_INDEX_FILE);
        fs::write(
            &index,
            concat!(
                "{\"id\":\"thread-1\",\"thread_name\":\"Old name\",\"updated_at\":\"2026-07-01T00:00:00Z\"}\n",
                "{\"id\":\"thread-2\",\"thread_name\":\"   \"}\n",
                "not json\n",
                "{\"id\":\"bad id;\",\"thread_name\":\"ignored\"}\n",
                "{\"id\":\"thread-1\",\"thread_name\":\"  New name  \"}\n"
            ),
        )
        .unwrap();
        let titles = from_session_index(&index);
        assert_eq!(titles.get("thread-1").map(String::as_str), Some("New name"));
        assert!(!titles.contains_key("thread-2"));
        assert!(!titles.contains_key("bad id;"));
        assert!(from_session_index(&temp.path().join("missing.jsonl")).is_empty());
    }

    #[test]
    fn state_db_titles_skip_first_message_echoes_and_override_the_index() {
        let temp = tempdir().unwrap();
        let index = temp.path().join(SESSION_INDEX_FILE);
        fs::write(
            &index,
            "{\"id\":\"thread-1\",\"thread_name\":\"Legacy name\"}\n{\"id\":\"thread-2\",\"thread_name\":\"Legacy fallback\"}\n",
        )
        .unwrap();
        let database = temp.path().join(STATE_DB_FILE);
        threads_db(
            &database,
            &[
                ("thread-1", " SQLite name ", Some("First prompt")),
                ("thread-2", "First prompt", Some("First prompt")),
                ("thread-3", "Renamed before sync", None),
                ("thread-4", "", None),
            ],
        );
        let titles = load_from(&index, &[database]);
        assert_eq!(titles.get("thread-1").map(String::as_str), Some("SQLite name"));
        assert_eq!(
            titles.get("thread-2").map(String::as_str),
            Some("Legacy fallback"),
            "标题与首条消息相同不算重命名，回退到索引名"
        );
        assert_eq!(
            titles.get("thread-3").map(String::as_str),
            Some("Renamed before sync")
        );
        assert!(!titles.contains_key("thread-4"));
    }

    #[test]
    fn a_missing_or_foreign_database_yields_no_titles() {
        let temp = tempdir().unwrap();
        assert!(from_state_db(&temp.path().join(STATE_DB_FILE)).is_empty());
        let other = temp.path().join("other.sqlite");
        rusqlite::Connection::open(&other)
            .unwrap()
            .execute("CREATE TABLE unrelated (x TEXT)", [])
            .unwrap();
        assert!(from_state_db(&other).is_empty());
    }

    #[test]
    fn sqlite_home_from_config_wins_over_the_environment_and_dedupes() {
        let temp = tempdir().unwrap();
        let root = temp.path().join("codex");
        let configured = temp.path().join("configured");
        let from_env = temp.path().join("from-env");
        let config = format!("sqlite_home = '{}'\n", configured.display());
        assert_eq!(
            state_db_paths(&root, &config, Some(&from_env)),
            vec![root.join(STATE_DB_FILE), configured.join(STATE_DB_FILE)]
        );
        assert_eq!(
            state_db_paths(&root, "", Some(&from_env)),
            vec![root.join(STATE_DB_FILE), from_env.join(STATE_DB_FILE)]
        );
        assert_eq!(
            state_db_paths(&root, "sqlite_home = ''\n", None),
            vec![root.join(STATE_DB_FILE)]
        );
        let same = format!("sqlite_home = '{}'\n", root.display());
        assert_eq!(
            state_db_paths(&root, &same, None),
            vec![root.join(STATE_DB_FILE)],
            "指向同一目录时不重复读取"
        );
    }
}
