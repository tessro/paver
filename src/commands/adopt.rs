//! Adopt command for scanning existing documentation.
//!
//! This module implements the `pave adopt` command which scans existing
//! documentation to help users onboard pave into projects that already have docs.

use anyhow::{Context, Result};
use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};

use crate::parser::ParsedDoc;
use crate::rules::DocType;
use crate::rules::detect_doc_type;

/// Arguments for the adopt command.
pub struct AdoptArgs {
    /// Path to scan for documentation.
    pub path: Option<PathBuf>,
    /// Output format.
    pub format: AdoptOutputFormat,
    /// Whether to print suggested config.
    pub suggest_config: bool,
    /// Whether to show what pave init would create (without creating).
    pub dry_run: bool,
}

/// Output format for the adopt command.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AdoptOutputFormat {
    #[default]
    Text,
    Json,
}

/// Analysis result for a single document.
#[derive(Debug, Clone, Serialize)]
pub struct DocAnalysis {
    /// Path to the document (relative).
    pub path: PathBuf,
    /// Document title.
    pub title: Option<String>,
    /// Detected document type.
    pub doc_type: String,
    /// Number of lines.
    pub line_count: usize,
    /// Whether it has a Purpose section.
    pub has_purpose: bool,
    /// Whether it has a Verification section.
    pub has_verification: bool,
    /// Whether it has an Examples section.
    pub has_examples: bool,
    /// Whether it has code blocks.
    pub has_code_blocks: bool,
    /// Whether it has frontmatter.
    pub has_frontmatter: bool,
    /// List of H2 sections found.
    pub sections: Vec<String>,
}

/// Summary of the adoption analysis.
#[derive(Debug, Clone, Serialize)]
pub struct AdoptionSummary {
    /// Total files found.
    pub total_files: usize,
    /// Files with Purpose section.
    pub files_with_purpose: usize,
    /// Files with Verification section.
    pub files_with_verification: usize,
    /// Files with Examples section.
    pub files_with_examples: usize,
    /// Files with code blocks.
    pub files_with_code_blocks: usize,
    /// Files with frontmatter.
    pub files_with_frontmatter: usize,
    /// Number of runbooks found.
    pub runbook_count: usize,
    /// Number of ADRs found.
    pub adr_count: usize,
    /// Number of components found.
    pub component_count: usize,
    /// Number of other documents.
    pub other_count: usize,
    /// Detected docs root.
    pub detected_docs_root: Option<PathBuf>,
    /// Maximum line count found.
    pub max_lines_found: usize,
}

/// Complete adoption report.
#[derive(Debug, Clone, Serialize)]
pub struct AdoptionReport {
    /// Summary statistics.
    pub summary: AdoptionSummary,
    /// Individual document analyses.
    pub documents: Vec<DocAnalysis>,
    /// Recommendations for adoption.
    pub recommendations: Vec<String>,
}

/// Execute the adopt command.
pub fn execute(args: AdoptArgs) -> Result<()> {
    // Determine the path to scan
    let scan_path = args.path.clone().unwrap_or_else(|| PathBuf::from("."));

    // Detect docs location
    let docs_root = detect_docs_root(&scan_path)?;

    if docs_root.is_none() {
        if args.format == AdoptOutputFormat::Json {
            let report = AdoptionReport {
                summary: AdoptionSummary {
                    total_files: 0,
                    files_with_purpose: 0,
                    files_with_verification: 0,
                    files_with_examples: 0,
                    files_with_code_blocks: 0,
                    files_with_frontmatter: 0,
                    runbook_count: 0,
                    adr_count: 0,
                    component_count: 0,
                    other_count: 0,
                    detected_docs_root: None,
                    max_lines_found: 0,
                },
                documents: Vec::new(),
                recommendations: vec![
                    "No documentation found. Run 'pave init' to create initial documentation."
                        .to_string(),
                ],
            };
            println!("{}", serde_json::to_string_pretty(&report)?);
        } else {
            println!("No documentation found.");
            println!();
            println!("Searched for common documentation locations:");
            println!("  - docs/");
            println!("  - documentation/");
            println!("  - doc/");
            println!("  - README.md");
            println!();
            println!("Run 'pave init' to create initial documentation structure.");
        }
        return Ok(());
    }

    let docs_root = docs_root.unwrap();

    // Scan for markdown files
    let documents = scan_docs(&docs_root)?;

    if documents.is_empty() {
        if args.format == AdoptOutputFormat::Json {
            let report = AdoptionReport {
                summary: AdoptionSummary {
                    total_files: 0,
                    files_with_purpose: 0,
                    files_with_verification: 0,
                    files_with_examples: 0,
                    files_with_code_blocks: 0,
                    files_with_frontmatter: 0,
                    runbook_count: 0,
                    adr_count: 0,
                    component_count: 0,
                    other_count: 0,
                    detected_docs_root: Some(docs_root),
                    max_lines_found: 0,
                },
                documents: Vec::new(),
                recommendations: vec![
                    "Documentation directory exists but contains no markdown files.".to_string(),
                    "Run 'pave init' to create initial documentation structure.".to_string(),
                ],
            };
            println!("{}", serde_json::to_string_pretty(&report)?);
        } else {
            println!("No markdown files found in '{}'.", docs_root.display());
            println!();
            println!("Run 'pave init' to create initial documentation structure.");
        }
        return Ok(());
    }

    // Generate report
    let report = generate_report(&docs_root, &documents)?;

    // Output based on format
    match args.format {
        AdoptOutputFormat::Json => {
            println!("{}", serde_json::to_string_pretty(&report)?);
        }
        AdoptOutputFormat::Text => {
            output_text_report(&report);

            if args.suggest_config {
                println!();
                output_suggested_config(&report);
            }

            if args.dry_run {
                println!();
                output_dry_run(&report);
            }
        }
    }

    Ok(())
}

/// Detect the documentation root directory.
fn detect_docs_root(scan_path: &Path) -> Result<Option<PathBuf>> {
    // If a specific file was provided, use its parent
    if scan_path.is_file() {
        return Ok(scan_path.parent().map(|p| p.to_path_buf()));
    }

    // Common documentation directory names
    let common_names = ["docs", "documentation", "doc"];

    // If an explicit directory was provided (not current dir),
    // check if it's a known docs directory name or if it should be scanned inside
    if scan_path.is_dir() && scan_path != Path::new(".") {
        let dir_name = scan_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_lowercase();

        // If the path itself is a known docs directory, use it directly
        if common_names.iter().any(|&name| dir_name == name) {
            return Ok(Some(scan_path.to_path_buf()));
        }

        // Otherwise, search inside this directory for docs directories
        for location in &common_names {
            let path = scan_path.join(location);
            if path.exists() && path.is_dir() {
                return Ok(Some(path));
            }
        }

        // Check for README.md in the provided directory
        let readme = scan_path.join("README.md");
        if readme.exists() {
            return Ok(Some(scan_path.to_path_buf()));
        }

        // If nothing found but directory was explicitly provided, return None
        return Ok(None);
    }

    // Search for common documentation locations relative to scan_path
    for location in &common_names {
        let path = scan_path.join(location);
        if path.exists() && path.is_dir() {
            return Ok(Some(path));
        }
    }

    // Check for README.md as a fallback
    let readme = scan_path.join("README.md");
    if readme.exists() {
        return Ok(Some(scan_path.to_path_buf()));
    }

    Ok(None)
}

/// Scan a directory for markdown files.
fn scan_docs(docs_root: &Path) -> Result<Vec<DocAnalysis>> {
    let mut documents = Vec::new();
    scan_docs_recursive(docs_root, docs_root, &mut documents)?;
    Ok(documents)
}

/// Recursively scan directory for markdown files.
fn scan_docs_recursive(
    docs_root: &Path,
    current: &Path,
    documents: &mut Vec<DocAnalysis>,
) -> Result<()> {
    let entries = fs::read_dir(current)
        .with_context(|| format!("failed to read directory: {}", current.display()))?;

    for entry in entries {
        let entry = entry?;
        let path = entry.path();

        if path.is_dir() {
            // Skip common non-doc directories
            let dir_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if matches!(
                dir_name,
                "node_modules"
                    | "target"
                    | ".git"
                    | ".github"
                    | "templates"
                    | "_site"
                    | ".pave"
                    | "vendor"
                    | "build"
            ) {
                continue;
            }
            scan_docs_recursive(docs_root, &path, documents)?;
        } else if path.extension().is_some_and(|ext| ext == "md")
            && let Some(analysis) = analyze_document(&path, docs_root)?
        {
            documents.push(analysis);
        }
    }

    Ok(())
}

/// Analyze a single markdown document.
fn analyze_document(path: &Path, docs_root: &Path) -> Result<Option<DocAnalysis>> {
    let content = fs::read_to_string(path)
        .with_context(|| format!("failed to read file: {}", path.display()))?;

    let relative_path = path.strip_prefix(docs_root).unwrap_or(path).to_path_buf();

    // Parse the document
    let parsed = ParsedDoc::parse_content(path.to_path_buf(), &content)?;

    // Detect document type
    let doc_type = detect_doc_type(&relative_path, &content);
    let doc_type_str = match doc_type {
        DocType::Component => "component",
        DocType::Runbook => "runbook",
        DocType::Adr => "adr",
        DocType::Other => "other",
    }
    .to_string();

    // Check for code blocks in any section
    let has_code_blocks = parsed.sections.iter().any(|s| s.has_code_blocks);

    // Collect section names
    let sections: Vec<String> = parsed.sections.iter().map(|s| s.name.clone()).collect();

    // Check for PAVED sections before moving title
    let has_purpose = parsed.has_section("Purpose");
    let has_verification = parsed.has_section("Verification");
    let has_examples = parsed.has_section("Examples");
    let has_frontmatter = parsed.frontmatter.is_some();
    let line_count = parsed.line_count;

    Ok(Some(DocAnalysis {
        path: relative_path,
        title: parsed.title,
        doc_type: doc_type_str,
        line_count,
        has_purpose,
        has_verification,
        has_examples,
        has_code_blocks,
        has_frontmatter,
        sections,
    }))
}

/// Generate the adoption report from analyzed documents.
fn generate_report(docs_root: &Path, documents: &[DocAnalysis]) -> Result<AdoptionReport> {
    let total_files = documents.len();
    let files_with_purpose = documents.iter().filter(|d| d.has_purpose).count();
    let files_with_verification = documents.iter().filter(|d| d.has_verification).count();
    let files_with_examples = documents.iter().filter(|d| d.has_examples).count();
    let files_with_code_blocks = documents.iter().filter(|d| d.has_code_blocks).count();
    let files_with_frontmatter = documents.iter().filter(|d| d.has_frontmatter).count();

    let runbook_count = documents.iter().filter(|d| d.doc_type == "runbook").count();
    let adr_count = documents.iter().filter(|d| d.doc_type == "adr").count();
    let component_count = documents
        .iter()
        .filter(|d| d.doc_type == "component")
        .count();
    let other_count = documents.iter().filter(|d| d.doc_type == "other").count();

    let max_lines_found = documents.iter().map(|d| d.line_count).max().unwrap_or(0);

    // Generate recommendations
    let mut recommendations = Vec::new();

    // Missing sections recommendations
    let missing_verification = total_files - files_with_verification;
    if missing_verification > 0 {
        recommendations.push(format!(
            "{} files are missing Verification sections",
            missing_verification
        ));
    }

    let missing_examples = total_files - files_with_examples;
    if missing_examples > 0 {
        recommendations.push(format!(
            "{} files are missing Examples sections",
            missing_examples
        ));
    }

    let missing_purpose = total_files - files_with_purpose;
    if missing_purpose > 0 {
        recommendations.push(format!(
            "{} files are missing Purpose sections",
            missing_purpose
        ));
    }

    // General recommendations based on analysis
    if files_with_verification < total_files / 2 {
        recommendations.push(
            "Consider starting with gradual adoption mode (require_verification = false)"
                .to_string(),
        );
    }

    if files_with_examples < total_files / 2 {
        recommendations.push(
            "Consider disabling require_examples initially (require_examples = false)".to_string(),
        );
    }

    if max_lines_found > 300 {
        recommendations.push(format!(
            "Some documents exceed 300 lines. Consider setting max_lines = {} or splitting large docs",
            ((max_lines_found / 100) + 1) * 100
        ));
    }

    Ok(AdoptionReport {
        summary: AdoptionSummary {
            total_files,
            files_with_purpose,
            files_with_verification,
            files_with_examples,
            files_with_code_blocks,
            files_with_frontmatter,
            runbook_count,
            adr_count,
            component_count,
            other_count,
            detected_docs_root: Some(docs_root.to_path_buf()),
            max_lines_found,
        },
        documents: documents.to_vec(),
        recommendations,
    })
}

/// Output the report in text format.
fn output_text_report(report: &AdoptionReport) {
    let summary = &report.summary;
    let docs_root = summary
        .detected_docs_root
        .as_ref()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| ".".to_string());

    println!(
        "Found {} markdown files in {}/",
        summary.total_files, docs_root
    );
    println!();
    println!("Structure analysis:");
    println!(
        "  {} files have Purpose section",
        summary.files_with_purpose
    );
    println!(
        "  {} files have Verification section",
        summary.files_with_verification
    );
    println!(
        "  {} files have Examples section",
        summary.files_with_examples
    );
    println!(
        "  {} files have code blocks",
        summary.files_with_code_blocks
    );

    // Document types
    if summary.runbook_count > 0 || summary.adr_count > 0 || summary.component_count > 0 {
        println!();
        println!("Document types detected:");
        if summary.component_count > 0 {
            println!("  {} component docs", summary.component_count);
        }
        if summary.runbook_count > 0 {
            println!("  {} runbooks", summary.runbook_count);
        }
        if summary.adr_count > 0 {
            println!("  {} ADRs", summary.adr_count);
        }
        if summary.other_count > 0 {
            println!("  {} other documents", summary.other_count);
        }
    }

    // Recommendations
    if !report.recommendations.is_empty() {
        println!();
        println!("Recommendations:");
        for rec in &report.recommendations {
            println!("  - {}", rec);
        }
    }

    println!();
    println!("Run 'pave adopt --suggest-config' to see recommended .pave.toml");
}

/// Output suggested configuration.
fn output_suggested_config(report: &AdoptionReport) {
    let summary = &report.summary;

    println!("Suggested .pave.toml:");
    println!();
    println!("[pave]");
    println!("version = \"0.1\"");
    println!();
    println!("[docs]");

    // Determine docs root - strip leading "./" for cleaner config
    let docs_root = summary
        .detected_docs_root
        .as_ref()
        .map(|p| {
            let s = p.display().to_string();
            s.strip_prefix("./").unwrap_or(&s).to_string()
        })
        .unwrap_or_else(|| "docs".to_string());
    println!("root = \"{}\"", docs_root);
    println!();
    println!("[rules]");

    // Set max_lines based on existing docs
    let suggested_max_lines = if summary.max_lines_found > 300 {
        ((summary.max_lines_found / 100) + 1) * 100
    } else {
        300
    };
    println!("max_lines = {}", suggested_max_lines);

    // Suggest gradual adoption if many docs are missing sections
    let require_verification = summary.files_with_verification >= summary.total_files / 2;
    println!("require_verification = {}", require_verification);

    let require_examples = summary.files_with_examples >= summary.total_files / 2;
    println!("require_examples = {}", require_examples);

    // Disable require_verification_commands if many docs don't have executable commands
    println!("require_verification_commands = false");
}

/// Output dry-run information.
fn output_dry_run(report: &AdoptionReport) {
    println!("Dry run: What 'pave init' would create:");
    println!();

    let docs_root = report
        .summary
        .detected_docs_root
        .as_ref()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| "docs".to_string());

    println!("  .pave.toml - Configuration file");

    // Check if templates directory would be created
    let has_templates_dir = report
        .documents
        .iter()
        .any(|d| d.path.to_string_lossy().contains("templates"));

    if !has_templates_dir {
        println!(
            "  {}/templates/component.md - Component template",
            docs_root
        );
        println!("  {}/templates/runbook.md - Runbook template", docs_root);
        println!("  {}/templates/adr.md - ADR template", docs_root);
    }

    println!();
    println!("Note: No files will be modified. Existing documentation will be preserved.");
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
    fn detect_docs_root_finds_docs_directory() {
        let dir = TempDir::new().unwrap();
        let docs_dir = dir.path().join("docs");
        fs::create_dir_all(&docs_dir).unwrap();

        let result = detect_docs_root(dir.path()).unwrap();
        assert!(result.is_some());
        assert_eq!(result.unwrap(), docs_dir);
    }

    #[test]
    fn detect_docs_root_finds_documentation_directory() {
        let dir = TempDir::new().unwrap();
        let docs_dir = dir.path().join("documentation");
        fs::create_dir_all(&docs_dir).unwrap();

        let result = detect_docs_root(dir.path()).unwrap();
        assert!(result.is_some());
        assert_eq!(result.unwrap(), docs_dir);
    }

    #[test]
    fn detect_docs_root_finds_readme() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("README.md"), "# README").unwrap();

        let result = detect_docs_root(dir.path()).unwrap();
        assert!(result.is_some());
        assert_eq!(result.unwrap(), dir.path());
    }

    #[test]
    fn detect_docs_root_returns_none_when_no_docs() {
        let dir = TempDir::new().unwrap();

        let result = detect_docs_root(dir.path()).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn detect_docs_root_uses_explicit_docs_path() {
        // When explicitly passing a known docs directory, use it directly
        let dir = TempDir::new().unwrap();
        let custom_docs = dir.path().join("docs");
        fs::create_dir_all(&custom_docs).unwrap();

        let result = detect_docs_root(&custom_docs).unwrap();
        assert!(result.is_some());
        assert_eq!(result.unwrap(), custom_docs);
    }

    #[test]
    fn detect_docs_root_searches_inside_unknown_directory() {
        // When passing an unknown directory name, search inside it
        let dir = TempDir::new().unwrap();
        let project_dir = dir.path().join("my-project");
        let docs_dir = project_dir.join("docs");
        fs::create_dir_all(&docs_dir).unwrap();

        let result = detect_docs_root(&project_dir).unwrap();
        assert!(result.is_some());
        assert_eq!(result.unwrap(), docs_dir);
    }

    #[test]
    fn analyze_document_detects_paved_sections() {
        let dir = TempDir::new().unwrap();
        let content = r#"# Test Component

## Purpose
This is a test component.

## Verification
```bash
cargo test
```

## Examples
```rust
fn example() {}
```
"#;
        create_test_doc(dir.path(), "test.md", content);

        let analysis = analyze_document(&dir.path().join("test.md"), dir.path())
            .unwrap()
            .unwrap();

        assert!(analysis.has_purpose);
        assert!(analysis.has_verification);
        assert!(analysis.has_examples);
        assert!(analysis.has_code_blocks);
        assert_eq!(analysis.title, Some("Test Component".to_string()));
    }

    #[test]
    fn analyze_document_detects_missing_sections() {
        let dir = TempDir::new().unwrap();
        let content = r#"# Simple Doc

## Introduction
Just some text here.
"#;
        create_test_doc(dir.path(), "simple.md", content);

        let analysis = analyze_document(&dir.path().join("simple.md"), dir.path())
            .unwrap()
            .unwrap();

        assert!(!analysis.has_purpose);
        assert!(!analysis.has_verification);
        assert!(!analysis.has_examples);
        assert!(!analysis.has_code_blocks);
    }

    #[test]
    fn analyze_document_detects_runbook() {
        let dir = TempDir::new().unwrap();
        let runbooks_dir = dir.path().join("runbooks");
        fs::create_dir_all(&runbooks_dir).unwrap();

        let content = r#"# Deploy Runbook

## When to Use
When deploying to production.

## Steps
1. Build the project
2. Deploy

## Rollback
Revert the deployment.
"#;
        create_test_doc(dir.path(), "runbooks/deploy.md", content);

        let analysis = analyze_document(&runbooks_dir.join("deploy.md"), dir.path())
            .unwrap()
            .unwrap();

        assert_eq!(analysis.doc_type, "runbook");
    }

    #[test]
    fn analyze_document_detects_adr() {
        let dir = TempDir::new().unwrap();
        let adr_dir = dir.path().join("adr");
        fs::create_dir_all(&adr_dir).unwrap();

        let content = r#"# ADR-001: Use Rust

## Status
Accepted

## Context
We need a systems language.

## Decision
Use Rust.

## Consequences
Good performance.
"#;
        create_test_doc(dir.path(), "adr/001-use-rust.md", content);

        let analysis = analyze_document(&adr_dir.join("001-use-rust.md"), dir.path())
            .unwrap()
            .unwrap();

        assert_eq!(analysis.doc_type, "adr");
    }

    #[test]
    fn scan_docs_finds_all_markdown_files() {
        let dir = TempDir::new().unwrap();

        create_test_doc(dir.path(), "readme.md", "# README\n## Purpose\nTest");
        create_test_doc(
            dir.path(),
            "components/auth.md",
            "# Auth\n## Purpose\nAuth component",
        );
        create_test_doc(
            dir.path(),
            "runbooks/deploy.md",
            "# Deploy\n## Steps\n1. Deploy",
        );

        let documents = scan_docs(dir.path()).unwrap();
        assert_eq!(documents.len(), 3);
    }

    #[test]
    fn scan_docs_skips_node_modules() {
        let dir = TempDir::new().unwrap();

        create_test_doc(dir.path(), "readme.md", "# README");
        create_test_doc(
            dir.path(),
            "node_modules/pkg/readme.md",
            "# Should be skipped",
        );

        let documents = scan_docs(dir.path()).unwrap();
        assert_eq!(documents.len(), 1);
    }

    #[test]
    fn generate_report_counts_sections_correctly() {
        let documents = vec![
            DocAnalysis {
                path: PathBuf::from("doc1.md"),
                title: Some("Doc 1".to_string()),
                doc_type: "component".to_string(),
                line_count: 50,
                has_purpose: true,
                has_verification: true,
                has_examples: true,
                has_code_blocks: true,
                has_frontmatter: false,
                sections: vec!["Purpose".to_string(), "Verification".to_string()],
            },
            DocAnalysis {
                path: PathBuf::from("doc2.md"),
                title: Some("Doc 2".to_string()),
                doc_type: "other".to_string(),
                line_count: 30,
                has_purpose: true,
                has_verification: false,
                has_examples: false,
                has_code_blocks: false,
                has_frontmatter: false,
                sections: vec!["Purpose".to_string()],
            },
        ];

        let report = generate_report(Path::new("docs"), &documents).unwrap();

        assert_eq!(report.summary.total_files, 2);
        assert_eq!(report.summary.files_with_purpose, 2);
        assert_eq!(report.summary.files_with_verification, 1);
        assert_eq!(report.summary.files_with_examples, 1);
        assert_eq!(report.summary.component_count, 1);
        assert_eq!(report.summary.other_count, 1);
    }

    #[test]
    fn generate_report_creates_recommendations() {
        let documents = vec![
            DocAnalysis {
                path: PathBuf::from("doc1.md"),
                title: Some("Doc 1".to_string()),
                doc_type: "other".to_string(),
                line_count: 50,
                has_purpose: false,
                has_verification: false,
                has_examples: false,
                has_code_blocks: false,
                has_frontmatter: false,
                sections: Vec::new(),
            },
            DocAnalysis {
                path: PathBuf::from("doc2.md"),
                title: Some("Doc 2".to_string()),
                doc_type: "other".to_string(),
                line_count: 400,
                has_purpose: false,
                has_verification: false,
                has_examples: false,
                has_code_blocks: false,
                has_frontmatter: false,
                sections: Vec::new(),
            },
        ];

        let report = generate_report(Path::new("docs"), &documents).unwrap();

        assert!(!report.recommendations.is_empty());
        assert!(
            report
                .recommendations
                .iter()
                .any(|r| r.contains("Verification"))
        );
        assert!(report.recommendations.iter().any(|r| r.contains("Purpose")));
        assert!(
            report
                .recommendations
                .iter()
                .any(|r| r.contains("max_lines") || r.contains("300 lines"))
        );
    }

    #[test]
    fn output_format_text_is_default() {
        assert_eq!(AdoptOutputFormat::default(), AdoptOutputFormat::Text);
    }

    #[test]
    fn scan_docs_integration() {
        let dir = TempDir::new().unwrap();
        let docs_dir = dir.path().join("docs");
        fs::create_dir_all(&docs_dir).unwrap();

        create_test_doc(
            &docs_dir,
            "components/auth.md",
            r#"# Auth Service

## Purpose
Handles authentication.

## Interface
REST API.

## Verification
```bash
cargo test
```

## Examples
```rust
auth::login()
```
"#,
        );

        create_test_doc(
            &docs_dir,
            "runbooks/deploy.md",
            r#"# Deploy Runbook

## When to Use
During deployments.

## Steps
1. Build
2. Deploy
"#,
        );

        let documents = scan_docs(&docs_dir).unwrap();
        let report = generate_report(&docs_dir, &documents).unwrap();

        assert_eq!(report.summary.total_files, 2);
        assert_eq!(report.summary.component_count, 1);
        assert_eq!(report.summary.runbook_count, 1);
        assert_eq!(report.summary.files_with_purpose, 1); // Only auth has Purpose
    }
}
