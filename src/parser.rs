//! Markdown parser for PAVED document validation.
//!
//! This module parses markdown documents and extracts structured information
//! about their sections, code blocks, and commands for validation purposes.

use anyhow::{Context, Result};
use serde::Deserialize;
use std::path::{Path, PathBuf};

/// Paver-specific frontmatter configuration.
#[derive(Debug, Clone, Deserialize, PartialEq, Default)]
pub struct PaverFrontmatter {
    /// Code paths that this document covers.
    #[serde(default)]
    pub paths: Vec<String>,
}

/// YAML frontmatter wrapper.
#[derive(Debug, Clone, Deserialize, PartialEq, Default)]
struct FrontmatterWrapper {
    /// Paver-specific configuration.
    #[serde(default)]
    paver: Option<PaverFrontmatter>,
}

/// A parsed PAVED document with extracted structure.
#[derive(Debug)]
pub struct ParsedDoc {
    /// Path to the source file.
    pub path: PathBuf,
    /// H1 heading (document title), if present.
    pub title: Option<String>,
    /// Extracted H2 sections.
    pub sections: Vec<Section>,
    /// Total number of lines in the document.
    pub line_count: usize,
    /// Paver-specific frontmatter configuration.
    pub frontmatter: Option<PaverFrontmatter>,
}

/// A fenced code block extracted from a section.
#[derive(Debug, Clone, PartialEq)]
pub struct CodeBlock {
    /// Language tag (e.g., "bash", "rust"), if present.
    pub language: Option<String>,
    /// The code content inside the block (without the fence markers).
    pub content: String,
    /// Line number where the code block starts (1-indexed, points to opening fence).
    pub start_line: usize,
    /// Whether this code block contains executable shell commands.
    pub is_executable: bool,
}

/// A section of a PAVED document (H2 heading and its content).
#[derive(Debug)]
pub struct Section {
    /// Section name (the H2 heading text without "## ").
    pub name: String,
    /// Line number where the section starts (1-indexed).
    pub start_line: usize,
    /// Content of the section (excluding the heading itself).
    pub content: String,
    /// Whether the section contains code blocks (triple backticks).
    pub has_code_blocks: bool,
    /// Whether the section contains executable commands.
    pub has_commands: bool,
    /// Extracted code blocks from this section.
    pub code_blocks: Vec<CodeBlock>,
}

impl Section {
    /// Returns only the code blocks that are marked as executable.
    ///
    /// Executable blocks are those with shell language tags (bash, sh, shell, zsh),
    /// content with shell prompts ($ or >), or preceded by a `<!-- paver:run -->` marker.
    pub fn executable_commands(&self) -> Vec<&CodeBlock> {
        self.code_blocks
            .iter()
            .filter(|b| b.is_executable)
            .collect()
    }
}

impl ParsedDoc {
    /// Parse a markdown file into a structured document.
    pub fn parse(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("Failed to read file: {}", path.display()))?;
        Self::parse_content(path.to_path_buf(), &content)
    }

    /// Parse markdown content into a structured document.
    pub fn parse_content(path: PathBuf, content: &str) -> Result<Self> {
        let lines: Vec<&str> = content.lines().collect();
        let line_count = lines.len();

        let frontmatter = Self::extract_frontmatter(content);
        let title = Self::extract_title(&lines);
        let sections = Self::extract_sections(&lines);

        Ok(ParsedDoc {
            path,
            title,
            sections,
            line_count,
            frontmatter,
        })
    }

    /// Check if the document has a section with the given name (case-insensitive).
    pub fn has_section(&self, name: &str) -> bool {
        self.sections
            .iter()
            .any(|s| s.name.eq_ignore_ascii_case(name))
    }

    /// Get a section by name (case-insensitive).
    pub fn get_section(&self, name: &str) -> Option<&Section> {
        self.sections
            .iter()
            .find(|s| s.name.eq_ignore_ascii_case(name))
    }

    /// Extract the H1 title from the document.
    fn extract_title(lines: &[&str]) -> Option<String> {
        for line in lines {
            let trimmed = line.trim();
            if let Some(title) = trimmed.strip_prefix("# ") {
                // Ensure it's not an H2 (## ) heading
                if !title.starts_with("# ") {
                    return Some(title.trim().to_string());
                }
            }
        }
        None
    }

    /// Extract all H2 sections from the document.
    fn extract_sections(lines: &[&str]) -> Vec<Section> {
        let mut sections = Vec::new();
        let mut section_starts: Vec<(usize, String)> = Vec::new();

        // Find all H2 headings and their positions
        for (idx, line) in lines.iter().enumerate() {
            let trimmed = line.trim();
            if let Some(heading) = trimmed.strip_prefix("## ") {
                // Ensure it's not an H3 (### ) heading
                if !heading.starts_with("# ") {
                    let name = heading.trim().to_string();
                    section_starts.push((idx, name));
                }
            }
        }

        // Extract content for each section
        for (i, (start_idx, name)) in section_starts.iter().enumerate() {
            let end_idx = if i + 1 < section_starts.len() {
                section_starts[i + 1].0
            } else {
                lines.len()
            };

            // Content starts after the heading line
            let content_lines = &lines[start_idx + 1..end_idx];
            let content = content_lines.join("\n");

            let has_code_blocks = Self::detect_code_blocks(content_lines);
            let has_commands = Self::detect_commands(content_lines);
            // Base line for content is start_idx + 2 (1-indexed: line after heading)
            let code_blocks = Self::extract_code_blocks(content_lines, start_idx + 2);

            sections.push(Section {
                name: name.clone(),
                start_line: start_idx + 1, // Convert to 1-indexed
                content,
                has_code_blocks,
                has_commands,
                code_blocks,
            });
        }

        sections
    }

    /// Detect if content contains code blocks (triple backticks).
    fn detect_code_blocks(lines: &[&str]) -> bool {
        lines.iter().any(|line| line.trim().starts_with("```"))
    }

    /// Detect if content contains executable commands.
    ///
    /// Looks for:
    /// - Lines starting with `$` (shell prompt)
    /// - Lines starting with common commands (make, npm, cargo, etc.)
    fn detect_commands(lines: &[&str]) -> bool {
        const COMMAND_PREFIXES: &[&str] = &[
            "$ ", "make ", "npm ", "cargo ", "paver ", "git ", "docker ", "kubectl ",
        ];

        for line in lines {
            let trimmed = line.trim();
            for prefix in COMMAND_PREFIXES {
                if trimmed.starts_with(prefix) {
                    return true;
                }
            }
        }
        false
    }

    /// Extract code blocks from section content.
    ///
    /// Parses fenced code blocks (``` markers) and extracts:
    /// - Language tag (if present after opening ```)
    /// - Content between the fences
    /// - Line number of the opening fence
    /// - Whether the block is executable (shell language, prompts, or paver:run marker)
    ///
    /// The `base_line` parameter is the 1-indexed line number of the first line in `lines`.
    fn extract_code_blocks(lines: &[&str], base_line: usize) -> Vec<CodeBlock> {
        let mut code_blocks = Vec::new();
        let mut in_code_block = false;
        let mut current_block_start: usize = 0;
        let mut current_language: Option<String> = None;
        let mut current_content: Vec<&str> = Vec::new();
        let mut opening_fence_len: usize = 0;
        let mut has_run_marker = false;

        for (idx, line) in lines.iter().enumerate() {
            let trimmed = line.trim();

            if !in_code_block {
                // Check for paver:run marker before the code block
                if Self::has_paver_run_marker(trimmed) {
                    has_run_marker = true;
                }
                // Check for opening fence (at least 3 backticks)
                else if let Some(fence_content) = Self::parse_opening_fence(trimmed) {
                    in_code_block = true;
                    opening_fence_len = fence_content.0;
                    current_block_start = base_line + idx;
                    current_language = fence_content.1;
                    current_content.clear();
                }
            } else {
                // Check for closing fence (at least as many backticks as opening, nothing after)
                if Self::is_closing_fence(trimmed, opening_fence_len) {
                    let content = current_content.join("\n");
                    let is_executable =
                        Self::is_block_executable(&current_language, &content, has_run_marker);
                    code_blocks.push(CodeBlock {
                        language: current_language.take(),
                        content,
                        start_line: current_block_start,
                        is_executable,
                    });
                    in_code_block = false;
                    current_content.clear();
                    has_run_marker = false;
                } else {
                    current_content.push(line);
                }
            }
        }

        // Handle unclosed code block at end of section (treat as if closed)
        if in_code_block && !current_content.is_empty() {
            let content = current_content.join("\n");
            let is_executable =
                Self::is_block_executable(&current_language, &content, has_run_marker);
            code_blocks.push(CodeBlock {
                language: current_language,
                content,
                start_line: current_block_start,
                is_executable,
            });
        }

        code_blocks
    }

    /// Parse an opening fence line, returning (fence_length, optional_language).
    /// Returns None if not an opening fence.
    fn parse_opening_fence(trimmed: &str) -> Option<(usize, Option<String>)> {
        if !trimmed.starts_with("```") {
            return None;
        }

        // Count backticks
        let fence_len = trimmed.chars().take_while(|&c| c == '`').count();
        if fence_len < 3 {
            return None;
        }

        // Extract language tag (first word after the backticks, if any)
        let language = trimmed[fence_len..]
            .split_whitespace()
            .next()
            .map(|s| s.to_string());

        Some((fence_len, language))
    }

    /// Check if a line is a closing fence (at least `min_len` backticks, nothing else).
    fn is_closing_fence(trimmed: &str, min_len: usize) -> bool {
        if !trimmed.starts_with("```") {
            return false;
        }

        let fence_len = trimmed.chars().take_while(|&c| c == '`').count();
        // Closing fence must have at least as many backticks and nothing after
        fence_len >= min_len && trimmed.len() == fence_len
    }

    /// Determine if a code block is executable based on language, content, and markers.
    ///
    /// A code block is considered executable if:
    /// 1. Language tag is a shell language: `bash`, `sh`, `shell`, `zsh`
    /// 2. Content contains lines starting with `$ ` or `> ` (shell prompts)
    /// 3. The block is preceded by a `<!-- paver:run -->` HTML comment marker
    fn is_block_executable(language: &Option<String>, content: &str, has_run_marker: bool) -> bool {
        // Check explicit marker first
        if has_run_marker {
            return true;
        }

        // Check shell language tags
        if let Some(lang) = language {
            let lang_lower = lang.to_lowercase();
            if matches!(lang_lower.as_str(), "bash" | "sh" | "shell" | "zsh") {
                return true;
            }
        }

        // Check for shell prompt prefixes in content
        content.lines().any(|line| {
            let trimmed = line.trim_start();
            trimmed.starts_with("$ ") || trimmed.starts_with("> ")
        })
    }

    /// Check if a line contains the paver:run marker.
    fn has_paver_run_marker(line: &str) -> bool {
        let trimmed = line.trim();
        trimmed.contains("<!-- paver:run -->") || trimmed.contains("<!--paver:run-->")
    }

    /// Extract paver frontmatter from document content.
    ///
    /// Looks for YAML frontmatter delimited by `---` at the start of the document.
    /// Returns the paver-specific configuration if present.
    fn extract_frontmatter(content: &str) -> Option<PaverFrontmatter> {
        let trimmed = content.trim_start();
        let after_first = trimmed.strip_prefix("---")?;

        // Find the closing ---
        let close_pos = after_first.find("\n---")?;
        let yaml_content = &after_first[..close_pos];

        // Parse the YAML and extract paver section
        let wrapper: FrontmatterWrapper = serde_yaml::from_str(yaml_content).ok()?;
        wrapper.paver
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_complete_paved_document() {
        let content = r#"# My Component

## Purpose
This is a test component.

## Interface
Use the API endpoints.

## Verification
Run the tests:
```bash
$ cargo test
```

## Examples
Basic usage example.
"#;

        let doc = ParsedDoc::parse_content(PathBuf::from("test.md"), content).unwrap();

        assert_eq!(doc.title, Some("My Component".to_string()));
        assert_eq!(doc.sections.len(), 4);
        assert!(doc.has_section("Purpose"));
        assert!(doc.has_section("Interface"));
        assert!(doc.has_section("Verification"));
        assert!(doc.has_section("Examples"));
    }

    #[test]
    fn parse_document_with_missing_sections() {
        let content = r#"# Incomplete Doc

## Purpose
Only has purpose section.
"#;

        let doc = ParsedDoc::parse_content(PathBuf::from("test.md"), content).unwrap();

        assert!(doc.has_section("Purpose"));
        assert!(!doc.has_section("Interface"));
        assert!(!doc.has_section("Verification"));
        assert!(!doc.has_section("Examples"));
    }

    #[test]
    fn detect_code_blocks_in_verification() {
        let content = r#"# Test

## Verification
Run these tests:
```bash
cargo test
```
"#;

        let doc = ParsedDoc::parse_content(PathBuf::from("test.md"), content).unwrap();
        let section = doc.get_section("Verification").unwrap();

        assert!(section.has_code_blocks);
    }

    #[test]
    fn handle_empty_document() {
        let content = "";
        let doc = ParsedDoc::parse_content(PathBuf::from("empty.md"), content).unwrap();

        assert_eq!(doc.title, None);
        assert!(doc.sections.is_empty());
        assert_eq!(doc.line_count, 0);
    }

    #[test]
    fn case_insensitive_section_lookup() {
        let content = r#"# Test

## Purpose
Test content.
"#;

        let doc = ParsedDoc::parse_content(PathBuf::from("test.md"), content).unwrap();

        assert!(doc.has_section("purpose"));
        assert!(doc.has_section("PURPOSE"));
        assert!(doc.has_section("Purpose"));
    }

    #[test]
    fn detect_commands_with_shell_prompt() {
        let content = r#"# Test

## Steps
Run this command:
$ make build
"#;

        let doc = ParsedDoc::parse_content(PathBuf::from("test.md"), content).unwrap();
        let section = doc.get_section("Steps").unwrap();

        assert!(section.has_commands);
    }

    #[test]
    fn detect_commands_with_common_prefixes() {
        let content = r#"# Test

## Build
cargo build --release
"#;

        let doc = ParsedDoc::parse_content(PathBuf::from("test.md"), content).unwrap();
        let section = doc.get_section("Build").unwrap();

        assert!(section.has_commands);
    }

    #[test]
    fn section_start_line_is_correct() {
        let content = r#"# Title

## First
Content line 1.

## Second
Content line 2.
"#;

        let doc = ParsedDoc::parse_content(PathBuf::from("test.md"), content).unwrap();

        // Line 1: # Title
        // Line 2: (blank)
        // Line 3: ## First  (start_line = 3)
        // Line 4: Content line 1.
        // Line 5: (blank)
        // Line 6: ## Second (start_line = 6)
        let first = doc.get_section("First").unwrap();
        let second = doc.get_section("Second").unwrap();

        assert_eq!(first.start_line, 3);
        assert_eq!(second.start_line, 6);
    }

    #[test]
    fn handle_non_standard_sections() {
        let content = r#"# Test

## Purpose
Standard section.

## Custom Section
Non-standard section.

## Another Custom
More custom content.
"#;

        let doc = ParsedDoc::parse_content(PathBuf::from("test.md"), content).unwrap();

        assert_eq!(doc.sections.len(), 3);
        assert!(doc.has_section("Purpose"));
        assert!(doc.has_section("Custom Section"));
        assert!(doc.has_section("Another Custom"));
    }

    #[test]
    fn section_content_excludes_heading() {
        let content = r#"# Test

## Purpose
This is the content.
Second line.
"#;

        let doc = ParsedDoc::parse_content(PathBuf::from("test.md"), content).unwrap();
        let section = doc.get_section("Purpose").unwrap();

        assert!(!section.content.contains("## Purpose"));
        assert!(section.content.contains("This is the content."));
        assert!(section.content.contains("Second line."));
    }

    #[test]
    fn extract_single_code_block_with_language() {
        let content = r#"# Test

## Verification
Run the test:
```bash
cargo test
```
"#;

        let doc = ParsedDoc::parse_content(PathBuf::from("test.md"), content).unwrap();
        let section = doc.get_section("Verification").unwrap();

        assert_eq!(section.code_blocks.len(), 1);
        let block = &section.code_blocks[0];
        assert_eq!(block.language, Some("bash".to_string()));
        assert_eq!(block.content, "cargo test");
        // Line 1: # Test, Line 2: blank, Line 3: ## Verification, Line 4: Run the test:, Line 5: ```bash
        assert_eq!(block.start_line, 5);
    }

    #[test]
    fn extract_multiple_code_blocks() {
        let content = r#"# Test

## Steps
First command:
```bash
make build
```
Second command:
```rust
fn main() {}
```
"#;

        let doc = ParsedDoc::parse_content(PathBuf::from("test.md"), content).unwrap();
        let section = doc.get_section("Steps").unwrap();

        assert_eq!(section.code_blocks.len(), 2);

        let first = &section.code_blocks[0];
        assert_eq!(first.language, Some("bash".to_string()));
        assert_eq!(first.content, "make build");

        let second = &section.code_blocks[1];
        assert_eq!(second.language, Some("rust".to_string()));
        assert_eq!(second.content, "fn main() {}");
    }

    #[test]
    fn extract_code_block_without_language() {
        let content = r#"# Test

## Example
```
plain text here
```
"#;

        let doc = ParsedDoc::parse_content(PathBuf::from("test.md"), content).unwrap();
        let section = doc.get_section("Example").unwrap();

        assert_eq!(section.code_blocks.len(), 1);
        let block = &section.code_blocks[0];
        assert_eq!(block.language, None);
        assert_eq!(block.content, "plain text here");
    }

    #[test]
    fn extract_multiline_code_block() {
        let content = r#"# Test

## Script
```bash
#!/bin/bash
echo "Line 1"
echo "Line 2"
exit 0
```
"#;

        let doc = ParsedDoc::parse_content(PathBuf::from("test.md"), content).unwrap();
        let section = doc.get_section("Script").unwrap();

        assert_eq!(section.code_blocks.len(), 1);
        let block = &section.code_blocks[0];
        assert_eq!(block.language, Some("bash".to_string()));
        assert!(block.content.contains("#!/bin/bash"));
        assert!(block.content.contains("echo \"Line 1\""));
        assert!(block.content.contains("echo \"Line 2\""));
        assert!(block.content.contains("exit 0"));
        // Verify it's actually multiline
        assert_eq!(block.content.lines().count(), 4);
    }

    #[test]
    fn nested_backticks_handled_correctly() {
        // Use 4 backticks to wrap code that contains 3 backticks
        let content = r#"# Test

## Docs
````markdown
Here is some code:
```rust
fn test() {}
```
````
"#;

        let doc = ParsedDoc::parse_content(PathBuf::from("test.md"), content).unwrap();
        let section = doc.get_section("Docs").unwrap();

        assert_eq!(section.code_blocks.len(), 1);
        let block = &section.code_blocks[0];
        assert_eq!(block.language, Some("markdown".to_string()));
        // The inner backticks should be preserved as content
        assert!(block.content.contains("```rust"));
        assert!(block.content.contains("fn test() {}"));
        assert!(block.content.contains("```"));
    }

    #[test]
    fn has_code_blocks_remains_accurate() {
        let content = r#"# Test

## With Code
```bash
test
```

## Without Code
Just plain text.
"#;

        let doc = ParsedDoc::parse_content(PathBuf::from("test.md"), content).unwrap();

        let with_code = doc.get_section("With Code").unwrap();
        assert!(with_code.has_code_blocks);
        assert_eq!(with_code.code_blocks.len(), 1);

        let without_code = doc.get_section("Without Code").unwrap();
        assert!(!without_code.has_code_blocks);
        assert!(without_code.code_blocks.is_empty());
    }

    #[test]
    fn code_block_line_numbers_are_correct() {
        let content = r#"# Title

## Section
Line of text.
Another line.
```bash
command here
```
"#;

        let doc = ParsedDoc::parse_content(PathBuf::from("test.md"), content).unwrap();
        let section = doc.get_section("Section").unwrap();

        // Line 1: # Title
        // Line 2: blank
        // Line 3: ## Section
        // Line 4: Line of text.
        // Line 5: Another line.
        // Line 6: ```bash
        assert_eq!(section.code_blocks.len(), 1);
        assert_eq!(section.code_blocks[0].start_line, 6);
    }

    #[test]
    fn bash_language_tag_is_executable() {
        let content = r#"# Test

## Verification
```bash
cargo test
```
"#;

        let doc = ParsedDoc::parse_content(PathBuf::from("test.md"), content).unwrap();
        let section = doc.get_section("Verification").unwrap();

        assert_eq!(section.code_blocks.len(), 1);
        assert!(section.code_blocks[0].is_executable);
    }

    #[test]
    fn shell_language_tags_are_executable() {
        let content = r#"# Test

## Commands
```sh
echo "sh"
```
```shell
echo "shell"
```
```zsh
echo "zsh"
```
"#;

        let doc = ParsedDoc::parse_content(PathBuf::from("test.md"), content).unwrap();
        let section = doc.get_section("Commands").unwrap();

        assert_eq!(section.code_blocks.len(), 3);
        for block in &section.code_blocks {
            assert!(
                block.is_executable,
                "Block with language {:?} should be executable",
                block.language
            );
        }
    }

    #[test]
    fn json_language_tag_is_not_executable() {
        let content = r#"# Test

## Config
```json
{"key": "value"}
```
"#;

        let doc = ParsedDoc::parse_content(PathBuf::from("test.md"), content).unwrap();
        let section = doc.get_section("Config").unwrap();

        assert_eq!(section.code_blocks.len(), 1);
        assert!(!section.code_blocks[0].is_executable);
    }

    #[test]
    fn dollar_prefix_makes_block_executable() {
        let content = r#"# Test

## Steps
```
$ make build
$ make test
```
"#;

        let doc = ParsedDoc::parse_content(PathBuf::from("test.md"), content).unwrap();
        let section = doc.get_section("Steps").unwrap();

        assert_eq!(section.code_blocks.len(), 1);
        assert!(section.code_blocks[0].is_executable);
    }

    #[test]
    fn angle_bracket_prefix_makes_block_executable() {
        let content = r#"# Test

## REPL
```
> console.log("hello")
```
"#;

        let doc = ParsedDoc::parse_content(PathBuf::from("test.md"), content).unwrap();
        let section = doc.get_section("REPL").unwrap();

        assert_eq!(section.code_blocks.len(), 1);
        assert!(section.code_blocks[0].is_executable);
    }

    #[test]
    fn paver_run_marker_makes_block_executable() {
        let content = r#"# Test

## Example
<!-- paver:run -->
```python
print("hello")
```
"#;

        let doc = ParsedDoc::parse_content(PathBuf::from("test.md"), content).unwrap();
        let section = doc.get_section("Example").unwrap();

        assert_eq!(section.code_blocks.len(), 1);
        assert!(section.code_blocks[0].is_executable);
        // Language should still be python
        assert_eq!(section.code_blocks[0].language, Some("python".to_string()));
    }

    #[test]
    fn paver_run_marker_without_spaces_works() {
        let content = r#"# Test

## Example
<!--paver:run-->
```ruby
puts "hello"
```
"#;

        let doc = ParsedDoc::parse_content(PathBuf::from("test.md"), content).unwrap();
        let section = doc.get_section("Example").unwrap();

        assert_eq!(section.code_blocks.len(), 1);
        assert!(section.code_blocks[0].is_executable);
    }

    #[test]
    fn mixed_executable_and_non_executable_blocks() {
        let content = r#"# Test

## Verification
Run the test:
```bash
cargo test
```

Expected output:
```json
{"status": "ok"}
```

Configuration:
```yaml
key: value
```
"#;

        let doc = ParsedDoc::parse_content(PathBuf::from("test.md"), content).unwrap();
        let section = doc.get_section("Verification").unwrap();

        assert_eq!(section.code_blocks.len(), 3);

        // bash block is executable
        assert!(section.code_blocks[0].is_executable);
        assert_eq!(section.code_blocks[0].language, Some("bash".to_string()));

        // json block is not executable
        assert!(!section.code_blocks[1].is_executable);
        assert_eq!(section.code_blocks[1].language, Some("json".to_string()));

        // yaml block is not executable
        assert!(!section.code_blocks[2].is_executable);
        assert_eq!(section.code_blocks[2].language, Some("yaml".to_string()));
    }

    #[test]
    fn executable_commands_method_returns_only_executable_blocks() {
        let content = r#"# Test

## Steps
```bash
make build
```
```json
{"result": "ok"}
```
```sh
make test
```
"#;

        let doc = ParsedDoc::parse_content(PathBuf::from("test.md"), content).unwrap();
        let section = doc.get_section("Steps").unwrap();

        let executable = section.executable_commands();
        assert_eq!(executable.len(), 2);
        assert_eq!(executable[0].language, Some("bash".to_string()));
        assert_eq!(executable[1].language, Some("sh".to_string()));
    }

    #[test]
    fn executable_commands_empty_when_no_executable_blocks() {
        let content = r#"# Test

## Config
```json
{"key": "value"}
```
```yaml
key: value
```
"#;

        let doc = ParsedDoc::parse_content(PathBuf::from("test.md"), content).unwrap();
        let section = doc.get_section("Config").unwrap();

        let executable = section.executable_commands();
        assert!(executable.is_empty());
    }

    #[test]
    fn non_shell_with_dollar_prefix_is_executable() {
        // A code block without language but with $ prefix should be executable
        let content = r#"# Test

## Commands
```
$ npm install
$ npm test
```
"#;

        let doc = ParsedDoc::parse_content(PathBuf::from("test.md"), content).unwrap();
        let section = doc.get_section("Commands").unwrap();

        assert_eq!(section.code_blocks.len(), 1);
        assert!(section.code_blocks[0].is_executable);
        assert_eq!(section.code_blocks[0].language, None);
    }

    #[test]
    fn paver_run_marker_only_applies_to_next_block() {
        let content = r#"# Test

## Steps
<!-- paver:run -->
```python
print("executable")
```
```python
print("not executable")
```
"#;

        let doc = ParsedDoc::parse_content(PathBuf::from("test.md"), content).unwrap();
        let section = doc.get_section("Steps").unwrap();

        assert_eq!(section.code_blocks.len(), 2);
        assert!(section.code_blocks[0].is_executable);
        assert!(!section.code_blocks[1].is_executable);
    }

    #[test]
    fn parse_document_with_paver_frontmatter() {
        let content = r#"---
paver:
  paths:
    - src/auth/
    - crates/auth/
---
# Auth Component

## Purpose
Authentication handling.
"#;

        let doc = ParsedDoc::parse_content(PathBuf::from("test.md"), content).unwrap();

        assert!(doc.frontmatter.is_some());
        let frontmatter = doc.frontmatter.unwrap();
        assert_eq!(frontmatter.paths.len(), 2);
        assert_eq!(frontmatter.paths[0], "src/auth/");
        assert_eq!(frontmatter.paths[1], "crates/auth/");
    }

    #[test]
    fn parse_document_without_frontmatter() {
        let content = r#"# Simple Doc

## Purpose
No frontmatter here.
"#;

        let doc = ParsedDoc::parse_content(PathBuf::from("test.md"), content).unwrap();
        assert!(doc.frontmatter.is_none());
    }

    #[test]
    fn parse_document_with_non_paver_frontmatter() {
        let content = r#"---
title: My Document
author: Someone
---
# My Document

## Purpose
Has frontmatter but not paver config.
"#;

        let doc = ParsedDoc::parse_content(PathBuf::from("test.md"), content).unwrap();
        assert!(doc.frontmatter.is_none());
    }

    #[test]
    fn parse_document_with_empty_paver_paths() {
        let content = r#"---
paver:
  paths: []
---
# Empty Paths

## Purpose
Has paver section but empty paths.
"#;

        let doc = ParsedDoc::parse_content(PathBuf::from("test.md"), content).unwrap();

        assert!(doc.frontmatter.is_some());
        let frontmatter = doc.frontmatter.unwrap();
        assert!(frontmatter.paths.is_empty());
    }
}
