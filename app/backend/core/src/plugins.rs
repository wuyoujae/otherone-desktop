use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

use crate::storage;

const MAX_IMPORTED_SKILL_MD_BYTES: u64 = 1024 * 1024;
const MAX_IMPORTED_MCP_CONFIG_BYTES: u64 = 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginEntry {
    pub id: String,
    pub name: String,
    pub description: String,
    pub kind: String,
    pub source: String,
    pub installed: bool,
    pub file_path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub has_binary: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bin_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bin_dir: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginInstallRequest {
    pub plugin_name: String,
    pub kind: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportSkillFromUrlRequest {
    pub url: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportMcpServersRequest {
    pub raw_config: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportMcpServersFromUrlRequest {
    pub url: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillContent {
    pub name: String,
    pub description: String,
    pub file_path: String,
    pub body: String,
}

#[derive(Debug, Clone)]
struct BuiltinPluginDefinition {
    name: &'static str,
    description: &'static str,
    has_binary: bool,
}

#[derive(Debug, Clone)]
struct ImportedMcpServer {
    name: String,
    transport: String,
    description: String,
    config_json: String,
}

pub fn init_plugin_database(conn: &Connection) -> Result<(), String> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS plugin_installs (
            name TEXT NOT NULL,
            kind TEXT NOT NULL DEFAULT 'skill',
            installed_at TEXT NOT NULL DEFAULT (datetime('now')),
            PRIMARY KEY (name, kind)
        );

        CREATE TABLE IF NOT EXISTS plugin_install_defaults (
            key TEXT NOT NULL PRIMARY KEY,
            applied_at TEXT NOT NULL DEFAULT (datetime('now'))
        );

        CREATE TABLE IF NOT EXISTS mcp_servers (
            name TEXT NOT NULL PRIMARY KEY,
            transport TEXT NOT NULL,
            description TEXT NOT NULL DEFAULT '',
            config_json TEXT NOT NULL,
            source TEXT NOT NULL DEFAULT 'imported',
            imported_at TEXT NOT NULL DEFAULT (datetime('now'))
        );",
    )
    .map_err(|error| format!("初始化插件表失败: {error}"))
}

pub fn load_plugin_list(data_root: &Path) -> Result<Vec<PluginEntry>, String> {
    let conn = open_plugin_database(data_root)?;
    let installed = load_installed_plugins(&conn)?;
    let mut entries = Vec::new();

    for (index, definition) in builtin_plugins().iter().enumerate() {
        let install_dir = plugin_install_dir(data_root, definition.name);
        let bin_dir = plugin_bin_dir(data_root, definition.name);
        entries.push(PluginEntry {
            id: format!("plugin-builtin-{index}"),
            name: definition.name.to_string(),
            description: definition.description.to_string(),
            kind: "plugin".to_string(),
            source: "builtin".to_string(),
            installed: installed.contains(&(definition.name.to_string(), "plugin".to_string())),
            file_path: install_dir.join("SKILL.md").to_string_lossy().to_string(),
            has_binary: Some(definition.has_binary),
            bin_path: definition
                .has_binary
                .then(|| bin_dir.join(binary_name()).to_string_lossy().to_string()),
            bin_dir: definition
                .has_binary
                .then(|| bin_dir.to_string_lossy().to_string()),
        });
    }

    for (index, (name, description, file_path)) in
        discover_imported_skills(data_root).into_iter().enumerate()
    {
        entries.push(PluginEntry {
            id: format!("skill-imported-{index}"),
            name: name.clone(),
            description,
            kind: "skill".to_string(),
            source: "imported".to_string(),
            installed: installed.contains(&(name, "skill".to_string())),
            file_path,
            has_binary: None,
            bin_path: None,
            bin_dir: None,
        });
    }

    for (index, server) in load_mcp_servers(&conn)?.into_iter().enumerate() {
        entries.push(PluginEntry {
            id: format!("mcp-imported-{index}"),
            name: server.name.clone(),
            description: server.description,
            kind: "mcp".to_string(),
            source: "imported".to_string(),
            installed: installed.contains(&(server.name.clone(), "mcp".to_string())),
            file_path: format!("mcp://{}", server.name),
            has_binary: None,
            bin_path: None,
            bin_dir: None,
        });
    }

    Ok(entries)
}

pub fn install_plugin(data_root: &Path, request: PluginInstallRequest) -> Result<(), String> {
    let plugin_name = require_text(&request.plugin_name, "pluginName")?.to_string();
    let kind = require_kind(&request.kind)?.to_string();
    let entries = load_plugin_list(data_root)?;

    if !entries
        .iter()
        .any(|entry| entry.name == plugin_name && entry.kind == kind)
    {
        return Err(format!("未找到名称为 '{plugin_name}' 的 {kind}。"));
    }

    let conn = open_plugin_database(data_root)?;
    conn.execute(
        "INSERT OR REPLACE INTO plugin_installs (name, kind) VALUES (?1, ?2)",
        params![plugin_name, kind],
    )
    .map_err(|error| format!("保存插件安装记录失败: {error}"))?;
    Ok(())
}

pub fn uninstall_plugin(data_root: &Path, request: PluginInstallRequest) -> Result<(), String> {
    let plugin_name = require_text(&request.plugin_name, "pluginName")?;
    let kind = require_kind(&request.kind)?;
    let conn = open_plugin_database(data_root)?;
    conn.execute(
        "DELETE FROM plugin_installs WHERE name = ?1 AND kind = ?2",
        params![plugin_name, kind],
    )
    .map_err(|error| format!("删除插件安装记录失败: {error}"))?;
    Ok(())
}

pub fn import_skill_from_url(
    data_root: &Path,
    request: ImportSkillFromUrlRequest,
) -> Result<PluginEntry, String> {
    let raw = download_text(
        &normalize_http_url(&request.url, "Skill URL")?,
        MAX_IMPORTED_SKILL_MD_BYTES,
    )?;
    let (name, description) = parse_imported_skill_metadata(&raw)?;

    if load_plugin_list(data_root)?
        .iter()
        .any(|entry| entry.kind == "skill" && entry.name == name)
    {
        return Err(format!(
            "已存在名为 '{name}' 的 Skill。请修改远程 SKILL.md 的 name 后再导入。"
        ));
    }

    let import_root = imported_skills_dir(data_root);
    std::fs::create_dir_all(&import_root)
        .map_err(|error| format!("创建 Skill 导入目录失败: {error}"))?;
    let dest = import_root.join(&name);
    if dest.exists() {
        return Err(format!("导入目录已存在: {}", dest.display()));
    }

    std::fs::create_dir_all(&dest).map_err(|error| format!("创建 Skill 目录失败: {error}"))?;
    let skill_path = dest.join("SKILL.md");
    std::fs::write(&skill_path, raw.as_bytes())
        .map_err(|error| format!("写入远程 SKILL.md 失败: {error}"))?;

    let conn = open_plugin_database(data_root)?;
    conn.execute(
        "INSERT OR REPLACE INTO plugin_installs (name, kind) VALUES (?1, 'skill')",
        params![name],
    )
    .map_err(|error| format!("保存 Skill 安装记录失败: {error}"))?;

    Ok(PluginEntry {
        id: format!("skill-imported-{}", name),
        name,
        description,
        kind: "skill".to_string(),
        source: "imported".to_string(),
        installed: true,
        file_path: skill_path.to_string_lossy().to_string(),
        has_binary: None,
        bin_path: None,
        bin_dir: None,
    })
}

pub fn import_mcp_servers(
    data_root: &Path,
    request: ImportMcpServersRequest,
) -> Result<Vec<PluginEntry>, String> {
    let servers = parse_mcp_import(&request.raw_config)?;
    import_mcp_servers_from_parsed(data_root, servers)
}

pub fn import_mcp_servers_from_url(
    data_root: &Path,
    request: ImportMcpServersFromUrlRequest,
) -> Result<Vec<PluginEntry>, String> {
    let raw = download_text(
        &normalize_http_url(&request.url, "MCP 配置 URL")?,
        MAX_IMPORTED_MCP_CONFIG_BYTES,
    )?;
    import_mcp_servers(data_root, ImportMcpServersRequest { raw_config: raw })
}

pub fn get_installed_skill_entries(data_root: &Path) -> Result<Vec<PluginEntry>, String> {
    Ok(load_plugin_list(data_root)?
        .into_iter()
        .filter(|entry| entry.installed && matches!(entry.kind.as_str(), "skill" | "plugin"))
        .collect())
}

pub fn get_installed_skills_content(data_root: &Path) -> Result<Vec<SkillContent>, String> {
    let entries = get_installed_skill_entries(data_root)?;
    let mut contents = Vec::new();

    for entry in entries.into_iter().filter(|entry| entry.kind == "skill") {
        if let Ok(body) = std::fs::read_to_string(&entry.file_path) {
            contents.push(SkillContent {
                name: entry.name,
                description: entry.description,
                file_path: entry.file_path,
                body,
            });
        }
    }

    Ok(contents)
}

pub fn get_plugin_metadata(data_root: &Path, name: &str) -> Result<Option<PluginEntry>, String> {
    let name = name.trim();
    if name.is_empty() {
        return Ok(None);
    }

    Ok(load_plugin_list(data_root)?
        .into_iter()
        .find(|entry| entry.kind == "plugin" && entry.installed && entry.name == name))
}

pub fn get_plugin_body(data_root: &Path, name: &str) -> Result<Option<String>, String> {
    let Some(entry) = get_plugin_metadata(data_root, name)? else {
        return Ok(None);
    };

    Ok(std::fs::read_to_string(entry.file_path).ok())
}

pub fn format_skills_for_prompt(data_root: &Path) -> Result<String, String> {
    let entries = get_installed_skill_entries(data_root)?;
    if entries.is_empty() {
        return Ok(String::new());
    }

    let skills: Vec<_> = entries
        .iter()
        .filter(|entry| entry.kind == "skill")
        .collect();
    let plugins: Vec<_> = entries
        .iter()
        .filter(|entry| entry.kind == "plugin")
        .collect();
    let mut lines = vec![
        String::new(),
        "The following capabilities provide specialized instructions for specific tasks."
            .to_string(),
    ];

    if !skills.is_empty() {
        lines.push(String::new());
        lines.push("## Skills".to_string());
        lines.push("Use the <function>skill</function> tool to load a skill's full instructions when a task matches its description.".to_string());
        lines.push(String::new());
        lines.push("<available_skills>".to_string());
        for skill in skills {
            lines.push("  <skill>".to_string());
            lines.push(format!("    <name>{}</name>", escape_xml(&skill.name)));
            lines.push(format!(
                "    <description>{}</description>",
                escape_xml(&skill.description)
            ));
            lines.push(format!(
                "    <location>{}</location>",
                escape_xml(&skill.file_path)
            ));
            lines.push("  </skill>".to_string());
        }
        lines.push("</available_skills>".to_string());
    }

    if !plugins.is_empty() {
        lines.push(String::new());
        lines.push("## Plugins".to_string());
        lines.push("Use the <function>plugin_tool</function> to load a plugin's full instructions and resource paths.".to_string());
        lines.push(String::new());
        lines.push("<available_plugins>".to_string());
        for plugin in plugins {
            lines.push("  <plugin>".to_string());
            lines.push(format!("    <name>{}</name>", escape_xml(&plugin.name)));
            lines.push(format!(
                "    <description>{}</description>",
                escape_xml(&plugin.description)
            ));
            if let Some(bin_path) = plugin.bin_path.as_ref() {
                lines.push(format!("    <binary>{}</binary>", escape_xml(bin_path)));
            }
            if let Some(bin_dir) = plugin.bin_dir.as_ref() {
                lines.push(format!("    <bin_dir>{}</bin_dir>", escape_xml(bin_dir)));
            }
            lines.push("  </plugin>".to_string());
        }
        lines.push("</available_plugins>".to_string());
    }

    Ok(lines.join("\n"))
}

fn import_mcp_servers_from_parsed(
    data_root: &Path,
    servers: Vec<ImportedMcpServer>,
) -> Result<Vec<PluginEntry>, String> {
    let conn = open_plugin_database(data_root)?;
    let existing = load_plugin_list(data_root)?;

    for server in &servers {
        if existing
            .iter()
            .any(|entry| entry.kind == "mcp" && entry.name == server.name)
        {
            return Err(format!(
                "已存在名为 '{}' 的 MCP server，请修改 name 后再导入。",
                server.name
            ));
        }
    }

    for server in &servers {
        conn.execute(
            "INSERT INTO mcp_servers (name, transport, description, config_json, source)
             VALUES (?1, ?2, ?3, ?4, 'imported')",
            params![
                server.name,
                server.transport,
                server.description,
                server.config_json
            ],
        )
        .map_err(|error| format!("保存 MCP server '{}' 失败: {error}", server.name))?;
        conn.execute(
            "INSERT OR REPLACE INTO plugin_installs (name, kind) VALUES (?1, 'mcp')",
            params![server.name],
        )
        .map_err(|error| format!("保存 MCP server '{}' 安装记录失败: {error}", server.name))?;
    }

    Ok(servers
        .into_iter()
        .enumerate()
        .map(|(index, server)| PluginEntry {
            id: format!("mcp-imported-{index}"),
            name: server.name.clone(),
            description: server.description,
            kind: "mcp".to_string(),
            source: "imported".to_string(),
            installed: true,
            file_path: format!("mcp://{}", server.name),
            has_binary: None,
            bin_path: None,
            bin_dir: None,
        })
        .collect())
}

fn open_plugin_database(data_root: &Path) -> Result<Connection, String> {
    let conn = storage::open_database(data_root)?;
    storage::init_database(&conn)?;
    init_plugin_database(&conn)?;
    Ok(conn)
}

fn load_installed_plugins(conn: &Connection) -> Result<HashSet<(String, String)>, String> {
    let mut stmt = conn
        .prepare("SELECT name, kind FROM plugin_installs")
        .map_err(|error| error.to_string())?;
    let rows = stmt
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(|error| error.to_string())?;
    let mut installed = HashSet::new();

    for row in rows {
        installed.insert(row.map_err(|error| error.to_string())?);
    }

    Ok(installed)
}

fn load_mcp_servers(conn: &Connection) -> Result<Vec<ImportedMcpServer>, String> {
    let mut stmt = conn
        .prepare("SELECT name, transport, description, config_json FROM mcp_servers ORDER BY imported_at DESC, name ASC")
        .map_err(|error| error.to_string())?;
    let rows = stmt
        .query_map([], |row| {
            Ok(ImportedMcpServer {
                name: row.get(0)?,
                transport: row.get(1)?,
                description: row.get(2)?,
                config_json: row.get(3)?,
            })
        })
        .map_err(|error| error.to_string())?;
    let mut servers = Vec::new();

    for row in rows {
        servers.push(row.map_err(|error| error.to_string())?);
    }

    Ok(servers)
}

fn builtin_plugins() -> Vec<BuiltinPluginDefinition> {
    vec![BuiltinPluginDefinition {
        name: "officecli",
        description: "在 AI 中创建、分析、校对和修改 Office 文档（.docx/.xlsx/.pptx）。",
        has_binary: true,
    }]
}

fn imported_skills_dir(data_root: &Path) -> PathBuf {
    data_root.join("skills").join("imported")
}

fn plugin_install_dir(data_root: &Path, plugin_name: &str) -> PathBuf {
    data_root.join("plugins").join(plugin_name)
}

fn plugin_bin_dir(data_root: &Path, plugin_name: &str) -> PathBuf {
    plugin_install_dir(data_root, plugin_name).join("bin")
}

fn binary_name() -> &'static str {
    if cfg!(target_os = "windows") {
        "officecli.exe"
    } else {
        "officecli"
    }
}

fn escape_xml(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

fn discover_imported_skills(data_root: &Path) -> Vec<(String, String, String)> {
    discover_skills_in_dir(&imported_skills_dir(data_root))
}

fn discover_skills_in_dir(dir: &Path) -> Vec<(String, String, String)> {
    let mut results = Vec::new();
    if !dir.exists() || !dir.is_dir() {
        return results;
    }

    let Ok(entries) = std::fs::read_dir(dir) else {
        return results;
    };

    for entry in entries.flatten() {
        let name_str = entry.file_name().to_string_lossy().to_string();
        if name_str.starts_with('.') || name_str == "node_modules" {
            continue;
        }

        let path = entry.path();
        if !path.is_dir() {
            continue;
        }

        let skill_md = path.join("SKILL.md");
        if skill_md.exists() && skill_md.is_file() {
            if let Ok(raw) = std::fs::read_to_string(&skill_md) {
                let (name, description) = parse_skill_frontmatter(&raw);
                results.push((
                    name.unwrap_or(name_str),
                    description.unwrap_or_default(),
                    skill_md.to_string_lossy().to_string(),
                ));
            }
        } else {
            results.extend(discover_skills_in_dir(&path));
        }
    }

    results
}

fn parse_skill_frontmatter(raw: &str) -> (Option<String>, Option<String>) {
    let content = raw.trim_start();
    if !content.starts_with("---") {
        return (None, None);
    }
    let rest = &content[3..];
    let Some(end_index) = rest.find("\n---") else {
        return (None, None);
    };
    let frontmatter = rest[..end_index].trim();
    let mut name = None;
    let mut description = None;

    for line in frontmatter.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some((key, value)) = line.split_once(':') {
            let value = value.trim().trim_matches('"').trim_matches('\'');
            match key.trim() {
                "name" => name = Some(value.to_string()),
                "description" => description = Some(value.to_string()),
                _ => {}
            }
        }
    }

    (name, description)
}

fn parse_imported_skill_metadata(raw: &str) -> Result<(String, String), String> {
    let (name, description) = parse_skill_frontmatter(raw);
    let name = name.ok_or_else(|| "SKILL.md frontmatter 缺少 name。".to_string())?;
    let description =
        description.ok_or_else(|| "SKILL.md frontmatter 缺少 description。".to_string())?;

    validate_skill_name(&name)?;
    if description.trim().is_empty() {
        return Err("SKILL.md frontmatter 的 description 不能为空。".to_string());
    }
    if description.len() > 1024 {
        return Err("SKILL.md frontmatter 的 description 不能超过 1024 个字符。".to_string());
    }

    Ok((name, description))
}

fn validate_skill_name(name: &str) -> Result<(), String> {
    if name.is_empty() || name.len() > 64 {
        return Err("Skill name 必须为 1 到 64 个字符。".to_string());
    }
    if name.starts_with('-') || name.ends_with('-') {
        return Err("Skill name 不能以连字符开头或结尾。".to_string());
    }
    if !name
        .chars()
        .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '-')
    {
        return Err("Skill name 只能包含小写字母、数字和连字符。".to_string());
    }
    Ok(())
}

fn parse_mcp_import(raw_config: &str) -> Result<Vec<ImportedMcpServer>, String> {
    let raw_config = require_text(raw_config, "MCP 配置")?;
    if raw_config.len() as u64 > MAX_IMPORTED_MCP_CONFIG_BYTES {
        return Err("MCP 配置超过 1MB。".to_string());
    }

    let value: Value =
        serde_json::from_str(raw_config).map_err(|error| format!("MCP JSON 配置无效: {error}"))?;
    let root = value
        .get("mcpServers")
        .or_else(|| value.get("servers"))
        .unwrap_or(&value);
    let object = root
        .as_object()
        .ok_or_else(|| "MCP 配置必须是对象。".to_string())?;
    let mut servers = Vec::new();

    for (name, config) in object {
        servers.push(parse_one_mcp_server(name, config)?);
    }

    if servers.is_empty() {
        return Err("MCP 配置中没有可导入的 server。".to_string());
    }

    Ok(servers)
}

fn parse_one_mcp_server(raw_name: &str, raw_config: &Value) -> Result<ImportedMcpServer, String> {
    let name = require_text(raw_name, "MCP server name")?.to_string();
    validate_mcp_name(&name)?;
    let object = raw_config
        .as_object()
        .ok_or_else(|| format!("MCP server '{name}' 的配置必须是对象。"))?;
    let transport = infer_mcp_transport(object)?;

    match transport.as_str() {
        "stdio" => {
            let command = object
                .get("command")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| format!("MCP server '{name}' 缺少 command。"))?;
            if command.contains('\0') {
                return Err(format!("MCP server '{name}' 的 command 无效。"));
            }
            validate_string_array_field(object, "args")?;
            validate_string_object_field(object, "env")?;
        }
        "http" | "sse" => {
            let url = object
                .get("url")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| format!("MCP server '{name}' 缺少 url。"))?;
            validate_http_mcp_url(url)?;
            validate_string_object_field(object, "headers")?;
        }
        _ => return Err(format!("暂不支持 MCP transport '{transport}'。")),
    }

    Ok(ImportedMcpServer {
        name: name.clone(),
        transport: transport.clone(),
        description: mcp_description(&name, &transport, object),
        config_json: serde_json::to_string(raw_config)
            .map_err(|error| format!("序列化 MCP server '{name}' 失败: {error}"))?,
    })
}

fn validate_mcp_name(name: &str) -> Result<(), String> {
    if name.is_empty() || name.len() > 64 {
        return Err("MCP server name 必须为 1 到 64 个字符。".to_string());
    }
    if name.starts_with('.') || name.starts_with('-') || name.ends_with('-') {
        return Err("MCP server name 不能以点或连字符开头，也不能以连字符结尾。".to_string());
    }
    if !name
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' || ch == '.')
    {
        return Err("MCP server name 只能包含字母、数字、点、下划线和连字符。".to_string());
    }
    Ok(())
}

fn infer_mcp_transport(object: &serde_json::Map<String, Value>) -> Result<String, String> {
    let raw_type = object
        .get("type")
        .or_else(|| object.get("transport"))
        .and_then(Value::as_str)
        .map(|value| value.trim().to_ascii_lowercase());

    let transport = match raw_type.as_deref() {
        Some("stdio") => "stdio",
        Some("http") | Some("streamable-http") => "http",
        Some("sse") => "sse",
        Some(other) => return Err(format!("暂不支持 MCP transport '{other}'。")),
        None if object.contains_key("command") => "stdio",
        None if object.contains_key("url") => "http",
        None => return Err("MCP server 配置必须包含 command 或 url。".to_string()),
    };

    Ok(transport.to_string())
}

fn validate_string_array_field(
    object: &serde_json::Map<String, Value>,
    field: &str,
) -> Result<(), String> {
    if let Some(value) = object.get(field) {
        let values = value
            .as_array()
            .ok_or_else(|| format!("MCP field '{field}' 必须是字符串数组。"))?;
        if values.iter().any(|item| !item.is_string()) {
            return Err(format!("MCP field '{field}' 必须只包含字符串。"));
        }
    }
    Ok(())
}

fn validate_string_object_field(
    object: &serde_json::Map<String, Value>,
    field: &str,
) -> Result<(), String> {
    if let Some(value) = object.get(field) {
        let values = value
            .as_object()
            .ok_or_else(|| format!("MCP field '{field}' 必须是对象。"))?;
        if values.values().any(|item| !item.is_string()) {
            return Err(format!("MCP field '{field}' 的值必须都是字符串。"));
        }
    }
    Ok(())
}

fn validate_http_mcp_url(url: &str) -> Result<(), String> {
    let parsed = reqwest::Url::parse(url).map_err(|_| "MCP URL 格式无效。".to_string())?;
    match parsed.scheme() {
        "http" | "https" => Ok(()),
        _ => Err("MCP URL 只支持 http 或 https。".to_string()),
    }
}

fn mcp_description(name: &str, transport: &str, object: &serde_json::Map<String, Value>) -> String {
    if let Some(description) = object.get("description").and_then(Value::as_str) {
        let description = description.trim();
        if !description.is_empty() {
            return description.chars().take(1024).collect();
        }
    }

    match transport {
        "stdio" => object
            .get("command")
            .and_then(Value::as_str)
            .map(|command| format!("stdio MCP server: {command}"))
            .unwrap_or_else(|| format!("stdio MCP server: {name}")),
        "http" | "sse" => object
            .get("url")
            .and_then(Value::as_str)
            .and_then(|url| reqwest::Url::parse(url).ok())
            .and_then(|url| url.host_str().map(ToString::to_string))
            .map(|host| format!("{transport} MCP server: {host}"))
            .unwrap_or_else(|| format!("{transport} MCP server: {name}")),
        _ => format!("MCP server: {name}"),
    }
}

fn normalize_http_url(raw_url: &str, label: &str) -> Result<String, String> {
    let trimmed = require_text(raw_url, label)?;
    let parsed = reqwest::Url::parse(trimmed).map_err(|_| format!("{label} 格式无效。"))?;
    match parsed.scheme() {
        "http" | "https" => {}
        _ => return Err(format!("{label} 只支持 http 或 https。")),
    }

    if parsed.host_str() == Some("github.com") {
        let segments: Vec<&str> = parsed
            .path_segments()
            .map(|items| items.collect())
            .unwrap_or_default();
        if segments.len() >= 5 && segments[2] == "blob" {
            let rest = segments[4..].join("/");
            return Ok(format!(
                "https://raw.githubusercontent.com/{}/{}/{}/{}",
                segments[0], segments[1], segments[3], rest
            ));
        }
    }

    Ok(parsed.to_string())
}

fn download_text(url: &str, max_bytes: u64) -> Result<String, String> {
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .user_agent("otherone-web/0.1")
        .build()
        .map_err(|error| format!("创建 HTTP 客户端失败: {error}"))?;
    let response = client
        .get(url)
        .send()
        .map_err(|error| format!("下载 URL 失败: {error}"))?;

    if !response.status().is_success() {
        return Err(format!("下载 URL 失败: HTTP {}", response.status()));
    }
    if let Some(length) = response.content_length() {
        if length > max_bytes {
            return Err("远程文件超过大小限制，已拒绝导入。".to_string());
        }
    }

    let bytes = response
        .bytes()
        .map_err(|error| format!("读取 URL 响应失败: {error}"))?;
    if bytes.len() as u64 > max_bytes {
        return Err("远程文件超过大小限制，已拒绝导入。".to_string());
    }

    String::from_utf8(bytes.to_vec()).map_err(|_| "远程文件必须是 UTF-8 文本。".to_string())
}

fn require_text<'a>(value: &'a str, label: &str) -> Result<&'a str, String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        Err(format!("{label} 不能为空。"))
    } else {
        Ok(trimmed)
    }
}

fn require_kind(value: &str) -> Result<&str, String> {
    match require_text(value, "kind")? {
        "skill" => Ok("skill"),
        "plugin" => Ok("plugin"),
        "mcp" => Ok("mcp"),
        _ => Err("kind 只支持 skill、plugin 或 mcp。".to_string()),
    }
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
    fn parses_skill_frontmatter() {
        let raw = "---\nname: demo-skill\ndescription: Demo skill\n---\nBody";
        let (name, description) = parse_imported_skill_metadata(raw).expect("parse skill");

        assert_eq!(name, "demo-skill");
        assert_eq!(description, "Demo skill");
    }

    #[test]
    fn imports_mcp_servers_from_json() {
        let data_root = test_dir("plugins-mcp");
        let entries = import_mcp_servers(
            &data_root,
            ImportMcpServersRequest {
                raw_config: r#"{"mcpServers":{"demo":{"command":"node","args":["server.js"]}}}"#
                    .to_string(),
            },
        )
        .expect("import mcp");

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "demo");

        let list = load_plugin_list(&data_root).expect("load plugin list");
        assert!(list
            .iter()
            .any(|entry| entry.kind == "mcp" && entry.name == "demo" && entry.installed));
    }
}
