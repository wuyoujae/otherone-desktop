// 统一插件管理模块 — 管理 Skill（文件系统发现）、Plugin（下载安装）、MCP（未来）的生命周期
// 关联：被 main.rs 注册为 Tauri commands，被 chat.rs 读取注入 system prompt

use rusqlite::params;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::AppHandle;

use crate::plugin_registry;

const DEFAULT_SKILLS_INSTALL_MARKER: &str = "builtin_resource_skills_installed_v1";
const MAX_IMPORTED_SKILL_MD_BYTES: u64 = 1024 * 1024;
const MAX_IMPORTED_MCP_CONFIG_BYTES: u64 = 1024 * 1024;

// ---------------------------------------------------------------------------
// 数据模型
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginEntry {
    pub id: String,
    pub name: String,
    pub description: String,
    pub kind: String,   // "skill" | "plugin" | "mcp"
    pub source: String, // "imported" | "created" | "builtin"
    pub installed: bool,
    pub file_path: String,
    /// 是否有二进制文件
    #[serde(skip_serializing_if = "Option::is_none")]
    pub has_binary: Option<bool>,
    /// 二进制文件路径（安装后填充，供系统提示词用）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bin_path: Option<String>,
    /// 二进制文件所在目录（安装后填充）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bin_dir: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillContent {
    pub name: String,
    pub description: String,
    pub body: String,
    pub file_path: String,
}

// ---------------------------------------------------------------------------
// 全局状态
// ---------------------------------------------------------------------------

static PLUGINS_MANAGER: std::sync::LazyLock<Mutex<PluginsManager>> =
    std::sync::LazyLock::new(|| Mutex::new(PluginsManager::new()));

struct PluginsManager {
    /// 所有 skill 条目（文件系统发现）
    skill_entries: Vec<PluginEntry>,
    /// 所有 plugin 条目（注册表定义）
    plugin_entries: Vec<PluginEntry>,
    /// 所有 MCP server 条目（导入配置）
    mcp_entries: Vec<PluginEntry>,
    /// 已安装的 (name, kind) 集合
    installed: std::collections::HashSet<(String, String)>,
    /// 已缓存的技能内容 name → content
    content_cache: std::collections::HashMap<String, SkillContent>,
    initialized: bool,
}

impl PluginsManager {
    fn new() -> Self {
        PluginsManager {
            skill_entries: Vec::new(),
            plugin_entries: Vec::new(),
            mcp_entries: Vec::new(),
            installed: std::collections::HashSet::new(),
            content_cache: std::collections::HashMap::new(),
            initialized: false,
        }
    }

    fn all_entries(&self) -> Vec<PluginEntry> {
        let mut all = Vec::new();
        all.extend(self.skill_entries.clone());
        all.extend(self.plugin_entries.clone());
        all.extend(self.mcp_entries.clone());
        all
    }
}

// ---------------------------------------------------------------------------
// 技能发现（文件系统扫描）
// ---------------------------------------------------------------------------

fn parse_skill_frontmatter(raw: &str) -> (Option<String>, Option<String>) {
    let content = raw.trim_start();
    if !content.starts_with("---") {
        return (None, None);
    }
    let rest = &content[3..];
    let end_idx = match rest.find("\n---") {
        Some(i) => i,
        None => return (None, None),
    };
    let fm = rest[..end_idx].trim();
    let mut name = None;
    let mut description = None;
    for line in fm.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some((k, v)) = line.split_once(':') {
            let v = v.trim().trim_matches('"').trim_matches('\'');
            match k.trim() {
                "name" => name = Some(v.to_string()),
                "description" => description = Some(v.to_string()),
                _ => {}
            }
        }
    }
    (name, description)
}

fn discover_skills_in_dir(dir: &PathBuf) -> Vec<(String, String, String)> {
    let mut results = Vec::new();
    if !dir.exists() || !dir.is_dir() {
        return results;
    }
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return results,
    };
    for entry in entries.flatten() {
        let name_str = entry.file_name().to_string_lossy().to_string();
        if name_str.starts_with('.') || name_str == "node_modules" {
            continue;
        }
        let path = entry.path();
        if path.is_dir() {
            let skill_md = path.join("SKILL.md");
            if skill_md.exists() && skill_md.is_file() {
                if let Ok(raw) = std::fs::read_to_string(&skill_md) {
                    let (fm_name, fm_desc) = parse_skill_frontmatter(&raw);
                    results.push((
                        fm_name.unwrap_or(name_str),
                        fm_desc.unwrap_or_default(),
                        skill_md.to_string_lossy().to_string(),
                    ));
                }
            } else {
                results.extend(discover_skills_in_dir(&path));
            }
        }
    }
    results
}

fn read_skill_content(file_path: &str) -> SkillContent {
    let raw = std::fs::read_to_string(file_path).unwrap_or_default();
    let (name, description) = parse_skill_frontmatter(&raw);
    let body = if raw.trim_start().starts_with("---") {
        let rest = &raw.trim_start()[3..];
        if let Some(end_idx) = rest.find("\n---") {
            rest[end_idx + 4..].trim().to_string()
        } else {
            raw
        }
    } else {
        raw
    };
    SkillContent {
        name: name.unwrap_or_default(),
        description: description.unwrap_or_default(),
        body,
        file_path: file_path.to_string(),
    }
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

fn imported_skills_dir(data_root: &Path) -> PathBuf {
    data_root.join("skills").join("imported")
}

fn discover_imported_skills(app: &AppHandle) -> Vec<(String, String, String)> {
    let data_root = match crate::app_settings::data_root(app) {
        Ok(path) => path,
        Err(_) => return Vec::new(),
    };
    discover_skills_in_dir(&imported_skills_dir(&data_root))
}

fn dedupe_discovered_skills(
    discovered: Vec<(String, String, String)>,
) -> Vec<(String, String, String)> {
    let mut seen = std::collections::HashSet::new();
    discovered
        .into_iter()
        .filter(|(name, _, _)| seen.insert(name.clone()))
        .collect()
}

fn copy_skill_directory(source: &Path, dest: &Path) -> Result<(), String> {
    let metadata = std::fs::symlink_metadata(source)
        .map_err(|e| format!("无法读取 Skill 源目录元数据: {}", e))?;
    if metadata.file_type().is_symlink() {
        return Err("Skill 源目录不能是符号链接。".to_string());
    }
    if !metadata.is_dir() {
        return Err("请选择包含 SKILL.md 的 Skill 目录。".to_string());
    }

    std::fs::create_dir_all(dest).map_err(|e| format!("创建导入目录失败: {}", e))?;

    let entries = std::fs::read_dir(source).map_err(|e| format!("读取 Skill 源目录失败: {}", e))?;
    for entry in entries {
        let entry = entry.map_err(|e| format!("读取 Skill 源目录项失败: {}", e))?;
        let from = entry.path();
        let to = dest.join(entry.file_name());
        let metadata =
            std::fs::symlink_metadata(&from).map_err(|e| format!("读取导入文件失败: {}", e))?;

        if metadata.file_type().is_symlink() {
            return Err(format!("Skill 目录中不支持符号链接: {}", from.display()));
        }

        if metadata.is_dir() {
            copy_skill_directory(&from, &to)?;
        } else if metadata.is_file() {
            std::fs::copy(&from, &to).map_err(|e| {
                format!(
                    "复制 Skill 文件失败: {} -> {}: {}",
                    from.display(),
                    to.display(),
                    e
                )
            })?;
        } else {
            return Err(format!(
                "Skill 目录中包含不支持的文件类型: {}",
                from.display()
            ));
        }
    }

    Ok(())
}

fn import_staging_dir(import_root: &Path, skill_name: &str) -> Result<PathBuf, String> {
    let suffix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| format!("生成导入临时目录失败: {}", e))?
        .as_millis();
    Ok(import_root.join(format!(".{}-importing-{}", skill_name, suffix)))
}

fn normalize_skill_url(raw_url: &str) -> Result<String, String> {
    let trimmed = raw_url.trim();
    if trimmed.is_empty() {
        return Err("请输入 Skill URL。".to_string());
    }

    let parsed = reqwest::Url::parse(trimmed).map_err(|_| "Skill URL 格式无效。".to_string())?;
    match parsed.scheme() {
        "http" | "https" => {}
        _ => return Err("Skill URL 只支持 http 或 https。".to_string()),
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

fn download_skill_md(url: &str) -> Result<String, String> {
    let url = normalize_skill_url(url)?;
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .user_agent("otherone-desktop/0.1")
        .build()
        .map_err(|e| format!("创建 HTTP 客户端失败: {}", e))?;

    let response = client
        .get(&url)
        .send()
        .map_err(|e| format!("下载 Skill URL 失败: {}", e))?;

    if !response.status().is_success() {
        return Err(format!("下载 Skill URL 失败: HTTP {}", response.status()));
    }

    if let Some(length) = response.content_length() {
        if length > MAX_IMPORTED_SKILL_MD_BYTES {
            return Err("远程 SKILL.md 超过 1MB，已拒绝导入。".to_string());
        }
    }

    let bytes = response
        .bytes()
        .map_err(|e| format!("读取 Skill URL 响应失败: {}", e))?;
    if bytes.len() as u64 > MAX_IMPORTED_SKILL_MD_BYTES {
        return Err("远程 SKILL.md 超过 1MB，已拒绝导入。".to_string());
    }

    String::from_utf8(bytes.to_vec()).map_err(|_| "远程 SKILL.md 必须是 UTF-8 文本。".to_string())
}

fn ensure_default_skill_installs(
    db: &rusqlite::Connection,
    skill_entries: &[PluginEntry],
    installed: &mut std::collections::HashSet<(String, String)>,
) -> Result<(), String> {
    if skill_entries.is_empty() {
        return Ok(());
    }

    let applied = db
        .query_row(
            "SELECT EXISTS(
                SELECT 1 FROM plugin_install_defaults WHERE key = ?1
            )",
            params![DEFAULT_SKILLS_INSTALL_MARKER],
            |row| row.get::<_, i64>(0),
        )
        .map(|value| value != 0)
        .map_err(|e| format!("读取默认 skill 初始化状态失败: {}", e))?;

    if applied {
        return Ok(());
    }

    for entry in skill_entries {
        db.execute(
            "INSERT OR IGNORE INTO plugin_installs (name, kind) VALUES (?1, 'skill')",
            params![entry.name],
        )
        .map_err(|e| format!("写入默认 skill 安装记录失败: {}", e))?;

        installed.insert((entry.name.clone(), "skill".to_string()));
    }

    db.execute(
        "INSERT OR REPLACE INTO plugin_install_defaults (key) VALUES (?1)",
        params![DEFAULT_SKILLS_INSTALL_MARKER],
    )
    .map_err(|e| format!("保存默认 skill 初始化状态失败: {}", e))?;

    Ok(())
}

#[derive(Debug, Clone)]
struct ImportedMcpServer {
    name: String,
    transport: String,
    description: String,
    config_json: String,
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

fn validate_string_array_field(
    object: &serde_json::Map<String, serde_json::Value>,
    field: &str,
) -> Result<(), String> {
    if let Some(value) = object.get(field) {
        let values = value
            .as_array()
            .ok_or_else(|| format!("MCP field '{}' 必须是字符串数组。", field))?;
        if values.iter().any(|item| !item.is_string()) {
            return Err(format!("MCP field '{}' 必须只包含字符串。", field));
        }
    }
    Ok(())
}

fn validate_string_object_field(
    object: &serde_json::Map<String, serde_json::Value>,
    field: &str,
) -> Result<(), String> {
    if let Some(value) = object.get(field) {
        let values = value
            .as_object()
            .ok_or_else(|| format!("MCP field '{}' 必须是对象。", field))?;
        if values.values().any(|item| !item.is_string()) {
            return Err(format!("MCP field '{}' 的值必须都是字符串。", field));
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

fn infer_mcp_transport(
    object: &serde_json::Map<String, serde_json::Value>,
) -> Result<String, String> {
    let raw_type = object
        .get("type")
        .or_else(|| object.get("transport"))
        .and_then(|value| value.as_str())
        .map(|value| value.trim().to_ascii_lowercase());

    let transport = match raw_type.as_deref() {
        Some("stdio") => "stdio",
        Some("http") | Some("streamable-http") => "http",
        Some("sse") => "sse",
        Some(other) => {
            return Err(format!(
                "暂不支持 MCP transport '{}’，当前只支持 stdio、http、sse。",
                other
            ));
        }
        None if object.contains_key("command") => "stdio",
        None if object.contains_key("url") => "http",
        None => return Err("MCP server 配置必须包含 command 或 url。".to_string()),
    };

    Ok(transport.to_string())
}

fn mcp_description(
    name: &str,
    transport: &str,
    object: &serde_json::Map<String, serde_json::Value>,
) -> String {
    if let Some(description) = object.get("description").and_then(|value| value.as_str()) {
        let description = description.trim();
        if !description.is_empty() {
            return description.chars().take(1024).collect();
        }
    }

    match transport {
        "stdio" => object
            .get("command")
            .and_then(|value| value.as_str())
            .map(|command| format!("stdio MCP server: {}", command))
            .unwrap_or_else(|| format!("stdio MCP server: {}", name)),
        "http" | "sse" => object
            .get("url")
            .and_then(|value| value.as_str())
            .and_then(|url| reqwest::Url::parse(url).ok())
            .and_then(|url| url.host_str().map(|host| host.to_string()))
            .map(|host| format!("{} MCP server: {}", transport, host))
            .unwrap_or_else(|| format!("{} MCP server: {}", transport, name)),
        _ => format!("MCP server: {}", name),
    }
}

fn parse_one_mcp_server(
    raw_name: &str,
    raw_config: &serde_json::Value,
) -> Result<ImportedMcpServer, String> {
    let name = raw_name.trim().to_string();
    validate_mcp_name(&name)?;

    let object = raw_config
        .as_object()
        .ok_or_else(|| format!("MCP server '{}' 的配置必须是对象。", name))?;
    let transport = infer_mcp_transport(object)?;

    match transport.as_str() {
        "stdio" => {
            let command = object
                .get("command")
                .and_then(|value| value.as_str())
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| format!("MCP server '{}' 缺少 command。", name))?;
            if command.contains('\0') {
                return Err(format!("MCP server '{}' 的 command 无效。", name));
            }
            validate_string_array_field(object, "args")?;
            validate_string_object_field(object, "env")?;
        }
        "http" | "sse" => {
            let url = object
                .get("url")
                .and_then(|value| value.as_str())
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| format!("MCP server '{}' 缺少 url。", name))?;
            validate_http_mcp_url(url)?;
            validate_string_object_field(object, "headers")?;
        }
        _ => unreachable!(),
    }

    let mut normalized = object.clone();
    normalized
        .entry("type".to_string())
        .or_insert_with(|| serde_json::Value::String(transport.clone()));

    let description = mcp_description(&name, &transport, &normalized);
    let config_json = serde_json::to_string_pretty(&serde_json::Value::Object(normalized))
        .map_err(|e| format!("序列化 MCP 配置失败: {}", e))?;

    Ok(ImportedMcpServer {
        name,
        transport,
        description,
        config_json,
    })
}

fn parse_mcp_import(raw: &str) -> Result<Vec<ImportedMcpServer>, String> {
    if raw.trim().is_empty() {
        return Err("请输入 MCP JSON 配置。".to_string());
    }

    let value = serde_json::from_str::<serde_json::Value>(raw)
        .map_err(|e| format!("MCP JSON 格式无效: {}", e))?;
    let mut servers = Vec::new();

    if let Some(mcp_servers) = value.get("mcpServers") {
        let mcp_servers = mcp_servers
            .as_object()
            .ok_or_else(|| "mcpServers 必须是对象。".to_string())?;
        for (name, config) in mcp_servers {
            servers.push(parse_one_mcp_server(name, config)?);
        }
    } else if let Some(object) = value.as_object() {
        let name = object
            .get("name")
            .and_then(|value| value.as_str())
            .ok_or_else(|| {
                "单个 MCP server 配置必须包含 name，或使用 mcpServers 包裹。".to_string()
            })?;
        let mut config = object.clone();
        config.remove("name");
        servers.push(parse_one_mcp_server(
            name,
            &serde_json::Value::Object(config),
        )?);
    } else {
        return Err("MCP JSON 必须是对象。".to_string());
    }

    if servers.is_empty() {
        return Err("MCP JSON 中没有可导入的 server。".to_string());
    }

    let mut seen = std::collections::HashSet::new();
    for server in &servers {
        if !seen.insert(server.name.clone()) {
            return Err(format!("MCP JSON 中存在重复 server name: {}", server.name));
        }
    }

    Ok(servers)
}

fn load_mcp_entries(db: &rusqlite::Connection) -> Result<Vec<PluginEntry>, String> {
    let mut stmt = db
        .prepare("SELECT name, transport, description FROM mcp_servers ORDER BY name ASC")
        .map_err(|e| format!("读取 MCP server 列表失败: {}", e))?;
    let rows = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })
        .map_err(|e| format!("读取 MCP server 列表失败: {}", e))?;

    let mut entries = Vec::new();
    for (index, row) in rows.enumerate() {
        let (name, transport, description) =
            row.map_err(|e| format!("读取 MCP server 记录失败: {}", e))?;
        entries.push(PluginEntry {
            id: format!("mcp-{}", index),
            name: name.clone(),
            description: if description.trim().is_empty() {
                format!("{} MCP server", transport)
            } else {
                description
            },
            kind: "mcp".to_string(),
            source: "imported".to_string(),
            installed: false,
            file_path: format!("mcp://{}", name),
            has_binary: None,
            bin_path: None,
            bin_dir: None,
        });
    }

    Ok(entries)
}

fn normalize_mcp_config_url(raw_url: &str) -> Result<String, String> {
    let trimmed = raw_url.trim();
    if trimmed.is_empty() {
        return Err("请输入 MCP 配置 URL。".to_string());
    }

    let parsed = reqwest::Url::parse(trimmed).map_err(|_| "MCP 配置 URL 格式无效。".to_string())?;
    match parsed.scheme() {
        "http" | "https" => {}
        _ => return Err("MCP 配置 URL 只支持 http 或 https。".to_string()),
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

fn download_mcp_config(url: &str) -> Result<String, String> {
    let url = normalize_mcp_config_url(url)?;
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .user_agent("otherone-desktop/0.1")
        .build()
        .map_err(|e| format!("创建 HTTP 客户端失败: {}", e))?;

    let response = client
        .get(&url)
        .send()
        .map_err(|e| format!("下载 MCP 配置失败: {}", e))?;

    if !response.status().is_success() {
        return Err(format!("下载 MCP 配置失败: HTTP {}", response.status()));
    }

    if let Some(length) = response.content_length() {
        if length > MAX_IMPORTED_MCP_CONFIG_BYTES {
            return Err("远程 MCP 配置超过 1MB，已拒绝导入。".to_string());
        }
    }

    let bytes = response
        .bytes()
        .map_err(|e| format!("读取 MCP 配置响应失败: {}", e))?;
    if bytes.len() as u64 > MAX_IMPORTED_MCP_CONFIG_BYTES {
        return Err("远程 MCP 配置超过 1MB，已拒绝导入。".to_string());
    }

    String::from_utf8(bytes.to_vec()).map_err(|_| "远程 MCP 配置必须是 UTF-8 文本。".to_string())
}

fn skills_resource_dir() -> PathBuf {
    if let Ok(exe) = std::env::current_exe() {
        for ancestor in exe.ancestors() {
            let c = ancestor
                .join("resources")
                .join("skills")
                .join("skills-main")
                .join("skills");
            if c.exists() {
                return c;
            }
        }
    }
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let c = cwd
        .join("resources")
        .join("skills")
        .join("skills-main")
        .join("skills");
    if c.exists() {
        return c;
    }
    for ancestor in cwd.ancestors() {
        let c = ancestor
            .join("resources")
            .join("skills")
            .join("skills-main")
            .join("skills");
        if c.exists() {
            return c;
        }
    }
    cwd
}

// ---------------------------------------------------------------------------
// Tauri commands
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn load_plugin_list(app: AppHandle) -> Result<Vec<PluginEntry>, String> {
    let mut mgr = PLUGINS_MANAGER.lock().map_err(|e| e.to_string())?;

    if !mgr.initialized {
        // ── Skills：文件系统发现 ──
        let dir = skills_resource_dir();
        let bundled_discovered = discover_skills_in_dir(&dir);
        let bundled_skill_count = bundled_discovered.len();
        let mut discovered = bundled_discovered;
        discovered.extend(discover_imported_skills(&app));
        let discovered = dedupe_discovered_skills(discovered);
        for (name, _desc, fp) in &discovered {
            mgr.content_cache
                .insert(name.clone(), read_skill_content(fp));
        }
        mgr.skill_entries = discovered
            .into_iter()
            .enumerate()
            .map(|(i, (name, desc, file_path))| PluginEntry {
                id: format!("skill-{}", i),
                name,
                description: desc,
                kind: "skill".to_string(),
                source: "imported".to_string(),
                installed: false,
                file_path,
                has_binary: None,
                bin_path: None,
                bin_dir: None,
            })
            .collect();

        // ── Plugins：内置注册表 ──
        mgr.plugin_entries = plugin_registry::builtin_plugins()
            .into_iter()
            .enumerate()
            .map(|(i, def)| PluginEntry {
                id: format!("plugin-{}", i),
                name: def.name,
                description: def.description,
                kind: "plugin".to_string(),
                source: "builtin".to_string(),
                installed: false,
                file_path: String::new(),
                has_binary: Some(!def.binaries.is_empty()),
                bin_path: None,
                bin_dir: None,
            })
            .collect();

        // ── 从 SQLite 加载安装状态 ──
        if let Ok(db) = crate::storage::open_database(&app) {
            init_plugin_db(&db)?;
            mgr.mcp_entries = load_mcp_entries(&db)?;
            if let Ok(mut stmt) = db.prepare("SELECT name, kind FROM plugin_installs") {
                if let Ok(rows) = stmt.query_map([], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
                }) {
                    for row in rows.flatten() {
                        mgr.installed.insert(row);
                    }
                }
            }

            let default_skill_entries: Vec<PluginEntry> = mgr
                .skill_entries
                .iter()
                .take(bundled_skill_count)
                .cloned()
                .collect();
            ensure_default_skill_installs(&db, &default_skill_entries, &mut mgr.installed)?;
        }

        mgr.initialized = true;
    }

    // ── 每次加载都刷新 installed 标志 + 补齐已安装 plugin 的元数据 ──
    let data_root = crate::app_settings::data_root(&app).unwrap_or_default();
    let installed_set: Vec<(String, String)> = mgr.installed.iter().cloned().collect();
    for entry in &mut mgr.skill_entries {
        entry.installed = installed_set.contains(&(entry.name.clone(), entry.kind.clone()));
    }

    let mut pending_content: Vec<(String, SkillContent)> = Vec::new();

    for entry in &mut mgr.plugin_entries {
        entry.installed = installed_set.contains(&(entry.name.clone(), entry.kind.clone()));
        if !entry.installed {
            continue;
        }

        // 补齐 file_path（从安装目录读 SKILL.md）
        if entry.file_path.is_empty() {
            let skill_path =
                plugin_registry::plugin_install_dir(&data_root, &entry.name).join("SKILL.md");
            if skill_path.exists() {
                let sp = skill_path.to_string_lossy().to_string();
                entry.file_path = sp.clone();
                pending_content.push((entry.name.clone(), read_skill_content(&sp)));
            }
        }

        // 补齐 bin_path / bin_dir
        if entry.has_binary == Some(true) && entry.bin_path.is_none() {
            if let Some(def) = plugin_registry::builtin_plugins()
                .into_iter()
                .find(|d| d.name == entry.name)
            {
                if let Some(bin_name) = def.binary_name {
                    let bin =
                        plugin_registry::plugin_bin_dir(&data_root, &entry.name).join(&bin_name);
                    if bin.exists() {
                        entry.bin_path = Some(bin.to_string_lossy().to_string());
                        if let Some(parent) = bin.parent() {
                            entry.bin_dir = Some(parent.to_string_lossy().to_string());
                        }
                    }
                }
            }
        }
    }

    for entry in &mut mgr.mcp_entries {
        entry.installed = installed_set.contains(&(entry.name.clone(), entry.kind.clone()));
    }

    // 延迟插入 content_cache（避免循环中的双重借用）
    for (name, content) in pending_content {
        mgr.content_cache.entry(name).or_insert(content);
    }

    Ok(mgr.all_entries())
}

#[tauri::command]
pub fn install_plugin(app: AppHandle, plugin_name: String, kind: String) -> Result<(), String> {
    match kind.as_str() {
        "skill" => install_skill(app, &plugin_name),
        "plugin" => install_plugin_kind(app, &plugin_name),
        "mcp" => install_mcp(app, &plugin_name),
        _ => Err(format!("暂不支持的插件类型: {}", kind)),
    }
}

#[tauri::command]
pub fn uninstall_plugin(app: AppHandle, plugin_name: String, kind: String) -> Result<(), String> {
    match kind.as_str() {
        "skill" => uninstall_skill(app, &plugin_name),
        "plugin" => uninstall_plugin_kind(app, &plugin_name),
        "mcp" => uninstall_mcp(app, &plugin_name),
        _ => Err(format!("暂不支持的插件类型: {}", kind)),
    }
}

// ---------------------------------------------------------------------------
// Skill 安装/卸载（简单：只记 SQLite）
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn import_skill_from_directory(
    app: AppHandle,
    source_dir: String,
) -> Result<PluginEntry, String> {
    let source = PathBuf::from(source_dir.trim());
    if source.as_os_str().is_empty() {
        return Err("请选择 Skill 目录。".to_string());
    }

    let source = source
        .canonicalize()
        .map_err(|e| format!("无法解析 Skill 目录: {}", e))?;
    let source_metadata =
        std::fs::symlink_metadata(&source).map_err(|e| format!("无法读取 Skill 目录: {}", e))?;
    if source_metadata.file_type().is_symlink() {
        return Err("Skill 目录不能是符号链接。".to_string());
    }
    if !source_metadata.is_dir() {
        return Err("请选择包含 SKILL.md 的 Skill 目录。".to_string());
    }

    let skill_md = source.join("SKILL.md");
    if !skill_md.is_file() {
        return Err("所选目录必须包含 SKILL.md。".to_string());
    }

    let raw =
        std::fs::read_to_string(&skill_md).map_err(|e| format!("读取 SKILL.md 失败: {}", e))?;
    let (name, description) = parse_imported_skill_metadata(&raw)?;

    let existing = load_plugin_list(app.clone())?;
    if existing
        .iter()
        .any(|entry| entry.kind == "skill" && entry.name == name)
    {
        return Err(format!(
            "已存在名为 '{}' 的 Skill。请修改 SKILL.md 中的 name 后再导入。",
            name
        ));
    }

    let data_root = crate::app_settings::data_root(&app)?;
    let import_root = imported_skills_dir(&data_root);
    std::fs::create_dir_all(&import_root).map_err(|e| format!("创建 Skill 导入目录失败: {}", e))?;

    let dest = import_root.join(&name);
    if dest.exists() {
        return Err(format!(
            "导入目录已存在: {}。请修改 Skill name 后再导入。",
            dest.display()
        ));
    }

    let staging = import_staging_dir(&import_root, &name)?;
    copy_skill_directory(&source, &staging)?;
    std::fs::rename(&staging, &dest).map_err(|e| format!("保存导入 Skill 失败: {}", e))?;

    let imported_skill_md = dest.join("SKILL.md");
    let file_path = imported_skill_md.to_string_lossy().to_string();
    let content = read_skill_content(&file_path);

    let db = crate::storage::open_database(&app).map_err(|e| format!("无法打开数据库: {}", e))?;
    init_plugin_db(&db)?;
    db.execute(
        "INSERT OR REPLACE INTO plugin_installs (name, kind) VALUES (?1, 'skill')",
        params![name],
    )
    .map_err(|e| format!("保存 Skill 安装记录失败: {}", e))?;

    let mut mgr = PLUGINS_MANAGER.lock().map_err(|e| e.to_string())?;
    let entry = PluginEntry {
        id: format!("skill-imported-{}", mgr.skill_entries.len()),
        name: name.clone(),
        description,
        kind: "skill".to_string(),
        source: "imported".to_string(),
        installed: true,
        file_path,
        has_binary: None,
        bin_path: None,
        bin_dir: None,
    };
    mgr.content_cache.insert(name.clone(), content);
    mgr.installed.insert((name, "skill".to_string()));
    mgr.skill_entries.push(entry.clone());

    Ok(entry)
}

#[tauri::command]
pub fn import_skill_from_url(app: AppHandle, url: String) -> Result<PluginEntry, String> {
    let raw = download_skill_md(&url)?;
    let (name, description) = parse_imported_skill_metadata(&raw)?;

    let existing = load_plugin_list(app.clone())?;
    if existing
        .iter()
        .any(|entry| entry.kind == "skill" && entry.name == name)
    {
        return Err(format!(
            "已存在名为 '{}' 的 Skill。请修改远程 SKILL.md 中的 name 后再导入。",
            name
        ));
    }

    let data_root = crate::app_settings::data_root(&app)?;
    let import_root = imported_skills_dir(&data_root);
    std::fs::create_dir_all(&import_root).map_err(|e| format!("创建 Skill 导入目录失败: {}", e))?;

    let dest = import_root.join(&name);
    if dest.exists() {
        return Err(format!(
            "导入目录已存在: {}。请修改 Skill name 后再导入。",
            dest.display()
        ));
    }

    let staging = import_staging_dir(&import_root, &name)?;
    std::fs::create_dir_all(&staging).map_err(|e| format!("创建 Skill 临时目录失败: {}", e))?;
    let staging_skill_md = staging.join("SKILL.md");
    std::fs::write(&staging_skill_md, raw.as_bytes())
        .map_err(|e| format!("写入远程 SKILL.md 失败: {}", e))?;
    std::fs::rename(&staging, &dest).map_err(|e| format!("保存导入 Skill 失败: {}", e))?;

    let imported_skill_md = dest.join("SKILL.md");
    let file_path = imported_skill_md.to_string_lossy().to_string();
    let content = read_skill_content(&file_path);

    let db = crate::storage::open_database(&app).map_err(|e| format!("无法打开数据库: {}", e))?;
    init_plugin_db(&db)?;
    db.execute(
        "INSERT OR REPLACE INTO plugin_installs (name, kind) VALUES (?1, 'skill')",
        params![name],
    )
    .map_err(|e| format!("保存 Skill 安装记录失败: {}", e))?;

    let mut mgr = PLUGINS_MANAGER.lock().map_err(|e| e.to_string())?;
    let entry = PluginEntry {
        id: format!("skill-imported-{}", mgr.skill_entries.len()),
        name: name.clone(),
        description,
        kind: "skill".to_string(),
        source: "imported".to_string(),
        installed: true,
        file_path,
        has_binary: None,
        bin_path: None,
        bin_dir: None,
    };
    mgr.content_cache.insert(name.clone(), content);
    mgr.installed.insert((name, "skill".to_string()));
    mgr.skill_entries.push(entry.clone());

    Ok(entry)
}

fn import_mcp_servers_from_raw(
    app: AppHandle,
    raw_config: String,
) -> Result<Vec<PluginEntry>, String> {
    let servers = parse_mcp_import(&raw_config)?;
    let existing = load_plugin_list(app.clone())?;

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

    let mut db =
        crate::storage::open_database(&app).map_err(|e| format!("无法打开数据库: {}", e))?;
    init_plugin_db(&db)?;
    let tx = db
        .transaction()
        .map_err(|e| format!("创建 MCP 导入事务失败: {}", e))?;

    for server in &servers {
        tx.execute(
            "INSERT INTO mcp_servers (name, transport, description, config_json, source)
             VALUES (?1, ?2, ?3, ?4, 'imported')",
            params![
                server.name,
                server.transport,
                server.description,
                server.config_json
            ],
        )
        .map_err(|e| format!("保存 MCP server '{}' 失败: {}", server.name, e))?;
        tx.execute(
            "INSERT OR REPLACE INTO plugin_installs (name, kind) VALUES (?1, 'mcp')",
            params![server.name],
        )
        .map_err(|e| format!("保存 MCP server '{}' 安装记录失败: {}", server.name, e))?;
    }

    tx.commit()
        .map_err(|e| format!("提交 MCP 导入事务失败: {}", e))?;

    let mut mgr = PLUGINS_MANAGER.lock().map_err(|e| e.to_string())?;
    let base_len = mgr.mcp_entries.len();
    let mut entries = Vec::new();

    for (offset, server) in servers.into_iter().enumerate() {
        let entry = PluginEntry {
            id: format!("mcp-imported-{}", base_len + offset),
            name: server.name.clone(),
            description: server.description,
            kind: "mcp".to_string(),
            source: "imported".to_string(),
            installed: true,
            file_path: format!("mcp://{}", server.name),
            has_binary: None,
            bin_path: None,
            bin_dir: None,
        };
        mgr.installed
            .insert((entry.name.clone(), "mcp".to_string()));
        mgr.mcp_entries.push(entry.clone());
        entries.push(entry);
    }

    Ok(entries)
}

#[tauri::command]
pub fn import_mcp_servers(app: AppHandle, raw_config: String) -> Result<Vec<PluginEntry>, String> {
    import_mcp_servers_from_raw(app, raw_config)
}

#[tauri::command]
pub fn import_mcp_servers_from_url(
    app: AppHandle,
    url: String,
) -> Result<Vec<PluginEntry>, String> {
    let raw = download_mcp_config(&url)?;
    import_mcp_servers_from_raw(app, raw)
}

pub(crate) fn reset_plugin_runtime_state() -> Result<(), String> {
    let mut mgr = PLUGINS_MANAGER.lock().map_err(|e| e.to_string())?;

    mgr.installed.clear();
    mgr.content_cache.clear();

    for entry in &mut mgr.skill_entries {
        entry.installed = false;
    }

    for entry in &mut mgr.plugin_entries {
        entry.installed = false;
        entry.file_path.clear();
        entry.bin_path = None;
        entry.bin_dir = None;
    }

    for entry in &mut mgr.mcp_entries {
        entry.installed = false;
    }

    Ok(())
}

fn install_skill(app: AppHandle, name: &str) -> Result<(), String> {
    let mut mgr = PLUGINS_MANAGER.lock().map_err(|e| e.to_string())?;
    let exists = mgr.skill_entries.iter().any(|s| s.name == name);
    if !exists {
        return Err(format!("未找到名为 '{}' 的 skill", name));
    }

    let db = crate::storage::open_database(&app).map_err(|e| format!("无法打开数据库: {}", e))?;
    db.execute(
        "INSERT OR REPLACE INTO plugin_installs (name, kind) VALUES (?1, 'skill')",
        params![name],
    )
    .map_err(|e| format!("保存失败: {}", e))?;

    mgr.installed
        .insert((name.to_string(), "skill".to_string()));
    if !mgr.content_cache.contains_key(name) {
        let fp = mgr
            .skill_entries
            .iter()
            .find(|e| e.name == name)
            .map(|e| e.file_path.clone());
        if let Some(fp) = fp {
            mgr.content_cache
                .insert(name.to_string(), read_skill_content(&fp));
        }
    }
    Ok(())
}

fn uninstall_skill(app: AppHandle, name: &str) -> Result<(), String> {
    let mut mgr = PLUGINS_MANAGER.lock().map_err(|e| e.to_string())?;
    let db = crate::storage::open_database(&app).map_err(|e| format!("无法打开数据库: {}", e))?;
    db.execute(
        "DELETE FROM plugin_installs WHERE name = ?1 AND kind = 'skill'",
        params![name],
    )
    .map_err(|e| format!("删除失败: {}", e))?;
    mgr.installed
        .remove(&(name.to_string(), "skill".to_string()));
    Ok(())
}

fn install_mcp(app: AppHandle, name: &str) -> Result<(), String> {
    let mut mgr = PLUGINS_MANAGER.lock().map_err(|e| e.to_string())?;
    let exists = mgr.mcp_entries.iter().any(|entry| entry.name == name);
    if !exists {
        return Err(format!("未找到名为 '{}' 的 MCP server", name));
    }

    let db = crate::storage::open_database(&app).map_err(|e| format!("无法打开数据库: {}", e))?;
    init_plugin_db(&db)?;
    db.execute(
        "INSERT OR REPLACE INTO plugin_installs (name, kind) VALUES (?1, 'mcp')",
        params![name],
    )
    .map_err(|e| format!("保存 MCP server 安装记录失败: {}", e))?;

    mgr.installed.insert((name.to_string(), "mcp".to_string()));
    Ok(())
}

fn uninstall_mcp(app: AppHandle, name: &str) -> Result<(), String> {
    let mut mgr = PLUGINS_MANAGER.lock().map_err(|e| e.to_string())?;
    let db = crate::storage::open_database(&app).map_err(|e| format!("无法打开数据库: {}", e))?;
    init_plugin_db(&db)?;
    db.execute(
        "DELETE FROM plugin_installs WHERE name = ?1 AND kind = 'mcp'",
        params![name],
    )
    .map_err(|e| format!("删除 MCP server 安装记录失败: {}", e))?;
    mgr.installed.remove(&(name.to_string(), "mcp".to_string()));
    Ok(())
}

// ---------------------------------------------------------------------------
// Plugin 安装/卸载（下载 SKILL.md + 二进制 → data_root）
// ---------------------------------------------------------------------------

fn install_plugin_kind(app: AppHandle, name: &str) -> Result<(), String> {
    // 1. 查注册表
    let def = plugin_registry::builtin_plugins()
        .into_iter()
        .find(|d| d.name == name)
        .ok_or_else(|| format!("未找到名为 '{}' 的插件定义", name))?;

    // 2. 确定安装目录
    let data_root = crate::app_settings::data_root(&app)?;
    let install_dir = plugin_registry::plugin_install_dir(&data_root, name);
    let bin_dir = plugin_registry::plugin_bin_dir(&data_root, name);

    std::fs::create_dir_all(&install_dir).map_err(|e| format!("创建插件目录失败: {}", e))?;
    std::fs::create_dir_all(&bin_dir).map_err(|e| format!("创建二进制目录失败: {}", e))?;

    // 3. 下载 SKILL.md
    eprintln!("[plugins] 正在下载 {} SKILL.md 从 {}", name, def.skill_url);
    let skill_path = install_dir.join("SKILL.md");
    download_file(&def.skill_url, &skill_path)?;
    eprintln!(
        "[plugins] {} SKILL.md 下载完成 → {}",
        name,
        skill_path.display()
    );

    // 4. 下载二进制（如果当前平台有对应的）
    if !def.binaries.is_empty() {
        let platform = plugin_registry::current_platform();
        if let Some(bin_url) = def.binaries.get(platform) {
            let bin_name = def.binary_name.as_deref().unwrap_or("plugin.bin");
            let bin_path = bin_dir.join(bin_name);
            eprintln!(
                "[plugins] 正在下载 {} 二进制 ({}) 从 {}",
                name, platform, bin_url
            );
            download_file(bin_url, &bin_path)?;
            eprintln!("[plugins] {} 二进制下载完成 → {}", name, bin_path.display());

            // Unix: chmod +x
            #[cfg(not(target_os = "windows"))]
            {
                use std::os::unix::fs::PermissionsExt;
                if let Ok(meta) = std::fs::metadata(&bin_path) {
                    let mut perms = meta.permissions();
                    perms.set_mode(0o755);
                    let _ = std::fs::set_permissions(&bin_path, perms);
                }
            }
        }
    }

    // 5. 缓存内容
    let mut mgr = PLUGINS_MANAGER.lock().map_err(|e| e.to_string())?;
    let content = read_skill_content(&skill_path.to_string_lossy());
    mgr.content_cache.insert(name.to_string(), content);

    // 6. 写 SQLite
    let db = crate::storage::open_database(&app).map_err(|e| format!("无法打开数据库: {}", e))?;
    db.execute(
        "INSERT OR REPLACE INTO plugin_installs (name, kind) VALUES (?1, 'plugin')",
        params![name],
    )
    .map_err(|e| format!("保存失败: {}", e))?;

    mgr.installed
        .insert((name.to_string(), "plugin".to_string()));
    Ok(())
}

fn uninstall_plugin_kind(app: AppHandle, name: &str) -> Result<(), String> {
    let mut mgr = PLUGINS_MANAGER.lock().map_err(|e| e.to_string())?;

    // 删除安装文件
    let data_root = crate::app_settings::data_root(&app)?;
    let install_dir = plugin_registry::plugin_install_dir(&data_root, name);
    if install_dir.exists() {
        std::fs::remove_dir_all(&install_dir).map_err(|e| format!("删除插件文件失败: {}", e))?;
    }

    // 删除 SQLite 记录
    let db = crate::storage::open_database(&app).map_err(|e| format!("无法打开数据库: {}", e))?;
    db.execute(
        "DELETE FROM plugin_installs WHERE name = ?1 AND kind = 'plugin'",
        params![name],
    )
    .map_err(|e| format!("删除失败: {}", e))?;

    mgr.installed
        .remove(&(name.to_string(), "plugin".to_string()));
    mgr.content_cache.remove(name);
    Ok(())
}

// ---------------------------------------------------------------------------
// 文件下载
// ---------------------------------------------------------------------------

fn download_file(url: &str, dest: &PathBuf) -> Result<(), String> {
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(120))
        .user_agent("otherone-desktop/0.1")
        .build()
        .map_err(|e| format!("创建 HTTP 客户端失败: {}", e))?;

    let response = client
        .get(url)
        .send()
        .map_err(|e| format!("下载失败: {}", e))?;

    if !response.status().is_success() {
        return Err(format!("下载失败: HTTP {}", response.status()));
    }

    let bytes = response
        .bytes()
        .map_err(|e| format!("读取响应失败: {}", e))?;

    std::fs::write(dest, &bytes).map_err(|e| format!("写入文件失败: {}", e))?;

    Ok(())
}

// ---------------------------------------------------------------------------
// 公开查询 API（供 chat.rs / tools.rs 使用）
// ---------------------------------------------------------------------------

/// 获取所有已安装的 skill/plugin 的完整内容
pub fn get_installed_skills_content() -> Vec<SkillContent> {
    let mgr = match PLUGINS_MANAGER.lock() {
        Ok(m) => m,
        Err(_) => return Vec::new(),
    };
    mgr.installed
        .iter()
        .filter_map(|(name, _kind)| mgr.content_cache.get(name).cloned())
        .collect()
}

/// 获取所有已安装的条目（skill + plugin）
pub fn get_installed_skill_entries() -> Vec<PluginEntry> {
    let mgr = match PLUGINS_MANAGER.lock() {
        Ok(m) => m,
        Err(_) => return Vec::new(),
    };
    mgr.all_entries()
        .into_iter()
        .filter(|e| mgr.installed.contains(&(e.name.clone(), e.kind.clone())))
        .collect()
}

/// 获取指定 plugin 的元数据（含 bin_path/bin_dir，供 plugin 工具使用）
pub fn get_plugin_metadata(name: &str) -> Option<PluginEntry> {
    let mgr = PLUGINS_MANAGER.lock().ok()?;
    mgr.plugin_entries
        .iter()
        .find(|e| {
            e.name == name
                && mgr
                    .installed
                    .contains(&(e.name.clone(), "plugin".to_string()))
        })
        .cloned()
}

/// 获取指定 plugin 的 SKILL.md body
pub fn get_plugin_body(name: &str) -> Option<String> {
    let mgr = PLUGINS_MANAGER.lock().ok()?;
    mgr.content_cache.get(name).map(|c| c.body.clone())
}

/// 生成 <available_skills> XML（注入到 system prompt，skills 部分）
pub fn format_skills_for_prompt() -> String {
    let entries = get_installed_skill_entries();
    if entries.is_empty() {
        return String::new();
    }

    // 拆分：纯 skill 和 plugin 各放一个块
    let skills: Vec<_> = entries.iter().filter(|e| e.kind == "skill").collect();
    let plugins: Vec<_> = entries.iter().filter(|e| e.kind == "plugin").collect();

    let mut lines = vec![
        String::new(),
        "The following capabilities provide specialized instructions for specific tasks."
            .to_string(),
    ];

    // ── Skills 块 ──
    if !skills.is_empty() {
        lines.push(String::new());
        lines.push("## Skills".to_string());
        lines.push("Use the <function>skill</function> tool to load a skill's full instructions when a task matches its description.".to_string());
        lines.push(String::new());
        lines.push("<available_skills>".to_string());
        for s in &skills {
            lines.push("  <skill>".to_string());
            lines.push(format!("    <name>{}</name>", escape_xml(&s.name)));
            lines.push(format!(
                "    <description>{}</description>",
                escape_xml(&s.description)
            ));
            lines.push(format!(
                "    <location>{}</location>",
                escape_xml(&s.file_path)
            ));
            lines.push("  </skill>".to_string());
        }
        lines.push("</available_skills>".to_string());
    }

    // ── Plugins 块 ──
    if !plugins.is_empty() {
        lines.push(String::new());
        lines.push("## Plugins".to_string());
        lines.push("Use the <function>plugin_tool</function> to load a plugin's full instructions and resource paths. Plugins may include CLI binaries — the tool returns bin_path so you know exactly where the executable lives.".to_string());
        lines.push(String::new());
        lines.push("<available_plugins>".to_string());
        for p in &plugins {
            lines.push("  <plugin>".to_string());
            lines.push(format!("    <name>{}</name>", escape_xml(&p.name)));
            lines.push(format!(
                "    <description>{}</description>",
                escape_xml(&p.description)
            ));
            if let Some(ref bin) = p.bin_path {
                lines.push(format!("    <binary>{}</binary>", escape_xml(bin)));
            }
            if let Some(ref dir) = p.bin_dir {
                lines.push(format!("    <bin_dir>{}</bin_dir>", escape_xml(dir)));
            }
            lines.push("  </plugin>".to_string());
        }
        lines.push("</available_plugins>".to_string());
    }

    lines.join("\n")
}

fn escape_xml(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

/// 初始化插件数据库表
pub fn init_plugin_db(db: &rusqlite::Connection) -> Result<(), String> {
    db.execute_batch(
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
    .map_err(|e| format!("初始化插件表失败: {}", e))
}
