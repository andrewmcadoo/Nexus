use std::borrow::Cow;
use std::collections::HashSet;
use std::sync::OnceLock;

use regex::Regex;

use crate::error::NexusError;
use crate::types::{
    ActionDetails, ActionKindTag, MatchMode, PatchDetails, PatchFormat, ProposedAction,
    SearchReplaceBlock,
};

const DEFAULT_RISK: u8 = 1;
const ACTION_INDEX_BASE: usize = 1;
const SINGLE_FILE_COUNT: usize = 1;
const SUMMARY_DIFF_LINE_THRESHOLD: usize = 2;
const JSON_KIND_KEY: &str = "\"kind\"";
const JSON_DETAILS_KEY: &str = "\"details\"";

pub struct ResponseParser {
    diff_fenced: OnceLock<Regex>,
    diff_raw: OnceLock<Regex>,
    search_replace: OnceLock<Regex>,
    json_fenced: OnceLock<Regex>,
}

impl Default for ResponseParser {
    fn default() -> Self {
        Self::new()
    }
}

impl ResponseParser {
    pub fn new() -> Self {
        Self {
            diff_fenced: OnceLock::new(),
            diff_raw: OnceLock::new(),
            search_replace: OnceLock::new(),
            json_fenced: OnceLock::new(),
        }
    }

    pub fn parse(&self, response: &str, run_id: &str) -> Result<Vec<ProposedAction>, NexusError> {
        self.validate_run_id(run_id)?;

        let mut actions = self.parse_unified_diffs(response, run_id);
        if !actions.is_empty() {
            return Ok(actions);
        }

        actions = self.parse_search_replace(response, run_id);
        if !actions.is_empty() {
            return Ok(actions);
        }

        self.parse_json_actions(response)
    }

    pub fn parse_unified_diffs(&self, response: &str, run_id: &str) -> Vec<ProposedAction> {
        let normalized = normalize_line_endings(response);
        let diffs = self.collect_unified_diffs(&normalized);
        self.build_patch_actions_from_diffs(diffs, run_id)
    }

    pub fn parse_search_replace(&self, response: &str, run_id: &str) -> Vec<ProposedAction> {
        let normalized = normalize_line_endings(response);
        let blocks = self.collect_search_replace_blocks(&normalized);
        self.build_search_replace_actions(blocks, run_id)
    }

    pub fn parse_json_actions(&self, response: &str) -> Result<Vec<ProposedAction>, NexusError> {
        let normalized = normalize_line_endings(response);
        if let Some(actions) = self.parse_fenced_json_actions(&normalized)? {
            return Ok(actions);
        }

        self.parse_inline_json_actions(&normalized)
    }

    pub fn extract_files_from_diff(&self, diff: &str) -> Vec<String> {
        let mut seen = HashSet::new();
        let mut files = Vec::new();

        for line in diff.lines() {
            if let Some(path) = extract_path_from_diff_line(line) {
                if seen.insert(path.clone()) {
                    files.push(path);
                }
            }
        }

        files
    }

    pub fn generate_summary_from_diff(&self, diff: &str, files: &[String]) -> String {
        if files.is_empty() {
            return summary_from_diff_fallback(diff);
        }

        if files.len() == SINGLE_FILE_COUNT {
            return format!("Apply patch to {}", files[0]);
        }

        let remaining = files.len().saturating_sub(SINGLE_FILE_COUNT);
        format!("Apply patch to {} and {} other files", files[0], remaining)
    }

    pub fn generate_action_id(&self, run_id: &str, index: usize) -> String {
        format!("{run_id}-action-{index}")
    }

    /// Validates a run_id for use in action IDs and event correlation.
    ///
    /// Validates that run_id is non-empty, contains no path separators or traversal
    /// sequences, and does not exceed 255 characters. These checks match the validation
    /// in the event_log module per ADR-014.
    fn validate_run_id(&self, run_id: &str) -> Result<(), NexusError> {
        if run_id.trim().is_empty() {
            return Err(NexusError::InvalidRunId(run_id.to_string()));
        }
        // Path separator and traversal checks (matches event_log validation)
        if run_id.contains('/') || run_id.contains('\\') || run_id.contains("..") {
            return Err(NexusError::InvalidRunId(run_id.to_string()));
        }
        // Length check (255 chars max, matches filesystem constraints)
        if run_id.len() > 255 {
            return Err(NexusError::InvalidRunId(run_id.to_string()));
        }
        Ok(())
    }

    fn diff_fenced_regex(&self) -> &Regex {
        self.diff_fenced.get_or_init(|| {
            Regex::new(r"(?s)```diff\s*(?P<diff>.*?)```").expect("diff fenced regex should compile")
        })
    }

    fn diff_raw_regex(&self) -> &Regex {
        self.diff_raw
            .get_or_init(|| Regex::new(r"(?m)^---\s+a/.*$").expect("diff raw regex should compile"))
    }

    fn search_replace_regex(&self) -> &Regex {
        self.search_replace.get_or_init(|| {
            Regex::new(
                r"(?s)<<<<<<< SEARCH(?:\s+(?P<path>[^\r\n]+))?\r?\n(?P<search>.*?)\r?\n=======\r?\n(?P<replace>.*?)\r?\n>>>>>>> REPLACE",
            )
            .expect("search/replace regex should compile")
        })
    }

    fn json_fenced_regex(&self) -> &Regex {
        self.json_fenced.get_or_init(|| {
            Regex::new(r"(?s)```json\s*(?P<json>\[.*\])\s*```")
                .expect("json fenced regex should compile")
        })
    }

    fn collect_unified_diffs(&self, response: &str) -> Vec<String> {
        let mut diffs = Vec::new();
        for capture in self.diff_fenced_regex().captures_iter(response) {
            if let Some(diff) = capture.name("diff") {
                let trimmed = diff.as_str().trim();
                if !trimmed.is_empty() {
                    diffs.push(trimmed.to_string());
                }
            }
        }

        let without_fenced = self.diff_fenced_regex().replace_all(response, "");
        diffs.extend(self.collect_raw_diff_blocks(without_fenced.as_ref()));

        diffs
    }

    fn collect_raw_diff_blocks(&self, response: &str) -> Vec<String> {
        let mut diffs = Vec::new();
        let mut starts: Vec<usize> = self
            .diff_raw_regex()
            .find_iter(response)
            .map(|matched| matched.start())
            .collect();

        if starts.is_empty() {
            return diffs;
        }

        starts.push(response.len());
        for window in starts.windows(2) {
            let diff = response[window[0]..window[1]].trim();
            if !diff.is_empty() {
                diffs.push(diff.to_string());
            }
        }

        diffs
    }

    fn build_patch_actions_from_diffs(
        &self,
        diffs: Vec<String>,
        run_id: &str,
    ) -> Vec<ProposedAction> {
        diffs
            .into_iter()
            .enumerate()
            .map(|(index, diff)| {
                let files = self.extract_files_from_diff(&diff);
                let summary = self.generate_summary_from_diff(&diff, &files);
                let details = patch_details_from_diff(diff, files.clone());
                self.build_patch_action(run_id, index + ACTION_INDEX_BASE, summary, details)
            })
            .collect()
    }

    fn collect_search_replace_blocks(&self, response: &str) -> Vec<SearchReplaceBlock> {
        let mut blocks = Vec::new();
        for capture in self.search_replace_regex().captures_iter(response) {
            let file = capture
                .name("path")
                .map(|value| value.as_str().trim().to_string())
                .unwrap_or_default();
            let search = capture
                .name("search")
                .map(|value| value.as_str().to_string())
                .unwrap_or_default();
            let replace = capture
                .name("replace")
                .map(|value| value.as_str().to_string())
                .unwrap_or_default();

            blocks.push(SearchReplaceBlock {
                file,
                search,
                replace,
                match_mode: MatchMode::Exact,
            });
        }

        blocks
    }

    fn build_search_replace_actions(
        &self,
        blocks: Vec<SearchReplaceBlock>,
        run_id: &str,
    ) -> Vec<ProposedAction> {
        blocks
            .into_iter()
            .enumerate()
            .map(|(index, block)| {
                let summary = summary_from_search_replace(&block.file);
                let details = patch_details_from_search_replace(block.clone());
                self.build_patch_action(run_id, index + ACTION_INDEX_BASE, summary, details)
            })
            .collect()
    }

    fn build_patch_action(
        &self,
        run_id: &str,
        index: usize,
        summary: String,
        details: PatchDetails,
    ) -> ProposedAction {
        ProposedAction {
            id: self.generate_action_id(run_id, index),
            summary,
            why: None,
            risk: DEFAULT_RISK,
            policy_tags: Vec::new(),
            requires_approval: true,
            created_by: None,
            approval_group: None,
            kind: ActionKindTag::Patch,
            details: ActionDetails::Patch(details),
        }
    }

    /// Parses JSON actions from fenced code blocks in the response.
    ///
    /// Looks for ```json ... ``` blocks containing a JSON array of `ProposedAction` objects.
    /// Returns the first successfully parsed array, or `None` if no fenced JSON blocks are found.
    ///
    /// # JSON Format
    ///
    /// Expects a JSON array where each element has at minimum:
    /// - `"kind"`: The action type (e.g., "patch", "shell")
    /// - `"details"`: Action-specific payload matching the kind
    ///
    /// # Errors
    ///
    /// Returns `NexusError::JsonError` if a fenced block is found but contains invalid JSON
    /// or does not conform to the `ProposedAction` schema.
    fn parse_fenced_json_actions(
        &self,
        response: &str,
    ) -> Result<Option<Vec<ProposedAction>>, NexusError> {
        for capture in self.json_fenced_regex().captures_iter(response) {
            if let Some(json) = capture.name("json") {
                let actions = parse_actions_from_json(json.as_str())?;
                return Ok(Some(actions));
            }
        }

        Ok(None)
    }

    /// Parses JSON actions embedded inline (not in fenced blocks) in the response.
    ///
    /// Scans for JSON arrays directly in the text by matching balanced `[...]` brackets.
    /// Only considers arrays that appear to be action arrays (contain `"kind"` and `"details"` keys).
    /// Returns the first successfully parsed action array, or an empty `Vec` if none found.
    ///
    /// # Heuristics
    ///
    /// Uses `looks_like_action_array()` to filter candidates before attempting JSON parse,
    /// reducing failed parse attempts on unrelated JSON arrays in the response.
    ///
    /// # Errors
    ///
    /// Returns `NexusError::JsonError` if a candidate array is found but parsing fails.
    fn parse_inline_json_actions(&self, response: &str) -> Result<Vec<ProposedAction>, NexusError> {
        for candidate in extract_json_arrays(response) {
            if !looks_like_action_array(&candidate) {
                continue;
            }
            return parse_actions_from_json(&candidate);
        }

        Ok(Vec::new())
    }
}

fn normalize_line_endings(input: &str) -> Cow<'_, str> {
    if input.contains("\r\n") {
        Cow::Owned(input.replace("\r\n", "\n"))
    } else {
        Cow::Borrowed(input)
    }
}

fn extract_path_from_diff_line(line: &str) -> Option<String> {
    if !(line.starts_with("--- ") || line.starts_with("+++ ")) {
        return None;
    }

    let trimmed = line[4..].trim();
    let token = trimmed.split_whitespace().next()?;
    if token == "/dev/null" {
        return None;
    }

    let normalized = token.trim_start_matches("a/").trim_start_matches("b/");
    if normalized.is_empty() {
        return None;
    }

    Some(normalized.to_string())
}

fn patch_details_from_diff(diff: String, files: Vec<String>) -> PatchDetails {
    PatchDetails {
        format: PatchFormat::Unified,
        diff: Some(diff),
        files,
        ..Default::default()
    }
}

fn patch_details_from_search_replace(block: SearchReplaceBlock) -> PatchDetails {
    let files = if block.file.is_empty() {
        Vec::new()
    } else {
        vec![block.file.clone()]
    };

    PatchDetails {
        format: PatchFormat::SearchReplace,
        search_replace_blocks: Some(vec![block]),
        files,
        ..Default::default()
    }
}

fn summary_from_diff_fallback(diff: &str) -> String {
    let line_count = diff.lines().count();
    if line_count <= SUMMARY_DIFF_LINE_THRESHOLD {
        return "Apply patch".to_string();
    }

    "Apply multi-file patch".to_string()
}

fn summary_from_search_replace(file: &str) -> String {
    if file.trim().is_empty() {
        return "Apply search/replace block".to_string();
    }

    format!("Apply search/replace to {}", file)
}

fn parse_actions_from_json(json: &str) -> Result<Vec<ProposedAction>, NexusError> {
    serde_json::from_str::<Vec<ProposedAction>>(json).map_err(|source| NexusError::JsonError {
        context: "Failed to parse JSON actions".to_string(),
        source,
    })
}

fn looks_like_action_array(candidate: &str) -> bool {
    candidate.contains(JSON_KIND_KEY) && candidate.contains(JSON_DETAILS_KEY)
}

fn extract_json_arrays(text: &str) -> Vec<String> {
    let mut arrays = Vec::new();
    let mut start: Option<usize> = None;
    let mut depth = 0usize;
    let mut in_string = false;
    let mut escape = false;

    for (index, ch) in text.char_indices() {
        if in_string {
            if escape {
                escape = false;
                continue;
            }
            if ch == '\\' {
                escape = true;
                continue;
            }
            if ch == '\"' {
                in_string = false;
            }
            continue;
        }

        match ch {
            '\"' => in_string = true,
            '[' => {
                if depth == 0 {
                    start = Some(index);
                }
                depth += 1;
            }
            ']' => {
                if depth == 0 {
                    continue;
                }
                depth -= 1;
                if depth == 0 {
                    if let Some(start_index) = start.take() {
                        arrays.push(text[start_index..=index].to_string());
                    }
                }
            }
            _ => {}
        }
    }

    arrays
}

#[cfg(test)]
mod tests {
    use super::*;

    const RUN_ID: &str = "run-123";

    #[test]
    fn parse_unified_diffs_from_fenced_block() {
        let parser = ResponseParser::new();
        let response = "Patch follows:\n```diff\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1 +1 @@\n-old\n+new\n```\n";

        let actions = parser.parse_unified_diffs(response, RUN_ID);

        assert_eq!(actions.len(), 1);
        let action = &actions[0];
        assert_eq!(action.id, "run-123-action-1");
        assert_eq!(action.kind, ActionKindTag::Patch);
        match &action.details {
            ActionDetails::Patch(details) => {
                assert_eq!(details.format, PatchFormat::Unified);
                assert!(details.diff.as_ref().unwrap().contains("--- a/src/lib.rs"));
                assert_eq!(details.files, vec!["src/lib.rs".to_string()]);
            }
            _ => panic!("expected patch details"),
        }
    }

    #[test]
    fn parse_unified_diffs_from_raw_diff() {
        let parser = ResponseParser::new();
        let response = "--- a/src/main.rs\n+++ b/src/main.rs\n@@ -1 +1 @@\n-old\n+new\n";

        let actions = parser.parse_unified_diffs(response, RUN_ID);

        assert_eq!(actions.len(), 1);
        match &actions[0].details {
            ActionDetails::Patch(details) => {
                assert_eq!(details.format, PatchFormat::Unified);
                assert_eq!(details.files, vec!["src/main.rs".to_string()]);
            }
            _ => panic!("expected patch details"),
        }
    }

    #[test]
    fn parse_multiple_raw_diffs() {
        let parser = ResponseParser::new();
        let response = "--- a/src/main.rs\n+++ b/src/main.rs\n@@ -1 +1 @@\n-old\n+new\n\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1 +1 @@\n-old\n+new\n";

        let actions = parser.parse_unified_diffs(response, RUN_ID);

        assert_eq!(actions.len(), 2);
        match &actions[0].details {
            ActionDetails::Patch(details) => {
                assert_eq!(details.files, vec!["src/main.rs".to_string()]);
            }
            _ => panic!("expected patch details"),
        }
        match &actions[1].details {
            ActionDetails::Patch(details) => {
                assert_eq!(details.files, vec!["src/lib.rs".to_string()]);
            }
            _ => panic!("expected patch details"),
        }
    }

    #[test]
    fn parse_search_replace_blocks() {
        let parser = ResponseParser::new();
        let response = "<<<<<<< SEARCH src/lib.rs\nold\n=======\nnew\n>>>>>>> REPLACE\n";

        let actions = parser.parse_search_replace(response, RUN_ID);

        assert_eq!(actions.len(), 1);
        match &actions[0].details {
            ActionDetails::Patch(details) => {
                assert_eq!(details.format, PatchFormat::SearchReplace);
                let blocks = details.search_replace_blocks.as_ref().unwrap();
                assert_eq!(blocks.len(), 1);
                assert_eq!(blocks[0].file, "src/lib.rs");
                assert_eq!(blocks[0].search, "old");
                assert_eq!(blocks[0].replace, "new");
            }
            _ => panic!("expected patch details"),
        }
    }

    #[test]
    fn parse_json_actions_from_fenced_block() {
        let parser = ResponseParser::new();
        let response = "```json\n[\n  {\"id\":\"action-1\",\"summary\":\"Update\",\"kind\":\"patch\",\"details\":{\"format\":\"unified\",\"diff\":\"--- a/src/lib.rs\\n+++ b/src/lib.rs\"}}\n]\n```";

        let actions = parser.parse_json_actions(response).expect("json parse");

        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0].id, "action-1");
        assert_eq!(actions[0].kind, ActionKindTag::Patch);
    }

    #[test]
    fn parse_orchestrates_fallbacks() {
        let parser = ResponseParser::new();
        let response = "<<<<<<< SEARCH src/lib.rs\nold\n=======\nnew\n>>>>>>> REPLACE\n";

        let actions = parser.parse(response, RUN_ID).expect("parse");

        assert_eq!(actions.len(), 1);
        match &actions[0].details {
            ActionDetails::Patch(details) => {
                assert_eq!(details.format, PatchFormat::SearchReplace);
            }
            _ => panic!("expected patch details"),
        }
    }

    #[test]
    fn test_parse_empty_response() {
        // Arrange
        let parser = ResponseParser::new();

        // Act
        let actions = parser.parse("", RUN_ID).expect("parse");

        // Assert
        assert!(actions.is_empty());
    }

    #[test]
    fn test_extract_files_from_diff() {
        // Arrange
        let parser = ResponseParser::new();
        let diff = "--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1 +1 @@\n-old\n+new\n";

        // Act
        let files = parser.extract_files_from_diff(diff);

        // Assert
        assert_eq!(files, vec!["src/lib.rs".to_string()]);
    }

    #[test]
    fn test_extract_files_multiple() {
        // Arrange
        let parser = ResponseParser::new();
        let diff = "--- a/src/main.rs\n+++ b/src/main.rs\n@@ -1 +1 @@\n-old\n+new\n\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1 +1 @@\n-old\n+new\n";

        // Act
        let files = parser.extract_files_from_diff(diff);

        // Assert
        assert_eq!(
            files,
            vec!["src/main.rs".to_string(), "src/lib.rs".to_string()]
        );
    }

    #[test]
    fn test_extract_files_handles_dev_null() {
        // Arrange
        let parser = ResponseParser::new();
        let diff = "--- /dev/null\n+++ b/src/new.rs\n";

        // Act
        let files = parser.extract_files_from_diff(diff);

        // Assert
        assert_eq!(files, vec!["src/new.rs".to_string()]);
    }

    #[test]
    fn test_generate_summary_single_file() {
        // Arrange
        let parser = ResponseParser::new();
        let diff = "--- a/src/lib.rs\n+++ b/src/lib.rs\n";
        let files = vec!["src/lib.rs".to_string()];

        // Act
        let summary = parser.generate_summary_from_diff(diff, &files);

        // Assert
        assert_eq!(summary, "Apply patch to src/lib.rs");
    }

    #[test]
    fn test_generate_summary_multiple_files() {
        // Arrange
        let parser = ResponseParser::new();
        let diff = "--- a/src/main.rs\n+++ b/src/main.rs\n";
        let files = vec![
            "src/main.rs".to_string(),
            "src/lib.rs".to_string(),
            "src/utils.rs".to_string(),
        ];

        // Act
        let summary = parser.generate_summary_from_diff(diff, &files);

        // Assert
        let remaining = files.len().saturating_sub(SINGLE_FILE_COUNT);
        let expected = format!("Apply patch to {} and {} other files", files[0], remaining);
        assert_eq!(summary, expected);
    }

    #[test]
    fn test_generate_summary_empty_files() {
        // Arrange
        let parser = ResponseParser::new();
        let diff = "diff-only";
        let files: Vec<String> = Vec::new();

        // Act
        let summary = parser.generate_summary_from_diff(diff, &files);

        // Assert
        assert_eq!(summary, "Apply patch");
    }
}
