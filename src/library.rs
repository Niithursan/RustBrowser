//! Local library data only. Native navigation history and rendering stay in the engine.
use crate::navigation;
use rusqlite::{Connection, ErrorCode, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use std::{
    path::Path,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;
pub const SCHEMA_VERSION: i64 = 3;
pub const HISTORY_LIMIT: i64 = 10_000;

pub fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct Filter {
    pub query: String,
    pub folder: Option<i64>,
    pub from: Option<i64>,
    pub until: Option<i64>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Action {
    SetPermission {
        origin: String,
        capability: crate::security::Capability,
        decision: Option<crate::security::Decision>,
    },
    ClearLibrary {
        history: bool,
        permissions: bool,
        confirmed: bool,
    },
    Refresh,
    SaveBookmark {
        id: Option<i64>,
        title: String,
        url: String,
        folder: Option<i64>,
    },
    RemoveBookmark {
        id: i64,
    },
    SaveFolder {
        id: Option<i64>,
        name: String,
    },
    RemoveFolder {
        id: i64,
    },
    RemoveVisit {
        id: i64,
    },
    ClearHistory {
        confirmed: bool,
    },
    Record {
        key: String,
        url: String,
        title: String,
        timestamp: i64,
    },
}

#[derive(Clone, Debug, Serialize)]
pub struct Bookmark {
    pub id: i64,
    pub title: String,
    pub url: String,
    pub folder: Option<i64>,
}
#[derive(Clone, Debug, Serialize)]
pub struct Folder {
    pub id: i64,
    pub name: String,
}
#[derive(Clone, Debug, Serialize)]
pub struct Visit {
    pub id: i64,
    pub title: String,
    pub url: String,
    pub timestamp: i64,
}
#[derive(Clone, Debug, Default, Serialize)]
pub struct Data {
    pub permissions: Vec<crate::security::Rule>,
    pub bookmarks: Vec<Bookmark>,
    pub folders: Vec<Folder>,
    pub history: Vec<Visit>,
}

fn text(value: &str, max: usize) -> Result<String> {
    let value = value.trim();
    if value.is_empty() || value.chars().count() > max || value.chars().any(char::is_control) {
        return Err("Text is empty, too long, or contains control characters.".into());
    }
    Ok(value.into())
}

pub fn bookmark_url(value: &str) -> Result<String> {
    let value = navigation::address(value)?;
    if value.len() > 8192 {
        return Err("Bookmark URL is too long.".into());
    }
    Ok(value)
}

/// Query strings often carry searches, sign-in codes and tokens. Do not record them.
/// Fragment-only navigation is not a new visit; fragments are not persisted.
pub fn history_url(value: &str) -> Option<String> {
    let value = bookmark_url(value).ok()?;
    let mut url = url::Url::parse(&value).ok()?;
    if url.query().is_some() {
        return None;
    }
    url.set_fragment(None);
    Some(url.into())
}

pub struct Database {
    connection: Connection,
}
impl Database {
    pub fn open(path: &Path) -> Result<(Self, bool)> {
        match Self::open_checked(path) {
            Ok(database) => Ok((database, false)),
            Err(error) => {
                let corrupt = error.downcast_ref::<rusqlite::Error>().is_some_and(|error|
                    matches!(error, rusqlite::Error::SqliteFailure(code, _) if matches!(code.code, ErrorCode::DatabaseCorrupt | ErrorCode::NotADatabase)))
                    || error.to_string() == "Database integrity check failed.";
                if !corrupt {
                    return Err(error);
                }
                let suffix = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
                let backup = path.with_extension(format!("corrupt-{suffix}"));
                std::fs::rename(path, &backup)?;
                for extension in ["-journal", "-wal", "-shm"] {
                    let sidecar =
                        std::path::PathBuf::from(format!("{}{extension}", path.display()));
                    if sidecar.exists() {
                        std::fs::rename(sidecar, format!("{}{extension}", backup.display()))?;
                    }
                }
                Ok((Self::open_checked(path)?, true))
            }
        }
    }

    fn open_checked(path: &Path) -> Result<Self> {
        let mut connection = Connection::open(path)?;
        connection.busy_timeout(Duration::from_secs(2))?;
        let integrity: String = connection.query_row("PRAGMA quick_check", [], |r| r.get(0))?;
        if integrity != "ok" {
            return Err("Database integrity check failed.".into());
        }
        let version: i64 = connection.query_row("PRAGMA user_version", [], |r| r.get(0))?;
        if version > SCHEMA_VERSION {
            return Err("Library schema is newer than this browser. Use the newer browser; the database was preserved.".into());
        }
        connection.execute_batch(
            "PRAGMA foreign_keys=ON; PRAGMA secure_delete=ON; PRAGMA synchronous=FULL;",
        )?;
        if version < 1 {
            let tx = connection.transaction()?;
            tx.execute_batch("CREATE TABLE folders(id INTEGER PRIMARY KEY, name TEXT NOT NULL UNIQUE COLLATE NOCASE);
                CREATE TABLE bookmarks(id INTEGER PRIMARY KEY, title TEXT NOT NULL, url TEXT NOT NULL UNIQUE,
                folder INTEGER REFERENCES folders(id) ON DELETE SET NULL);
                PRAGMA user_version=1;")?;
            tx.commit()?;
        }
        if version < 2 {
            let tx = connection.transaction()?;
            tx.execute_batch(
                "CREATE TABLE visits(id INTEGER PRIMARY KEY, event_key TEXT NOT NULL UNIQUE,
                url TEXT NOT NULL, title TEXT NOT NULL, visited_at INTEGER NOT NULL);
                CREATE INDEX visits_time ON visits(visited_at DESC);
                CREATE INDEX visits_url ON visits(url,visited_at DESC);
                PRAGMA user_version=2;",
            )?;
            tx.commit()?;
        }
        if version < 3 {
            let tx = connection.transaction()?;
            tx.execute_batch("CREATE TABLE permissions(origin TEXT NOT NULL, capability TEXT NOT NULL, decision TEXT NOT NULL, PRIMARY KEY(origin,capability)); PRAGMA user_version=3;")?;
            tx.commit()?;
        }
        Ok(Self { connection })
    }

    pub fn apply(&mut self, action: Action) -> Result<()> {
        match action {
            Action::SetPermission {
                origin,
                capability,
                decision,
            } => {
                if crate::security::permission_origin(&origin).as_deref() != Some(origin.as_str()) {
                    return Err("Invalid permission origin.".into());
                }
                let capability = serde_json::to_string(&capability)?;
                if let Some(decision) = decision {
                    let count: i64 =
                        self.connection
                            .query_row("SELECT count(*) FROM permissions", [], |r| r.get(0))?;
                    if count >= 512 {
                        return Err("Permission rule limit reached; revoke an entry first.".into());
                    }
                    self.connection.execute("INSERT INTO permissions(origin,capability,decision) VALUES(?1,?2,?3) ON CONFLICT(origin,capability) DO UPDATE SET decision=excluded.decision",params![origin,capability,serde_json::to_string(&decision)?])?;
                } else {
                    self.connection.execute(
                        "DELETE FROM permissions WHERE origin=?1 AND capability=?2",
                        params![origin, capability],
                    )?;
                }
            }
            Action::ClearLibrary {
                history,
                permissions,
                confirmed,
            } => {
                if !confirmed {
                    return Err("Confirmation required.".into());
                }
                let tx = self.connection.transaction()?;
                if history {
                    tx.execute("DELETE FROM visits", [])?;
                }
                if permissions {
                    tx.execute("DELETE FROM permissions", [])?;
                }
                tx.commit()?;
            }
            Action::Refresh => {}
            Action::SaveBookmark {
                id,
                title,
                url,
                folder,
            } => {
                let title = text(&title, 256)?;
                let url = bookmark_url(&url)?;
                if let Some(id) = id {
                    if self.connection.execute(
                        "UPDATE bookmarks SET title=?1,url=?2,folder=?3 WHERE id=?4",
                        params![title, url, folder, id],
                    )? == 0
                    {
                        return Err("Bookmark no longer exists.".into());
                    }
                } else {
                    let count: i64 =
                        self.connection
                            .query_row("SELECT count(*) FROM bookmarks", [], |r| r.get(0))?;
                    if count >= 5000 {
                        return Err("Bookmark limit (5000) reached.".into());
                    }
                    self.connection.execute("INSERT INTO bookmarks(title,url,folder) VALUES(?1,?2,?3) ON CONFLICT(url) DO NOTHING", params![title,url,folder])?;
                }
            }
            Action::RemoveBookmark { id } => {
                self.connection
                    .execute("DELETE FROM bookmarks WHERE id=?1", [id])?;
            }
            Action::SaveFolder { id, name } => {
                let name = text(&name, 80)?;
                if let Some(id) = id {
                    if self
                        .connection
                        .execute("UPDATE folders SET name=?1 WHERE id=?2", params![name, id])?
                        == 0
                    {
                        return Err("Folder no longer exists.".into());
                    }
                } else {
                    let count: i64 =
                        self.connection
                            .query_row("SELECT count(*) FROM folders", [], |r| r.get(0))?;
                    if count >= 200 {
                        return Err("Folder limit (200) reached.".into());
                    }
                    self.connection
                        .execute("INSERT INTO folders(name) VALUES(?1)", [name])?;
                }
            }
            Action::RemoveFolder { id } => {
                self.connection
                    .execute("DELETE FROM folders WHERE id=?1", [id])?;
            }
            Action::RemoveVisit { id } => {
                self.connection
                    .execute("DELETE FROM visits WHERE id=?1", [id])?;
            }
            Action::ClearHistory { confirmed } => {
                if !confirmed {
                    return Err("Clearing history requires confirmation.".into());
                }
                self.connection.execute("DELETE FROM visits", [])?;
            }
            Action::Record {
                key,
                url,
                title,
                timestamp,
            } => {
                let Some(url) = history_url(&url) else {
                    return Ok(());
                };
                let key = text(&key, 200)?;
                if !(0..=4_102_444_800).contains(&timestamp) {
                    return Err("Invalid visit timestamp.".into());
                }
                let title: String = title
                    .chars()
                    .filter(|c| !c.is_control())
                    .take(256)
                    .collect();
                let tx = self.connection.transaction()?;
                let duplicate: bool = tx.query_row(
                    "SELECT EXISTS(SELECT 1 FROM visits WHERE event_key=?1)",
                    [&key],
                    |r| r.get(0),
                )?;
                if !duplicate {
                    let recent: Option<i64> = tx.query_row("SELECT id FROM visits WHERE url=?1 AND visited_at BETWEEN ?2 AND ?3 ORDER BY visited_at DESC LIMIT 1", params![url,timestamp.saturating_sub(30),timestamp], |r| r.get(0)).optional()?;
                    if let Some(id) = recent {
                        tx.execute("UPDATE visits SET title=?1 WHERE id=?2", params![title, id])?;
                    } else {
                        tx.execute("INSERT INTO visits(event_key,url,title,visited_at) VALUES(?1,?2,?3,?4)", params![key,url,title,timestamp])?;
                    }
                    tx.execute("DELETE FROM visits WHERE id NOT IN (SELECT id FROM visits ORDER BY visited_at DESC,id DESC LIMIT ?1)", [HISTORY_LIMIT])?;
                }
                tx.commit()?;
            }
        }
        Ok(())
    }

    pub fn query(&self, filter: &Filter) -> Result<Data> {
        if filter.query.len() > 512
            || filter
                .from
                .is_some_and(|v| !(0..=4_102_444_800).contains(&v))
            || filter
                .until
                .is_some_and(|v| !(0..=4_102_444_800).contains(&v))
            || matches!((filter.from,filter.until), (Some(a),Some(b)) if a>=b)
        {
            return Err("Invalid search or date range.".into());
        }
        let query = filter.query.trim();
        let folders = self
            .connection
            .prepare("SELECT id,name FROM folders ORDER BY name COLLATE NOCASE")?
            .query_map([], |r| {
                Ok(Folder {
                    id: r.get(0)?,
                    name: r.get(1)?,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        let bookmarks = self.connection.prepare("SELECT id,title,url,folder FROM bookmarks WHERE instr(lower(title||' '||url),lower(?1))>0 AND (?2 IS NULL OR coalesce(folder,0)=?2) ORDER BY title COLLATE NOCASE,id LIMIT 200")?.query_map(params![query,filter.folder], |r| Ok(Bookmark { id:r.get(0)?, title:r.get(1)?, url:r.get(2)?, folder:r.get(3)? }))?.collect::<std::result::Result<Vec<_>,_>>()?;
        let history = self.connection.prepare("SELECT id,title,url,visited_at FROM visits WHERE instr(lower(title||' '||url),lower(?1))>0 AND (?2 IS NULL OR visited_at>=?2) AND (?3 IS NULL OR visited_at<?3) ORDER BY visited_at DESC,id DESC LIMIT 200")?.query_map(params![query,filter.from,filter.until], |r| Ok(Visit { id:r.get(0)?, title:r.get(1)?, url:r.get(2)?, timestamp:r.get(3)? }))?.collect::<std::result::Result<Vec<_>,_>>()?;
        let mut permissions = Vec::new();
        let mut statement=self.connection.prepare("SELECT origin,capability,decision FROM permissions ORDER BY origin,capability LIMIT 513")?;
        let values = statement.query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
            ))
        })?;
        for value in values {
            let (origin, capability, decision) = value?;
            if crate::security::permission_origin(&origin).as_deref() != Some(origin.as_str()) {
                continue;
            }
            if let (Ok(capability), Ok(decision)) = (
                serde_json::from_str(&capability),
                serde_json::from_str(&decision),
            ) {
                permissions.push(crate::security::Rule {
                    origin,
                    capability,
                    decision,
                });
            }
        }
        Ok(Data {
            permissions,
            bookmarks,
            folders,
            history,
        })
    }
}
