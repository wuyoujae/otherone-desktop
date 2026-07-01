use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderConfig {
    pub id: String,
    pub name: String,
    pub provider: String,
    pub official_url: String,
    pub base_url: String,
    pub api_key: String,
    pub models: Vec<ModelConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelConfig {
    pub id: String,
    pub name: String,
    pub context_length: i64,
    pub context_window: i64,
    pub threshold_percentage: f64,
    pub max_iterations: i64,
    pub temperature: f64,
    pub top_p: f64,
    pub stream: bool,
    pub parallel_tool_calls: bool,
    pub tool_choice: String,
    pub extra_params: String,
    pub reasoning_effort: String,
    pub default_model: bool,
}

pub fn load_api_configs(data_root: &Path) -> Result<Vec<ProviderConfig>, String> {
    let conn = open_database(data_root)?;
    init_database(&conn)?;
    load_api_configs_from_connection(&conn)
}

pub fn save_api_configs(data_root: &Path, providers: Vec<ProviderConfig>) -> Result<(), String> {
    let mut conn = open_database(data_root)?;
    init_database(&conn)?;
    save_api_configs_to_connection(&mut conn, &providers)
}

pub fn load_api_configs_from_connection(conn: &Connection) -> Result<Vec<ProviderConfig>, String> {
    let mut provider_stmt = conn
        .prepare(
            "SELECT id, name, provider_type, official_url, base_url, api_key
             FROM api_providers
             ORDER BY sort_order ASC, rowid ASC",
        )
        .map_err(|error| error.to_string())?;

    let provider_rows = provider_stmt
        .query_map([], |row| {
            Ok(ProviderConfig {
                id: row.get(0)?,
                name: row.get(1)?,
                provider: row.get(2)?,
                official_url: row.get(3)?,
                base_url: row.get(4)?,
                api_key: row.get(5)?,
                models: Vec::new(),
            })
        })
        .map_err(|error| error.to_string())?;

    let mut providers = Vec::new();

    for provider_result in provider_rows {
        let mut provider = provider_result.map_err(|error| error.to_string())?;
        provider.models = load_models(conn, &provider.id)?;
        providers.push(provider);
    }

    Ok(providers)
}

pub fn save_api_configs_to_connection(
    conn: &mut Connection,
    providers: &[ProviderConfig],
) -> Result<(), String> {
    let tx = conn.transaction().map_err(|error| error.to_string())?;
    tx.execute("DELETE FROM api_models", [])
        .map_err(|error| error.to_string())?;
    tx.execute("DELETE FROM api_providers", [])
        .map_err(|error| error.to_string())?;

    for (provider_index, provider) in providers.iter().enumerate() {
        tx.execute(
            "INSERT INTO api_providers
             (id, name, provider_type, official_url, base_url, api_key, sort_order)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                provider.id,
                provider.name,
                provider.provider,
                provider.official_url,
                provider.base_url,
                provider.api_key,
                provider_index as i64
            ],
        )
        .map_err(|error| error.to_string())?;

        for (model_index, model) in provider.models.iter().enumerate() {
            tx.execute(
                "INSERT INTO api_models
                 (
                   id, provider_id, name, context_length, context_window,
                   threshold_percentage, max_iterations, temperature, top_p,
                   stream, parallel_tool_calls, tool_choice, extra_params,
                   reasoning_effort, default_model, sort_order
                 )
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)",
                params![
                    model.id,
                    provider.id,
                    model.name,
                    model.context_length,
                    model.context_window,
                    model.threshold_percentage,
                    model.max_iterations,
                    model.temperature,
                    model.top_p,
                    bool_to_int(model.stream),
                    bool_to_int(model.parallel_tool_calls),
                    model.tool_choice,
                    model.extra_params,
                    model.reasoning_effort,
                    bool_to_int(model.default_model),
                    model_index as i64
                ],
            )
            .map_err(|error| error.to_string())?;
        }
    }

    tx.commit().map_err(|error| error.to_string())
}

pub fn open_database(data_root: &Path) -> Result<Connection, String> {
    let db_path = database_path(data_root);
    let db_dir = db_path
        .parent()
        .ok_or_else(|| "Failed to resolve database directory".to_string())?;

    fs::create_dir_all(db_dir).map_err(|error| error.to_string())?;
    Connection::open(db_path).map_err(|error| error.to_string())
}

pub fn database_path(data_root: &Path) -> PathBuf {
    data_root.join("otherone.sqlite")
}

pub fn init_database(conn: &Connection) -> Result<(), String> {
    conn.execute_batch(
        "
        PRAGMA foreign_keys = ON;

        CREATE TABLE IF NOT EXISTS api_providers (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            provider_type TEXT NOT NULL,
            official_url TEXT NOT NULL DEFAULT '',
            base_url TEXT NOT NULL DEFAULT '',
            api_key TEXT NOT NULL DEFAULT '',
            sort_order INTEGER NOT NULL DEFAULT 0,
            updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
        );

        CREATE TABLE IF NOT EXISTS api_models (
            id TEXT PRIMARY KEY,
            provider_id TEXT NOT NULL,
            name TEXT NOT NULL,
            context_length INTEGER NOT NULL,
            context_window INTEGER NOT NULL,
            threshold_percentage REAL NOT NULL,
            max_iterations INTEGER NOT NULL,
            temperature REAL NOT NULL,
            top_p REAL NOT NULL,
            stream INTEGER NOT NULL,
            parallel_tool_calls INTEGER NOT NULL,
            tool_choice TEXT NOT NULL DEFAULT 'auto',
            extra_params TEXT NOT NULL DEFAULT '',
            reasoning_effort TEXT NOT NULL,
            default_model INTEGER NOT NULL,
            sort_order INTEGER NOT NULL DEFAULT 0,
            updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
            FOREIGN KEY(provider_id) REFERENCES api_providers(id) ON DELETE CASCADE
        );
        ",
    )
    .map_err(|error| error.to_string())?;

    ensure_column(
        conn,
        "api_models",
        "tool_choice",
        "ALTER TABLE api_models ADD COLUMN tool_choice TEXT NOT NULL DEFAULT 'auto'",
    )?;
    ensure_column(
        conn,
        "api_models",
        "extra_params",
        "ALTER TABLE api_models ADD COLUMN extra_params TEXT NOT NULL DEFAULT ''",
    )?;
    Ok(())
}

fn load_models(conn: &Connection, provider_id: &str) -> Result<Vec<ModelConfig>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT
               id, name, context_length, context_window, threshold_percentage,
               max_iterations, temperature, top_p, stream, parallel_tool_calls,
               tool_choice, extra_params, reasoning_effort, default_model
             FROM api_models
             WHERE provider_id = ?1
             ORDER BY sort_order ASC, rowid ASC",
        )
        .map_err(|error| error.to_string())?;

    let rows = stmt
        .query_map([provider_id], |row| {
            Ok(ModelConfig {
                id: row.get(0)?,
                name: row.get(1)?,
                context_length: row.get(2)?,
                context_window: row.get(3)?,
                threshold_percentage: row.get(4)?,
                max_iterations: row.get(5)?,
                temperature: row.get(6)?,
                top_p: row.get(7)?,
                stream: int_to_bool(row.get(8)?),
                parallel_tool_calls: int_to_bool(row.get(9)?),
                tool_choice: row.get(10)?,
                extra_params: row.get(11)?,
                reasoning_effort: row.get(12)?,
                default_model: int_to_bool(row.get(13)?),
            })
        })
        .map_err(|error| error.to_string())?;

    let mut models = Vec::new();

    for row in rows {
        models.push(row.map_err(|error| error.to_string())?);
    }

    Ok(models)
}

fn ensure_column(
    conn: &Connection,
    table_name: &str,
    column_name: &str,
    alter_statement: &str,
) -> Result<(), String> {
    let mut stmt = conn
        .prepare(&format!("PRAGMA table_info({table_name})"))
        .map_err(|error| error.to_string())?;
    let columns = stmt
        .query_map([], |row| row.get::<_, String>(1))
        .map_err(|error| error.to_string())?;

    for column in columns {
        if column.map_err(|error| error.to_string())? == column_name {
            return Ok(());
        }
    }

    conn.execute(alter_statement, [])
        .map_err(|error| error.to_string())?;
    Ok(())
}

fn bool_to_int(value: bool) -> i64 {
    if value {
        1
    } else {
        0
    }
}

fn int_to_bool(value: i64) -> bool {
    value != 0
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn test_dir(name: &str) -> PathBuf {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let path = std::env::temp_dir().join(format!("otherone-core-{name}-{suffix}"));
        std::fs::create_dir_all(&path).expect("create test dir");
        path
    }

    #[test]
    fn saves_and_loads_provider_models() {
        let temp = test_dir("storage-config");
        let provider = ProviderConfig {
            id: "provider-1".to_string(),
            name: "OpenAI".to_string(),
            provider: "OpenAI".to_string(),
            official_url: "https://openai.com".to_string(),
            base_url: "https://api.openai.com/v1".to_string(),
            api_key: "test-key".to_string(),
            models: vec![ModelConfig {
                id: "model-1".to_string(),
                name: "gpt-test".to_string(),
                context_length: 128_000,
                context_window: 16_000,
                threshold_percentage: 0.8,
                max_iterations: 8,
                temperature: 0.7,
                top_p: 1.0,
                stream: true,
                parallel_tool_calls: true,
                tool_choice: "auto".to_string(),
                extra_params: "{}".to_string(),
                reasoning_effort: "medium".to_string(),
                default_model: true,
            }],
        };

        save_api_configs(&temp, vec![provider.clone()]).expect("save configs");
        let providers = load_api_configs(&temp).expect("load configs");

        assert_eq!(providers.len(), 1);
        assert_eq!(providers[0].id, provider.id);
        assert_eq!(providers[0].models.len(), 1);
        assert_eq!(providers[0].models[0].name, "gpt-test");
        assert!(database_path(&temp).exists());
    }
}
