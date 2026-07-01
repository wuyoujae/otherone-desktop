use otherone::ai::types::{FunctionDefinition, Tool};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::cmp::Reverse;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::artifacts::{self, FileArtifact};
use crate::{plugins, workflow};

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

pub type ToolRealizationMap = HashMap<String, Box<dyn Fn(Value) -> String + Send + Sync>>;
pub type ArtifactSink = Arc<dyn Fn(FileArtifact) + Send + Sync>;

struct ToolMeta {
    name: &'static str,
    definition: Tool,
    label_fn: fn(&Value) -> String,
    expandable: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct TextFilePayload {
    file_path: String,
    content: String,
    num_lines: usize,
    start_line: usize,
    total_lines: usize,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ReadFileOutput {
    #[serde(rename = "type")]
    kind: String,
    file: TextFilePayload,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct StructuredPatchHunk {
    old_start: usize,
    old_lines: usize,
    new_start: usize,
    new_lines: usize,
    lines: Vec<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct WriteFileOutput {
    #[serde(rename = "type")]
    kind: String,
    file_path: String,
    content: String,
    structured_patch: Vec<StructuredPatchHunk>,
    original_file: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct EditFileOutput {
    file_path: String,
    old_string: String,
    new_string: String,
    original_file: String,
    structured_patch: Vec<StructuredPatchHunk>,
    replace_all: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct GlobSearchOutput {
    duration_ms: u128,
    num_files: usize,
    filenames: Vec<String>,
    truncated: bool,
}

#[derive(Serialize)]
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

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct WebFetchOutput {
    bytes: usize,
    code: u16,
    code_text: String,
    result: String,
    duration_ms: u128,
    url: String,
}

#[derive(Serialize)]
struct SearchHit {
    title: String,
    url: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct WebSearchOutput {
    query: String,
    results: Vec<SearchHit>,
    summary: String,
    duration_seconds: f64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ShellOutput {
    stdout: String,
    stderr: String,
    exit_code: Option<i32>,
    interrupted: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ReplOutput {
    language: String,
    stdout: String,
    stderr: String,
    exit_code: Option<i32>,
    duration_ms: u128,
}

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

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
struct SkillToolInput {
    skill: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
struct PluginToolInput {
    plugin: String,
}

pub fn build_tools_for_web_session(
    data_root: PathBuf,
    session_id: String,
    artifact_sink: Option<ArtifactSink>,
) -> (Vec<Tool>, ToolRealizationMap) {
    let workspace = Arc::new(workspace_root(&data_root));
    let data_root = Arc::new(data_root);
    let session_id = Arc::new(session_id);
    let sink = artifact_sink;
    let metas = build_tool_metas();
    let tools = metas.iter().map(|meta| meta.definition.clone()).collect();
    let mut realize: ToolRealizationMap = HashMap::new();

    insert_workspace_tool(&mut realize, "read_file", workspace.clone(), read_file_impl);
    insert_workspace_tool(&mut realize, "write_file", workspace.clone(), {
        let data_root = data_root.clone();
        let session_id = session_id.clone();
        let sink = sink.clone();
        move |workspace, args| {
            let result = write_file_impl(workspace, args);
            record_write_file_artifact(&data_root, &session_id, &result, sink.as_ref());
            result
        }
    });
    insert_workspace_tool(&mut realize, "edit_file", workspace.clone(), {
        let data_root = data_root.clone();
        let session_id = session_id.clone();
        let sink = sink.clone();
        move |workspace, args| {
            let result = edit_file_impl(workspace, args);
            record_edit_file_artifact(&data_root, &session_id, &result, sink.as_ref());
            result
        }
    });
    insert_workspace_tool(
        &mut realize,
        "glob_search",
        workspace.clone(),
        glob_search_impl,
    );
    insert_workspace_tool(
        &mut realize,
        "grep_search",
        workspace.clone(),
        grep_search_impl,
    );
    realize.insert("web_fetch".to_string(), Box::new(web_fetch_impl));
    realize.insert("web_search".to_string(), Box::new(web_search_impl));
    insert_workspace_tool(&mut realize, "bash", workspace.clone(), bash_impl);
    insert_workspace_tool(
        &mut realize,
        "powershell",
        workspace.clone(),
        powershell_impl,
    );
    insert_workspace_tool(&mut realize, "repl", workspace.clone(), repl_impl);
    insert_data_tool(
        &mut realize,
        "create_todo",
        data_root.clone(),
        create_todo_impl,
    );
    insert_data_tool(
        &mut realize,
        "list_todos",
        data_root.clone(),
        list_todos_impl,
    );
    insert_data_tool(
        &mut realize,
        "update_todo",
        data_root.clone(),
        update_todo_impl,
    );
    insert_data_tool(
        &mut realize,
        "delete_todo",
        data_root.clone(),
        delete_todo_impl,
    );
    insert_data_tool(&mut realize, "skill", data_root.clone(), skill_impl);
    insert_data_tool(&mut realize, "plugin_tool", data_root, plugin_impl);

    (tools, realize)
}

fn insert_workspace_tool<F>(
    map: &mut ToolRealizationMap,
    name: &str,
    workspace: Arc<PathBuf>,
    handler: F,
) where
    F: Fn(&Path, Value) -> String + Send + Sync + 'static,
{
    map.insert(
        name.to_string(),
        Box::new(move |args| handler(&workspace, args)),
    );
}

fn insert_data_tool<F>(
    map: &mut ToolRealizationMap,
    name: &str,
    data_root: Arc<PathBuf>,
    handler: F,
) where
    F: Fn(&Path, Value) -> String + Send + Sync + 'static,
{
    map.insert(
        name.to_string(),
        Box::new(move |args| handler(&data_root, args)),
    );
}

fn build_tool_metas() -> Vec<ToolMeta> {
    vec![
        ToolMeta {
            name: "read_file",
            definition: read_file_def(),
            label_fn: read_file_label,
            expandable: false,
        },
        ToolMeta {
            name: "write_file",
            definition: write_file_def(),
            label_fn: write_file_label,
            expandable: false,
        },
        ToolMeta {
            name: "edit_file",
            definition: edit_file_def(),
            label_fn: edit_file_label,
            expandable: false,
        },
        ToolMeta {
            name: "glob_search",
            definition: glob_search_def(),
            label_fn: glob_search_label,
            expandable: true,
        },
        ToolMeta {
            name: "grep_search",
            definition: grep_search_def(),
            label_fn: grep_search_label,
            expandable: true,
        },
        ToolMeta {
            name: "web_fetch",
            definition: web_fetch_def(),
            label_fn: web_fetch_label,
            expandable: true,
        },
        ToolMeta {
            name: "web_search",
            definition: web_search_def(),
            label_fn: web_search_label,
            expandable: true,
        },
        ToolMeta {
            name: "bash",
            definition: bash_def(),
            label_fn: bash_label,
            expandable: true,
        },
        ToolMeta {
            name: "powershell",
            definition: powershell_def(),
            label_fn: powershell_label,
            expandable: true,
        },
        ToolMeta {
            name: "repl",
            definition: repl_def(),
            label_fn: repl_label,
            expandable: true,
        },
        ToolMeta {
            name: "create_todo",
            definition: create_todo_def(),
            label_fn: create_todo_label,
            expandable: true,
        },
        ToolMeta {
            name: "list_todos",
            definition: list_todos_def(),
            label_fn: list_todos_label,
            expandable: true,
        },
        ToolMeta {
            name: "update_todo",
            definition: update_todo_def(),
            label_fn: update_todo_label,
            expandable: true,
        },
        ToolMeta {
            name: "delete_todo",
            definition: delete_todo_def(),
            label_fn: delete_todo_label,
            expandable: false,
        },
        ToolMeta {
            name: "skill",
            definition: skill_def(),
            label_fn: skill_label,
            expandable: false,
        },
        ToolMeta {
            name: "plugin_tool",
            definition: plugin_tool_def(),
            label_fn: plugin_label,
            expandable: false,
        },
    ]
}

fn read_file_def() -> Tool {
    tool(
        "read_file",
        "Read a text file from the server workspace. Returns content with line numbers. Rejects binary files and files over 10MB.",
        serde_json::json!({"type":"object","properties":{"file_path":{"type":"string"},"offset":{"type":"integer"},"limit":{"type":"integer"}},"required":["file_path"]}),
    )
}

fn write_file_def() -> Tool {
    tool(
        "write_file",
        "Write a text file inside the server workspace. Creates parent directories when needed.",
        serde_json::json!({"type":"object","properties":{"file_path":{"type":"string"},"content":{"type":"string"}},"required":["file_path","content"]}),
    )
}

fn edit_file_def() -> Tool {
    tool(
        "edit_file",
        "Replace text in an existing server workspace file. Set replace_all=true to replace all occurrences.",
        serde_json::json!({"type":"object","properties":{"file_path":{"type":"string"},"old_string":{"type":"string"},"new_string":{"type":"string"},"replace_all":{"type":"boolean"}},"required":["file_path","old_string","new_string"]}),
    )
}

fn glob_search_def() -> Tool {
    tool(
        "glob_search",
        "Find files in the server workspace matching a glob pattern. Returns up to 100 results.",
        serde_json::json!({"type":"object","properties":{"pattern":{"type":"string"},"path":{"type":"string"}},"required":["pattern"]}),
    )
}

fn grep_search_def() -> Tool {
    tool(
        "grep_search",
        "Search server workspace file contents with regex. Supports context lines, glob filtering, and content/count output modes.",
        serde_json::json!({"type":"object","properties":{"pattern":{"type":"string"},"path":{"type":"string"},"glob":{"type":"string"},"output_mode":{"type":"string","enum":["files_with_matches","content","count"]},"-B":{"type":"integer"},"-A":{"type":"integer"},"-C":{"type":"integer"},"-n":{"type":"boolean"},"-i":{"type":"boolean"},"type":{"type":"string"},"head_limit":{"type":"integer"},"offset":{"type":"integer"},"multiline":{"type":"boolean"}},"required":["pattern"]}),
    )
}

fn web_fetch_def() -> Tool {
    tool(
        "web_fetch",
        "Fetch a URL, convert HTML to readable text, and answer a prompt about the content.",
        serde_json::json!({"type":"object","properties":{"url":{"type":"string","format":"uri"},"prompt":{"type":"string"}},"required":["url","prompt"]}),
    )
}

fn web_search_def() -> Tool {
    tool(
        "web_search",
        "Search the web through DuckDuckGo HTML results and return up to 8 cited results.",
        serde_json::json!({"type":"object","properties":{"query":{"type":"string"},"allowed_domains":{"type":"array","items":{"type":"string"}},"blocked_domains":{"type":"array","items":{"type":"string"}}},"required":["query"]}),
    )
}

fn bash_def() -> Tool {
    tool(
        "bash",
        "Execute a shell command in the server workspace. Returns stdout and stderr.",
        serde_json::json!({"type":"object","properties":{"command":{"type":"string"},"timeout":{"type":"integer"},"description":{"type":"string"}},"required":["command"]}),
    )
}

fn powershell_def() -> Tool {
    tool(
        "powershell",
        "Execute a PowerShell command in the server workspace. Returns stdout and stderr.",
        serde_json::json!({"type":"object","properties":{"command":{"type":"string"},"timeout":{"type":"integer"},"description":{"type":"string"}},"required":["command"]}),
    )
}

fn repl_def() -> Tool {
    tool(
        "repl",
        "Execute Python, JavaScript, or shell code in the server workspace.",
        serde_json::json!({"type":"object","properties":{"code":{"type":"string"},"language":{"type":"string","enum":["python","py","javascript","js","node","sh","shell","bash"]},"timeout_ms":{"type":"integer"}},"required":["code","language"]}),
    )
}

fn create_todo_def() -> Tool {
    tool(
        "create_todo",
        "Create one or more workflow todo tasks.",
        serde_json::json!({"type":"object","properties":{"tasks":{"type":"array","minItems":1,"items":{"type":"object","properties":{"prompt":{"type":["string","null"]},"title":{"type":"string"},"content":{"type":"string"},"startAt":{"type":["string","null"]},"scheduledAt":{"type":["string","null"]},"endAt":{"type":["string","null"]},"occurrenceDate":{"type":["string","null"]},"timeText":{"type":["string","null"]},"repeatStartDate":{"type":["string","null"]},"repeatEndDate":{"type":["string","null"]},"metadata":{"type":["object","null"]}},"required":["title","content"]}}},"required":["tasks"]}),
    )
}

fn list_todos_def() -> Tool {
    tool(
        "list_todos",
        "List workflow todo tasks. Dates must be YYYY-MM-DD.",
        serde_json::json!({"type":"object","properties":{"startDate":{"type":["string","null"]},"endDate":{"type":["string","null"]},"status":{"type":["string","null"],"enum":["pending","completed",null]},"search":{"type":["string","null"]},"limit":{"type":"integer"}}}),
    )
}

fn update_todo_def() -> Tool {
    tool(
        "update_todo",
        "Update one workflow todo task by id.",
        serde_json::json!({"type":"object","properties":{"id":{"type":"string"},"prompt":{"type":["string","null"]},"title":{"type":["string","null"]},"content":{"type":["string","null"]},"startAt":{"type":["string","null"]},"scheduledAt":{"type":["string","null"]},"endAt":{"type":["string","null"]},"occurrenceDate":{"type":["string","null"]},"timeText":{"type":["string","null"]},"repeatStartDate":{"type":["string","null"]},"repeatEndDate":{"type":["string","null"]},"metadata":{"type":["object","null"]},"status":{"type":["string","null"],"enum":["pending","completed",null]}},"required":["id"]}),
    )
}

fn delete_todo_def() -> Tool {
    tool(
        "delete_todo",
        "Delete one workflow todo task by id.",
        serde_json::json!({"type":"object","properties":{"id":{"type":"string"}},"required":["id"]}),
    )
}

fn skill_def() -> Tool {
    tool(
        "skill",
        "Load a skill's full SKILL.md instructions. Use a skill name from <available_skills>.",
        serde_json::json!({"type":"object","properties":{"skill":{"type":"string"}},"required":["skill"]}),
    )
}

fn plugin_tool_def() -> Tool {
    tool(
        "plugin_tool",
        "Load a plugin's instructions and resource paths. Use a plugin name from <available_plugins>.",
        serde_json::json!({"type":"object","properties":{"plugin":{"type":"string"}},"required":["plugin"]}),
    )
}

fn tool(name: &str, description: &str, parameters: Value) -> Tool {
    Tool {
        tool_type: "function".to_string(),
        function: FunctionDefinition {
            name: name.to_string(),
            description: description.to_string(),
            parameters: Some(parameters),
        },
    }
}

pub fn tool_label(name: &str, args: &Value) -> String {
    build_tool_metas()
        .iter()
        .find(|meta| meta.name == name)
        .map(|meta| (meta.label_fn)(args))
        .unwrap_or_else(|| format!("Running {name}"))
}

pub fn tool_expandable(name: &str) -> bool {
    build_tool_metas()
        .iter()
        .any(|meta| meta.name == name && meta.expandable)
}

fn file_name_label(args: &Value, prefix: &str) -> String {
    let name = args
        .get("file_path")
        .and_then(Value::as_str)
        .and_then(|path| Path::new(path).file_name())
        .and_then(|name| name.to_str())
        .unwrap_or("file");
    format!("{prefix} {name}")
}

fn read_file_label(args: &Value) -> String {
    file_name_label(args, "Reading")
}

fn write_file_label(args: &Value) -> String {
    file_name_label(args, "Writing")
}

fn edit_file_label(args: &Value) -> String {
    file_name_label(args, "Editing")
}

fn glob_search_label(args: &Value) -> String {
    format!(
        "Searching {}",
        args.get("pattern").and_then(Value::as_str).unwrap_or("*")
    )
}

fn grep_search_label(args: &Value) -> String {
    format!(
        "Searching \"{}\"",
        args.get("pattern").and_then(Value::as_str).unwrap_or("")
    )
}

fn web_fetch_label(args: &Value) -> String {
    let url = args.get("url").and_then(Value::as_str).unwrap_or("URL");
    reqwest::Url::parse(url)
        .ok()
        .and_then(|url| url.host_str().map(|host| format!("Fetching {host}")))
        .unwrap_or_else(|| format!("Fetching {url}"))
}

fn web_search_label(args: &Value) -> String {
    format!(
        "Searching \"{}\"",
        args.get("query").and_then(Value::as_str).unwrap_or("")
    )
}

fn bash_label(_: &Value) -> String {
    "Running shell command".to_string()
}

fn powershell_label(_: &Value) -> String {
    "Running PowerShell command".to_string()
}

fn repl_label(args: &Value) -> String {
    format!(
        "Running {} code",
        args.get("language")
            .and_then(Value::as_str)
            .unwrap_or("code")
    )
}

fn create_todo_label(args: &Value) -> String {
    let count = args
        .get("tasks")
        .and_then(Value::as_array)
        .map(Vec::len)
        .unwrap_or(1);
    if count > 1 {
        format!("Creating {count} Todos")
    } else {
        "Creating Todo".to_string()
    }
}

fn list_todos_label(_: &Value) -> String {
    "Listing Todos".to_string()
}

fn update_todo_label(args: &Value) -> String {
    match args
        .get("id")
        .and_then(Value::as_str)
        .filter(|id| !id.is_empty())
    {
        Some(id) => format!("Updating Todo {id}"),
        None => "Updating Todo".to_string(),
    }
}

fn delete_todo_label(args: &Value) -> String {
    match args
        .get("id")
        .and_then(Value::as_str)
        .filter(|id| !id.is_empty())
    {
        Some(id) => format!("Deleting Todo {id}"),
        None => "Deleting Todo".to_string(),
    }
}

fn skill_label(args: &Value) -> String {
    format!(
        "Loading {}",
        args.get("skill").and_then(Value::as_str).unwrap_or("skill")
    )
}

fn plugin_label(args: &Value) -> String {
    format!(
        "Loading {}",
        args.get("plugin")
            .and_then(Value::as_str)
            .unwrap_or("plugin")
    )
}

fn workspace_root(data_root: &Path) -> PathBuf {
    let root = data_root.join("workspace");
    let _ = fs::create_dir_all(&root);
    root
}

fn canonical_workspace(workspace: &Path) -> Result<PathBuf, String> {
    fs::create_dir_all(workspace).map_err(|error| format!("Cannot create workspace: {error}"))?;
    workspace
        .canonicalize()
        .map_err(|error| format!("Cannot resolve workspace: {error}"))
}

fn resolve_workspace_path(
    workspace: &Path,
    input: &str,
    allow_missing: bool,
) -> Result<PathBuf, String> {
    let root = canonical_workspace(workspace)?;
    let candidate = if Path::new(input).is_absolute() {
        PathBuf::from(input)
    } else {
        root.join(input)
    };
    let resolved = if candidate.exists() {
        candidate
            .canonicalize()
            .map_err(|error| format!("Path cannot be resolved: {error}"))?
    } else if allow_missing {
        let parent = candidate.parent().unwrap_or(root.as_path());
        let parent = parent
            .canonicalize()
            .unwrap_or_else(|_| parent.to_path_buf());
        parent.join(candidate.file_name().unwrap_or_default())
    } else {
        return Err(format!("Path does not exist: {}", candidate.display()));
    };

    if !resolved.starts_with(&root) {
        return Err("Path is outside the server workspace.".to_string());
    }

    Ok(resolved)
}

fn is_binary_file(path: &Path) -> Result<bool, String> {
    let mut file = fs::File::open(path).map_err(|error| format!("Cannot open file: {error}"))?;
    let mut buffer = [0u8; 8192];
    let bytes_read = file
        .read(&mut buffer)
        .map_err(|error| format!("Cannot read file: {error}"))?;
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

fn read_file_impl(workspace: &Path, args: Value) -> String {
    let input: ReadFileInput = match serde_json::from_value(args) {
        Ok(value) => value,
        Err(error) => return jerr(format!("Argument parse failed: {error}")),
    };
    let path = match resolve_workspace_path(workspace, &input.file_path, false) {
        Ok(path) => path,
        Err(error) => return jerr(error),
    };
    let metadata = match fs::metadata(&path) {
        Ok(metadata) => metadata,
        Err(error) => return jerr(format!("Cannot read file metadata: {error}")),
    };
    if metadata.len() > MAX_READ_SIZE {
        return jerr(format!("File is too large ({} bytes).", metadata.len()));
    }
    match is_binary_file(&path) {
        Ok(true) => return jerr("File appears to be binary."),
        Err(error) => return jerr(error),
        Ok(false) => {}
    }
    let content = match fs::read_to_string(&path) {
        Ok(content) => content,
        Err(error) => return jerr(format!("Read failed: {error}")),
    };
    let lines: Vec<&str> = content.lines().collect();
    let total = lines.len();
    let start = input.offset.unwrap_or(0).min(total);
    let end = input
        .limit
        .map_or(total, |limit| start.saturating_add(limit).min(total));
    let output = ReadFileOutput {
        kind: "text".to_string(),
        file: TextFilePayload {
            file_path: path.to_string_lossy().to_string(),
            content: lines[start..end].join("\n"),
            num_lines: end.saturating_sub(start),
            start_line: start.saturating_add(1),
            total_lines: total,
        },
    };
    to_json(&output)
}

fn write_file_impl(workspace: &Path, args: Value) -> String {
    let input: WriteFileInput = match serde_json::from_value(args) {
        Ok(value) => value,
        Err(error) => return jerr(format!("Argument parse failed: {error}")),
    };
    if input.content.len() > MAX_WRITE_SIZE {
        return jerr(format!(
            "Content is too large ({} bytes).",
            input.content.len()
        ));
    }
    let path = match resolve_workspace_path(workspace, &input.file_path, true) {
        Ok(path) => path,
        Err(error) => return jerr(error),
    };
    let original = fs::read_to_string(&path).ok();
    if let Some(parent) = path.parent() {
        if let Err(error) = fs::create_dir_all(parent) {
            return jerr(format!("Cannot create parent directory: {error}"));
        }
    }
    if let Err(error) = fs::write(&path, &input.content) {
        return jerr(format!("Write failed: {error}"));
    }
    let output = WriteFileOutput {
        kind: if original.is_some() {
            "update"
        } else {
            "create"
        }
        .to_string(),
        file_path: path.to_string_lossy().to_string(),
        content: input.content.clone(),
        structured_patch: make_patch(original.as_deref().unwrap_or(""), &input.content),
        original_file: original,
    };
    to_json(&output)
}

fn edit_file_impl(workspace: &Path, args: Value) -> String {
    let input: EditFileInput = match serde_json::from_value(args) {
        Ok(value) => value,
        Err(error) => return jerr(format!("Argument parse failed: {error}")),
    };
    let path = match resolve_workspace_path(workspace, &input.file_path, false) {
        Ok(path) => path,
        Err(error) => return jerr(error),
    };
    let original = match fs::read_to_string(&path) {
        Ok(content) => content,
        Err(error) => return jerr(format!("Read failed: {error}")),
    };
    if input.old_string == input.new_string {
        return jerr("old_string and new_string cannot be identical.");
    }
    if !original.contains(&input.old_string) {
        return jerr("old_string was not found in the file.");
    }
    let updated = if input.replace_all {
        original.replace(&input.old_string, &input.new_string)
    } else {
        original.replacen(&input.old_string, &input.new_string, 1)
    };
    if let Err(error) = fs::write(&path, &updated) {
        return jerr(format!("Write failed: {error}"));
    }
    let output = EditFileOutput {
        file_path: path.to_string_lossy().to_string(),
        old_string: input.old_string,
        new_string: input.new_string,
        original_file: original.clone(),
        structured_patch: make_patch(&original, &updated),
        replace_all: input.replace_all,
    };
    to_json(&output)
}

fn expand_braces(pattern: &str) -> Vec<String> {
    let Some(open) = pattern.find('{') else {
        return vec![pattern.to_owned()];
    };
    let Some(close) = pattern[open..].find('}').map(|index| open + index) else {
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
    for component in path.components() {
        let text = component.as_os_str().to_string_lossy();
        if text.contains('*') || text.contains('?') || text.contains('[') {
            break;
        }
        prefix.push(component.as_os_str());
        saw = true;
    }
    if saw {
        prefix
    } else {
        PathBuf::from(".")
    }
}

fn glob_search_impl(workspace: &Path, args: Value) -> String {
    let input: GlobSearchInput = match serde_json::from_value(args) {
        Ok(value) => value,
        Err(error) => return jerr(format!("Argument parse failed: {error}")),
    };
    let started = Instant::now();
    let base_dir = match input.path.as_deref() {
        Some(path) => match resolve_workspace_path(workspace, path, false) {
            Ok(path) => path,
            Err(error) => return jerr(error),
        },
        None => match canonical_workspace(workspace) {
            Ok(path) => path,
            Err(error) => return jerr(error),
        },
    };
    let search_pattern = if Path::new(&input.pattern).is_absolute() {
        input.pattern.clone()
    } else {
        base_dir.join(&input.pattern).to_string_lossy().to_string()
    };
    let mut seen = HashSet::new();
    let mut matches = Vec::new();

    for pattern in expand_braces(&search_pattern) {
        let compiled = match glob::Pattern::new(&pattern) {
            Ok(pattern) => pattern,
            Err(error) => return jerr(format!("Invalid glob pattern: {error}")),
        };
        let walker = walkdir::WalkDir::new(derive_glob_walk_root(&pattern))
            .into_iter()
            .filter_entry(|entry| {
                !(entry.file_type().is_dir()
                    && entry
                        .file_name()
                        .to_str()
                        .is_some_and(|name| GLOB_IGNORED_DIRS.contains(&name)))
            });
        for entry in walker.flatten() {
            if !entry.file_type().is_file() {
                continue;
            }
            let path = entry.path();
            if compiled.matches_path(path)
                && path.starts_with(workspace)
                && seen.insert(path.to_path_buf())
            {
                matches.push(path.to_path_buf());
            }
        }
    }

    matches.sort_by_key(|path| {
        fs::metadata(path)
            .and_then(|m| m.modified())
            .ok()
            .map(Reverse)
    });
    let truncated = matches.len() > 100;
    let filenames = matches
        .into_iter()
        .take(100)
        .map(|path| path.to_string_lossy().to_string())
        .collect::<Vec<_>>();
    to_json(&GlobSearchOutput {
        duration_ms: started.elapsed().as_millis(),
        num_files: filenames.len(),
        filenames,
        truncated,
    })
}

fn collect_search_files(base: &Path) -> Result<Vec<PathBuf>, String> {
    if base.is_file() {
        return Ok(vec![base.to_path_buf()]);
    }
    let mut files = Vec::new();
    let walker = walkdir::WalkDir::new(base)
        .into_iter()
        .filter_entry(|entry| {
            !(entry.file_type().is_dir()
                && entry
                    .file_name()
                    .to_str()
                    .is_some_and(|name| GLOB_IGNORED_DIRS.contains(&name)))
        });
    for entry in walker {
        let entry = entry.map_err(|error| format!("Directory traversal failed: {error}"))?;
        if entry.file_type().is_file() {
            files.push(entry.path().to_path_buf());
        }
    }
    Ok(files)
}

fn match_optional_filters(
    path: &Path,
    glob_filter: Option<&glob::Pattern>,
    file_type: Option<&str>,
) -> bool {
    if let Some(glob_filter) = glob_filter {
        let path_text = path.to_string_lossy();
        if !glob_filter.matches(&path_text) && !glob_filter.matches_path(path) {
            return false;
        }
    }
    if let Some(file_type) = file_type {
        if path.extension().and_then(|ext| ext.to_str()) != Some(file_type) {
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
    let offset_value = offset.unwrap_or(0);
    let mut items = items.into_iter().skip(offset_value).collect::<Vec<_>>();
    let limit_value = limit.unwrap_or(250);
    if limit_value == 0 {
        return (items, None, (offset_value > 0).then_some(offset_value));
    }
    let truncated = items.len() > limit_value;
    items.truncate(limit_value);
    (
        items,
        truncated.then_some(limit_value),
        (offset_value > 0).then_some(offset_value),
    )
}

fn grep_search_impl(workspace: &Path, args: Value) -> String {
    let input: GrepSearchInput = match serde_json::from_value(args) {
        Ok(value) => value,
        Err(error) => return jerr(format!("Argument parse failed: {error}")),
    };
    let base_path = match input.path.as_deref() {
        Some(path) => match resolve_workspace_path(workspace, path, false) {
            Ok(path) => path,
            Err(error) => return jerr(error),
        },
        None => match canonical_workspace(workspace) {
            Ok(path) => path,
            Err(error) => return jerr(error),
        },
    };
    let regex = match regex::RegexBuilder::new(&input.pattern)
        .case_insensitive(input.case_insensitive.unwrap_or(false))
        .dot_matches_new_line(input.multiline.unwrap_or(false))
        .build()
    {
        Ok(regex) => regex,
        Err(error) => return jerr(format!("Invalid regex: {error}")),
    };
    let glob_filter = match input.glob.as_deref() {
        Some(glob) => match glob::Pattern::new(glob) {
            Ok(pattern) => Some(pattern),
            Err(error) => return jerr(format!("Invalid glob pattern: {error}")),
        },
        None => None,
    };
    let output_mode = input
        .output_mode
        .clone()
        .unwrap_or_else(|| "files_with_matches".to_string());
    let context = input.context.or(input.context_short).unwrap_or(0);
    let files = match collect_search_files(&base_path) {
        Ok(files) => files,
        Err(error) => return jerr(error),
    };
    let mut filenames = Vec::new();
    let mut content_lines = Vec::new();
    let mut total_matches = 0usize;

    for file_path in &files {
        if !match_optional_filters(file_path, glob_filter.as_ref(), input.file_type.as_deref()) {
            continue;
        }
        let file_content = match fs::read_to_string(file_path) {
            Ok(content) => content,
            Err(_) => continue,
        };
        if output_mode == "count" {
            let count = regex.find_iter(&file_content).count();
            if count > 0 {
                filenames.push(file_path.to_string_lossy().to_string());
                total_matches += count;
            }
            continue;
        }
        let lines = file_content.lines().collect::<Vec<_>>();
        let mut matched = Vec::new();
        for (index, line) in lines.iter().enumerate() {
            if regex.is_match(line) {
                total_matches += 1;
                matched.push(index);
            }
        }
        if matched.is_empty() {
            continue;
        }
        filenames.push(file_path.to_string_lossy().to_string());
        if output_mode == "content" {
            for index in matched {
                let start = index.saturating_sub(input.before.unwrap_or(context));
                let end = (index + input.after.unwrap_or(context) + 1).min(lines.len());
                for (line_index, line) in lines.iter().enumerate().take(end).skip(start) {
                    let prefix = if input.line_numbers.unwrap_or(true) {
                        format!("{}:{}:", file_path.to_string_lossy(), line_index + 1)
                    } else {
                        format!("{}:", file_path.to_string_lossy())
                    };
                    content_lines.push(format!("{prefix}{line}"));
                }
            }
        }
    }

    let (filenames, applied_limit, applied_offset) =
        apply_limit(filenames, input.head_limit, input.offset);
    if output_mode == "content" {
        let (lines, line_limit, line_offset) =
            apply_limit(content_lines, input.head_limit, input.offset);
        return to_json(&GrepSearchOutput {
            mode: Some("content".to_string()),
            num_files: filenames.len(),
            filenames,
            num_lines: Some(lines.len()),
            content: Some(lines.join("\n")),
            num_matches: None,
            applied_limit: line_limit,
            applied_offset: line_offset,
        });
    }
    to_json(&GrepSearchOutput {
        mode: Some(output_mode.clone()),
        num_files: filenames.len(),
        filenames,
        content: None,
        num_lines: None,
        num_matches: (output_mode == "count").then_some(total_matches),
        applied_limit,
        applied_offset,
    })
}

fn build_http_client() -> Result<reqwest::blocking::Client, String> {
    reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(20))
        .redirect(reqwest::redirect::Policy::limited(10))
        .user_agent("otherone-agent/0.1")
        .build()
        .map_err(|error| format!("Cannot create HTTP client: {error}"))
}

fn normalize_url(url: &str) -> Result<String, String> {
    let mut parsed = reqwest::Url::parse(url).map_err(|error| format!("Invalid URL: {error}"))?;
    if parsed.scheme() == "http" {
        let host = parsed.host_str().unwrap_or_default();
        if host != "localhost" && host != "127.0.0.1" && host != "::1" {
            parsed
                .set_scheme("https")
                .map_err(|()| "Cannot upgrade URL to https.".to_string())?;
        }
    }
    Ok(parsed.to_string())
}

fn web_fetch_impl(args: Value) -> String {
    let input: WebFetchInput = match serde_json::from_value(args) {
        Ok(value) => value,
        Err(error) => return jerr(format!("Argument parse failed: {error}")),
    };
    let started = Instant::now();
    let client = match build_http_client() {
        Ok(client) => client,
        Err(error) => return jerr(error),
    };
    let request_url = match normalize_url(&input.url) {
        Ok(url) => url,
        Err(error) => return jerr(error),
    };
    let response = match client.get(&request_url).send() {
        Ok(response) => response,
        Err(error) => return jerr(format!("Request failed: {error}")),
    };
    let status = response.status();
    let final_url = response.url().to_string();
    let content_type = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
        .to_string();
    let body = match response.text() {
        Ok(body) => body,
        Err(error) => return jerr(format!("Response read failed: {error}")),
    };
    let normalized = if content_type.contains("html") {
        html_to_text(&body)
    } else {
        body.trim().to_string()
    };
    let preview = collapse_ws(&normalized);
    let excerpt = preview.chars().take(1500).collect::<String>();
    let prompt = input.prompt.trim();
    let result = if prompt.is_empty() {
        format!("Fetched {final_url}\n{excerpt}")
    } else {
        format!("Fetched {final_url}\nPrompt: {prompt}\n{excerpt}")
    };
    to_json(&WebFetchOutput {
        bytes: body.len(),
        code: status.as_u16(),
        code_text: status.canonical_reason().unwrap_or("Unknown").to_string(),
        result,
        duration_ms: started.elapsed().as_millis(),
        url: final_url,
    })
}

fn html_to_text(html: &str) -> String {
    let mut text = html.to_string();
    for tag in ["script", "style", "noscript", "head"] {
        let regex = regex::Regex::new(&format!("(?is)<{tag}[^>]*>.*?</{tag}>")).unwrap();
        text = regex.replace_all(&text, "").to_string();
    }
    text = text.replace("</p>", "\n").replace("<br", "\n<br");
    text = regex::Regex::new(r"<[^>]+>")
        .unwrap()
        .replace_all(&text, "")
        .to_string();
    text = regex::Regex::new(r"[ \t]+")
        .unwrap()
        .replace_all(&text, " ")
        .to_string();
    regex::Regex::new(r"\n{3,}")
        .unwrap()
        .replace_all(text.trim(), "\n\n")
        .to_string()
}

fn collapse_ws(value: &str) -> String {
    regex::Regex::new(r"\s+")
        .unwrap()
        .replace_all(value.trim(), " ")
        .to_string()
}

fn build_search_url(query: &str) -> Result<String, String> {
    let mut url = reqwest::Url::parse("https://html.duckduckgo.com/html/")
        .map_err(|error| format!("URL parse failed: {error}"))?;
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
    domains.iter().any(|domain| {
        let domain = domain
            .trim_start_matches("https://")
            .trim_start_matches("http://")
            .trim_end_matches('/');
        host.eq_ignore_ascii_case(domain) || host.ends_with(&format!(".{domain}"))
    })
}

fn extract_quoted_value(value: &str) -> Option<(String, &str)> {
    let value = value.trim_start();
    let delimiter = value.chars().next()?;
    if delimiter != '"' && delimiter != '\'' {
        return None;
    }
    let chars = value.chars().collect::<Vec<_>>();
    let mut index = 1;
    while index < chars.len() {
        if chars[index] == '\\' {
            index += 2;
            continue;
        }
        if chars[index] == delimiter {
            return Some((chars[1..index].iter().collect(), &value[index + 1..]));
        }
        index += 1;
    }
    None
}

fn decode_ddg_redirect(url: &str) -> Option<String> {
    let parsed = reqwest::Url::parse(url).ok()?;
    parsed
        .query_pairs()
        .find(|(key, _)| key == "uddg")
        .map(|(_, value)| value.to_string())
}

fn extract_search_hits(html: &str) -> Vec<SearchHit> {
    let mut hits = Vec::new();
    let mut remaining = html;
    while let Some(start) = remaining.find("result__a") {
        let after = &remaining[start..];
        let Some(href_index) = after.find("href=") else {
            remaining = &after[1..];
            continue;
        };
        let href_slice = &after[href_index + 5..];
        let Some((url, rest)) = extract_quoted_value(href_slice) else {
            remaining = &after[1..];
            continue;
        };
        let Some(close_index) = rest.find('>') else {
            remaining = &after[1..];
            continue;
        };
        let after_tag = &rest[close_index + 1..];
        let Some(end_index) = after_tag.find("</a>") else {
            remaining = &after[1..];
            continue;
        };
        if let Some(decoded) = decode_ddg_redirect(&url) {
            hits.push(SearchHit {
                title: html_to_text(&after_tag[..end_index]).trim().to_string(),
                url: decoded,
            });
        }
        remaining = &after_tag[end_index + 4..];
    }
    hits
}

fn web_search_impl(args: Value) -> String {
    let input: WebSearchInput = match serde_json::from_value(args) {
        Ok(value) => value,
        Err(error) => return jerr(format!("Argument parse failed: {error}")),
    };
    let started = Instant::now();
    let client = match build_http_client() {
        Ok(client) => client,
        Err(error) => return jerr(error),
    };
    let search_url = match build_search_url(&input.query) {
        Ok(url) => url,
        Err(error) => return jerr(error),
    };
    let response = match client.get(&search_url).send() {
        Ok(response) => response,
        Err(error) => return jerr(format!("Search request failed: {error}")),
    };
    let html = match response.text() {
        Ok(html) => html,
        Err(error) => return jerr(format!("Response read failed: {error}")),
    };
    let mut hits = extract_search_hits(&html);
    if let Some(allowed) = input.allowed_domains.as_ref() {
        hits.retain(|hit| host_matches(&hit.url, allowed));
    }
    if let Some(blocked) = input.blocked_domains.as_ref() {
        hits.retain(|hit| !host_matches(&hit.url, blocked));
    }
    let mut seen = HashSet::new();
    hits.retain(|hit| seen.insert(hit.url.clone()));
    hits.truncate(8);
    let summary = if hits.is_empty() {
        format!("No search results matched \"{}\".", input.query)
    } else {
        hits.iter()
            .map(|hit| format!("- [{}]({})", hit.title, hit.url))
            .collect::<Vec<_>>()
            .join("\n")
    };
    to_json(&WebSearchOutput {
        query: input.query,
        results: hits,
        summary,
        duration_seconds: started.elapsed().as_secs_f64(),
    })
}

fn command_available(name: &str) -> bool {
    Command::new(name)
        .arg("--version")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

fn execute_shell(
    workspace: &Path,
    shell: &str,
    args: &[&str],
    timeout_ms: Option<u64>,
) -> ShellOutput {
    let mut command = Command::new(shell);
    command
        .args(args)
        .current_dir(workspace)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let started = Instant::now();
    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(error) => {
            return ShellOutput {
                stdout: String::new(),
                stderr: format!("Process start failed: {error}"),
                exit_code: None,
                interrupted: false,
            }
        }
    };
    let timeout = timeout_ms.map(Duration::from_millis);

    loop {
        if let Ok(Some(_status)) = child.try_wait() {
            let output = child.wait_with_output().ok();
            return ShellOutput {
                stdout: output
                    .as_ref()
                    .map(|output| String::from_utf8_lossy(&output.stdout).to_string())
                    .unwrap_or_default(),
                stderr: output
                    .as_ref()
                    .map(|output| String::from_utf8_lossy(&output.stderr).to_string())
                    .unwrap_or_default(),
                exit_code: output.as_ref().and_then(|output| output.status.code()),
                interrupted: false,
            };
        }
        if timeout.is_some_and(|timeout| started.elapsed() >= timeout) {
            let _ = child.kill();
            let output = child.wait_with_output().ok();
            return ShellOutput {
                stdout: output
                    .as_ref()
                    .map(|output| String::from_utf8_lossy(&output.stdout).to_string())
                    .unwrap_or_default(),
                stderr: output
                    .as_ref()
                    .map(|output| String::from_utf8_lossy(&output.stderr).to_string())
                    .unwrap_or_default(),
                exit_code: output.as_ref().and_then(|output| output.status.code()),
                interrupted: true,
            };
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn bash_impl(workspace: &Path, args: Value) -> String {
    let input: ShellInput = match serde_json::from_value(args) {
        Ok(value) => value,
        Err(error) => return jerr(format!("Argument parse failed: {error}")),
    };
    let _ = input.description.as_deref();
    let shell = if command_available("bash") {
        "bash"
    } else if command_available("sh") {
        "sh"
    } else {
        return jerr("Shell runtime was not found.");
    };
    to_json(&execute_shell(
        workspace,
        shell,
        &["-lc", &input.command],
        input.timeout,
    ))
}

fn powershell_impl(workspace: &Path, args: Value) -> String {
    let input: ShellInput = match serde_json::from_value(args) {
        Ok(value) => value,
        Err(error) => return jerr(format!("Argument parse failed: {error}")),
    };
    let _ = input.description.as_deref();
    let shell = if command_available("pwsh") {
        "pwsh"
    } else if command_available("powershell") {
        "powershell"
    } else {
        return jerr("PowerShell runtime was not found.");
    };
    to_json(&execute_shell(
        workspace,
        shell,
        &["-Command", &input.command],
        input.timeout,
    ))
}

fn repl_impl(workspace: &Path, args: Value) -> String {
    let input: ReplInput = match serde_json::from_value(args) {
        Ok(value) => value,
        Err(error) => return jerr(format!("Argument parse failed: {error}")),
    };
    if input.code.trim().is_empty() {
        return jerr("code cannot be empty.");
    }
    let language = input.language.trim().to_ascii_lowercase();
    let (runtime, runtime_args): (&str, &[&str]) = match language.as_str() {
        "py" | "python" => {
            if command_available("python3") {
                ("python3", &["-c"][..])
            } else if command_available("python") {
                ("python", &["-c"][..])
            } else {
                return jerr("Python runtime was not found.");
            }
        }
        "js" | "javascript" | "node" => {
            if command_available("node") {
                ("node", &["-e"][..])
            } else {
                return jerr("Node.js runtime was not found.");
            }
        }
        "sh" | "shell" | "bash" => {
            if command_available("bash") {
                ("bash", &["-lc"][..])
            } else if command_available("sh") {
                ("sh", &["-lc"][..])
            } else {
                return jerr("Shell runtime was not found.");
            }
        }
        other => return jerr(format!("Unsupported language: {other}")),
    };
    let mut command = Command::new(runtime);
    command
        .args(runtime_args)
        .arg(&input.code)
        .current_dir(workspace)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let started = Instant::now();
    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(error) => return jerr(format!("Process start failed: {error}")),
    };

    loop {
        if let Ok(Some(_status)) = child.try_wait() {
            let output = match child.wait_with_output() {
                Ok(output) => output,
                Err(error) => return jerr(format!("Output read failed: {error}")),
            };
            return to_json(&ReplOutput {
                language: input.language,
                stdout: String::from_utf8_lossy(&output.stdout).to_string(),
                stderr: String::from_utf8_lossy(&output.stderr).to_string(),
                exit_code: output.status.code(),
                duration_ms: started.elapsed().as_millis(),
            });
        }
        if input
            .timeout_ms
            .is_some_and(|timeout| started.elapsed() >= Duration::from_millis(timeout))
        {
            let _ = child.kill();
            let _ = child.wait();
            return jerr(format!(
                "REPL execution timed out ({}ms).",
                input.timeout_ms.unwrap_or_default()
            ));
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn create_todo_impl(data_root: &Path, args: Value) -> String {
    let input: workflow::WorkflowTodoCreateToolInput = match serde_json::from_value(args) {
        Ok(value) => value,
        Err(error) => return jerr(format!("Argument parse failed: {error}")),
    };
    match workflow::create_workflow_tasks_from_tool(data_root, input) {
        Ok(tasks) => {
            to_json(&serde_json::json!({"status":"created","count":tasks.len(),"tasks":tasks}))
        }
        Err(error) => jerr(error),
    }
}

fn list_todos_impl(data_root: &Path, args: Value) -> String {
    let input: workflow::WorkflowTodoListToolInput = match serde_json::from_value(args) {
        Ok(value) => value,
        Err(error) => return jerr(format!("Argument parse failed: {error}")),
    };
    match workflow::list_workflow_tasks_for_tool(data_root, input) {
        Ok(tasks) => to_json(&serde_json::json!({"status":"ok","count":tasks.len(),"tasks":tasks})),
        Err(error) => jerr(error),
    }
}

fn update_todo_impl(data_root: &Path, args: Value) -> String {
    let input: workflow::WorkflowTodoUpdateToolInput = match serde_json::from_value(args) {
        Ok(value) => value,
        Err(error) => return jerr(format!("Argument parse failed: {error}")),
    };
    match workflow::update_workflow_task_from_tool(data_root, input) {
        Ok(task) => to_json(&serde_json::json!({"status":"updated","task":task})),
        Err(error) => jerr(error),
    }
}

fn delete_todo_impl(data_root: &Path, args: Value) -> String {
    let input: workflow::WorkflowTodoDeleteToolInput = match serde_json::from_value(args) {
        Ok(value) => value,
        Err(error) => return jerr(format!("Argument parse failed: {error}")),
    };
    match workflow::delete_workflow_task_from_tool(data_root, input) {
        Ok(id) => to_json(&serde_json::json!({"status":"deleted","id":id})),
        Err(error) => jerr(error),
    }
}

fn skill_impl(data_root: &Path, args: Value) -> String {
    let input: SkillToolInput = match serde_json::from_value(args) {
        Ok(value) => value,
        Err(error) => return jerr(format!("Argument parse failed: {error}")),
    };
    match plugins::get_installed_skills_content(data_root) {
        Ok(contents) => match contents.iter().find(|content| content.name == input.skill) {
            Some(content) => to_json(&serde_json::json!({
                "skill": content.name,
                "description": content.description,
                "path": content.file_path,
                "body": content.body,
            })),
            None => jerr(format!(
                "Skill '{}' was not found or is not installed.",
                input.skill
            )),
        },
        Err(error) => jerr(error),
    }
}

fn plugin_impl(data_root: &Path, args: Value) -> String {
    let input: PluginToolInput = match serde_json::from_value(args) {
        Ok(value) => value,
        Err(error) => return jerr(format!("Argument parse failed: {error}")),
    };
    let entry = match plugins::get_plugin_metadata(data_root, &input.plugin) {
        Ok(Some(entry)) => entry,
        Ok(None) => {
            return jerr(format!(
                "Plugin '{}' was not found or is not installed.",
                input.plugin
            ))
        }
        Err(error) => return jerr(error),
    };
    let body = plugins::get_plugin_body(data_root, &input.plugin)
        .ok()
        .flatten()
        .unwrap_or_default();
    to_json(&serde_json::json!({
        "name": entry.name,
        "description": entry.description,
        "skill_path": entry.file_path,
        "bin_path": entry.bin_path,
        "bin_dir": entry.bin_dir,
        "body": body,
    }))
}

fn record_write_file_artifact(
    data_root: &Path,
    session_id: &str,
    tool_result: &str,
    sink: Option<&ArtifactSink>,
) {
    let Ok(value) = serde_json::from_str::<Value>(tool_result) else {
        return;
    };
    if value.get("error").is_some() {
        return;
    }
    let Some(file_path) = value
        .get("filePath")
        .or_else(|| value.get("file_path"))
        .and_then(Value::as_str)
        .filter(|path| !path.trim().is_empty())
    else {
        return;
    };
    let action = match value
        .get("type")
        .or_else(|| value.get("kind"))
        .and_then(Value::as_str)
        .unwrap_or("update")
    {
        "create" => "added",
        _ => "edited",
    };
    let patch_json = value
        .get("structuredPatch")
        .or_else(|| value.get("structured_patch"))
        .map(Value::to_string)
        .unwrap_or_else(|| "[]".to_string());
    if let Ok(artifact) = artifacts::record_file_artifact(
        data_root,
        session_id,
        action,
        "write_file",
        file_path,
        patch_json,
    ) {
        if let Some(sink) = sink {
            sink(artifact);
        }
    }
}

fn record_edit_file_artifact(
    data_root: &Path,
    session_id: &str,
    tool_result: &str,
    sink: Option<&ArtifactSink>,
) {
    let Ok(value) = serde_json::from_str::<Value>(tool_result) else {
        return;
    };
    if value.get("error").is_some() {
        return;
    }
    let Some(file_path) = value
        .get("filePath")
        .or_else(|| value.get("file_path"))
        .and_then(Value::as_str)
        .filter(|path| !path.trim().is_empty())
    else {
        return;
    };
    let patch_json = value
        .get("structuredPatch")
        .or_else(|| value.get("structured_patch"))
        .map(Value::to_string)
        .unwrap_or_else(|| "[]".to_string());
    if let Ok(artifact) = artifacts::record_file_artifact(
        data_root,
        session_id,
        "edited",
        "edit_file",
        file_path,
        patch_json,
    ) {
        if let Some(sink) = sink {
            sink(artifact);
        }
    }
}

fn to_json(value: &impl Serialize) -> String {
    serde_json::to_string(value).unwrap_or_else(|error| jerr(format!("Serialize failed: {error}")))
}

fn jerr(message: impl ToString) -> String {
    serde_json::json!({"error": message.to_string()}).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn test_dir(name: &str) -> PathBuf {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let path = std::env::temp_dir().join(format!("otherone-core-tools-{name}-{suffix}"));
        std::fs::create_dir_all(&path).expect("create test dir");
        path
    }

    #[test]
    fn write_file_records_artifact_and_emits_sink() {
        let data_root = test_dir("artifact");
        let emitted = Arc::new(Mutex::new(Vec::new()));
        let emitted_for_sink = emitted.clone();
        let (_tools, realize) = build_tools_for_web_session(
            data_root.clone(),
            "session-1".to_string(),
            Some(Arc::new(move |artifact| {
                emitted_for_sink.lock().expect("lock").push(artifact);
            })),
        );
        let write_file = realize.get("write_file").expect("write tool");
        let result = write_file(serde_json::json!({
            "file_path": "notes/demo.txt",
            "content": "hello"
        }));
        assert!(serde_json::from_str::<Value>(&result)
            .expect("json")
            .get("error")
            .is_none());

        let artifacts =
            artifacts::list_file_artifacts(&data_root, "session-1").expect("list artifacts");
        assert_eq!(artifacts.len(), 1);
        assert_eq!(artifacts[0].action, "added");
        assert_eq!(emitted.lock().expect("lock").len(), 1);
    }

    #[test]
    fn todo_tool_creates_listable_task() {
        let data_root = test_dir("todo");
        let (_tools, realize) =
            build_tools_for_web_session(data_root.clone(), "session-1".to_string(), None);
        let create_todo = realize.get("create_todo").expect("create todo");
        let result = create_todo(serde_json::json!({
            "tasks": [{
                "title": "Write tests",
                "content": "- Add Web tool tests",
                "occurrenceDate": "2026-07-01"
            }]
        }));
        assert!(serde_json::from_str::<Value>(&result)
            .expect("json")
            .get("error")
            .is_none());

        let tasks = workflow::list_workflow_tasks(&data_root).expect("list tasks");
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].title, "Write tests");
    }
}
