//! Index document generation for PAVED documentation.
//!
//! This module implements the `pave index` command which generates an index
//! document that serves as a map to all PAVED documentation.

use anyhow::{Context, Result};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use crate::config::{CONFIG_FILENAME, PaveConfig};

/// Document type detected from content or path.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DocType {
    Component,
    Runbook,
    Adr,
    Other,
}

impl DocType {
    /// Returns a display name for the document type.
    pub fn display_name(&self) -> &'static str {
        match self {
            DocType::Component => "Components",
            DocType::Runbook => "Runbooks",
            DocType::Adr => "Architecture Decisions",
            DocType::Other => "Other Documents",
        }
    }
}

/// Parsed information about a documentation file.
#[derive(Debug, Clone)]
pub struct DocInfo {
    /// Relative path from docs root to the file.
    pub path: PathBuf,
    /// Document title (extracted from first # heading).
    pub title: String,
    /// Purpose summary (first sentence of Purpose section).
    pub purpose: Option<String>,
    /// Detected document type.
    pub doc_type: DocType,
}

/// Custom section marker for update mode.
const CUSTOM_SECTION_START: &str = "<!-- CUSTOM CONTENT START -->";
const CUSTOM_SECTION_END: &str = "<!-- CUSTOM CONTENT END -->";

/// Run the index command.
pub fn run(output: &Path, update: bool) -> Result<()> {
    // Find and load config
    let config = load_config()?;
    let docs_root = &config.docs.root;

    // Check if docs directory exists
    if !docs_root.exists() {
        anyhow::bail!(
            "documentation directory '{}' does not exist",
            docs_root.display()
        );
    }

    // Scan for markdown files
    let docs = scan_docs(docs_root)?;

    if docs.is_empty() {
        println!("No documentation files found in '{}'", docs_root.display());
        return Ok(());
    }

    // Load existing custom content if updating
    let custom_content = if update && output.exists() {
        extract_custom_content(output)?
    } else {
        None
    };

    // Generate the index document
    let index_content = generate_index(&docs, custom_content.as_deref())?;

    // Ensure parent directory exists
    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create directory: {}", parent.display()))?;
    }

    // Write the index file
    fs::write(output, &index_content)
        .with_context(|| format!("failed to write index file: {}", output.display()))?;

    println!("Generated index at: {}", output.display());
    println!("  - {} documents indexed", docs.len());

    Ok(())
}

/// Load pave configuration from current directory or parents.
fn load_config() -> Result<PaveConfig> {
    let cwd = std::env::current_dir().context("failed to get current directory")?;

    // Search for config file in current directory and parents
    let mut search_path = cwd.as_path();
    loop {
        let config_path = search_path.join(CONFIG_FILENAME);
        if config_path.exists() {
            return PaveConfig::load(&config_path);
        }

        match search_path.parent() {
            Some(parent) => search_path = parent,
            None => break,
        }
    }

    // No config found, use defaults
    Ok(PaveConfig::default())
}

/// Scan the docs directory for markdown files.
fn scan_docs(docs_root: &Path) -> Result<Vec<DocInfo>> {
    let mut docs = Vec::new();
    scan_docs_recursive(docs_root, docs_root, &mut docs)?;
    Ok(docs)
}

/// Recursively scan directory for markdown files.
fn scan_docs_recursive(docs_root: &Path, current: &Path, docs: &mut Vec<DocInfo>) -> Result<()> {
    let entries = fs::read_dir(current)
        .with_context(|| format!("failed to read directory: {}", current.display()))?;

    for entry in entries {
        let entry = entry?;
        let path = entry.path();

        if path.is_dir() {
            // Skip the templates directory - these are scaffolds, not actual documentation
            if path.file_name().is_some_and(|n| n == "templates") {
                continue;
            }
            scan_docs_recursive(docs_root, &path, docs)?;
        } else if path.extension().is_some_and(|ext| ext == "md") {
            // Skip the index file itself
            let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if file_name == "index.md" {
                continue;
            }

            if let Some(doc_info) = parse_doc(&path, docs_root)? {
                docs.push(doc_info);
            }
        }
    }

    Ok(())
}

/// Parse a markdown document to extract metadata.
fn parse_doc(path: &Path, docs_root: &Path) -> Result<Option<DocInfo>> {
    let content = fs::read_to_string(path)
        .with_context(|| format!("failed to read file: {}", path.display()))?;

    let relative_path = path.strip_prefix(docs_root).unwrap_or(path).to_path_buf();

    // Extract title from first # heading
    let title = extract_title(&content).unwrap_or_else(|| {
        path.file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("Untitled")
            .to_string()
    });

    // Extract purpose from Purpose section
    let purpose = extract_purpose(&content);

    // Detect document type
    let doc_type = detect_doc_type(&relative_path, &content);

    Ok(Some(DocInfo {
        path: relative_path,
        title,
        purpose,
        doc_type,
    }))
}

/// Extract the title from the first # heading.
fn extract_title(content: &str) -> Option<String> {
    for line in content.lines() {
        let trimmed = line.trim();
        if let Some(title) = trimmed.strip_prefix("# ") {
            return Some(title.trim().to_string());
        }
    }
    None
}

/// Extract the first sentence from the Purpose section.
fn extract_purpose(content: &str) -> Option<String> {
    let mut in_purpose = false;
    let mut purpose_lines = Vec::new();

    for line in content.lines() {
        let trimmed = line.trim();

        // Check if we're entering the Purpose section
        if trimmed.starts_with("## Purpose") || trimmed.starts_with("## What") {
            in_purpose = true;
            continue;
        }

        // Check if we're leaving the Purpose section
        if in_purpose && trimmed.starts_with("## ") {
            break;
        }

        // Collect non-empty lines in the Purpose section
        if in_purpose && !trimmed.is_empty() {
            purpose_lines.push(trimmed);
        }
    }

    if purpose_lines.is_empty() {
        return None;
    }

    // Join lines and extract first sentence
    let purpose_text = purpose_lines.join(" ");
    let first_sentence = purpose_text
        .split_once(". ")
        .map(|(s, _)| format!("{}.", s))
        .unwrap_or_else(|| {
            // No period with space found, use the whole text (truncated if too long)
            // Use chars().take() to avoid slicing in the middle of a UTF-8 character
            if purpose_text.chars().count() > 100 {
                let truncated: String = purpose_text.chars().take(100).collect();
                format!("{}...", truncated)
            } else {
                purpose_text
            }
        });

    Some(first_sentence)
}

/// Detect the document type from path and content.
fn detect_doc_type(relative_path: &Path, content: &str) -> DocType {
    let path_str = relative_path.to_string_lossy().to_lowercase();

    // Check path patterns
    if path_str.contains("component") {
        return DocType::Component;
    }
    if path_str.contains("runbook") {
        return DocType::Runbook;
    }
    if path_str.contains("adr") || path_str.contains("decision") {
        return DocType::Adr;
    }

    // Check content patterns
    let content_lower = content.to_lowercase();

    // ADRs typically have a Status section
    if content_lower.contains("## status")
        && (content_lower.contains("accepted")
            || content_lower.contains("proposed")
            || content_lower.contains("deprecated"))
    {
        return DocType::Adr;
    }

    // Runbooks have specific sections
    if content_lower.contains("## when to use")
        || content_lower.contains("## preconditions")
        || content_lower.contains("## steps")
    {
        return DocType::Runbook;
    }

    // Components have Interface/Configuration sections
    if content_lower.contains("## interface") || content_lower.contains("## configuration") {
        return DocType::Component;
    }

    DocType::Other
}

/// Extract custom content from existing index file.
fn extract_custom_content(path: &Path) -> Result<Option<String>> {
    let content = fs::read_to_string(path)
        .with_context(|| format!("failed to read existing index: {}", path.display()))?;

    let start = content.find(CUSTOM_SECTION_START);
    let end = content.find(CUSTOM_SECTION_END);

    match (start, end) {
        (Some(s), Some(e)) if s < e => {
            let custom = &content[s + CUSTOM_SECTION_START.len()..e];
            Ok(Some(custom.trim().to_string()))
        }
        _ => Ok(None),
    }
}

/// Generate the index document content.
fn generate_index(docs: &[DocInfo], custom_content: Option<&str>) -> Result<String> {
    let mut output = String::new();

    // Header
    output.push_str("# Documentation Index\n\n");
    output.push_str("> Start here. This is your map to all documentation.\n\n");

    // Group documents by type
    let mut grouped: HashMap<DocType, Vec<&DocInfo>> = HashMap::new();
    for doc in docs {
        grouped.entry(doc.doc_type).or_default().push(doc);
    }

    // Sort documents within each group by title
    for docs_in_group in grouped.values_mut() {
        docs_in_group.sort_by(|a, b| a.title.to_lowercase().cmp(&b.title.to_lowercase()));
    }

    // Identify top-level docs for Quick Links
    let top_level_paths: std::collections::HashSet<_> = docs
        .iter()
        .filter(|d| d.path.components().count() == 1)
        .map(|d| &d.path)
        .collect();

    // Generate Quick Links section for top-level docs
    if !top_level_paths.is_empty() {
        output.push_str("## Quick Links\n\n");
        let mut top_level: Vec<_> = docs
            .iter()
            .filter(|d| top_level_paths.contains(&d.path))
            .collect();
        top_level.sort_by(|a, b| a.title.to_lowercase().cmp(&b.title.to_lowercase()));
        for doc in top_level {
            output.push_str(&format!("- [{}](./{})\n", doc.title, doc.path.display()));
        }
        output.push('\n');
    }

    // Generate sections for each document type (in a specific order)
    // Exclude top-level docs that are already in Quick Links
    let type_order = [
        DocType::Component,
        DocType::Runbook,
        DocType::Adr,
        DocType::Other,
    ];

    for doc_type in type_order {
        if let Some(docs_in_group) = grouped.get(&doc_type) {
            // Filter out top-level docs already shown in Quick Links
            let nested_docs: Vec<_> = docs_in_group
                .iter()
                .filter(|d| !top_level_paths.contains(&d.path))
                .collect();

            if nested_docs.is_empty() {
                continue;
            }

            output.push_str(&format!("## {}\n\n", doc_type.display_name()));

            // Use table format for components, list for others
            if doc_type == DocType::Component {
                output.push_str("| Component | Purpose |\n");
                output.push_str("|-----------|----------|\n");
                for doc in nested_docs {
                    let purpose = doc.purpose.as_deref().unwrap_or("-");
                    output.push_str(&format!(
                        "| [{}](./{}) | {} |\n",
                        doc.title,
                        doc.path.display(),
                        purpose
                    ));
                }
            } else {
                for doc in nested_docs {
                    output.push_str(&format!("- [{}](./{})\n", doc.title, doc.path.display()));
                }
            }

            output.push('\n');
        }
    }

    // Custom content section
    if let Some(custom) = custom_content {
        output.push_str(CUSTOM_SECTION_START);
        output.push('\n');
        output.push_str(custom);
        output.push('\n');
        output.push_str(CUSTOM_SECTION_END);
        output.push_str("\n\n");
    }

    // Footer
    let timestamp = chrono::Local::now().format("%Y-%m-%d");
    output.push_str("---\n");
    output.push_str(&format!(
        "*Generated by pave. Last updated: {}*\n",
        timestamp
    ));

    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn create_test_doc(dir: &Path, name: &str, content: &str) {
        let path = dir.join(name);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, content).unwrap();
    }

    #[test]
    fn test_extract_title() {
        let content = "# My Title\n\nSome content.";
        assert_eq!(extract_title(content), Some("My Title".to_string()));

        let content = "No title here";
        assert_eq!(extract_title(content), None);

        let content = "  # Spaced Title  \n";
        assert_eq!(extract_title(content), Some("Spaced Title".to_string()));
    }

    #[test]
    fn test_extract_purpose() {
        let content =
            "# Title\n\n## Purpose\n\nThis is the purpose. More details here.\n\n## Other";
        let purpose = extract_purpose(content);
        assert_eq!(purpose, Some("This is the purpose.".to_string()));

        let content = "# Title\n\nNo purpose section";
        assert_eq!(extract_purpose(content), None);
    }

    #[test]
    fn test_detect_doc_type_from_path() {
        let path = PathBuf::from("components/auth.md");
        assert_eq!(detect_doc_type(&path, ""), DocType::Component);

        let path = PathBuf::from("runbooks/deploy.md");
        assert_eq!(detect_doc_type(&path, ""), DocType::Runbook);

        let path = PathBuf::from("adrs/001-use-rust.md");
        assert_eq!(detect_doc_type(&path, ""), DocType::Adr);

        let path = PathBuf::from("random.md");
        assert_eq!(detect_doc_type(&path, ""), DocType::Other);
    }

    #[test]
    fn test_detect_doc_type_from_content() {
        let path = PathBuf::from("doc.md");

        let adr_content = "# Decision\n\n## Status\n\nAccepted\n\n## Context";
        assert_eq!(detect_doc_type(&path, adr_content), DocType::Adr);

        let runbook_content = "# Deploy\n\n## When to Use\n\nWhen deploying...";
        assert_eq!(detect_doc_type(&path, runbook_content), DocType::Runbook);

        let component_content = "# Auth\n\n## Interface\n\nProvides auth...";
        assert_eq!(
            detect_doc_type(&path, component_content),
            DocType::Component
        );
    }

    #[test]
    fn test_extract_custom_content() {
        let dir = TempDir::new().unwrap();
        let index_path = dir.path().join("index.md");

        let content = format!(
            "# Index\n\n{}\nMy custom notes\n{}\n\n---\n",
            CUSTOM_SECTION_START, CUSTOM_SECTION_END
        );
        fs::write(&index_path, content).unwrap();

        let custom = extract_custom_content(&index_path).unwrap();
        assert_eq!(custom, Some("My custom notes".to_string()));
    }

    #[test]
    fn test_generate_index_with_multiple_doc_types() {
        let docs = vec![
            DocInfo {
                path: PathBuf::from("components/auth.md"),
                title: "Auth Service".to_string(),
                purpose: Some("Handles user authentication.".to_string()),
                doc_type: DocType::Component,
            },
            DocInfo {
                path: PathBuf::from("runbooks/deploy.md"),
                title: "Deploy to Production".to_string(),
                purpose: None,
                doc_type: DocType::Runbook,
            },
            DocInfo {
                path: PathBuf::from("adrs/001-use-rust.md"),
                title: "ADR-001: Use Rust".to_string(),
                purpose: None,
                doc_type: DocType::Adr,
            },
        ];

        let result = generate_index(&docs, None).unwrap();

        assert!(result.contains("# Documentation Index"));
        assert!(result.contains("## Components"));
        assert!(result.contains("[Auth Service](./components/auth.md)"));
        assert!(result.contains("Handles user authentication."));
        assert!(result.contains("## Runbooks"));
        assert!(result.contains("[Deploy to Production](./runbooks/deploy.md)"));
        assert!(result.contains("## Architecture Decisions"));
        assert!(result.contains("[ADR-001: Use Rust](./adrs/001-use-rust.md)"));
        assert!(result.contains("*Generated by pave."));
    }

    #[test]
    fn test_generate_index_preserves_custom_content() {
        let docs = vec![DocInfo {
            path: PathBuf::from("readme.md"),
            title: "README".to_string(),
            purpose: None,
            doc_type: DocType::Other,
        }];

        let custom = "My preserved notes";
        let result = generate_index(&docs, Some(custom)).unwrap();

        assert!(result.contains(CUSTOM_SECTION_START));
        assert!(result.contains("My preserved notes"));
        assert!(result.contains(CUSTOM_SECTION_END));
    }

    #[test]
    fn test_scan_and_generate_integration() {
        let dir = TempDir::new().unwrap();
        let docs_root = dir.path();

        // Create test documents
        create_test_doc(
            docs_root,
            "getting-started.md",
            "# Getting Started\n\n## Purpose\n\nHelps you get started.\n",
        );
        create_test_doc(
            docs_root,
            "components/auth.md",
            "# Auth Service\n\n## Purpose\n\nHandles authentication.\n\n## Interface\n\n...",
        );
        create_test_doc(
            docs_root,
            "runbooks/deploy.md",
            "# Deploy Guide\n\n## When to Use\n\nWhen deploying...\n\n## Steps\n\n1. ...",
        );

        let docs = scan_docs(docs_root).unwrap();

        assert_eq!(docs.len(), 3);

        // Verify types are detected correctly
        let auth_doc = docs.iter().find(|d| d.title == "Auth Service").unwrap();
        assert_eq!(auth_doc.doc_type, DocType::Component);

        let deploy_doc = docs.iter().find(|d| d.title == "Deploy Guide").unwrap();
        assert_eq!(deploy_doc.doc_type, DocType::Runbook);
    }

    #[test]
    fn test_links_are_valid_relative_paths() {
        let docs = vec![
            DocInfo {
                path: PathBuf::from("components/auth.md"),
                title: "Auth".to_string(),
                purpose: None,
                doc_type: DocType::Component,
            },
            DocInfo {
                path: PathBuf::from("deep/nested/doc.md"),
                title: "Nested".to_string(),
                purpose: None,
                doc_type: DocType::Other,
            },
        ];

        let result = generate_index(&docs, None).unwrap();

        // Links should be relative with ./
        assert!(result.contains("(./components/auth.md)"));
        assert!(result.contains("(./deep/nested/doc.md)"));
    }
}
