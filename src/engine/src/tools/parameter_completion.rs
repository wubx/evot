//! Smart parameter completion for tool calls.
//!
//! This module provides automatic parameter completion for common tool call scenarios,
//! particularly when LLMs generate tool calls with missing required parameters.

use std::path::Path;

use serde_json::Value;

/// Attempts to auto-complete missing parameters for tool calls based on context.
pub fn auto_complete_parameters(
    tool_name: &str,
    mut params: Value,
    context: &ParameterCompletionContext,
) -> Result<Value, String> {
    match tool_name {
        "write_file" => complete_write_file_params(&mut params, context),
        _ => Ok(params), // No completion available for other tools
    }
}

/// Context information for parameter completion.
pub struct ParameterCompletionContext {
    pub cwd: std::path::PathBuf,
    pub task_context: Option<String>,
}

impl ParameterCompletionContext {
    pub fn new(cwd: std::path::PathBuf) -> Self {
        Self {
            cwd,
            task_context: None,
        }
    }

    pub fn with_task_context(mut self, context: String) -> Self {
        self.task_context = Some(context);
        self
    }
}

/// Auto-complete parameters for write_file tool calls.
fn complete_write_file_params(
    params: &mut Value,
    context: &ParameterCompletionContext,
) -> Result<Value, String> {
    let obj = params
        .as_object_mut()
        .ok_or("Parameters must be an object")?;

    // If path is missing but content is present, try to generate a path
    if !obj.contains_key("path") && obj.contains_key("content") {
        if let Some(content) = obj.get("content").and_then(|v| v.as_str()) {
            let suggested_path = suggest_file_path(content, context)?;
            obj.insert("path".to_string(), Value::String(suggested_path));
        }
    }

    Ok(params.clone())
}

/// Generate a suitable file path based on content and context.
fn suggest_file_path(
    content: &str,
    context: &ParameterCompletionContext,
) -> Result<String, String> {
    // Detect file type from content
    let (extension, base_name) = detect_file_type_and_name(content);

    // Use task context to determine directory if available
    let directory = if let Some(task_ctx) = &context.task_context {
        extract_directory_from_context(task_ctx).unwrap_or_else(|| context.cwd.clone())
    } else {
        context.cwd.clone()
    };

    // Generate unique filename
    let filename = generate_unique_filename(&directory, &base_name, &extension)?;

    Ok(directory.join(filename).to_string_lossy().to_string())
}

/// Detect file type and suggest base name from content.
fn detect_file_type_and_name(content: &str) -> (String, String) {
    let trimmed = content.trim();

    // SVG detection
    if trimmed.starts_with("<?xml") && trimmed.contains("<svg") || trimmed.starts_with("<svg") {
        let name = extract_svg_title(content).unwrap_or_else(|| "diagram".to_string());
        return ("svg".to_string(), name);
    }

    // HTML detection
    if trimmed.starts_with("<!DOCTYPE html") || trimmed.starts_with("<html") {
        return ("html".to_string(), "page".to_string());
    }

    // JSON detection
    if (trimmed.starts_with('{') && trimmed.ends_with('}'))
        || (trimmed.starts_with('[') && trimmed.ends_with(']'))
    {
        return ("json".to_string(), "data".to_string());
    }

    // CSS detection
    if trimmed.contains('{')
        && trimmed.contains('}')
        && (trimmed.contains("color:") || trimmed.contains("font-") || trimmed.contains("margin:"))
    {
        return ("css".to_string(), "styles".to_string());
    }

    // JavaScript detection
    if trimmed.contains("function")
        || trimmed.contains("const ")
        || trimmed.contains("let ")
        || trimmed.contains("var ")
    {
        return ("js".to_string(), "script".to_string());
    }

    // Python detection
    if trimmed.contains("def ")
        || trimmed.contains("import ")
        || trimmed.contains("from ")
        || trimmed.starts_with("#!/usr/bin/env python")
    {
        return ("py".to_string(), "script".to_string());
    }

    // Markdown detection
    if trimmed.contains("# ") || trimmed.contains("## ") || trimmed.contains("```") {
        return ("md".to_string(), "document".to_string());
    }

    // Default to text
    ("txt".to_string(), "file".to_string())
}

/// Extract title or meaningful name from SVG content.
fn extract_svg_title(content: &str) -> Option<String> {
    // Look for <title> tag
    if let Some(start) = content.find("<title>") {
        if let Some(end) = content[start + 7..].find("</title>") {
            let title = &content[start + 7..start + 7 + end];
            if !title.trim().is_empty() {
                return Some(sanitize_filename(title.trim()));
            }
        }
    }

    // Look for common SVG patterns that might indicate purpose
    if content.contains("dashboard") || content.contains("仪表板") {
        return Some("dashboard".to_string());
    }
    if content.contains("login") || content.contains("登录") {
        return Some("login".to_string());
    }
    if content.contains("chart") || content.contains("图表") {
        return Some("chart".to_string());
    }
    if content.contains("mockup") || content.contains("视觉稿") {
        return Some("mockup".to_string());
    }

    None
}

/// Extract directory path from task context.
fn extract_directory_from_context(context: &str) -> Option<std::path::PathBuf> {
    // Look for directory mentions in context
    if let Some(start) = context.find("mockup") {
        if let Some(dir_start) = context[..start].rfind('/') {
            if let Some(dir_end) = context[start..]
                .find(' ')
                .or_else(|| Some(context.len() - start))
            {
                let dir_path = &context[dir_start..start + dir_end];
                return Some(std::path::PathBuf::from(dir_path));
            }
        }
    }

    // Look for explicit path mentions
    for word in context.split_whitespace() {
        if word.starts_with('/') && word.contains('/') {
            if let Some(path) = Path::new(word).parent() {
                return Some(path.to_path_buf());
            }
        }
    }

    None
}

/// Generate a unique filename to avoid conflicts.
fn generate_unique_filename(
    directory: &Path,
    base_name: &str,
    extension: &str,
) -> Result<String, String> {
    let sanitized_base = sanitize_filename(base_name);
    let mut filename = format!("{}.{}", sanitized_base, extension);
    let mut counter = 1;

    // Check if file exists and increment counter if needed
    while directory.join(&filename).exists() {
        filename = format!("{}_{}.{}", sanitized_base, counter, extension);
        counter += 1;

        // Prevent infinite loop
        if counter > 1000 {
            return Err("Unable to generate unique filename".to_string());
        }
    }

    Ok(filename)
}

/// Sanitize filename by removing invalid characters.
fn sanitize_filename(name: &str) -> String {
    name.chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            c if c.is_control() => '_',
            c => c,
        })
        .collect::<String>()
        .trim_matches('.')
        .to_string()
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn test_svg_detection() {
        let svg_content = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 800 600">
            <title>Dashboard Mockup</title>
            <rect width="800" height="600" fill="#f5f5f5"/>
        </svg>"##;

        let (ext, name) = detect_file_type_and_name(svg_content);
        assert_eq!(ext, "svg");
        assert_eq!(name, "Dashboard_Mockup");
    }

    #[test]
    fn test_write_file_completion() {
        let context = ParameterCompletionContext::new(std::path::PathBuf::from("/tmp"))
            .with_task_context("save to /data1/test/mockup directory".to_string());

        let mut params = json!({
            "content": "<svg><title>Login Page</title></svg>"
        });

        let result = complete_write_file_params(&mut params, &context).unwrap();
        let path = result["path"].as_str().unwrap();

        assert!(path.contains("Login_Page"));
        assert!(path.ends_with(".svg"));
    }

    #[test]
    fn test_filename_sanitization() {
        assert_eq!(
            sanitize_filename("file/name:with*bad?chars"),
            "file_name_with_bad_chars"
        );
        assert_eq!(sanitize_filename("normal_filename"), "normal_filename");
    }
}
