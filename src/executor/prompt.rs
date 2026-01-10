use serde::{Deserialize, Serialize};
use std::path::Path;

use super::FileContext;
use crate::types::PatchFormat;

const DEFAULT_SYSTEM_PROMPT: &str = r#"You are an expert code refactoring assistant. Your task is to generate precise code changes based on the user's request.

IMPORTANT RULES:
1. Output changes as unified diffs (preferred) or search/replace blocks
2. Use the exact file paths provided
3. Preserve existing code style and formatting
4. Make minimal, focused changes
5. Do not add unnecessary modifications

OUTPUT FORMAT (choose one):

Option A - Unified Diff:
```diff
--- a/path/to/file.rs
+++ b/path/to/file.rs
@@ -10,5 +10,6 @@
 existing context
-old line to remove
+new line to add
 more context
```

Option B - Search/Replace:
File: path/to/file.rs
<<<<<<< SEARCH
exact code to find
=======
replacement code
>>>>>>> REPLACE

Always include enough context for unique matching.
"#;

const DEFAULT_LANGUAGE_HINT: &str = "text";
const ROLE_SYSTEM: &str = "system";
const ROLE_USER: &str = "user";
const FORMAT_UNIFIED: &str = "unified_diff";
const FORMAT_SEARCH_REPLACE: &str = "search_replace";
const FORMAT_WHOLE_FILE: &str = "whole_file";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

pub struct PromptBuilder {
    system_prompt: String,
}

impl Default for PromptBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl PromptBuilder {
    pub fn new() -> Self {
        Self {
            system_prompt: DEFAULT_SYSTEM_PROMPT.to_string(),
        }
    }

    pub fn with_system_prompt(mut self, prompt: impl Into<String>) -> Self {
        self.system_prompt = prompt.into();
        self
    }

    pub fn build_messages(
        &self,
        task: &str,
        files: &[FileContext],
        preferred_format: PatchFormat,
    ) -> Vec<ChatMessage> {
        let user_message = build_user_message(task, files, preferred_format);

        vec![
            ChatMessage {
                role: ROLE_SYSTEM.to_string(),
                content: self.system_prompt.clone(),
            },
            ChatMessage {
                role: ROLE_USER.to_string(),
                content: user_message,
            },
        ]
    }
}

fn build_user_message(task: &str, files: &[FileContext], preferred_format: PatchFormat) -> String {
    let mut message = String::new();
    push_files_section(&mut message, files);
    push_task_section(&mut message, task);
    push_format_section(&mut message, preferred_format);
    message
}

fn push_files_section(message: &mut String, files: &[FileContext]) {
    message.push_str("## Files\n\n");
    for file in files {
        let language_hint = language_hint(file);
        message.push_str("### ");
        message.push_str(&file.path);
        message.push('\n');
        message.push_str("```");
        message.push_str(&language_hint);
        message.push('\n');
        message.push_str(&file.content);
        if !file.content.ends_with('\n') {
            message.push('\n');
        }
        message.push_str("```\n\n");
    }
}

fn push_task_section(message: &mut String, task: &str) {
    message.push_str("## Task\n");
    message.push_str(task);
    message.push_str("\n\n");
}

fn push_format_section(message: &mut String, preferred_format: PatchFormat) {
    message.push_str("## Preferred Format\n");
    message.push_str(format_label(preferred_format));
    message.push('\n');
}

fn format_label(preferred_format: PatchFormat) -> &'static str {
    match preferred_format {
        PatchFormat::Unified => FORMAT_UNIFIED,
        PatchFormat::SearchReplace => FORMAT_SEARCH_REPLACE,
        PatchFormat::WholeFile => FORMAT_WHOLE_FILE,
    }
}

fn language_hint(file: &FileContext) -> String {
    if let Some(language) = &file.language {
        let trimmed = language.trim();
        if !trimmed.is_empty() {
            return trimmed.to_string();
        }
    }

    language_from_path(&file.path)
}

fn language_from_path(path: &str) -> String {
    let extension = Path::new(path)
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or(DEFAULT_LANGUAGE_HINT);
    let normalized = extension.trim().to_lowercase();
    if normalized.is_empty() {
        return DEFAULT_LANGUAGE_HINT.to_string();
    }
    map_extension_to_language(&normalized).to_string()
}

fn map_extension_to_language(extension: &str) -> &str {
    match extension {
        "rs" => "rust",
        "md" => "markdown",
        "yml" | "yaml" => "yaml",
        "toml" => "toml",
        "json" => "json",
        "js" => "javascript",
        "ts" => "typescript",
        "tsx" => "tsx",
        "jsx" => "jsx",
        "py" => "python",
        "go" => "go",
        "rb" => "ruby",
        "java" => "java",
        "kt" | "kts" => "kotlin",
        "c" => "c",
        "h" => "c",
        "cpp" | "cc" | "cxx" | "hpp" | "hh" | "hxx" => "cpp",
        "cs" => "csharp",
        "sh" => "bash",
        _ => extension,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::executor::FileContext;
    use crate::types::PatchFormat;

    #[test]
    fn new_uses_default_system_prompt() {
        let builder = PromptBuilder::new();
        assert_eq!(builder.system_prompt, DEFAULT_SYSTEM_PROMPT);
    }

    #[test]
    fn with_system_prompt_overrides_default() {
        let builder = PromptBuilder::new().with_system_prompt("custom prompt");
        assert_eq!(builder.system_prompt, "custom prompt");
    }

    #[test]
    fn build_messages_includes_system_and_user_messages() {
        let files = vec![FileContext {
            path: "src/main.rs".to_string(),
            content: "fn main() {}\n".to_string(),
            language: None,
        }];

        let messages =
            PromptBuilder::new().build_messages("Refactor main", &files, PatchFormat::Unified);

        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].role, ROLE_SYSTEM);
        assert_eq!(messages[1].role, ROLE_USER);
        assert!(messages[1].content.contains("## Files"));
        assert!(messages[1].content.contains("### src/main.rs"));
        assert!(messages[1].content.contains("```rust\nfn main() {}\n```"));
        assert!(messages[1].content.contains("## Task\nRefactor main"));
        assert!(
            messages[1]
                .content
                .contains("## Preferred Format\nunified_diff")
        );
    }

    #[test]
    fn build_messages_uses_explicit_language_hint() {
        let files = vec![FileContext {
            path: "Cargo.toml".to_string(),
            content: "[package]\nname = \"nexus\"\n".to_string(),
            language: Some("toml".to_string()),
        }];

        let messages = PromptBuilder::new().build_messages(
            "Update manifest",
            &files,
            PatchFormat::SearchReplace,
        );

        assert!(
            messages[1]
                .content
                .contains("```toml\n[package]\nname = \"nexus\"\n```")
        );
        assert!(
            messages[1]
                .content
                .contains("## Preferred Format\nsearch_replace")
        );
    }

    #[test]
    fn build_messages_supports_whole_file_format() {
        let files = vec![FileContext {
            path: "README.md".to_string(),
            content: "# Nexus\n".to_string(),
            language: None,
        }];

        let messages =
            PromptBuilder::new().build_messages("Rewrite readme", &files, PatchFormat::WholeFile);

        assert!(
            messages[1]
                .content
                .contains("## Preferred Format\nwhole_file")
        );
        assert!(messages[1].content.contains("```markdown\n# Nexus\n```"));
    }
}
