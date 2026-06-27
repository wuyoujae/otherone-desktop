// 统一插件管理模块 — 管理 Skill（文件系统发现）、Plugin（下载安装）、MCP（未来）的生命周期
// 关联：被 main.rs 注册为 Tauri commands，被 chat.rs 读取注入 system prompt

use rusqlite::params;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Mutex;
use tauri::AppHandle;

use crate::plugin_registry;

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
            installed: std::collections::HashSet::new(),
            content_cache: std::collections::HashMap::new(),
            initialized: false,
        }
    }

    fn all_entries(&self) -> Vec<PluginEntry> {
        let mut all = Vec::new();
        all.extend(self.skill_entries.clone());
        all.extend(self.plugin_entries.clone());
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
        let discovered = discover_skills_in_dir(&dir);
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
            if let Ok(mut stmt) = db.prepare("SELECT name, kind FROM plugin_installs") {
                if let Ok(rows) = stmt.query_map([], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
                }) {
                    for row in rows.flatten() {
                        mgr.installed.insert(row);
                    }
                }
            }
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
        _ => Err(format!("暂不支持的插件类型: {}", kind)),
    }
}

#[tauri::command]
pub fn uninstall_plugin(app: AppHandle, plugin_name: String, kind: String) -> Result<(), String> {
    match kind.as_str() {
        "skill" => uninstall_skill(app, &plugin_name),
        "plugin" => uninstall_plugin_kind(app, &plugin_name),
        _ => Err(format!("暂不支持的插件类型: {}", kind)),
    }
}

// ---------------------------------------------------------------------------
// Skill 安装/卸载（简单：只记 SQLite）
// ---------------------------------------------------------------------------

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
        );",
    )
    .map_err(|e| format!("初始化插件表失败: {}", e))
}
