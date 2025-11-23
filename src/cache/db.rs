use crate::parser::{CommandOption, CommandSpec};
use sqlx::{sqlite::SqlitePoolOptions, SqlitePool};
use std::collections::HashMap;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

pub struct Cache {
    pool: SqlitePool,
}

impl Cache {
    pub async fn new(path: &Path) -> Result<Self, sqlx::Error> {
        // Ensure parent directory exists
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).ok();
        }

        let database_url = format!("sqlite:{}?mode=rwc", path.display());

        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect(&database_url)
            .await?;

        let cache = Cache { pool };
        cache.run_migrations().await?;

        Ok(cache)
    }

    async fn run_migrations(&self) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS command_specs (
                command_name TEXT PRIMARY KEY,
                help_hash TEXT NOT NULL,
                spec_json TEXT NOT NULL,
                danger_level TEXT NOT NULL,
                created_at INTEGER NOT NULL,
                last_used INTEGER,
                use_count INTEGER DEFAULT 0
            )
            "#,
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            r#"
            CREATE INDEX IF NOT EXISTS idx_command_hash
            ON command_specs(command_name, help_hash)
            "#,
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS command_values (
                command_name TEXT NOT NULL,
                flag_name TEXT NOT NULL,
                value TEXT NOT NULL,
                last_used INTEGER NOT NULL,
                PRIMARY KEY (command_name, flag_name)
            )
            "#,
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS command_history (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                command_name TEXT NOT NULL,
                args_json TEXT NOT NULL,
                timestamp INTEGER NOT NULL,
                success INTEGER
            )
            "#,
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Get a cached command spec
    pub async fn get_spec(&self, command_name: &str) -> Result<Option<CommandSpec>, sqlx::Error> {
        let row: Option<(String,)> = sqlx::query_as(
            "SELECT spec_json FROM command_specs WHERE command_name = ?",
        )
        .bind(command_name)
        .fetch_optional(&self.pool)
        .await?;

        match row {
            Some((json,)) => {
                let spec: CommandSpec = serde_json::from_str(&json)
                    .map_err(|e| sqlx::Error::Decode(Box::new(e)))?;
                Ok(Some(spec))
            }
            None => Ok(None),
        }
    }

    /// Save a command spec to cache
    pub async fn save_spec(
        &self,
        command_name: &str,
        spec: &CommandSpec,
    ) -> Result<(), sqlx::Error> {
        let now = current_timestamp();
        let spec_json = serde_json::to_string(spec)
            .map_err(|e| sqlx::Error::Decode(Box::new(e)))?;

        sqlx::query(
            r#"
            INSERT INTO command_specs (command_name, help_hash, spec_json, danger_level, created_at, last_used, use_count)
            VALUES (?, ?, ?, ?, ?, ?, 1)
            ON CONFLICT(command_name) DO UPDATE SET
                help_hash = excluded.help_hash,
                spec_json = excluded.spec_json,
                danger_level = excluded.danger_level,
                last_used = excluded.last_used,
                use_count = use_count + 1
            "#,
        )
        .bind(command_name)
        .bind(&spec.version_hash)
        .bind(&spec_json)
        .bind(spec.danger_level.to_string())
        .bind(now)
        .bind(now)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Update usage statistics
    pub async fn update_usage(&self, command_name: &str) -> Result<(), sqlx::Error> {
        let now = current_timestamp();

        sqlx::query(
            "UPDATE command_specs SET last_used = ?, use_count = use_count + 1 WHERE command_name = ?",
        )
        .bind(now)
        .bind(command_name)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Get cached values for a command
    pub async fn get_values(
        &self,
        command_name: &str,
    ) -> Result<HashMap<String, String>, sqlx::Error> {
        let rows: Vec<(String, String)> = sqlx::query_as(
            "SELECT flag_name, value FROM command_values WHERE command_name = ?",
        )
        .bind(command_name)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.into_iter().collect())
    }

    /// Save non-sensitive values for a command
    pub async fn save_values(
        &self,
        command_name: &str,
        values: &HashMap<String, String>,
        options: &[CommandOption],
    ) -> Result<(), sqlx::Error> {
        let now = current_timestamp();

        // Create a set of sensitive flag names
        let sensitive_flags: std::collections::HashSet<String> = options
            .iter()
            .filter(|o| o.sensitive)
            .flat_map(|o| o.flags.iter().cloned())
            .collect();

        for (flag, value) in values {
            // Skip sensitive values
            if sensitive_flags.contains(flag) {
                continue;
            }

            // Skip empty values
            if value.is_empty() {
                continue;
            }

            sqlx::query(
                r#"
                INSERT INTO command_values (command_name, flag_name, value, last_used)
                VALUES (?, ?, ?, ?)
                ON CONFLICT(command_name, flag_name) DO UPDATE SET
                    value = excluded.value,
                    last_used = excluded.last_used
                "#,
            )
            .bind(command_name)
            .bind(flag)
            .bind(value)
            .bind(now)
            .execute(&self.pool)
            .await?;
        }

        Ok(())
    }

    /// Clear cached values for a command
    pub async fn clear_values(&self, command_name: &str) -> Result<(), sqlx::Error> {
        sqlx::query("DELETE FROM command_values WHERE command_name = ?")
            .bind(command_name)
            .execute(&self.pool)
            .await?;

        Ok(())
    }

    /// Log command execution to history
    #[allow(dead_code)]
    pub async fn log_execution(
        &self,
        command_name: &str,
        args: &HashMap<String, String>,
        success: bool,
    ) -> Result<(), sqlx::Error> {
        let now = current_timestamp();
        let args_json = serde_json::to_string(args)
            .map_err(|e| sqlx::Error::Decode(Box::new(e)))?;

        sqlx::query(
            r#"
            INSERT INTO command_history (command_name, args_json, timestamp, success)
            VALUES (?, ?, ?, ?)
            "#,
        )
        .bind(command_name)
        .bind(args_json)
        .bind(now)
        .bind(success)
        .execute(&self.pool)
        .await?;

        Ok(())
    }
}

fn current_timestamp() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
}
