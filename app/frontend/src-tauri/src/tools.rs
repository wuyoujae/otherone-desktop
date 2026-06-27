// 工具模块 — 定义和实现 Agent 可用的工具
// 关联：被 chat.rs 使用，通过 AiOptions.tools / tools_realize 注册到 Agent 循环

use otherone::ai::types::{FunctionDefinition, Tool};
use serde::Deserialize;
use serde_json::Value;
use std::cmp::Reverse;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Instant;
use tauri::AppHandle;

// ---------------------------------------------------------------------------
// 常量
// ---------------------------------------------------------------------------

const MAX_READ_SIZE: u64 = 10 * 1024 * 1024;
const MAX_WRITE_SIZE: usize = 10 * 1024 * 1024;

const GLOB_IGNORED_DIRS: &[&str] = &[
    ".git",
    "node_modules",
    ".build",
    "target",
    "dist",
    "coverage",
];

// ---------------------------------------------------------------------------
// 工具元数据
// ---------------------------------------------------------------------------

pub struct ToolMeta {
    pub name: String,
    pub display_name: String,
    pub definition: Tool,
    pub realization: Box<dyn Fn(Value) -> String + Send + Sync>,
    pub label_fn: Box<dyn Fn(&Value) -> String + Send + Sync>,
    pub expandable: bool,
}

// ---------------------------------------------------------------------------
// 输出结构体
// ---------------------------------------------------------------------------

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct TextFilePayload {
    file_path: String,
    content: String,
    num_lines: usize,
    start_line: usize,
    total_lines: usize,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct ReadFileOutput {
    #[serde(rename = "type")]
    kind: String,
    file: TextFilePayload,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct StructuredPatchHunk {
    old_start: usize,
    old_lines: usize,
    new_start: usize,
    new_lines: usize,
    lines: Vec<String>,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct WriteFileOutput {
    #[serde(rename = "type")]
    kind: String,
    file_path: String,
    content: String,
    structured_patch: Vec<StructuredPatchHunk>,
    original_file: Option<String>,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct EditFileOutput {
    file_path: String,
    old_string: String,
    new_string: String,
    original_file: String,
    structured_patch: Vec<StructuredPatchHunk>,
    replace_all: bool,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct GlobSearchOutput {
    duration_ms: u128,
    num_files: usize,
    filenames: Vec<String>,
    truncated: bool,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct GrepSearchOutput {
    mode: Option<String>,
    num_files: usize,
    filenames: Vec<String>,
    content: Option<String>,
    num_lines: Option<usize>,
    num_matches: Option<usize>,
    applied_limit: Option<usize>,
    applied_offset: Option<usize>,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct WebFetchOutput {
    bytes: usize,
    code: u16,
    code_text: String,
    result: String,
    duration_ms: u128,
    url: String,
}

#[derive(serde::Serialize)]
struct SearchHit {
    title: String,
    url: String,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct WebSearchOutput {
    query: String,
    results: Vec<SearchHit>,
    summary: String,
    duration_seconds: f64,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct ShellOutput {
    stdout: String,
    stderr: String,
    exit_code: Option<i32>,
    interrupted: bool,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct ReplOutput {
    language: String,
    stdout: String,
    stderr: String,
    exit_code: Option<i32>,
    duration_ms: u128,
}

// ---------------------------------------------------------------------------
// 输入结构体
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
struct ReadFileInput {
    file_path: String,
    #[serde(default)]
    offset: Option<usize>,
    #[serde(default)]
    limit: Option<usize>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
struct WriteFileInput {
    file_path: String,
    content: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
struct EditFileInput {
    file_path: String,
    old_string: String,
    new_string: String,
    #[serde(default)]
    replace_all: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
struct GlobSearchInput {
    pattern: String,
    #[serde(default)]
    path: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
struct GrepSearchInput {
    pattern: String,
    #[serde(default)]
    path: Option<String>,
    #[serde(default)]
    glob: Option<String>,
    #[serde(default)]
    output_mode: Option<String>,
    #[serde(default, rename = "-B")]
    before: Option<usize>,
    #[serde(default, rename = "-A")]
    after: Option<usize>,
    #[serde(default, rename = "-C")]
    context_short: Option<usize>,
    #[serde(default)]
    context: Option<usize>,
    #[serde(default, rename = "-n")]
    line_numbers: Option<bool>,
    #[serde(default, rename = "-i")]
    case_insensitive: Option<bool>,
    #[serde(default, rename = "type")]
    file_type: Option<String>,
    #[serde(default)]
    head_limit: Option<usize>,
    #[serde(default)]
    offset: Option<usize>,
    #[serde(default)]
    multiline: Option<bool>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
struct WebFetchInput {
    url: String,
    prompt: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
struct WebSearchInput {
    query: String,
    #[serde(default)]
    allowed_domains: Option<Vec<String>>,
    #[serde(default)]
    blocked_domains: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
struct ShellInput {
    command: String,
    #[serde(default)]
    timeout: Option<u64>,
    #[serde(default)]
    description: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
struct ReplInput {
    code: String,
    language: String,
    #[serde(default)]
    timeout_ms: Option<u64>,
}

// ---------------------------------------------------------------------------
// 工具定义
// ---------------------------------------------------------------------------

fn read_file_def() -> Tool {
    Tool { tool_type: "function".to_string(), function: FunctionDefinition { name: "read_file".to_string(), description: "Read a text file from the user's machine. Returns content with line numbers. Rejects binary files and files >10MB. Use offset/limit for pagination.".to_string(), parameters: Some(serde_json::json!({"type":"object","properties":{"file_path":{"type":"string","description":"Absolute path to the file to read"},"offset":{"type":"integer","description":"0-based start line (default 0)"},"limit":{"type":"integer","description":"Max lines to return (default all)"}},"required":["file_path"]})) } }
}
fn write_file_def() -> Tool {
    Tool { tool_type: "function".to_string(), function: FunctionDefinition { name: "write_file".to_string(), description: "Write content to a file. Creates the file if it doesn't exist (type=create), overwrites if it does (type=update). Auto-creates parent directories. Content >10MB rejected.".to_string(), parameters: Some(serde_json::json!({"type":"object","properties":{"file_path":{"type":"string","description":"Absolute path to the file to write"},"content":{"type":"string","description":"Full text content to write"}},"required":["file_path","content"]})) } }
}
fn edit_file_def() -> Tool {
    Tool { tool_type: "function".to_string(), function: FunctionDefinition { name: "edit_file".to_string(), description: "Replace a string in an existing file. Set replace_all=true to replace all occurrences, default is single replacement. Returns structured patch.".to_string(), parameters: Some(serde_json::json!({"type":"object","properties":{"file_path":{"type":"string","description":"Absolute path to the file to edit"},"old_string":{"type":"string","description":"Exact string to find and replace"},"new_string":{"type":"string","description":"Replacement string"},"replace_all":{"type":"boolean","description":"Replace all occurrences (default false)"}},"required":["file_path","old_string","new_string"]})) } }
}
fn glob_search_def() -> Tool {
    Tool { tool_type: "function".to_string(), function: FunctionDefinition { name: "glob_search".to_string(), description: "Find files matching a glob pattern. Supports brace expansion like **/*.{rs,toml}. Skips .git, node_modules, target etc. Returns up to 100 results sorted by modification time.".to_string(), parameters: Some(serde_json::json!({"type":"object","properties":{"pattern":{"type":"string","description":"Glob pattern, e.g. '**/*.tsx'"},"path":{"type":"string","description":"Root directory to search. Defaults to user home."}},"required":["pattern"]})) } }
}
fn grep_search_def() -> Tool {
    Tool { tool_type: "function".to_string(), function: FunctionDefinition { name: "grep_search".to_string(), description: "Search file contents with regex. Supports context lines (-A/-B/-C), glob filtering, case insensitivity, and three output modes: files_with_matches (default), content, count.".to_string(), parameters: Some(serde_json::json!({"type":"object","properties":{"pattern":{"type":"string","description":"Regex pattern to search"},"path":{"type":"string","description":"Directory or file to search (default user home)"},"glob":{"type":"string","description":"Glob filter like '**/*.rs'"},"output_mode":{"type":"string","enum":["files_with_matches","content","count"],"description":"Output mode"},"-B":{"type":"integer","description":"Lines before match"},"-A":{"type":"integer","description":"Lines after match"},"-C":{"type":"integer","description":"Lines of context"},"-n":{"type":"boolean","description":"Show line numbers"},"-i":{"type":"boolean","description":"Case insensitive"},"type":{"type":"string","description":"File extension filter"},"head_limit":{"type":"integer","description":"Max results"},"offset":{"type":"integer","description":"Result offset"},"multiline":{"type":"boolean","description":"Dot matches newline"}},"required":["pattern"]})) } }
}
fn web_fetch_def() -> Tool {
    Tool { tool_type: "function".to_string(), function: FunctionDefinition { name: "web_fetch".to_string(), description: "Fetch a URL, convert HTML to readable text, and answer a prompt about the content. HTTP auto-upgrades to HTTPS (except localhost).".to_string(), parameters: Some(serde_json::json!({"type":"object","properties":{"url":{"type":"string","format":"uri","description":"URL to fetch"},"prompt":{"type":"string","description":"What to extract from the page"}},"required":["url","prompt"]})) } }
}
fn web_search_def() -> Tool {
    Tool { tool_type: "function".to_string(), function: FunctionDefinition { name: "web_search".to_string(), description: "Search the web via DuckDuckGo and return up to 8 cited results. Filter by allowed/blocked domains.".to_string(), parameters: Some(serde_json::json!({"type":"object","properties":{"query":{"type":"string","minLength":2,"description":"Search query"},"allowed_domains":{"type":"array","items":{"type":"string"},"description":"Only include results from these domains"},"blocked_domains":{"type":"array","items":{"type":"string"},"description":"Exclude results from these domains"}},"required":["query"]})) } }
}
fn bash_def() -> Tool {
    Tool { tool_type: "function".to_string(), function: FunctionDefinition { name: "bash".to_string(), description: "Execute a shell command on the user's machine. Returns stdout and stderr. Use timeout to limit execution time in ms.".to_string(), parameters: Some(serde_json::json!({"type":"object","properties":{"command":{"type":"string","description":"The shell command to execute"},"timeout":{"type":"integer","description":"Timeout in milliseconds"},"description":{"type":"string","description":"Brief description of what the command does"}},"required":["command"]})) } }
}
fn powershell_def() -> Tool {
    Tool { tool_type: "function".to_string(), function: FunctionDefinition { name: "powershell".to_string(), description: "Execute a PowerShell command on the user's machine. Auto-detects pwsh or powershell. Returns stdout and stderr.".to_string(), parameters: Some(serde_json::json!({"type":"object","properties":{"command":{"type":"string","description":"The PowerShell command to execute"},"timeout":{"type":"integer","description":"Timeout in milliseconds"},"description":{"type":"string","description":"Brief description"}},"required":["command"]})) } }
}
fn repl_def() -> Tool {
    Tool { tool_type: "function".to_string(), function: FunctionDefinition { name: "repl".to_string(), description: "Execute code in a subprocess. Supports Python (py), JavaScript (js/node), and Shell (sh/bash).".to_string(), parameters: Some(serde_json::json!({"type":"object","properties":{"code":{"type":"string","description":"Code to execute"},"language":{"type":"string","enum":["python","py","javascript","js","node","sh","shell","bash"],"description":"Language"},"timeout_ms":{"type":"integer","description":"Timeout in ms"}},"required":["code","language"]})) } }
}
fn skill_def() -> Tool {
    Tool { tool_type: "function".to_string(), function: FunctionDefinition { name: "skill".to_string(), description: "Load a skill's full SKILL.md instructions. Call with the skill name from <available_skills> when a task matches a skill's description. The returned body contains detailed instructions the agent should follow.".to_string(), parameters: Some(serde_json::json!({"type":"object","properties":{"skill":{"type":"string","description":"Name of the skill to load, as shown in <available_skills>"}},"required":["skill"]})) } }
}
fn plugin_tool_def() -> Tool {
    Tool { tool_type: "function".to_string(), function: FunctionDefinition { name: "plugin_tool".to_string(), description: "Load a plugin's full instructions and resource paths. Use when a task needs a plugin from <available_plugins>. Returns the SKILL.md body, binary executable path (if any), and bin directory. Call this BEFORE invoking plugin commands so you know the exact binary location.".to_string(), parameters: Some(serde_json::json!({"type":"object","properties":{"plugin":{"type":"string","description":"Name of the plugin to load, as shown in <available_plugins>"}},"required":["plugin"]})) } }
}

// ---------------------------------------------------------------------------
// 标签生成
// ---------------------------------------------------------------------------

fn file_name_label(args: &Value, prefix: &str) -> String {
    let name = args
        .get("file_path")
        .and_then(|v| v.as_str())
        .and_then(|p| Path::new(p).file_name())
        .and_then(|n| n.to_str())
        .unwrap_or("file");
    format!("{prefix} {name}")
}
fn read_file_label(args: &Value) -> String {
    file_name_label(args, "正在读取")
}
fn write_file_label(args: &Value) -> String {
    file_name_label(args, "正在写入")
}
fn edit_file_label(args: &Value) -> String {
    file_name_label(args, "正在编辑")
}
fn glob_search_label(args: &Value) -> String {
    let p = args.get("pattern").and_then(|v| v.as_str()).unwrap_or("*");
    format!("正在搜索 {}", p)
}
fn grep_search_label(args: &Value) -> String {
    let p = args.get("pattern").and_then(|v| v.as_str()).unwrap_or("");
    format!("正在搜索 \"{}\"", p)
}
fn web_fetch_label(args: &Value) -> String {
    let u = args.get("url").and_then(|v| v.as_str()).unwrap_or("URL");
    let host = reqwest::Url::parse(u)
        .map(|p| p.host_str().unwrap_or(u).to_string())
        .unwrap_or(u.to_string());
    format!("正在抓取 {}", host)
}
fn web_search_label(args: &Value) -> String {
    let q = args.get("query").and_then(|v| v.as_str()).unwrap_or("");
    format!("正在搜索 \"{}\"", q)
}
fn bash_label(_: &Value) -> String {
    "正在执行命令".to_string()
}
fn powershell_label(_: &Value) -> String {
    "正在执行 PowerShell 命令".to_string()
}
fn repl_label(args: &Value) -> String {
    let lang = args
        .get("language")
        .and_then(|v| v.as_str())
        .unwrap_or("code");
    format!("正在执行 {} 代码", lang)
}
fn skill_label(args: &Value) -> String {
    let s = args
        .get("skill")
        .and_then(|v| v.as_str())
        .unwrap_or("skill");
    format!("正在加载 {}", s)
}
fn plugin_label(args: &Value) -> String {
    let s = args
        .get("plugin")
        .and_then(|v| v.as_str())
        .unwrap_or("plugin");
    format!("正在加载 {}", s)
}

// ---------------------------------------------------------------------------
// 核心实现：路径 / 补丁 / 花括号
// ---------------------------------------------------------------------------

fn is_binary_file(path: &Path) -> Result<bool, String> {
    let mut file = fs::File::open(path).map_err(|e| format!("无法打开文件: {}", e))?;
    let mut buffer = [0u8; 8192];
    let bytes_read = file
        .read(&mut buffer)
        .map_err(|e| format!("无法读取文件: {}", e))?;
    Ok(buffer[..bytes_read].contains(&0))
}

fn make_patch(original: &str, updated: &str) -> Vec<StructuredPatchHunk> {
    let mut lines = Vec::new();
    for line in original.lines() {
        lines.push(format!("-{line}"));
    }
    for line in updated.lines() {
        lines.push(format!("+{line}"));
    }
    vec![StructuredPatchHunk {
        old_start: 1,
        old_lines: original.lines().count(),
        new_start: 1,
        new_lines: updated.lines().count(),
        lines,
    }]
}

fn normalize_path(path: &str) -> Result<PathBuf, String> {
    let c = if Path::new(path).is_absolute() {
        PathBuf::from(path)
    } else {
        std::env::current_dir()
            .map_err(|e| format!("无法获取当前目录: {}", e))?
            .join(path)
    };
    c.canonicalize()
        .map_err(|e| format!("路径无法解析: {} ({})", e, c.display()))
}

fn normalize_path_allow_missing(path: &str) -> Result<PathBuf, String> {
    let c = if Path::new(path).is_absolute() {
        PathBuf::from(path)
    } else {
        std::env::current_dir()
            .map_err(|e| format!("无法获取当前目录: {}", e))?
            .join(path)
    };
    if c.exists() {
        return c
            .canonicalize()
            .map_err(|e| format!("路径无法解析: {} ({})", e, c.display()));
    }
    if let (Some(p), Some(n)) = (c.parent(), c.file_name()) {
        let cp = p.canonicalize().unwrap_or_else(|_| p.to_path_buf());
        return Ok(cp.join(n));
    }
    Ok(c)
}

fn user_home() -> PathBuf {
    #[cfg(target_os = "windows")]
    {
        std::env::var("USERPROFILE")
            .ok()
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("C:\\"))
    }
    #[cfg(not(target_os = "windows"))]
    {
        std::env::var("HOME")
            .ok()
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("/"))
    }
}

fn expand_braces(pattern: &str) -> Vec<String> {
    let Some(open) = pattern.find('{') else {
        return vec![pattern.to_owned()];
    };
    let Some(close) = pattern[open..].find('}').map(|i| open + i) else {
        return vec![pattern.to_owned()];
    };
    let prefix = &pattern[..open];
    let suffix = &pattern[close + 1..];
    pattern[open + 1..close]
        .split(',')
        .flat_map(|alt| expand_braces(&format!("{prefix}{alt}{suffix}")))
        .collect()
}

fn derive_glob_walk_root(pattern: &str) -> PathBuf {
    let path = Path::new(pattern);
    let mut prefix = PathBuf::new();
    let mut saw = false;
    for c in path.components() {
        let t = c.as_os_str().to_string_lossy();
        if t.contains('*') || t.contains('?') || t.contains('[') {
            break;
        }
        prefix.push(c.as_os_str());
        saw = true;
    }
    if saw {
        prefix
    } else {
        std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
    }
}

// ---------------------------------------------------------------------------
// read_file
// ---------------------------------------------------------------------------

fn read_file_impl(args: Value) -> String {
    let input: ReadFileInput = match serde_json::from_value(args) {
        Ok(v) => v,
        Err(e) => return jerr(format!("参数解析失败: {e}")),
    };
    let resolved = match normalize_path(&input.file_path) {
        Ok(p) => p,
        Err(e) => return jerr(e),
    };
    let rs = resolved.to_string_lossy();
    let metadata = match fs::metadata(&resolved) {
        Ok(m) => m,
        Err(e) => return jerr(format!("无法获取文件信息: {e}")),
    };
    if metadata.len() > MAX_READ_SIZE {
        return jerr(format!(
            "文件过大 ({} > {} bytes)",
            metadata.len(),
            MAX_READ_SIZE
        ));
    }
    match is_binary_file(&resolved) {
        Ok(true) => return jerr("文件似乎是二进制文件"),
        Err(e) => return jerr(e),
        _ => {}
    }
    let content = match fs::read_to_string(&resolved) {
        Ok(c) => c,
        Err(e) => return jerr(format!("读取失败: {e}")),
    };
    let lines: Vec<&str> = content.lines().collect();
    let total = lines.len();
    let start = input.offset.unwrap_or(0).min(total);
    let end = input
        .limit
        .map_or(total, |l| start.saturating_add(l).min(total));
    let selected = lines[start..end].join("\n");
    let o = ReadFileOutput {
        kind: "text".to_string(),
        file: TextFilePayload {
            file_path: rs.to_string(),
            content: selected,
            num_lines: end.saturating_sub(start),
            start_line: start.saturating_add(1),
            total_lines: total,
        },
    };
    serde_json::to_string(&o).unwrap_or_else(|e| jerr(format!("序列化失败: {e}")))
}

// ---------------------------------------------------------------------------
// write_file
// ---------------------------------------------------------------------------

fn write_file_impl(args: Value) -> String {
    let input: WriteFileInput = match serde_json::from_value(args) {
        Ok(v) => v,
        Err(e) => return jerr(format!("参数解析失败: {e}")),
    };
    if input.content.len() > MAX_WRITE_SIZE {
        return jerr(format!(
            "内容过大 ({} > {} bytes)",
            input.content.len(),
            MAX_WRITE_SIZE
        ));
    }
    let ap = match normalize_path_allow_missing(&input.file_path) {
        Ok(p) => p,
        Err(e) => return jerr(e),
    };
    let original = fs::read_to_string(&ap).ok();
    if let Some(parent) = ap.parent() {
        if let Err(e) = fs::create_dir_all(parent) {
            return jerr(format!("无法创建父目录: {e}"));
        }
    }
    if let Err(e) = fs::write(&ap, &input.content) {
        return jerr(format!("写入失败: {e}"));
    }
    let kind = if original.is_some() {
        "update"
    } else {
        "create"
    };
    let o = WriteFileOutput {
        kind: kind.to_string(),
        file_path: ap.to_string_lossy().into_owned(),
        content: input.content.clone(),
        structured_patch: make_patch(original.as_deref().unwrap_or(""), &input.content),
        original_file: original,
    };
    serde_json::to_string(&o).unwrap_or_else(|e| jerr(format!("序列化失败: {e}")))
}

// ---------------------------------------------------------------------------
// edit_file
// ---------------------------------------------------------------------------

fn edit_file_impl(args: Value) -> String {
    let input: EditFileInput = match serde_json::from_value(args) {
        Ok(v) => v,
        Err(e) => return jerr(format!("参数解析失败: {e}")),
    };
    let ap = match normalize_path(&input.file_path) {
        Ok(p) => p,
        Err(e) => return jerr(e),
    };
    let original = match fs::read_to_string(&ap) {
        Ok(c) => c,
        Err(e) => return jerr(format!("读取失败: {e}")),
    };
    if input.old_string == input.new_string {
        return jerr("old_string 和 new_string 不能相同");
    }
    if !original.contains(&input.old_string) {
        return jerr("old_string 在文件中未找到");
    }
    let updated = if input.replace_all {
        original.replace(&input.old_string, &input.new_string)
    } else {
        original.replacen(&input.old_string, &input.new_string, 1)
    };
    if let Err(e) = fs::write(&ap, &updated) {
        return jerr(format!("写入失败: {e}"));
    }
    let o = EditFileOutput {
        file_path: ap.to_string_lossy().into_owned(),
        old_string: input.old_string,
        new_string: input.new_string,
        original_file: original.clone(),
        structured_patch: make_patch(&original, &updated),
        replace_all: input.replace_all,
    };
    serde_json::to_string(&o).unwrap_or_else(|e| jerr(format!("序列化失败: {e}")))
}

// ---------------------------------------------------------------------------
// glob_search
// ---------------------------------------------------------------------------

fn glob_search_impl(args: Value) -> String {
    let input: GlobSearchInput = match serde_json::from_value(args) {
        Ok(v) => v,
        Err(e) => return jerr(format!("参数解析失败: {e}")),
    };
    let started = Instant::now();
    let base_dir = match input.path.as_deref() {
        Some(p) => match normalize_path(p) {
            Ok(d) => d,
            Err(e) => return jerr(e),
        },
        None => user_home(),
    };
    let sp = if Path::new(&input.pattern).is_absolute() {
        input.pattern.clone()
    } else {
        base_dir.join(&input.pattern).to_string_lossy().into_owned()
    };
    let expanded = expand_braces(&sp);
    let mut seen: HashSet<PathBuf> = HashSet::new();
    let mut matches: Vec<PathBuf> = Vec::new();
    for pat in &expanded {
        let compiled = match glob::Pattern::new(pat) {
            Ok(c) => c,
            Err(e) => return jerr(format!("无效的 glob 模式: {e}")),
        };
        let walker = walkdir::WalkDir::new(derive_glob_walk_root(pat))
            .into_iter()
            .filter_entry(|e| {
                !(e.file_type().is_dir()
                    && e.file_name()
                        .to_str()
                        .is_some_and(|n| GLOB_IGNORED_DIRS.contains(&n)))
            });
        for entry in walker.flatten() {
            if !entry.file_type().is_file() {
                continue;
            }
            let c = entry.path();
            if compiled.matches_path(c) && seen.insert(c.to_path_buf()) {
                matches.push(c.to_path_buf());
            }
        }
    }
    matches.sort_by_key(|p| fs::metadata(p).and_then(|m| m.modified()).ok().map(Reverse));
    let truncated = matches.len() > 100;
    let filenames: Vec<String> = matches
        .into_iter()
        .take(100)
        .map(|p| p.to_string_lossy().into_owned())
        .collect();
    let o = GlobSearchOutput {
        duration_ms: started.elapsed().as_millis(),
        num_files: filenames.len(),
        filenames,
        truncated,
    };
    serde_json::to_string(&o).unwrap_or_else(|e| jerr(format!("序列化失败: {e}")))
}

// ---------------------------------------------------------------------------
// grep_search
// ---------------------------------------------------------------------------

fn collect_search_files(base: &Path) -> Result<Vec<PathBuf>, String> {
    if base.is_file() {
        return Ok(vec![base.to_path_buf()]);
    }
    let mut files = Vec::new();
    for entry in walkdir::WalkDir::new(base) {
        let entry = entry.map_err(|e| format!("遍历目录失败: {e}"))?;
        if entry.file_type().is_file() {
            files.push(entry.path().to_path_buf());
        }
    }
    Ok(files)
}

fn match_optional_filters(path: &Path, gf: Option<&glob::Pattern>, ft: Option<&str>) -> bool {
    if let Some(gf) = gf {
        let ps = path.to_string_lossy();
        if !gf.matches(&*ps) && !gf.matches_path(path) {
            return false;
        }
    }
    if let Some(ft) = ft {
        if path.extension().and_then(|e| e.to_str()) != Some(ft) {
            return false;
        }
    }
    true
}

fn apply_limit<T>(
    items: Vec<T>,
    limit: Option<usize>,
    offset: Option<usize>,
) -> (Vec<T>, Option<usize>, Option<usize>) {
    let ov = offset.unwrap_or(0);
    let mut items: Vec<T> = items.into_iter().skip(ov).collect();
    let el = limit.unwrap_or(250);
    if el == 0 {
        return (items, None, (ov > 0).then_some(ov));
    }
    let truncated = items.len() > el;
    items.truncate(el);
    (items, truncated.then_some(el), (ov > 0).then_some(ov))
}

fn grep_search_impl(args: Value) -> String {
    let input: GrepSearchInput = match serde_json::from_value(args) {
        Ok(v) => v,
        Err(e) => return jerr(format!("参数解析失败: {e}")),
    };
    let base_path = match input.path.as_deref() {
        Some(p) => match normalize_path(p) {
            Ok(d) => d,
            Err(e) => return jerr(e),
        },
        None => user_home(),
    };
    let regex = match regex::RegexBuilder::new(&input.pattern)
        .case_insensitive(input.case_insensitive.unwrap_or(false))
        .dot_matches_new_line(input.multiline.unwrap_or(false))
        .build()
    {
        Ok(r) => r,
        Err(e) => return jerr(format!("无效的正则表达式: {e}")),
    };
    let glob_filter: Option<glob::Pattern> = match input.glob.as_deref() {
        Some(g) => match glob::Pattern::new(g) {
            Ok(p) => Some(p),
            Err(e) => return jerr(format!("无效的 glob 模式: {e}")),
        },
        None => None,
    };
    let file_type = input.file_type.as_deref();
    let output_mode = input
        .output_mode
        .clone()
        .unwrap_or_else(|| "files_with_matches".to_string());
    let ctx = input.context.or(input.context_short).unwrap_or(0);
    let files = match collect_search_files(&base_path) {
        Ok(f) => f,
        Err(e) => return jerr(e),
    };
    let mut filenames = Vec::new();
    let mut content_lines = Vec::new();
    let mut total_matches = 0usize;
    for fp in &files {
        if !match_optional_filters(fp, glob_filter.as_ref(), file_type) {
            continue;
        }
        let fc = match fs::read_to_string(fp) {
            Ok(c) => c,
            Err(_) => continue,
        };
        if output_mode == "count" {
            let count = regex.find_iter(&fc).count();
            if count > 0 {
                filenames.push(fp.to_string_lossy().into_owned());
                total_matches += count;
            }
            continue;
        }
        let lines: Vec<&str> = fc.lines().collect();
        let mut matched: Vec<usize> = Vec::new();
        for (idx, line) in lines.iter().enumerate() {
            if regex.is_match(line) {
                total_matches += 1;
                matched.push(idx);
            }
        }
        if matched.is_empty() {
            continue;
        }
        filenames.push(fp.to_string_lossy().into_owned());
        if output_mode == "content" {
            for idx in matched {
                let s = idx.saturating_sub(input.before.unwrap_or(ctx));
                let e = (idx + input.after.unwrap_or(ctx) + 1).min(lines.len());
                for (cur, line) in lines.iter().enumerate().take(e).skip(s) {
                    let prefix = if input.line_numbers.unwrap_or(true) {
                        format!("{}:{}:", fp.to_string_lossy(), cur + 1)
                    } else {
                        format!("{}:", fp.to_string_lossy())
                    };
                    content_lines.push(format!("{prefix}{line}"));
                }
            }
        }
    }
    let (filenames, al, ao) = apply_limit(filenames, input.head_limit, input.offset);
    if output_mode == "content" {
        let (lines, ll, lo) = apply_limit(content_lines, input.head_limit, input.offset);
        let o = GrepSearchOutput {
            mode: Some("content".to_string()),
            num_files: filenames.len(),
            filenames,
            num_lines: Some(lines.len()),
            content: Some(lines.join("\n")),
            num_matches: None,
            applied_limit: ll,
            applied_offset: lo,
        };
        return serde_json::to_string(&o).unwrap_or_else(|e| jerr(format!("序列化失败: {e}")));
    }
    let o = GrepSearchOutput {
        mode: Some(output_mode.clone()),
        num_files: filenames.len(),
        filenames,
        content: None,
        num_lines: None,
        num_matches: (output_mode == "count").then_some(total_matches),
        applied_limit: al,
        applied_offset: ao,
    };
    serde_json::to_string(&o).unwrap_or_else(|e| jerr(format!("序列化失败: {e}")))
}

// ---------------------------------------------------------------------------
// web_fetch
// ---------------------------------------------------------------------------

fn build_http_client() -> Result<reqwest::blocking::Client, String> {
    reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(20))
        .redirect(reqwest::redirect::Policy::limited(10))
        .user_agent("otherone-agent/0.1")
        .build()
        .map_err(|e| format!("创建 HTTP 客户端失败: {e}"))
}

fn normalize_url(url: &str) -> Result<String, String> {
    let mut parsed = reqwest::Url::parse(url).map_err(|e| format!("无效的 URL: {e}"))?;
    if parsed.scheme() == "http" {
        let host = parsed.host_str().unwrap_or_default();
        if host != "localhost" && host != "127.0.0.1" && host != "::1" {
            parsed
                .set_scheme("https")
                .map_err(|()| "无法升级 URL 到 https".to_string())?;
        }
    }
    Ok(parsed.to_string())
}

fn web_fetch_impl(args: Value) -> String {
    let input: WebFetchInput = match serde_json::from_value(args) {
        Ok(v) => v,
        Err(e) => return jerr(format!("参数解析失败: {e}")),
    };
    let started = Instant::now();
    let client = match build_http_client() {
        Ok(c) => c,
        Err(e) => return jerr(e),
    };
    let request_url = match normalize_url(&input.url) {
        Ok(u) => u,
        Err(e) => return jerr(e),
    };
    let response: reqwest::blocking::Response = match client.get(&request_url).send() {
        Ok(r) => r,
        Err(e) => return jerr(format!("请求失败: {e}")),
    };
    let status = response.status();
    let final_url = response.url().to_string();
    let code = status.as_u16();
    let code_text = status.canonical_reason().unwrap_or("Unknown").to_string();
    let ct = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v: &reqwest::header::HeaderValue| v.to_str().ok())
        .unwrap_or_default()
        .to_string();
    let body: String = match response.text() {
        Ok(b) => b,
        Err(e) => return jerr(format!("读取响应失败: {e}")),
    };
    let bytes = body.len();
    let normalized = if ct.contains("html") {
        html_to_text(&body)
    } else {
        body.trim().to_string()
    };
    let preview = collapse_ws(&normalized);
    let excerpt: String = preview.chars().take(1500).collect();
    let result = format!("Fetched {final_url}\n{excerpt}");
    let o = WebFetchOutput {
        bytes,
        code,
        code_text,
        result,
        duration_ms: started.elapsed().as_millis(),
        url: final_url,
    };
    serde_json::to_string(&o).unwrap_or_else(|e| jerr(format!("序列化失败: {e}")))
}

fn html_to_text(html: &str) -> String {
    let mut s = html.to_string();
    for tag in ["script", "style", "noscript", "head"] {
        let re = regex::Regex::new(&format!("(?is)<{tag}[^>]*>.*?</{tag}>")).unwrap();
        s = re.replace_all(&s, "").to_string();
    }
    s = s.replace("</p>", "\n").replace("<br", "\n<br");
    s = regex::Regex::new(r"<[^>]+>")
        .unwrap()
        .replace_all(&s, "")
        .to_string();
    s = regex::Regex::new(r"[ \t]+")
        .unwrap()
        .replace_all(&s, " ")
        .to_string();
    s = regex::Regex::new(r"\n{3,}")
        .unwrap()
        .replace_all(&s, "\n\n")
        .to_string();
    s.trim().to_string()
}

fn collapse_ws(s: &str) -> String {
    regex::Regex::new(r"\s+")
        .unwrap()
        .replace_all(s.trim(), " ")
        .to_string()
}

// ---------------------------------------------------------------------------
// web_search
// ---------------------------------------------------------------------------

fn build_search_url(query: &str) -> Result<String, String> {
    let mut url = reqwest::Url::parse("https://html.duckduckgo.com/html/")
        .map_err(|e| format!("URL 解析失败: {e}"))?;
    url.query_pairs_mut().append_pair("q", query);
    Ok(url.to_string())
}

fn host_matches(url: &str, domains: &[String]) -> bool {
    let Ok(parsed) = reqwest::Url::parse(url) else {
        return false;
    };
    let Some(host) = parsed.host_str() else {
        return false;
    };
    domains.iter().any(|d| {
        let d = d
            .trim_start_matches("https://")
            .trim_start_matches("http://")
            .trim_end_matches('/');
        host.eq_ignore_ascii_case(d) || host.ends_with(&format!(".{d}"))
    })
}

fn extract_quoted_value(s: &str) -> Option<(String, &str)> {
    let s = s.trim_start();
    let delim = s.chars().next()?;
    if delim != '"' && delim != '\'' {
        return None;
    }
    let mut i = 1;
    let chars: Vec<char> = s.chars().collect();
    while i < chars.len() {
        if chars[i] == '\\' {
            i += 2;
            continue;
        }
        if chars[i] == delim {
            return Some((chars[1..i].iter().collect(), &s[i + 1..]));
        }
        i += 1;
    }
    None
}

fn decode_ddg_redirect(url: &str) -> Option<String> {
    let parsed = reqwest::Url::parse(url).ok()?;
    parsed
        .query_pairs()
        .find(|(k, _)| k == "uddg")
        .map(|(_, v)| {
            let s = v
                .replace("%3A", ":")
                .replace("%2F", "/")
                .replace("%3F", "?")
                .replace("%3D", "=")
                .replace("%26", "&")
                .replace("%25", "%");
            let s = s
                .replace("%20", " ")
                .replace("%2B", "+")
                .replace("%23", "#");
            s
        })
}

fn extract_search_hits(html: &str) -> Vec<SearchHit> {
    let mut hits = Vec::new();
    let mut remaining = html;
    while let Some(start) = remaining.find("result__a") {
        let after = &remaining[start..];
        let Some(href_idx) = after.find("href=") else {
            remaining = &after[1..];
            continue;
        };
        let href_slice = &after[href_idx + 5..];
        let Some((url, rest)) = extract_quoted_value(href_slice) else {
            remaining = &after[1..];
            continue;
        };
        let Some(close_idx) = rest.find('>') else {
            remaining = &after[1..];
            continue;
        };
        let after_tag = &rest[close_idx + 1..];
        let Some(end_idx) = after_tag.find("</a>") else {
            remaining = &after[1..];
            continue;
        };
        let title = html_to_text(&after_tag[..end_idx]);
        if let Some(decoded) = decode_ddg_redirect(&url) {
            hits.push(SearchHit {
                title: title.trim().to_string(),
                url: decoded,
            });
        }
        remaining = &after_tag[end_idx + 4..];
    }
    hits
}

fn web_search_impl(args: Value) -> String {
    let input: WebSearchInput = match serde_json::from_value(args) {
        Ok(v) => v,
        Err(e) => return jerr(format!("参数解析失败: {e}")),
    };
    let started = Instant::now();
    let client = match build_http_client() {
        Ok(c) => c,
        Err(e) => return jerr(e),
    };
    let search_url = match build_search_url(&input.query) {
        Ok(u) => u,
        Err(e) => return jerr(e),
    };
    let response = match client.get(&search_url).send() {
        Ok(r) => r,
        Err(e) => return jerr(format!("搜索请求失败: {e}")),
    };
    let html = match response.text() {
        Ok(h) => h,
        Err(e) => return jerr(format!("读取响应失败: {e}")),
    };
    let mut hits = extract_search_hits(&html);
    if let Some(allowed) = input.allowed_domains.as_ref() {
        hits.retain(|h| host_matches(&h.url, allowed));
    }
    if let Some(blocked) = input.blocked_domains.as_ref() {
        hits.retain(|h| !host_matches(&h.url, blocked));
    }
    // dedupe
    {
        let mut seen = HashSet::new();
        hits.retain(|h| seen.insert(h.url.clone()));
    }
    hits.truncate(8);
    let summary = if hits.is_empty() {
        format!("未找到与 \"{}\" 匹配的搜索结果。", input.query)
    } else {
        hits.iter()
            .map(|h| format!("- [{}]({})", h.title, h.url))
            .collect::<Vec<_>>()
            .join("\n")
    };
    let o = WebSearchOutput {
        query: input.query.clone(),
        results: hits,
        summary,
        duration_seconds: started.elapsed().as_secs_f64(),
    };
    serde_json::to_string(&o).unwrap_or_else(|e| jerr(format!("序列化失败: {e}")))
}

// ---------------------------------------------------------------------------
// bash / powershell 共享执行
// ---------------------------------------------------------------------------

fn execute_shell(shell: &str, args: &[&str], timeout_ms: Option<u64>) -> ShellOutput {
    let mut cmd = Command::new(shell);
    cmd.args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let started = Instant::now();
    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            return ShellOutput {
                stdout: String::new(),
                stderr: format!("启动进程失败: {e}"),
                exit_code: None,
                interrupted: false,
            }
        }
    };
    let timeout_dur = timeout_ms.map(std::time::Duration::from_millis);
    let output = loop {
        if let Ok(Some(status)) = child.try_wait() {
            let out = child
                .wait_with_output()
                .unwrap_or_else(|_| std::process::Output {
                    status,
                    stdout: Vec::new(),
                    stderr: Vec::new(),
                });
            break (out, false);
        }
        if let Some(td) = timeout_dur {
            if started.elapsed() >= td {
                let _ = child.kill();
                let out = child
                    .wait_with_output()
                    .unwrap_or_else(|_| std::process::Output {
                        status: std::process::ExitStatus::default(),
                        stdout: Vec::new(),
                        stderr: Vec::new(),
                    });
                break (out, true);
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    };
    let (output, interrupted) = output;
    ShellOutput {
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        exit_code: output.status.code(),
        interrupted,
    }
}

fn cmd_available(name: &str) -> bool {
    std::process::Command::new(name)
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

fn bash_impl(args: Value) -> String {
    let input: ShellInput = match serde_json::from_value(args) {
        Ok(v) => v,
        Err(e) => return jerr(format!("参数解析失败: {e}")),
    };
    let o = execute_shell("bash", &["-lc", &input.command], input.timeout);
    serde_json::to_string(&o).unwrap_or_else(|e| jerr(format!("序列化失败: {e}")))
}

fn powershell_impl(args: Value) -> String {
    let input: ShellInput = match serde_json::from_value(args) {
        Ok(v) => v,
        Err(e) => return jerr(format!("参数解析失败: {e}")),
    };
    let shell = if cmd_available("pwsh") {
        "pwsh"
    } else if cmd_available("powershell") {
        "powershell"
    } else {
        return jerr("未找到 PowerShell (pwsh 或 powershell)");
    };
    let o = execute_shell(shell, &["-Command", &input.command], input.timeout);
    serde_json::to_string(&o).unwrap_or_else(|e| jerr(format!("序列化失败: {e}")))
}

// ---------------------------------------------------------------------------
// REPL
// ---------------------------------------------------------------------------

fn repl_impl(args: Value) -> String {
    let input: ReplInput = match serde_json::from_value(args) {
        Ok(v) => v,
        Err(e) => return jerr(format!("参数解析失败: {e}")),
    };
    if input.code.trim().is_empty() {
        return jerr("code 不能为空");
    }
    let (shell, args): (&str, &[&str]) = match input.language.trim().to_ascii_lowercase().as_str() {
        "py" | "python" => {
            let p = if cmd_available("python3") {
                "python3"
            } else if cmd_available("python") {
                "python"
            } else {
                return jerr("未找到 Python 运行时");
            };
            (p, &["-c"][..])
        }
        "js" | "javascript" | "node" => {
            let n = if cmd_available("node") {
                "node"
            } else {
                return jerr("未找到 Node.js 运行时");
            };
            (n, &["-e"][..])
        }
        "sh" | "shell" | "bash" => {
            let b = if cmd_available("bash") {
                "bash"
            } else if cmd_available("sh") {
                "sh"
            } else {
                return jerr("未找到 Shell 运行时");
            };
            (b, &["-lc"][..])
        }
        other => return jerr(format!("不支持的语言: {other}")),
    };
    let mut cmd = Command::new(shell);
    cmd.args(args)
        .arg(&input.code)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let started = Instant::now();
    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => return jerr(format!("启动进程失败: {e}")),
    };
    let output = loop {
        if let Ok(Some(status)) = child.try_wait() {
            let out = child
                .wait_with_output()
                .unwrap_or_else(|_| std::process::Output {
                    status,
                    stdout: Vec::new(),
                    stderr: Vec::new(),
                });
            break out;
        }
        if let Some(tm) = input.timeout_ms {
            if started.elapsed() >= std::time::Duration::from_millis(tm) {
                let _ = child.kill();
                child.wait_with_output().ok();
                return jerr(format!("REPL 执行超时 ({tm}ms)"));
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    };
    let o = ReplOutput {
        language: input.language,
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        exit_code: output.status.code(),
        duration_ms: started.elapsed().as_millis(),
    };
    serde_json::to_string(&o).unwrap_or_else(|e| jerr(format!("序列化失败: {e}")))
}

// ---------------------------------------------------------------------------
// 辅助
// ---------------------------------------------------------------------------

fn jerr(msg: impl ToString) -> String {
    serde_json::json!({"error": msg.to_string()}).to_string()
}

// ---------------------------------------------------------------------------
// Skill 工具 — 渐进式披露 Tier 2: AI 调用此工具加载 skill 的完整 SKILL.md
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
struct SkillToolInput {
    skill: String,
}

fn skill_impl(args: Value) -> String {
    let input: SkillToolInput = match serde_json::from_value(args) {
        Ok(v) => v,
        Err(e) => return jerr(format!("参数解析失败: {e}")),
    };
    let contents = crate::plugins::get_installed_skills_content();
    let content = match contents.iter().find(|c| c.name == input.skill) {
        Some(c) => c,
        None => {
            return jerr(format!(
                "未找到 skill '{}'。请检查 skill 是否已安装，可用的 skills 列表见系统提示。",
                input.skill
            ))
        }
    };
    serde_json::to_string(&serde_json::json!({
        "skill": content.name,
        "description": content.description,
        "path": content.file_path,
        "body": content.body,
    }))
    .unwrap_or_else(|e| jerr(format!("序列化失败: {e}")))
}

// ---------------------------------------------------------------------------
// Plugin 工具 — 加载插件（SKILL.md + 二进制路径）
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
struct PluginToolInput {
    plugin: String,
}

fn plugin_impl(args: Value) -> String {
    let input: PluginToolInput = match serde_json::from_value(args) {
        Ok(v) => v,
        Err(e) => return jerr(format!("参数解析失败: {e}")),
    };
    let entry = match crate::plugins::get_plugin_metadata(&input.plugin) {
        Some(e) => e,
        None => {
            return jerr(format!(
                "未找到插件 '{}'。请检查是否已安装，可用的插件列表见系统提示。",
                input.plugin
            ))
        }
    };

    let body = crate::plugins::get_plugin_body(&input.plugin).unwrap_or_default();

    serde_json::to_string(&serde_json::json!({
        "name": entry.name,
        "description": entry.description,
        "skill_path": entry.file_path,
        "bin_path": entry.bin_path,
        "bin_dir": entry.bin_dir,
        "body": body,
    }))
    .unwrap_or_else(|e| jerr(format!("序列化失败: {e}")))
}

// ---------------------------------------------------------------------------
// 工具注册表
// ---------------------------------------------------------------------------

fn build_tool_metas() -> Vec<ToolMeta> {
    vec![
        ToolMeta {
            name: "read_file".into(),
            display_name: "读取文件".into(),
            definition: read_file_def(),
            realization: Box::new(read_file_impl),
            label_fn: Box::new(read_file_label),
            expandable: false,
        },
        ToolMeta {
            name: "write_file".into(),
            display_name: "写入文件".into(),
            definition: write_file_def(),
            realization: Box::new(write_file_impl),
            label_fn: Box::new(write_file_label),
            expandable: false,
        },
        ToolMeta {
            name: "edit_file".into(),
            display_name: "编辑文件".into(),
            definition: edit_file_def(),
            realization: Box::new(edit_file_impl),
            label_fn: Box::new(edit_file_label),
            expandable: false,
        },
        ToolMeta {
            name: "glob_search".into(),
            display_name: "搜索文件".into(),
            definition: glob_search_def(),
            realization: Box::new(glob_search_impl),
            label_fn: Box::new(glob_search_label),
            expandable: true,
        },
        ToolMeta {
            name: "grep_search".into(),
            display_name: "内容搜索".into(),
            definition: grep_search_def(),
            realization: Box::new(grep_search_impl),
            label_fn: Box::new(grep_search_label),
            expandable: true,
        },
        ToolMeta {
            name: "web_fetch".into(),
            display_name: "网页抓取".into(),
            definition: web_fetch_def(),
            realization: Box::new(web_fetch_impl),
            label_fn: Box::new(web_fetch_label),
            expandable: true,
        },
        ToolMeta {
            name: "web_search".into(),
            display_name: "网页搜索".into(),
            definition: web_search_def(),
            realization: Box::new(web_search_impl),
            label_fn: Box::new(web_search_label),
            expandable: true,
        },
        ToolMeta {
            name: "bash".into(),
            display_name: "执行命令".into(),
            definition: bash_def(),
            realization: Box::new(bash_impl),
            label_fn: Box::new(bash_label),
            expandable: true,
        },
        ToolMeta {
            name: "powershell".into(),
            display_name: "执行命令".into(),
            definition: powershell_def(),
            realization: Box::new(powershell_impl),
            label_fn: Box::new(powershell_label),
            expandable: true,
        },
        ToolMeta {
            name: "repl".into(),
            display_name: "代码执行".into(),
            definition: repl_def(),
            realization: Box::new(repl_impl),
            label_fn: Box::new(repl_label),
            expandable: true,
        },
        ToolMeta {
            name: "skill".into(),
            display_name: "加载技能".into(),
            definition: skill_def(),
            realization: Box::new(skill_impl),
            label_fn: Box::new(skill_label),
            expandable: false,
        },
        ToolMeta {
            name: "plugin_tool".into(),
            display_name: "加载插件".into(),
            definition: plugin_tool_def(),
            realization: Box::new(plugin_impl),
            label_fn: Box::new(plugin_label),
            expandable: false,
        },
    ]
}

// ---------------------------------------------------------------------------
// 公开 API
// ---------------------------------------------------------------------------

#[allow(dead_code)]
pub fn build_tools() -> (
    Vec<Tool>,
    HashMap<String, Box<dyn Fn(Value) -> String + Send + Sync>>,
) {
    let metas = build_tool_metas();
    let tools: Vec<Tool> = metas.iter().map(|m| m.definition.clone()).collect();
    let mut realize: HashMap<String, Box<dyn Fn(Value) -> String + Send + Sync>> = HashMap::new();
    for meta in metas {
        realize.insert(meta.name.clone(), meta.realization);
    }
    (tools, realize)
}

pub fn build_tools_for_session(
    app: AppHandle,
    session_id: String,
) -> (
    Vec<Tool>,
    HashMap<String, Box<dyn Fn(Value) -> String + Send + Sync>>,
) {
    let metas = build_tool_metas();
    let tools: Vec<Tool> = metas.iter().map(|m| m.definition.clone()).collect();
    let mut realize: HashMap<String, Box<dyn Fn(Value) -> String + Send + Sync>> = HashMap::new();

    for meta in metas {
        let ToolMeta {
            name, realization, ..
        } = meta;
        if name == "edit_file" {
            let app = app.clone();
            let session_id = session_id.clone();
            realize.insert(
                name,
                Box::new(move |args| {
                    let result = realization(args);
                    if let Err(error) =
                        crate::artifacts::record_edit_file_artifact(&app, &session_id, &result)
                    {
                        eprintln!("[artifacts] record edit_file failed: {error}");
                    }
                    result
                }),
            );
        } else {
            realize.insert(name, realization);
        }
    }

    (tools, realize)
}

pub fn build_weixin_safe_tools() -> (
    Vec<Tool>,
    HashMap<String, Box<dyn Fn(Value) -> String + Send + Sync>>,
) {
    const SAFE_TOOL_NAMES: &[&str] = &["web_fetch", "web_search", "skill", "plugin_tool"];

    let metas: Vec<ToolMeta> = build_tool_metas()
        .into_iter()
        .filter(|meta| SAFE_TOOL_NAMES.contains(&meta.name.as_str()))
        .collect();
    let tools: Vec<Tool> = metas.iter().map(|meta| meta.definition.clone()).collect();
    let mut realize: HashMap<String, Box<dyn Fn(Value) -> String + Send + Sync>> = HashMap::new();

    for meta in metas {
        realize.insert(meta.name, meta.realization);
    }

    (tools, realize)
}

pub fn tool_label(name: &str, args: &Value) -> String {
    build_tool_metas()
        .iter()
        .find(|m| m.name == name)
        .map(|m| (m.label_fn)(args))
        .unwrap_or_else(|| format!("正在执行 {}", name))
}

pub fn tool_expandable(name: &str) -> bool {
    build_tool_metas()
        .iter()
        .any(|m| m.name == name && m.expandable)
}
