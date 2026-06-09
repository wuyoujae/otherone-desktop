// 内置插件注册表 — 定义可下载安装的插件（Skill + 衍生品）
// 关联：被 plugins.rs 使用，提供插件的元数据和安装逻辑
// 预期结果：每个插件定义包含 SKILL.md URL、二进制下载 URL、安装后路径

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

// ---------------------------------------------------------------------------
// 插件定义
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginDefinition {
    /// 唯一标识名
    pub name: String,
    /// 用户可见名称
    pub display_name: String,
    /// 简短描述
    pub description: String,
    /// SKILL.md 下载 URL（必填——每个插件至少有一个 skill 指令）
    pub skill_url: String,
    /// 按平台分发的二进制文件 URL 映射: "win-x64" → url, "mac-arm64" → url ...
    pub binaries: HashMap<String, String>,
    /// 二进制文件名（下载后保存为此名）
    pub binary_name: Option<String>,
    /// 是否有其他需要下载的资源
    pub extra_resources: HashMap<String, String>,
}

/// 获取所有内置插件定义
pub fn builtin_plugins() -> Vec<PluginDefinition> {
    vec![officecli()]
}

// ---------------------------------------------------------------------------
// officecli
// ---------------------------------------------------------------------------

fn officecli() -> PluginDefinition {
    let mut binaries = HashMap::new();
    binaries.insert(
        "win-x64".to_string(),
        "https://github.com/iOfficeAI/OfficeCLI/releases/download/v1.0.106/officecli-win-x64.exe"
            .to_string(),
    );
    binaries.insert(
        "win-arm64".to_string(),
        "https://github.com/iOfficeAI/OfficeCLI/releases/download/v1.0.106/officecli-win-arm64.exe"
            .to_string(),
    );
    binaries.insert(
        "mac-arm64".to_string(),
        "https://github.com/iOfficeAI/OfficeCLI/releases/download/v1.0.106/officecli-mac-arm64"
            .to_string(),
    );
    binaries.insert(
        "mac-x64".to_string(),
        "https://github.com/iOfficeAI/OfficeCLI/releases/download/v1.0.106/officecli-mac-x64"
            .to_string(),
    );
    binaries.insert(
        "linux-x64".to_string(),
        "https://github.com/iOfficeAI/OfficeCLI/releases/download/v1.0.106/officecli-linux-x64"
            .to_string(),
    );
    binaries.insert(
        "linux-arm64".to_string(),
        "https://github.com/iOfficeAI/OfficeCLI/releases/download/v1.0.106/officecli-linux-arm64"
            .to_string(),
    );

    PluginDefinition {
        name: "officecli".to_string(),
        display_name: "OfficeCLI".to_string(),
        description: "在 AI 中创建、分析、校对和修改 Office 文档（.docx/.xlsx/.pptx）。零依赖，无需安装 Office——让任何 AI 智能体完全掌控 Word、Excel 和 PowerPoint。"
            .to_string(),
        skill_url: "https://officecli.ai/SKILL.md".to_string(),
        binaries,
        binary_name: Some(if cfg!(target_os = "windows") {
            "officecli.exe".to_string()
        } else {
            "officecli".to_string()
        }),
        extra_resources: HashMap::new(),
    }
}

// ---------------------------------------------------------------------------
// 工具函数
// ---------------------------------------------------------------------------

/// 检测当前平台标识
pub fn current_platform() -> &'static str {
    if cfg!(target_os = "windows") {
        if cfg!(target_arch = "aarch64") { "win-arm64" } else { "win-x64" }
    } else if cfg!(target_os = "macos") {
        if cfg!(target_arch = "aarch64") { "mac-arm64" } else { "mac-x64" }
    } else {
        if cfg!(target_arch = "aarch64") { "linux-arm64" } else { "linux-x64" }
    }
}

/// 获取插件的安装目录（在 data_root 下）
pub fn plugin_install_dir(data_root: &PathBuf, plugin_name: &str) -> PathBuf {
    data_root.join("plugins").join(plugin_name)
}

/// 获取插件二进制目录
pub fn plugin_bin_dir(data_root: &PathBuf, plugin_name: &str) -> PathBuf {
    plugin_install_dir(data_root, plugin_name).join("bin")
}
