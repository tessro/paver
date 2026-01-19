//! Markdown parser for PAVED document validation.
//!
//! This module parses markdown documents and extracts structured information
//! about their sections, code blocks, and commands for validation purposes.

use anyhow::{Context, Result};
use serde::Deserialize;
use std::path::{Path, PathBuf};

/// Pave-specific frontmatter configuration.
#[derive(Debug, Clone, Deserialize, PartialEq, Default)]
pub struct PaveFrontmatter {
    /// Code paths that this document covers.
    #[serde(default)]
    pub paths: Vec<String>,
    /// Working directory for verification commands in this document.
    #[serde(default)]
    pub working_dir: Option<String>,
}

/// YAML frontmatter wrapper.
#[derive(Debug, Clone, Deserialize, PartialEq, Default)]
struct FrontmatterWrapper {
    /// Pave-specific configuration.
    #[serde(default)]
    pave: Option<PaveFrontmatter>,
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
    /// Pave-specific frontmatter configuration.
    pub frontmatter: Option<PaveFrontmatter>,
}

/// Strategy for matching expected output.
#[derive(Debug, Clone, PartialEq)]
pub enum ExpectMatchStrategy {
    /// Match if output contains the expected string (default).
    Contains,
    /// Match if output matches the regex pattern.
    Regex,
    /// Match if output exactly equals expected (trimmed).
    Exact,
}

/// Expected output specification for a code block.
#[derive(Debug, Clone, PartialEq)]
pub struct ExpectedOutput {
    /// The expected output content.
    pub content: String,
    /// The matching strategy to use.
    pub strategy: ExpectMatchStrategy,
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
    /// Expected output for this code block, if specified.
    pub expected_output: Option<ExpectedOutput>,
    /// Working directory override for this code block.
    pub working_dir: Option<String>,
    /// Environment variables to set for this code block.
    pub env_vars: Vec<(String, String)>,
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
    /// content with shell prompts ($ or >), or preceded by a `<!-- pave:run -->` marker.
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
        let mut in_code_block = false;

        // Find all H2 headings and their positions, skipping content inside code blocks
        for (idx, line) in lines.iter().enumerate() {
            let trimmed = line.trim();

            // Track code block state
            if trimmed.starts_with("```") {
                in_code_block = !in_code_block;
                continue;
            }

            // Skip headings inside code blocks
            if in_code_block {
                continue;
            }

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
            "$ ", "make ", "npm ", "cargo ", "pave ", "git ", "docker ", "kubectl ",
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
    /// - Whether the block is executable (shell language, prompts, or pave:run marker)
    /// - Expected output (inline or from explicit blocks)
    ///
    /// The `base_line` parameter is the 1-indexed line number of the first line in `lines`.
    fn extract_code_blocks(lines: &[&str], base_line: usize) -> Vec<CodeBlock> {
        let mut code_blocks: Vec<CodeBlock> = Vec::new();
        let mut in_code_block = false;
        let mut current_block_start: usize = 0;
        let mut current_language: Option<String> = None;
        let mut current_content: Vec<&str> = Vec::new();
        let mut opening_fence_len: usize = 0;
        let mut has_run_marker = false;
        let mut pending_expect_marker: Option<ExpectMatchStrategy> = None;
        let mut pending_working_dir: Option<String> = None;
        let mut pending_env_vars: Vec<(String, String)> = Vec::new();

        for (idx, line) in lines.iter().enumerate() {
            let trimmed = line.trim();

            if !in_code_block {
                // Check for pave:run marker before the code block
                if Self::has_pave_run_marker(trimmed) {
                    has_run_marker = true;
                }
                // Check for pave:expect marker before a code block
                else if let Some(strategy) = Self::parse_expect_marker(trimmed) {
                    pending_expect_marker = Some(strategy);
                }
                // Check for pave:working_dir marker
                else if let Some(dir) = Self::parse_working_dir_marker(trimmed) {
                    pending_working_dir = Some(dir);
                }
                // Check for pave:env marker
                else if let Some(env_var) = Self::parse_env_marker(trimmed) {
                    pending_env_vars.push(env_var);
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

                    // If there's a pending expect marker, this block is expected output
                    if let Some(strategy) = pending_expect_marker.take() {
                        // Attach expected output to the last executable block
                        if let Some(last_block) = code_blocks.last_mut()
                            && last_block.is_executable
                            && last_block.expected_output.is_none()
                        {
                            last_block.expected_output = Some(ExpectedOutput {
                                content: content.clone(),
                                strategy,
                            });
                        }
                        // This block is not added as a code block itself
                        // Also clear working_dir/env since they were for an expect block
                        pending_working_dir = None;
                        pending_env_vars.clear();
                    } else {
                        let is_executable =
                            Self::is_block_executable(&current_language, &content, has_run_marker);

                        // Extract inline expected output from shell-style blocks
                        let (command_content, inline_output) =
                            Self::extract_inline_expected_output(&content);

                        code_blocks.push(CodeBlock {
                            language: current_language.take(),
                            content: command_content,
                            start_line: current_block_start,
                            is_executable,
                            expected_output: inline_output,
                            working_dir: pending_working_dir.take(),
                            env_vars: std::mem::take(&mut pending_env_vars),
                        });
                    }
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
            let (command_content, inline_output) = Self::extract_inline_expected_output(&content);
            code_blocks.push(CodeBlock {
                language: current_language,
                content: command_content,
                start_line: current_block_start,
                is_executable,
                expected_output: inline_output,
                working_dir: pending_working_dir,
                env_vars: pending_env_vars,
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
    /// 3. The block is preceded by a `<!-- pave:run -->` HTML comment marker
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

    /// Check if a line contains the pave:run marker.
    fn has_pave_run_marker(line: &str) -> bool {
        let trimmed = line.trim();
        trimmed.contains("<!-- pave:run -->") || trimmed.contains("<!--pave:run-->")
    }

    /// Parse a pave:expect marker and return the matching strategy.
    ///
    /// Supports:
    /// - `<!-- pave:expect -->` or `<!-- pave:expect:contains -->` - contains matching (default)
    /// - `<!-- pave:expect:regex -->` - regex matching
    /// - `<!-- pave:expect:exact -->` - exact matching
    fn parse_expect_marker(line: &str) -> Option<ExpectMatchStrategy> {
        let trimmed = line.trim();

        // Check for markers with and without spaces
        let patterns = [
            (
                "<!-- pave:expect:contains -->",
                ExpectMatchStrategy::Contains,
            ),
            (
                "<!--pave:expect:contains-->",
                ExpectMatchStrategy::Contains,
            ),
            ("<!-- pave:expect:regex -->", ExpectMatchStrategy::Regex),
            ("<!--pave:expect:regex-->", ExpectMatchStrategy::Regex),
            ("<!-- pave:expect:exact -->", ExpectMatchStrategy::Exact),
            ("<!--pave:expect:exact-->", ExpectMatchStrategy::Exact),
            ("<!-- pave:expect -->", ExpectMatchStrategy::Contains),
            ("<!--pave:expect-->", ExpectMatchStrategy::Contains),
        ];

        for (pattern, strategy) in patterns {
            if trimmed.contains(pattern) {
                return Some(strategy);
            }
        }

        None
    }

    /// Parse a pave:working_dir marker and return the directory path.
    ///
    /// Supports:
    /// - `<!-- pave:working_dir path/to/dir -->`
    /// - `<!--pave:working_dir path/to/dir-->`
    fn parse_working_dir_marker(line: &str) -> Option<String> {
        let trimmed = line.trim();

        // Try with spaces first
        if let Some(rest) = trimmed.strip_prefix("<!-- pave:working_dir ")
            && let Some(dir) = rest.strip_suffix(" -->")
        {
            let dir = dir.trim();
            if !dir.is_empty() {
                return Some(dir.to_string());
            }
        }

        // Try without spaces
        if let Some(rest) = trimmed.strip_prefix("<!--pave:working_dir ")
            && let Some(dir) = rest.strip_suffix("-->")
        {
            let dir = dir.trim();
            if !dir.is_empty() {
                return Some(dir.to_string());
            }
        }

        None
    }

    /// Parse a pave:env marker and return the environment variable (key, value).
    ///
    /// Supports:
    /// - `<!-- pave:env KEY=VALUE -->`
    /// - `<!--pave:env KEY=VALUE-->`
    fn parse_env_marker(line: &str) -> Option<(String, String)> {
        let trimmed = line.trim();

        let env_str = if let Some(rest) = trimmed.strip_prefix("<!-- pave:env ") {
            rest.strip_suffix(" -->")
        } else if let Some(rest) = trimmed.strip_prefix("<!--pave:env ") {
            rest.strip_suffix("-->")
        } else {
            None
        };

        if let Some(env_str) = env_str {
            let env_str = env_str.trim();
            if let Some(eq_pos) = env_str.find('=') {
                let key = env_str[..eq_pos].trim().to_string();
                let value = env_str[eq_pos + 1..].trim().to_string();
                if !key.is_empty() {
                    return Some((key, value));
                }
            }
        }

        None
    }

    /// Extract inline expected output from a code block with shell prompts.
    ///
    /// In a code block like:
    /// ```bash
    /// $ pave check
    /// Checked 5 documents: all checks passed
    /// ```
    ///
    /// The line after `$ pave check` (that doesn't start with `$`) is treated
    /// as expected output using the `contains` strategy.
    ///
    /// This only applies to blocks that contain shell prompt lines (`$ ` or `> `).
    /// Other blocks are returned unchanged.
    ///
    /// Returns (command_content, optional_expected_output).
    fn extract_inline_expected_output(content: &str) -> (String, Option<ExpectedOutput>) {
        // First, check if content has shell prompt lines
        let has_shell_prompts = content.lines().any(|line| {
            let trimmed = line.trim();
            trimmed.starts_with("$ ") || trimmed.starts_with("> ")
        });

        // If no shell prompts, return content unchanged
        if !has_shell_prompts {
            return (content.to_string(), None);
        }

        let lines: Vec<&str> = content.lines().collect();

        let mut command_lines = Vec::new();
        let mut output_lines = Vec::new();
        let mut seen_command = false;

        for line in lines {
            let trimmed = line.trim();

            // Check if this is a shell prompt line
            if trimmed.starts_with("$ ") || trimmed.starts_with("> ") {
                command_lines.push(line);
                seen_command = true;
            } else if seen_command {
                // Any non-command line after seeing a command is output
                // Skip empty lines and comment lines at the start of output
                if output_lines.is_empty() && (trimmed.is_empty() || trimmed.starts_with('#')) {
                    continue;
                }
                output_lines.push(line);
            } else {
                // Line before any command - treat as part of content
                command_lines.push(line);
            }
        }

        let command_content = command_lines.join("\n");

        // Only create expected output if we have non-empty output lines
        let output_content: String = output_lines.to_vec().join("\n");

        let expected_output = if !output_content.trim().is_empty() {
            Some(ExpectedOutput {
                content: output_content,
                strategy: ExpectMatchStrategy::Contains,
            })
        } else {
            None
        };

        (command_content, expected_output)
    }

    /// Extract pave frontmatter from document content.
    ///
    /// Looks for YAML frontmatter delimited by `---` at the start of the document.
    /// Returns the pave-specific configuration if present.
    fn extract_frontmatter(content: &str) -> Option<PaveFrontmatter> {
        let trimmed = content.trim_start();
        let after_first = trimmed.strip_prefix("---")?;

        // Find the closing ---
        let close_pos = after_first.find("\n---")?;
        let yaml_content = &after_first[..close_pos];

        // Parse the YAML and extract pave section
        let wrapper: FrontmatterWrapper = serde_yaml::from_str(yaml_content).ok()?;
        wrapper.pave
    }
}

/// Tracks whether we're inside a code block while iterating through lines.
///
/// This properly handles:
/// - Language tags after opening fences (e.g., `` ```bash ``)
/// - Nested code blocks using longer fences (e.g., ```` ```` ```` to wrap `` ``` ``)
/// - Closing fences that must have at least as many backticks as the opening
#[derive(Debug, Default, Clone)]
pub struct CodeBlockTracker {
    /// Current fence length if inside a code block, None if outside.
    fence_len: Option<usize>,
}

impl CodeBlockTracker {
    /// Create a new tracker starting outside any code block.
    pub fn new() -> Self {
        Self { fence_len: None }
    }

    /// Check if currently inside a code block.
    pub fn in_code_block(&self) -> bool {
        self.fence_len.is_some()
    }

    /// Process a line and update the code block state.
    /// Returns true if this line is a fence marker (opening or closing).
    pub fn process_line(&mut self, line: &str) -> bool {
        let trimmed = line.trim_start();

        if !trimmed.starts_with("```") {
            return false;
        }

        // Count backticks at the start
        let backtick_count = trimmed.chars().take_while(|&c| c == '`').count();
        if backtick_count < 3 {
            return false;
        }

        if let Some(opening_len) = self.fence_len {
            // We're inside a code block - check if this is a valid closing fence
            // Closing fence must have at least as many backticks and nothing after
            let after_backticks = &trimmed[backtick_count..];
            if backtick_count >= opening_len && after_backticks.trim().is_empty() {
                self.fence_len = None;
                return true;
            }
            // Not a valid closing fence - treat as content
            false
        } else {
            // We're outside - this is an opening fence
            self.fence_len = Some(backtick_count);
            true
        }
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
    fn skip_headings_inside_code_blocks() {
        let content = r#"# Test

## Purpose
Real purpose section.

## Interface
Here's an example markdown file:

```markdown
## Verification
Fake verification inside code block.
```

## Verification
Real verification section.
```bash
cargo test
```
"#;

        let doc = ParsedDoc::parse_content(PathBuf::from("test.md"), content).unwrap();

        // Should have 3 sections: Purpose, Interface, Verification
        // The "## Verification" inside the markdown code block should be ignored
        assert_eq!(doc.sections.len(), 3);
        assert!(doc.has_section("Purpose"));
        assert!(doc.has_section("Interface"));
        assert!(doc.has_section("Verification"));

        // The Verification section should be the real one (line 17), not the fake one inside code block
        let verification = doc.get_section("Verification").unwrap();
        assert!(verification.content.contains("Real verification section"));
        assert!(!verification.content.contains("Fake verification"));
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
    fn pave_run_marker_makes_block_executable() {
        let content = r#"# Test

## Example
<!-- pave:run -->
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
    fn pave_run_marker_without_spaces_works() {
        let content = r#"# Test

## Example
<!--pave:run-->
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
    fn pave_run_marker_only_applies_to_next_block() {
        let content = r#"# Test

## Steps
<!-- pave:run -->
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
    fn parse_document_with_pave_frontmatter() {
        let content = r#"---
pave:
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
    fn parse_document_with_non_pave_frontmatter() {
        let content = r#"---
title: My Document
author: Someone
---
# My Document

## Purpose
Has frontmatter but not pave config.
"#;

        let doc = ParsedDoc::parse_content(PathBuf::from("test.md"), content).unwrap();
        assert!(doc.frontmatter.is_none());
    }

    #[test]
    fn parse_document_with_empty_pave_paths() {
        let content = r#"---
pave:
  paths: []
---
# Empty Paths

## Purpose
Has pave section but empty paths.
"#;

        let doc = ParsedDoc::parse_content(PathBuf::from("test.md"), content).unwrap();

        assert!(doc.frontmatter.is_some());
        let frontmatter = doc.frontmatter.unwrap();
        assert!(frontmatter.paths.is_empty());
    }

    #[test]
    fn inline_expected_output_parsing() {
        let content = r#"# Test

## Verification
```bash
$ pave check
Checked 5 documents: all checks passed
```
"#;

        let doc = ParsedDoc::parse_content(PathBuf::from("test.md"), content).unwrap();
        let section = doc.get_section("Verification").unwrap();

        assert_eq!(section.code_blocks.len(), 1);
        let block = &section.code_blocks[0];
        assert!(block.is_executable);
        // Command should be extracted without the output
        assert!(block.content.contains("$ pave check"));
        assert!(!block.content.contains("Checked 5 documents"));
        // Expected output should be captured
        assert!(block.expected_output.is_some());
        let expected = block.expected_output.as_ref().unwrap();
        assert!(expected.content.contains("Checked 5 documents"));
        assert_eq!(expected.strategy, ExpectMatchStrategy::Contains);
    }

    #[test]
    fn explicit_expect_contains_marker() {
        let content = r#"# Test

## Verification
```bash
cargo test
```
<!-- pave:expect:contains -->
```
test result: ok
```
"#;

        let doc = ParsedDoc::parse_content(PathBuf::from("test.md"), content).unwrap();
        let section = doc.get_section("Verification").unwrap();

        // Only one code block should be returned (the expect block is consumed)
        assert_eq!(section.code_blocks.len(), 1);
        let block = &section.code_blocks[0];
        assert!(block.is_executable);
        assert!(block.expected_output.is_some());
        let expected = block.expected_output.as_ref().unwrap();
        assert!(expected.content.contains("test result: ok"));
        assert_eq!(expected.strategy, ExpectMatchStrategy::Contains);
    }

    #[test]
    fn explicit_expect_regex_marker() {
        let content = r#"# Test

## Verification
```bash
cargo test
```
<!-- pave:expect:regex -->
```
test result: ok\. \d+ passed
```
"#;

        let doc = ParsedDoc::parse_content(PathBuf::from("test.md"), content).unwrap();
        let section = doc.get_section("Verification").unwrap();

        assert_eq!(section.code_blocks.len(), 1);
        let block = &section.code_blocks[0];
        assert!(block.expected_output.is_some());
        let expected = block.expected_output.as_ref().unwrap();
        assert_eq!(expected.strategy, ExpectMatchStrategy::Regex);
    }

    #[test]
    fn explicit_expect_exact_marker() {
        let content = r#"# Test

## Verification
```bash
echo hello
```
<!-- pave:expect:exact -->
```
hello
```
"#;

        let doc = ParsedDoc::parse_content(PathBuf::from("test.md"), content).unwrap();
        let section = doc.get_section("Verification").unwrap();

        assert_eq!(section.code_blocks.len(), 1);
        let block = &section.code_blocks[0];
        assert!(block.expected_output.is_some());
        let expected = block.expected_output.as_ref().unwrap();
        assert_eq!(expected.strategy, ExpectMatchStrategy::Exact);
        assert_eq!(expected.content.trim(), "hello");
    }

    #[test]
    fn expect_marker_without_spaces() {
        let content = r#"# Test

## Verification
```bash
echo test
```
<!--pave:expect:contains-->
```
test
```
"#;

        let doc = ParsedDoc::parse_content(PathBuf::from("test.md"), content).unwrap();
        let section = doc.get_section("Verification").unwrap();

        assert_eq!(section.code_blocks.len(), 1);
        let block = &section.code_blocks[0];
        assert!(block.expected_output.is_some());
    }

    #[test]
    fn default_expect_marker_uses_contains() {
        let content = r#"# Test

## Verification
```bash
echo test
```
<!-- pave:expect -->
```
test
```
"#;

        let doc = ParsedDoc::parse_content(PathBuf::from("test.md"), content).unwrap();
        let section = doc.get_section("Verification").unwrap();

        assert_eq!(section.code_blocks.len(), 1);
        let block = &section.code_blocks[0];
        assert!(block.expected_output.is_some());
        let expected = block.expected_output.as_ref().unwrap();
        assert_eq!(expected.strategy, ExpectMatchStrategy::Contains);
    }

    #[test]
    fn no_expected_output_for_non_shell_blocks() {
        let content = r#"# Test

## Example
```rust
fn main() {}
```
"#;

        let doc = ParsedDoc::parse_content(PathBuf::from("test.md"), content).unwrap();
        let section = doc.get_section("Example").unwrap();

        assert_eq!(section.code_blocks.len(), 1);
        let block = &section.code_blocks[0];
        assert!(block.expected_output.is_none());
        assert!(block.content.contains("fn main()"));
    }

    #[test]
    fn multiple_commands_without_inline_output() {
        let content = r#"# Test

## Verification
```bash
$ echo hello
$ echo world
```
"#;

        let doc = ParsedDoc::parse_content(PathBuf::from("test.md"), content).unwrap();
        let section = doc.get_section("Verification").unwrap();

        assert_eq!(section.code_blocks.len(), 1);
        let block = &section.code_blocks[0];
        assert!(block.is_executable);
        // Both commands should be in content
        assert!(block.content.contains("$ echo hello"));
        assert!(block.content.contains("$ echo world"));
        // No expected output since there's nothing after the commands
        assert!(block.expected_output.is_none());
    }

    #[test]
    fn parse_document_with_pave_working_dir_in_frontmatter() {
        let content = r#"---
pave:
  working_dir: packages/api
---
# API Component

## Purpose
API service.
"#;

        let doc = ParsedDoc::parse_content(PathBuf::from("test.md"), content).unwrap();

        assert!(doc.frontmatter.is_some());
        let frontmatter = doc.frontmatter.unwrap();
        assert_eq!(frontmatter.working_dir, Some("packages/api".to_string()));
    }

    #[test]
    fn parse_pave_working_dir_inline_marker() {
        let content = r#"# Test

## Verification
<!-- pave:working_dir packages/api -->
```bash
npm test
```
"#;

        let doc = ParsedDoc::parse_content(PathBuf::from("test.md"), content).unwrap();
        let section = doc.get_section("Verification").unwrap();

        assert_eq!(section.code_blocks.len(), 1);
        let block = &section.code_blocks[0];
        assert!(block.is_executable);
        assert_eq!(block.working_dir, Some("packages/api".to_string()));
    }

    #[test]
    fn parse_pave_working_dir_inline_marker_without_spaces() {
        let content = r#"# Test

## Verification
<!--pave:working_dir src/components-->
```bash
npm test
```
"#;

        let doc = ParsedDoc::parse_content(PathBuf::from("test.md"), content).unwrap();
        let section = doc.get_section("Verification").unwrap();

        assert_eq!(section.code_blocks.len(), 1);
        let block = &section.code_blocks[0];
        assert_eq!(block.working_dir, Some("src/components".to_string()));
    }

    #[test]
    fn parse_pave_env_inline_marker() {
        let content = r#"# Test

## Verification
<!-- pave:env TEST_DB=sqlite:memory -->
```bash
cargo test
```
"#;

        let doc = ParsedDoc::parse_content(PathBuf::from("test.md"), content).unwrap();
        let section = doc.get_section("Verification").unwrap();

        assert_eq!(section.code_blocks.len(), 1);
        let block = &section.code_blocks[0];
        assert!(block.is_executable);
        assert_eq!(block.env_vars.len(), 1);
        assert_eq!(
            block.env_vars[0],
            ("TEST_DB".to_string(), "sqlite:memory".to_string())
        );
    }

    #[test]
    fn parse_multiple_pave_env_markers() {
        let content = r#"# Test

## Verification
<!-- pave:env DEBUG=1 -->
<!-- pave:env LOG_LEVEL=trace -->
```bash
cargo test
```
"#;

        let doc = ParsedDoc::parse_content(PathBuf::from("test.md"), content).unwrap();
        let section = doc.get_section("Verification").unwrap();

        assert_eq!(section.code_blocks.len(), 1);
        let block = &section.code_blocks[0];
        assert_eq!(block.env_vars.len(), 2);
        assert!(
            block
                .env_vars
                .contains(&("DEBUG".to_string(), "1".to_string()))
        );
        assert!(
            block
                .env_vars
                .contains(&("LOG_LEVEL".to_string(), "trace".to_string()))
        );
    }

    #[test]
    fn parse_pave_env_with_working_dir() {
        let content = r#"# Test

## Verification
<!-- pave:working_dir packages/api -->
<!-- pave:env NODE_ENV=test -->
```bash
npm test
```
"#;

        let doc = ParsedDoc::parse_content(PathBuf::from("test.md"), content).unwrap();
        let section = doc.get_section("Verification").unwrap();

        assert_eq!(section.code_blocks.len(), 1);
        let block = &section.code_blocks[0];
        assert_eq!(block.working_dir, Some("packages/api".to_string()));
        assert_eq!(block.env_vars.len(), 1);
        assert_eq!(
            block.env_vars[0],
            ("NODE_ENV".to_string(), "test".to_string())
        );
    }

    #[test]
    fn markers_only_apply_to_next_block() {
        let content = r#"# Test

## Verification
<!-- pave:working_dir packages/api -->
```bash
npm test
```
```bash
echo no working dir
```
"#;

        let doc = ParsedDoc::parse_content(PathBuf::from("test.md"), content).unwrap();
        let section = doc.get_section("Verification").unwrap();

        assert_eq!(section.code_blocks.len(), 2);
        // First block has working_dir
        assert_eq!(
            section.code_blocks[0].working_dir,
            Some("packages/api".to_string())
        );
        // Second block has no working_dir
        assert_eq!(section.code_blocks[1].working_dir, None);
    }

    #[test]
    fn code_block_default_env_vars_is_empty() {
        let content = r#"# Test

## Verification
```bash
echo test
```
"#;

        let doc = ParsedDoc::parse_content(PathBuf::from("test.md"), content).unwrap();
        let section = doc.get_section("Verification").unwrap();

        assert_eq!(section.code_blocks.len(), 1);
        let block = &section.code_blocks[0];
        assert!(block.env_vars.is_empty());
        assert!(block.working_dir.is_none());
    }

    #[test]
    fn code_block_tracker_basic() {
        let mut tracker = CodeBlockTracker::new();
        assert!(!tracker.in_code_block());

        // Opening fence
        assert!(tracker.process_line("```"));
        assert!(tracker.in_code_block());

        // Content inside
        assert!(!tracker.process_line("some code"));
        assert!(tracker.in_code_block());

        // Closing fence
        assert!(tracker.process_line("```"));
        assert!(!tracker.in_code_block());
    }

    #[test]
    fn code_block_tracker_with_language_tag() {
        let mut tracker = CodeBlockTracker::new();

        // Opening fence with language tag
        assert!(tracker.process_line("```bash"));
        assert!(tracker.in_code_block());

        // Content
        assert!(!tracker.process_line("echo hello"));
        assert!(tracker.in_code_block());

        // Closing fence
        assert!(tracker.process_line("```"));
        assert!(!tracker.in_code_block());
    }

    #[test]
    fn code_block_tracker_nested_fences() {
        let mut tracker = CodeBlockTracker::new();

        // Opening fence with 4 backticks (for nesting)
        assert!(tracker.process_line("````markdown"));
        assert!(tracker.in_code_block());

        // Inner fence with 3 backticks (not a closing fence)
        assert!(!tracker.process_line("```rust"));
        assert!(tracker.in_code_block());

        // Content
        assert!(!tracker.process_line("fn main() {}"));
        assert!(tracker.in_code_block());

        // Inner closing fence (not the outer closing fence)
        assert!(!tracker.process_line("```"));
        assert!(tracker.in_code_block());

        // Outer closing fence
        assert!(tracker.process_line("````"));
        assert!(!tracker.in_code_block());
    }

    #[test]
    fn code_block_tracker_closing_fence_must_be_clean() {
        let mut tracker = CodeBlockTracker::new();

        // Opening fence
        assert!(tracker.process_line("```"));
        assert!(tracker.in_code_block());

        // Line that starts with backticks but has content after (not a closing fence)
        assert!(!tracker.process_line("```python"));
        assert!(tracker.in_code_block());

        // Valid closing fence
        assert!(tracker.process_line("```"));
        assert!(!tracker.in_code_block());
    }

    #[test]
    fn code_block_tracker_indented_fence() {
        let mut tracker = CodeBlockTracker::new();

        // Opening fence with leading whitespace
        assert!(tracker.process_line("  ```bash"));
        assert!(tracker.in_code_block());

        // Content
        assert!(!tracker.process_line("  echo hello"));
        assert!(tracker.in_code_block());

        // Closing fence with leading whitespace
        assert!(tracker.process_line("  ```"));
        assert!(!tracker.in_code_block());
    }
}
