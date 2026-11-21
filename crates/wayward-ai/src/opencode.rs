use std::path::PathBuf;
use std::time::SystemTime;

use anyhow::Context;
use async_trait::async_trait;
use rusqlite::{Connection, OpenFlags};

use crate::{Meter, Usage, UsageProvider};

pub struct OpenCodeProvider {
    db_path: PathBuf,
    limit: f64,
    reset_after_secs: u64,
    reset_anchor_secs: Option<u64>,
}

impl OpenCodeProvider {
    pub fn new(
        db_path: PathBuf,
        limit: f64,
        reset_after_secs: u64,
        reset_anchor_secs: Option<u64>,
    ) -> Self {
        Self {
            db_path,
            limit,
            reset_after_secs,
            reset_anchor_secs,
        }
    }
}

#[async_trait]
impl UsageProvider for OpenCodeProvider {
    fn id(&self) -> &'static str {
        "opencode"
    }

    async fn fetch(&mut self) -> anyhow::Result<Usage> {
        let connection =
            Connection::open_with_flags(&self.db_path, OpenFlags::SQLITE_OPEN_READ_ONLY)
                .with_context(|| format!("opening {}", self.db_path.display()))?;

        let now_millis = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .context("system clock is before the epoch")?
            .as_millis();
        let window_millis = u128::from(self.reset_after_secs) * 1_000;
        let cutoff = i64::try_from(now_millis.saturating_sub(window_millis)).unwrap_or(i64::MAX);

        let total: i64 = connection
            .query_row(
                "SELECT COALESCE(SUM(\
                     COALESCE(tokens_input, 0) + COALESCE(tokens_output, 0) + \
                     COALESCE(tokens_reasoning, 0) + COALESCE(tokens_cache_read, 0) + \
                     COALESCE(tokens_cache_write, 0)), 0) \
                 FROM session WHERE time_created >= ?1",
                [cutoff],
                |row| row.get(0),
            )
            .context("summing OpenCode session tokens")?;

        let reset_at = crate::window::next_reset(
            self.reset_anchor_secs,
            self.reset_after_secs,
            SystemTime::now(),
        );
        Ok(Usage {
            plan: None,
            meters: vec![Meter {
                label: "usage window".into(),
                used: total as f64,
                limit: self.limit,
                reset_at: Some(reset_at),
            }],
            spend: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;
    use std::time::{Duration, SystemTime};

    use rusqlite::{params, Connection};

    use super::*;
    use crate::test_support::{block_on, TempPath};

    fn now_millis() -> i64 {
        let duration = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .expect("clock is after epoch");
        i64::try_from(duration.as_millis()).expect("epoch millis fit in i64")
    }

    fn create_db(path: &Path, schema: &str) -> Connection {
        std::fs::create_dir_all(path.parent().expect("db path has a parent"))
            .expect("create fixture directory");
        let connection = Connection::open(path).expect("create fixture database");
        connection
            .execute(schema, [])
            .expect("create fixture session table");
        connection
    }

    const SESSION_SCHEMA: &str = "CREATE TABLE session (\
        id TEXT PRIMARY KEY, \
        project_id TEXT NOT NULL, \
        directory TEXT NOT NULL, \
        cost REAL DEFAULT 0 NOT NULL, \
        tokens_input INTEGER DEFAULT 0, \
        tokens_output INTEGER DEFAULT 0, \
        tokens_reasoning INTEGER DEFAULT 0, \
        tokens_cache_read INTEGER DEFAULT 0, \
        tokens_cache_write INTEGER DEFAULT 0, \
        time_created INTEGER NOT NULL, \
        time_updated INTEGER NOT NULL, \
        time_archived INTEGER\
        )";

    fn insert_session(connection: &Connection, id: &str, time_created: i64, tokens: [i64; 5]) {
        connection
            .execute(
                "INSERT INTO session \
                 (id, project_id, directory, tokens_input, tokens_output, \
                  tokens_reasoning, tokens_cache_read, tokens_cache_write, \
                  time_created, time_updated) \
                 VALUES (?1, 'p', 'd', ?2, ?3, ?4, ?5, ?6, ?7, ?7)",
                params![
                    id,
                    tokens[0],
                    tokens[1],
                    tokens[2],
                    tokens[3],
                    tokens[4],
                    time_created
                ],
            )
            .expect("insert fixture session");
    }

    #[test]
    fn token_columns_of_recent_sessions_sum_into_one_window_meter() {
        let dir = TempPath::new("opencode-happy");
        let db_path = dir.path().join("opencode.db");
        let now = now_millis();
        let connection = create_db(&db_path, SESSION_SCHEMA);
        insert_session(&connection, "s1", now, [10, 20, 30, 40, 50]);
        insert_session(&connection, "s2", now, [1, 2, 3, 4, 5]);
        insert_session(
            &connection,
            "old",
            now - Duration::from_secs(2 * 86_400).as_millis() as i64,
            [1_000, 1_000, 1_000, 1_000, 1_000],
        );
        drop(connection);

        let before = SystemTime::now();
        let mut provider = OpenCodeProvider::new(db_path, 500.0, 3_600, None);
        let usage = block_on(provider.fetch()).expect("fetch should succeed");

        assert!(usage.plan.is_none());
        assert!(usage.spend.is_none());
        assert_eq!(usage.meters.len(), 1);
        let meter = &usage.meters[0];
        assert_eq!(meter.label, "usage window");
        assert_eq!(meter.used, 165.0);
        assert_eq!(meter.limit, 500.0);
        assert!(meter.reset_at.expect("meter has reset time") > before);
    }

    #[test]
    fn id_is_opencode() {
        assert_eq!(
            OpenCodeProvider::new(PathBuf::new(), 1.0, 1, None).id(),
            "opencode"
        );
    }

    #[test]
    fn missing_database_returns_error() {
        let dir = TempPath::new("opencode-missing");
        let mut provider =
            OpenCodeProvider::new(dir.path().join("opencode.db"), 500.0, 3_600, None);

        assert!(block_on(provider.fetch()).is_err());
    }

    #[test]
    fn wrong_schema_returns_error() {
        let dir = TempPath::new("opencode-wrong-schema");
        let db_path = dir.path().join("opencode.db");
        let connection = create_db(&db_path, "CREATE TABLE other (id TEXT PRIMARY KEY)");
        drop(connection);
        let mut provider = OpenCodeProvider::new(db_path, 500.0, 3_600, None);

        assert!(block_on(provider.fetch()).is_err());
    }
}
