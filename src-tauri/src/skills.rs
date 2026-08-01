//! 技能管理：扫描 `{kimi_code_home}/skills/*/SKILL.md`，解析 YAML front-matter。

use serde::Serialize;
use std::collections::HashMap;
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, Serialize)]
pub struct SkillInfo {
    /// 目录名（作为 id 使用）
    pub dir: String,
    /// 展示名：front-matter name 或目录名
    pub name: String,
    pub description: Option<String>,
    /// SKILL.md 绝对路径
    pub path: String,
}

/// 手写 YAML front-matter 解析：仅支持 `---` 包围的简单 `key: value`。
pub fn parse_front_matter(content: &str) -> HashMap<String, String> {
    let mut map = HashMap::new();
    let mut lines = content.lines();
    if lines.next().map(|l| l.trim()) != Some("---") {
        return map;
    }
    for line in lines {
        let trimmed = line.trim_end();
        if trimmed.trim() == "---" {
            break;
        }
        if let Some((key, value)) = trimmed.split_once(':') {
            let key = key.trim();
            if key.is_empty() {
                continue;
            }
            let value = value.trim().trim_matches('"').trim_matches('\'');
            map.insert(key.to_string(), value.to_string());
        }
    }
    map
}

pub fn list_skills(kimi_home: &Path) -> Vec<SkillInfo> {
    let mut out = Vec::new();
    let dir = kimi_home.join("skills");
    let Ok(entries) = fs::read_dir(&dir) else {
        return out;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let md = path.join("SKILL.md");
        if !md.is_file() {
            continue;
        }
        let dir_name = entry.file_name().to_string_lossy().into_owned();
        let content = fs::read_to_string(&md).unwrap_or_default();
        let front = parse_front_matter(&content);
        let name = front
            .get("name")
            .filter(|n| !n.is_empty())
            .cloned()
            .unwrap_or_else(|| dir_name.clone());
        let description = front.get("description").filter(|d| !d.is_empty()).cloned();
        out.push(SkillInfo {
            dir: dir_name,
            name,
            description,
            path: md.to_string_lossy().into_owned(),
        });
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

/// 读取技能全文。name 为目录名（SkillInfo.dir），拒绝路径穿越。
pub fn read_skill_content(kimi_home: &Path, name: &str) -> Result<String, String> {
    if name.is_empty() || name.contains("..") || name.contains(['/', '\\']) {
        return Err("非法技能名".into());
    }
    let path = kimi_home.join("skills").join(name).join("SKILL.md");
    fs::read_to_string(&path).map_err(|e| format!("读取失败: {e}"))
}
