//! Markdown parser for PAVED document validation.
//!
//! This module parses markdown documents and extracts structured information
//! about their sections, code blocks, and commands for validation purposes.

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

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

        let title = Self::extract_title(&lines);
        let sections = Self::extract_sections(&lines);

        Ok(ParsedDoc {
            path,
            title,
            sections,
            line_count,
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

            sections.push(Section {
                name: name.clone(),
                start_line: start_idx + 1, // Convert to 1-indexed
                content,
                has_code_blocks,
                has_commands,
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
}
