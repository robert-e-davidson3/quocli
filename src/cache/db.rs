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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::{ArgumentType, DangerLevel, OptionLevel, PositionalArg};
    use tempfile::TempDir;

    // Helper to create a test cache in a temporary directory
    async fn create_test_cache() -> (Cache, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test_cache.db");
        let cache = Cache::new(&db_path).await.unwrap();
        (cache, temp_dir)
    }

    // Helper to create a minimal CommandSpec for testing
    fn create_test_spec(command: &str) -> CommandSpec {
        CommandSpec {
            command: command.to_string(),
            version_hash: "test_hash_123".to_string(),
            description: "Test command description".to_string(),
            options: vec![],
            positional_args: vec![],
            subcommands: vec![],
            danger_level: DangerLevel::Low,
            examples: vec!["example1".to_string()],
            positionals_first: false,
        }
    }

    // Helper to create a CommandSpec with options
    fn create_spec_with_options() -> CommandSpec {
        CommandSpec {
            command: "test".to_string(),
            version_hash: "hash456".to_string(),
            description: "Test with options".to_string(),
            options: vec![
                CommandOption {
                    flags: vec!["--verbose".to_string(), "-v".to_string()],
                    description: "Enable verbose output".to_string(),
                    argument_type: ArgumentType::Bool,
                    argument_name: None,
                    required: false,
                    sensitive: false,
                    repeatable: false,
                    conflicts_with: vec![],
                    requires: vec![],
                    default: None,
                    enum_values: vec![],
                    level: OptionLevel::Basic,
                },
                CommandOption {
                    flags: vec!["--password".to_string()],
                    description: "Password".to_string(),
                    argument_type: ArgumentType::String,
                    argument_name: Some("PASS".to_string()),
                    required: false,
                    sensitive: true,
                    repeatable: false,
                    conflicts_with: vec![],
                    requires: vec![],
                    default: None,
                    enum_values: vec![],
                    level: OptionLevel::Basic,
                },
                CommandOption {
                    flags: vec!["--output".to_string(), "-o".to_string()],
                    description: "Output file".to_string(),
                    argument_type: ArgumentType::Path,
                    argument_name: Some("FILE".to_string()),
                    required: false,
                    sensitive: false,
                    repeatable: false,
                    conflicts_with: vec![],
                    requires: vec![],
                    default: None,
                    enum_values: vec![],
                    level: OptionLevel::Basic,
                },
            ],
            positional_args: vec![PositionalArg {
                name: "file".to_string(),
                description: "Input file".to_string(),
                required: true,
                sensitive: false,
                argument_type: ArgumentType::Path,
                default: None,
            }],
            subcommands: vec![],
            danger_level: DangerLevel::Medium,
            examples: vec![],
            positionals_first: false,
        }
    }

    #[tokio::test]
    async fn test_cache_new_creates_database() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("new_cache.db");

        assert!(!db_path.exists());
        let _cache = Cache::new(&db_path).await.unwrap();
        assert!(db_path.exists());
    }

    #[tokio::test]
    async fn test_cache_new_creates_parent_directories() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("nested").join("dirs").join("cache.db");

        let _cache = Cache::new(&db_path).await.unwrap();
        assert!(db_path.exists());
    }

    #[tokio::test]
    async fn test_save_and_get_spec() {
        let (cache, _temp) = create_test_cache().await;
        let spec = create_test_spec("ls");

        // Save spec
        cache.save_spec("ls", &spec).await.unwrap();

        // Retrieve spec
        let retrieved = cache.get_spec("ls").await.unwrap();
        assert!(retrieved.is_some());

        let retrieved = retrieved.unwrap();
        assert_eq!(retrieved.command, "ls");
        assert_eq!(retrieved.version_hash, "test_hash_123");
        assert_eq!(retrieved.description, "Test command description");
    }

    #[tokio::test]
    async fn test_get_spec_not_found() {
        let (cache, _temp) = create_test_cache().await;

        let result = cache.get_spec("nonexistent").await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_save_spec_updates_existing() {
        let (cache, _temp) = create_test_cache().await;

        // Save initial spec
        let spec1 = create_test_spec("git");
        cache.save_spec("git", &spec1).await.unwrap();

        // Save updated spec
        let mut spec2 = create_test_spec("git");
        spec2.version_hash = "updated_hash".to_string();
        spec2.description = "Updated description".to_string();
        cache.save_spec("git", &spec2).await.unwrap();

        // Retrieve and verify update
        let retrieved = cache.get_spec("git").await.unwrap().unwrap();
        assert_eq!(retrieved.version_hash, "updated_hash");
        assert_eq!(retrieved.description, "Updated description");
    }

    #[tokio::test]
    async fn test_save_spec_preserves_options() {
        let (cache, _temp) = create_test_cache().await;
        let spec = create_spec_with_options();

        cache.save_spec("test", &spec).await.unwrap();
        let retrieved = cache.get_spec("test").await.unwrap().unwrap();

        assert_eq!(retrieved.options.len(), 3);
        assert_eq!(retrieved.options[0].flags, vec!["--verbose", "-v"]);
        assert_eq!(retrieved.options[1].flags, vec!["--password"]);
        assert!(retrieved.options[1].sensitive);
        assert_eq!(retrieved.positional_args.len(), 1);
        assert_eq!(retrieved.positional_args[0].name, "file");
    }

    #[tokio::test]
    async fn test_update_usage() {
        let (cache, _temp) = create_test_cache().await;
        let spec = create_test_spec("curl");

        cache.save_spec("curl", &spec).await.unwrap();

        // Update usage multiple times
        cache.update_usage("curl").await.unwrap();
        cache.update_usage("curl").await.unwrap();
        cache.update_usage("curl").await.unwrap();

        // Verify spec still exists (we can't directly check use_count without raw SQL)
        let retrieved = cache.get_spec("curl").await.unwrap();
        assert!(retrieved.is_some());
    }

    #[tokio::test]
    async fn test_save_and_get_values() {
        let (cache, _temp) = create_test_cache().await;
        let spec = create_spec_with_options();

        let mut values = HashMap::new();
        values.insert("--verbose".to_string(), "true".to_string());
        values.insert("--output".to_string(), "/tmp/out.txt".to_string());

        cache.save_values("test", &values, &spec.options).await.unwrap();

        let retrieved = cache.get_values("test").await.unwrap();
        assert_eq!(retrieved.get("--verbose"), Some(&"true".to_string()));
        assert_eq!(retrieved.get("--output"), Some(&"/tmp/out.txt".to_string()));
    }

    #[tokio::test]
    async fn test_save_values_filters_sensitive() {
        let (cache, _temp) = create_test_cache().await;
        let spec = create_spec_with_options();

        let mut values = HashMap::new();
        values.insert("--verbose".to_string(), "true".to_string());
        values.insert("--password".to_string(), "secret123".to_string()); // sensitive!
        values.insert("--output".to_string(), "/tmp/out.txt".to_string());

        cache.save_values("test", &values, &spec.options).await.unwrap();

        let retrieved = cache.get_values("test").await.unwrap();
        assert_eq!(retrieved.get("--verbose"), Some(&"true".to_string()));
        assert!(retrieved.get("--password").is_none()); // Should be filtered
        assert_eq!(retrieved.get("--output"), Some(&"/tmp/out.txt".to_string()));
    }

    #[tokio::test]
    async fn test_save_values_skips_empty() {
        let (cache, _temp) = create_test_cache().await;
        let spec = create_spec_with_options();

        let mut values = HashMap::new();
        values.insert("--verbose".to_string(), "true".to_string());
        values.insert("--output".to_string(), "".to_string()); // empty!

        cache.save_values("test", &values, &spec.options).await.unwrap();

        let retrieved = cache.get_values("test").await.unwrap();
        assert_eq!(retrieved.get("--verbose"), Some(&"true".to_string()));
        assert!(retrieved.get("--output").is_none()); // Should be skipped
    }

    #[tokio::test]
    async fn test_save_values_updates_existing() {
        let (cache, _temp) = create_test_cache().await;
        let spec = create_spec_with_options();

        // Save initial values
        let mut values1 = HashMap::new();
        values1.insert("--verbose".to_string(), "true".to_string());
        cache.save_values("test", &values1, &spec.options).await.unwrap();

        // Save updated values
        let mut values2 = HashMap::new();
        values2.insert("--verbose".to_string(), "false".to_string());
        values2.insert("--output".to_string(), "/new/path".to_string());
        cache.save_values("test", &values2, &spec.options).await.unwrap();

        let retrieved = cache.get_values("test").await.unwrap();
        assert_eq!(retrieved.get("--verbose"), Some(&"false".to_string()));
        assert_eq!(retrieved.get("--output"), Some(&"/new/path".to_string()));
    }

    #[tokio::test]
    async fn test_get_values_empty_command() {
        let (cache, _temp) = create_test_cache().await;

        let retrieved = cache.get_values("nonexistent").await.unwrap();
        assert!(retrieved.is_empty());
    }

    #[tokio::test]
    async fn test_clear_values() {
        let (cache, _temp) = create_test_cache().await;
        let spec = create_spec_with_options();

        // Save some values
        let mut values = HashMap::new();
        values.insert("--verbose".to_string(), "true".to_string());
        values.insert("--output".to_string(), "/tmp/out.txt".to_string());
        cache.save_values("test", &values, &spec.options).await.unwrap();

        // Clear values
        cache.clear_values("test").await.unwrap();

        // Verify cleared
        let retrieved = cache.get_values("test").await.unwrap();
        assert!(retrieved.is_empty());
    }

    #[tokio::test]
    async fn test_clear_values_nonexistent() {
        let (cache, _temp) = create_test_cache().await;

        // Should not error on nonexistent command
        let result = cache.clear_values("nonexistent").await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_log_execution() {
        let (cache, _temp) = create_test_cache().await;

        let mut args = HashMap::new();
        args.insert("--verbose".to_string(), "true".to_string());
        args.insert("file".to_string(), "input.txt".to_string());

        // Log successful execution
        cache.log_execution("test", &args, true).await.unwrap();

        // Log failed execution
        cache.log_execution("test", &args, false).await.unwrap();

        // We can't directly verify the history without raw SQL, but no error means success
    }

    #[tokio::test]
    async fn test_log_execution_empty_args() {
        let (cache, _temp) = create_test_cache().await;

        let args = HashMap::new();
        let result = cache.log_execution("test", &args, true).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_multiple_commands() {
        let (cache, _temp) = create_test_cache().await;

        // Save specs for multiple commands
        let spec1 = create_test_spec("ls");
        let spec2 = create_test_spec("cat");
        let spec3 = create_test_spec("grep");

        cache.save_spec("ls", &spec1).await.unwrap();
        cache.save_spec("cat", &spec2).await.unwrap();
        cache.save_spec("grep", &spec3).await.unwrap();

        // Retrieve each
        assert!(cache.get_spec("ls").await.unwrap().is_some());
        assert!(cache.get_spec("cat").await.unwrap().is_some());
        assert!(cache.get_spec("grep").await.unwrap().is_some());
    }

    #[tokio::test]
    async fn test_values_isolation_between_commands() {
        let (cache, _temp) = create_test_cache().await;
        let spec = create_spec_with_options();

        // Save values for command1
        let mut values1 = HashMap::new();
        values1.insert("--verbose".to_string(), "true".to_string());
        cache.save_values("cmd1", &values1, &spec.options).await.unwrap();

        // Save values for command2
        let mut values2 = HashMap::new();
        values2.insert("--verbose".to_string(), "false".to_string());
        cache.save_values("cmd2", &values2, &spec.options).await.unwrap();

        // Values should be isolated
        let retrieved1 = cache.get_values("cmd1").await.unwrap();
        let retrieved2 = cache.get_values("cmd2").await.unwrap();

        assert_eq!(retrieved1.get("--verbose"), Some(&"true".to_string()));
        assert_eq!(retrieved2.get("--verbose"), Some(&"false".to_string()));
    }

    #[tokio::test]
    async fn test_spec_with_all_danger_levels() {
        let (cache, _temp) = create_test_cache().await;

        for (name, level) in [
            ("low", DangerLevel::Low),
            ("medium", DangerLevel::Medium),
            ("high", DangerLevel::High),
            ("critical", DangerLevel::Critical),
        ] {
            let mut spec = create_test_spec(name);
            spec.danger_level = level.clone();
            cache.save_spec(name, &spec).await.unwrap();

            let retrieved = cache.get_spec(name).await.unwrap().unwrap();
            assert_eq!(retrieved.danger_level, level);
        }
    }

    #[tokio::test]
    async fn test_spec_with_positionals_first() {
        let (cache, _temp) = create_test_cache().await;

        let mut spec = create_test_spec("find");
        spec.positionals_first = true;

        cache.save_spec("find", &spec).await.unwrap();

        let retrieved = cache.get_spec("find").await.unwrap().unwrap();
        assert!(retrieved.positionals_first);
    }

    #[tokio::test]
    async fn test_spec_with_subcommands() {
        let (cache, _temp) = create_test_cache().await;

        let mut spec = create_test_spec("git");
        spec.subcommands = vec!["commit".to_string(), "push".to_string(), "pull".to_string()];

        cache.save_spec("git", &spec).await.unwrap();

        let retrieved = cache.get_spec("git").await.unwrap().unwrap();
        assert_eq!(retrieved.subcommands, vec!["commit", "push", "pull"]);
    }

    #[tokio::test]
    async fn test_spec_with_examples() {
        let (cache, _temp) = create_test_cache().await;

        let mut spec = create_test_spec("curl");
        spec.examples = vec![
            "curl https://example.com".to_string(),
            "curl -X POST -d 'data' https://api.example.com".to_string(),
        ];

        cache.save_spec("curl", &spec).await.unwrap();

        let retrieved = cache.get_spec("curl").await.unwrap().unwrap();
        assert_eq!(retrieved.examples.len(), 2);
    }

    #[tokio::test]
    async fn test_values_with_special_characters() {
        let (cache, _temp) = create_test_cache().await;
        let spec = create_spec_with_options();

        let mut values = HashMap::new();
        values.insert("--output".to_string(), "/path/with spaces/and'quotes".to_string());

        cache.save_values("test", &values, &spec.options).await.unwrap();

        let retrieved = cache.get_values("test").await.unwrap();
        assert_eq!(
            retrieved.get("--output"),
            Some(&"/path/with spaces/and'quotes".to_string())
        );
    }

    #[tokio::test]
    async fn test_concurrent_operations() {
        let (cache, _temp) = create_test_cache().await;

        // Perform multiple concurrent saves
        let futures: Vec<_> = (0..10)
            .map(|i| {
                let cache = &cache;
                async move {
                    let spec = create_test_spec(&format!("cmd{}", i));
                    cache.save_spec(&format!("cmd{}", i), &spec).await
                }
            })
            .collect();

        let results = futures::future::join_all(futures).await;
        for result in results {
            assert!(result.is_ok());
        }

        // Verify all were saved
        for i in 0..10 {
            let spec = cache.get_spec(&format!("cmd{}", i)).await.unwrap();
            assert!(spec.is_some());
        }
    }
}
