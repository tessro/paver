//! Migrate command for bulk insertion of missing PAVED sections.
//!
//! This module implements the `pave migrate` command which helps bulk-update
//! existing documentation by inserting missing PAVED sections with placeholder content.

use anyhow::{Context, Result};
use serde::Serialize;
use std::collections::HashSet;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use crate::config::{CONFIG_FILENAME, PaveConfig};
use crate::parser::{CodeBlockTracker, ParsedDoc};
use crate::rules::{DocType, detect_doc_type};

/// Arguments for the migrate command.
pub struct MigrateArgs {
    /// Path to migrate (file or directory).
    pub path: Option<PathBuf>,
    /// Output format.
    pub format: MigrateOutputFormat,
    /// Show what would change without modifying files.
    pub dry_run: bool,
    /// Only add these sections (comma-separated).
    pub sections: Option<String>,
    /// Confirm each file before modifying.
    pub interactive: bool,
    /// Create .bak files before modifying (default: true).
    pub backup: bool,
}

/// Output format for the migrate command.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MigrateOutputFormat {
    #[default]
    Text,
    Json,
}

/// A section that needs to be added to a document.
#[derive(Debug, Clone, Serialize)]
pub struct MissingSection {
    /// Name of the section.
    pub name: String,
    /// Placeholder content to insert.
    pub placeholder: String,
}

/// Analysis result for a single file.
#[derive(Debug, Clone, Serialize)]
pub struct FileAnalysis {
    /// Path to the file (relative).
    pub path: PathBuf,
    /// Detected document type.
    pub doc_type: String,
    /// Sections that need to be added.
    pub missing_sections: Vec<MissingSection>,
}

/// Status of a file after migration.
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum MigrationStatus {
    /// File was modified successfully.
    Modified,
    /// File was skipped (no changes needed or user declined).
    Skipped,
    /// An error occurred while processing the file.
    Failed,
}

/// Result for a single file after migration.
#[derive(Debug, Clone, Serialize)]
pub struct FileResult {
    /// Path to the file (relative).
    pub path: PathBuf,
    /// Status of the migration.
    pub status: MigrationStatus,
    /// Message describing what happened.
    pub message: String,
    /// Sections that were added.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub sections_added: Vec<String>,
    /// Backup path if created.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub backup_path: Option<PathBuf>,
}

/// Complete migration report.
#[derive(Debug, Clone, Serialize)]
pub struct MigrationReport {
    /// Number of files scanned.
    pub files_scanned: usize,
    /// Number of files modified.
    pub files_modified: usize,
    /// Number of files skipped.
    pub files_skipped: usize,
    /// Number of files that failed.
    pub files_failed: usize,
    /// Whether this was a dry run.
    pub dry_run: bool,
    /// Results for each file.
    pub files: Vec<FileResult>,
}

/// Standard sections for different document types.
fn get_required_sections(doc_type: DocType) -> Vec<(&'static str, &'static str)> {
    match doc_type {
        DocType::Component => vec![
            (
                "Purpose",
                "<!-- TODO: Describe the purpose of this component -->",
            ),
            (
                "Interface",
                "<!-- TODO: Describe the interface or configuration options -->",
            ),
            (
                "Verification",
                "<!-- TODO: Add verification commands -->\n\n```bash\n# Add verification command here\n```",
            ),
            (
                "Examples",
                "<!-- TODO: Add usage examples -->\n\n```bash\n# Add example here\n```",
            ),
        ],
        DocType::Runbook => vec![
            (
                "Purpose",
                "<!-- TODO: Describe the purpose of this runbook -->",
            ),
            (
                "When to Use",
                "<!-- TODO: Describe when this runbook should be used -->",
            ),
            ("Steps", "<!-- TODO: Add the steps for this runbook -->"),
            ("Rollback", "<!-- TODO: Add rollback instructions -->"),
            (
                "Verification",
                "<!-- TODO: Add verification commands -->\n\n```bash\n# Add verification command here\n```",
            ),
        ],
        DocType::Adr => vec![
            ("Purpose", "<!-- TODO: Describe the purpose of this ADR -->"),
            (
                "Status",
                "Proposed\n\n<!-- TODO: Update status to Accepted/Deprecated/Superseded as needed -->",
            ),
            (
                "Context",
                "<!-- TODO: Describe the context and problem statement -->",
            ),
            (
                "Decision",
                "<!-- TODO: Describe the decision and rationale -->",
            ),
            (
                "Consequences",
                "<!-- TODO: Describe the consequences of this decision -->",
            ),
        ],
        DocType::Other => vec![
            (
                "Purpose",
                "<!-- TODO: Describe the purpose of this document -->",
            ),
            (
                "Verification",
                "<!-- TODO: Add verification commands -->\n\n```bash\n# Add verification command here\n```",
            ),
            (
                "Examples",
                "<!-- TODO: Add usage examples -->\n\n```bash\n# Add example here\n```",
            ),
        ],
    }
}

/// Section ordering for insertion (lower number = earlier in document).
fn section_order(name: &str) -> usize {
    match name.to_lowercase().as_str() {
        "purpose" => 1,
        "status" => 2,        // ADR
        "context" => 3,       // ADR
        "decision" => 4,      // ADR
        "consequences" => 5,  // ADR
        "interface" => 6,     // Component
        "configuration" => 7, // Component
        "when to use" => 8,   // Runbook
        "preconditions" => 9, // Runbook
        "steps" => 10,        // Runbook
        "rollback" => 11,     // Runbook
        "verification" => 90,
        "examples" => 95,
        _ => 50,
    }
}

/// Find the config file by walking up the directory tree.
fn find_config() -> Result<PathBuf> {
    let current_dir = env::current_dir()?;
    let mut dir = current_dir.as_path();

    loop {
        let config_path = dir.join(CONFIG_FILENAME);
        if config_path.exists() {
            return Ok(config_path);
        }

        match dir.parent() {
            Some(parent) => dir = parent,
            None => anyhow::bail!(
                "No {} found in current directory or any parent directory",
                CONFIG_FILENAME
            ),
        }
    }
}

/// Recursively find markdown files in a directory.
fn find_markdown_files(paths: &[PathBuf]) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();

    for path in paths {
        if path.is_file() && path.extension().is_some_and(|ext| ext == "md") {
            files.push(path.clone());
        } else if path.is_dir() {
            collect_markdown_files_recursive(path, &mut files)?;
        }
    }

    files.sort();
    Ok(files)
}

/// Recursively collect markdown files from a directory.
fn collect_markdown_files_recursive(dir: &Path, files: &mut Vec<PathBuf>) -> Result<()> {
    let entries = fs::read_dir(dir)
        .with_context(|| format!("failed to read directory: {}", dir.display()))?;

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
            collect_markdown_files_recursive(&path, files)?;
        } else if path.extension().is_some_and(|ext| ext == "md") {
            files.push(path);
        }
    }

    Ok(())
}

/// Analyze a file to determine which sections are missing.
fn analyze_file(
    path: &Path,
    docs_root: &Path,
    filter_sections: &Option<HashSet<String>>,
) -> Result<Option<FileAnalysis>> {
    let content = fs::read_to_string(path)
        .with_context(|| format!("failed to read file: {}", path.display()))?;

    let relative_path = path.strip_prefix(docs_root).unwrap_or(path).to_path_buf();

    // Parse the document
    let doc = ParsedDoc::parse_content(path.to_path_buf(), &content)?;

    // Detect document type
    let doc_type = detect_doc_type(&relative_path, &content);
    let doc_type_str = match doc_type {
        DocType::Component => "component",
        DocType::Runbook => "runbook",
        DocType::Adr => "adr",
        DocType::Other => "other",
    }
    .to_string();

    // Get required sections for this document type
    let required_sections = get_required_sections(doc_type);

    // Find missing sections
    let mut missing_sections = Vec::new();
    for (name, placeholder) in required_sections {
        // Skip if not in filter set
        if let Some(filter) = filter_sections
            && !filter.contains(&name.to_lowercase())
        {
            continue;
        }

        if !doc.has_section(name) {
            missing_sections.push(MissingSection {
                name: name.to_string(),
                placeholder: placeholder.to_string(),
            });
        }
    }

    // If no sections are missing, skip this file
    if missing_sections.is_empty() {
        return Ok(None);
    }

    Ok(Some(FileAnalysis {
        path: relative_path,
        doc_type: doc_type_str,
        missing_sections,
    }))
}

/// Insert missing sections into a document.
fn insert_sections(content: &str, missing_sections: &[MissingSection]) -> String {
    let mut lines: Vec<String> = content.lines().map(|s| s.to_string()).collect();

    // Sort sections by order to insert them in the right position
    let mut sections_to_insert: Vec<_> = missing_sections.iter().collect();
    sections_to_insert.sort_by_key(|s| section_order(&s.name));

    // For each section to insert, find the best position
    for section in sections_to_insert {
        let insert_pos = find_insertion_position(&lines, &section.name);

        // Build the section content
        let section_content = format!("\n## {}\n\n{}\n", section.name, section.placeholder);

        // Insert at the position
        let section_lines: Vec<String> = section_content.lines().map(|s| s.to_string()).collect();
        for (i, line) in section_lines.into_iter().enumerate() {
            lines.insert(insert_pos + i, line);
        }
    }

    // Ensure file ends with a newline
    let result = lines.join("\n");
    if result.ends_with('\n') {
        result
    } else {
        result + "\n"
    }
}

/// Find the best position to insert a new section.
fn find_insertion_position(lines: &[String], section_name: &str) -> usize {
    let target_order = section_order(section_name);

    // Find all existing H2 sections and their orders
    let mut sections: Vec<(usize, usize)> = Vec::new(); // (line_idx, order)
    let mut tracker = CodeBlockTracker::new();

    for (idx, line) in lines.iter().enumerate() {
        let trimmed = line.trim();

        // Track code block state (handles language tags and nested fences)
        tracker.process_line(trimmed);

        // Skip headings inside code blocks
        if tracker.in_code_block() {
            continue;
        }

        if let Some(heading) = trimmed.strip_prefix("## ")
            && !heading.starts_with('#')
        {
            let order = section_order(heading.trim());
            sections.push((idx, order));
        }
    }

    // Find the right position based on ordering
    // Insert before the first section with a higher order
    for (line_idx, order) in &sections {
        if *order > target_order {
            return *line_idx;
        }
    }

    // If no section has a higher order, insert at the end
    // But before any trailing whitespace
    let mut insert_at = lines.len();
    while insert_at > 0 && lines[insert_at - 1].trim().is_empty() {
        insert_at -= 1;
    }

    insert_at
}

/// Create a backup of the file.
fn create_backup(path: &Path) -> Result<PathBuf> {
    let backup_path = path.with_extension("md.bak");
    fs::copy(path, &backup_path)
        .with_context(|| format!("failed to create backup of {}", path.display()))?;
    Ok(backup_path)
}

/// Prompt user for confirmation in interactive mode.
fn prompt_user(file: &FileAnalysis) -> bool {
    use std::io::{self, Write};

    println!();
    println!("File: {}", file.path.display());
    println!("Type: {}", file.doc_type);
    println!("Missing sections:");
    for section in &file.missing_sections {
        println!("  + {}", section.name);
    }
    print!("Apply changes? [y/N] ");
    io::stdout().flush().ok();

    let mut input = String::new();
    if io::stdin().read_line(&mut input).is_ok() {
        let input = input.trim().to_lowercase();
        return input == "y" || input == "yes";
    }

    false
}

/// Execute the migrate command.
pub fn execute(args: MigrateArgs) -> Result<()> {
    // Find and load config
    let config_path = find_config()?;
    let config = PaveConfig::load(&config_path)?;
    let config_dir = config_path.parent().unwrap_or_else(|| Path::new("."));

    // Determine paths to process
    let paths = if let Some(path) = args.path {
        vec![path]
    } else {
        vec![config_dir.join(&config.docs.root)]
    };

    // Find markdown files
    let files = find_markdown_files(&paths)?;
    let docs_root = config_dir.join(&config.docs.root);

    // Parse sections filter
    let filter_sections: Option<HashSet<String>> = args
        .sections
        .map(|s| s.split(',').map(|s| s.trim().to_lowercase()).collect());

    // Analyze files
    let mut analyses: Vec<FileAnalysis> = Vec::new();
    for file in &files {
        if let Some(analysis) = analyze_file(file, &docs_root, &filter_sections)? {
            analyses.push(analysis);
        }
    }

    // Build report
    let mut report = MigrationReport {
        files_scanned: files.len(),
        files_modified: 0,
        files_skipped: 0,
        files_failed: 0,
        dry_run: args.dry_run,
        files: Vec::new(),
    };

    // If dry run, just report what would happen
    if args.dry_run {
        for analysis in &analyses {
            report.files.push(FileResult {
                path: analysis.path.clone(),
                status: MigrationStatus::Skipped,
                message: "would be modified".to_string(),
                sections_added: analysis
                    .missing_sections
                    .iter()
                    .map(|s| s.name.clone())
                    .collect(),
                backup_path: None,
            });
            report.files_skipped += 1;
        }

        output_report(&report, args.format, args.dry_run);
        return Ok(());
    }

    // Process files
    for analysis in analyses {
        let full_path = docs_root.join(&analysis.path);

        // Interactive mode: prompt for each file
        if args.interactive && !prompt_user(&analysis) {
            report.files.push(FileResult {
                path: analysis.path.clone(),
                status: MigrationStatus::Skipped,
                message: "user declined".to_string(),
                sections_added: Vec::new(),
                backup_path: None,
            });
            report.files_skipped += 1;
            continue;
        }

        // Read current content
        let content = match fs::read_to_string(&full_path) {
            Ok(c) => c,
            Err(e) => {
                report.files.push(FileResult {
                    path: analysis.path.clone(),
                    status: MigrationStatus::Failed,
                    message: format!("failed to read file: {}", e),
                    sections_added: Vec::new(),
                    backup_path: None,
                });
                report.files_failed += 1;
                continue;
            }
        };

        // Create backup if requested
        let backup_path = if args.backup {
            match create_backup(&full_path) {
                Ok(p) => Some(p.strip_prefix(&docs_root).unwrap_or(&p).to_path_buf()),
                Err(e) => {
                    report.files.push(FileResult {
                        path: analysis.path.clone(),
                        status: MigrationStatus::Failed,
                        message: format!("failed to create backup: {}", e),
                        sections_added: Vec::new(),
                        backup_path: None,
                    });
                    report.files_failed += 1;
                    continue;
                }
            }
        } else {
            None
        };

        // Insert missing sections
        let new_content = insert_sections(&content, &analysis.missing_sections);

        // Write back
        match fs::write(&full_path, &new_content) {
            Ok(()) => {
                report.files.push(FileResult {
                    path: analysis.path.clone(),
                    status: MigrationStatus::Modified,
                    message: format!("added {} section(s)", analysis.missing_sections.len()),
                    sections_added: analysis
                        .missing_sections
                        .iter()
                        .map(|s| s.name.clone())
                        .collect(),
                    backup_path,
                });
                report.files_modified += 1;
            }
            Err(e) => {
                report.files.push(FileResult {
                    path: analysis.path.clone(),
                    status: MigrationStatus::Failed,
                    message: format!("failed to write file: {}", e),
                    sections_added: Vec::new(),
                    backup_path,
                });
                report.files_failed += 1;
            }
        }
    }

    output_report(&report, args.format, args.dry_run);

    Ok(())
}

/// Output the migration report.
fn output_report(report: &MigrationReport, format: MigrateOutputFormat, dry_run: bool) {
    match format {
        MigrateOutputFormat::Json => {
            println!("{}", serde_json::to_string_pretty(report).unwrap());
        }
        MigrateOutputFormat::Text => {
            output_text_report(report, dry_run);
        }
    }
}

/// Output the report in text format.
fn output_text_report(report: &MigrationReport, dry_run: bool) {
    if dry_run {
        if report.files.is_empty() {
            println!("No files need migration.");
            return;
        }

        println!("Would modify {} file(s):", report.files.len());
        println!();

        for file in &report.files {
            println!("{}", file.path.display());
            for section in &file.sections_added {
                println!("  + Add ## {} section", section);
            }
            println!();
        }

        println!("Run without --dry-run to apply changes.");
    } else {
        if report.files_modified == 0 && report.files_failed == 0 {
            println!("No files needed migration.");
            return;
        }

        println!(
            "Migration complete: {} modified, {} skipped, {} failed",
            report.files_modified, report.files_skipped, report.files_failed
        );
        println!();

        // Show modified files
        let modified: Vec<_> = report
            .files
            .iter()
            .filter(|f| f.status == MigrationStatus::Modified)
            .collect();
        if !modified.is_empty() {
            println!("Modified files:");
            for file in modified {
                println!("  {} - {}", file.path.display(), file.message);
                if let Some(backup) = &file.backup_path {
                    println!("    Backup: {}", backup.display());
                }
            }
        }

        // Show failed files
        let failed: Vec<_> = report
            .files
            .iter()
            .filter(|f| f.status == MigrationStatus::Failed)
            .collect();
        if !failed.is_empty() {
            println!();
            println!("Failed files:");
            for file in failed {
                println!("  {} - {}", file.path.display(), file.message);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn create_test_config(temp_dir: &TempDir) -> PathBuf {
        let config_content = r#"
[pave]
version = "0.1"

[docs]
root = "docs"
"#;
        let config_path = temp_dir.path().join(".pave.toml");
        fs::write(&config_path, config_content).unwrap();
        config_path
    }

    fn create_test_doc(temp_dir: &TempDir, path: &str, content: &str) -> PathBuf {
        let full_path = temp_dir.path().join(path);
        if let Some(parent) = full_path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(&full_path, content).unwrap();
        full_path
    }

    #[test]
    fn test_section_order() {
        assert!(section_order("Purpose") < section_order("Verification"));
        assert!(section_order("Verification") < section_order("Examples"));
        assert!(section_order("Status") < section_order("Context"));
        assert!(section_order("Context") < section_order("Decision"));
    }

    #[test]
    fn test_find_insertion_position_empty_sections() {
        let lines: Vec<String> = vec![
            "# Title".to_string(),
            "".to_string(),
            "Some content.".to_string(),
        ];

        let pos = find_insertion_position(&lines, "Purpose");
        assert_eq!(pos, 3);
    }

    #[test]
    fn test_find_insertion_position_with_existing_sections() {
        let lines: Vec<String> = vec![
            "# Title".to_string(),
            "".to_string(),
            "## Purpose".to_string(),
            "Content".to_string(),
            "".to_string(),
            "## Examples".to_string(),
            "Example content".to_string(),
        ];

        // Verification should go before Examples
        let pos = find_insertion_position(&lines, "Verification");
        assert_eq!(pos, 5);
    }

    #[test]
    fn test_find_insertion_position_at_end() {
        let lines: Vec<String> = vec![
            "# Title".to_string(),
            "".to_string(),
            "## Purpose".to_string(),
            "Content".to_string(),
        ];

        // Examples should go at the end
        let pos = find_insertion_position(&lines, "Examples");
        assert_eq!(pos, 4);
    }

    #[test]
    fn test_insert_sections_basic() {
        let content = "# Title\n\n## Purpose\nContent here.\n";
        let missing = vec![MissingSection {
            name: "Verification".to_string(),
            placeholder: "<!-- TODO: Add verification -->".to_string(),
        }];

        let result = insert_sections(content, &missing);
        assert!(result.contains("## Verification"));
        assert!(result.contains("<!-- TODO: Add verification -->"));
    }

    #[test]
    fn test_insert_sections_respects_order() {
        let content = "# Title\n\n## Examples\nExample content.\n";
        let missing = vec![
            MissingSection {
                name: "Purpose".to_string(),
                placeholder: "<!-- TODO: Purpose -->".to_string(),
            },
            MissingSection {
                name: "Verification".to_string(),
                placeholder: "<!-- TODO: Verification -->".to_string(),
            },
        ];

        let result = insert_sections(content, &missing);

        // Purpose should come before Examples
        let purpose_pos = result.find("## Purpose").unwrap();
        let examples_pos = result.find("## Examples").unwrap();
        let verification_pos = result.find("## Verification").unwrap();

        assert!(purpose_pos < verification_pos);
        assert!(verification_pos < examples_pos);
    }

    #[test]
    fn test_get_required_sections_component() {
        let sections = get_required_sections(DocType::Component);
        let names: Vec<_> = sections.iter().map(|(n, _)| *n).collect();

        assert!(names.contains(&"Purpose"));
        assert!(names.contains(&"Interface"));
        assert!(names.contains(&"Verification"));
        assert!(names.contains(&"Examples"));
    }

    #[test]
    fn test_get_required_sections_runbook() {
        let sections = get_required_sections(DocType::Runbook);
        let names: Vec<_> = sections.iter().map(|(n, _)| *n).collect();

        assert!(names.contains(&"Purpose"));
        assert!(names.contains(&"When to Use"));
        assert!(names.contains(&"Steps"));
        assert!(names.contains(&"Rollback"));
        assert!(names.contains(&"Verification"));
    }

    #[test]
    fn test_get_required_sections_adr() {
        let sections = get_required_sections(DocType::Adr);
        let names: Vec<_> = sections.iter().map(|(n, _)| *n).collect();

        assert!(names.contains(&"Purpose"));
        assert!(names.contains(&"Status"));
        assert!(names.contains(&"Context"));
        assert!(names.contains(&"Decision"));
        assert!(names.contains(&"Consequences"));
    }

    #[test]
    fn test_analyze_file_finds_missing_sections() {
        let temp_dir = TempDir::new().unwrap();
        create_test_config(&temp_dir);

        // Put the file in a "components" directory to ensure it's detected as a component type
        let content = "# Auth Component\n\n## Purpose\nThis is a test.\n";
        let path = create_test_doc(&temp_dir, "docs/components/auth.md", content);

        let analysis = analyze_file(&path, temp_dir.path().join("docs").as_path(), &None)
            .unwrap()
            .unwrap();

        // Should be missing Interface, Verification, Examples (for component type)
        let missing_names: Vec<_> = analysis
            .missing_sections
            .iter()
            .map(|s| s.name.as_str())
            .collect();
        assert!(missing_names.contains(&"Interface"));
        assert!(missing_names.contains(&"Verification"));
        assert!(missing_names.contains(&"Examples"));
    }

    #[test]
    fn test_analyze_file_with_section_filter() {
        let temp_dir = TempDir::new().unwrap();
        create_test_config(&temp_dir);

        // Put the file in a "components" directory to ensure it's detected as a component type
        let content = "# Auth Component\n\n## Purpose\nThis is a test.\n";
        let path = create_test_doc(&temp_dir, "docs/components/auth.md", content);

        let filter: HashSet<String> = vec!["verification".to_string()].into_iter().collect();
        let analysis = analyze_file(&path, temp_dir.path().join("docs").as_path(), &Some(filter))
            .unwrap()
            .unwrap();

        // Should only be missing Verification (filtered)
        assert_eq!(analysis.missing_sections.len(), 1);
        assert_eq!(analysis.missing_sections[0].name, "Verification");
    }

    #[test]
    fn test_analyze_file_no_missing_sections() {
        let temp_dir = TempDir::new().unwrap();
        create_test_config(&temp_dir);

        let content = r#"# Component

## Purpose
This is a test.

## Interface
API here.

## Verification
```bash
cargo test
```

## Examples
```bash
example
```
"#;
        let path = create_test_doc(&temp_dir, "docs/test.md", content);

        let analysis = analyze_file(&path, temp_dir.path().join("docs").as_path(), &None).unwrap();

        // Should return None since no sections are missing
        assert!(analysis.is_none());
    }

    #[test]
    fn test_create_backup() {
        let temp_dir = TempDir::new().unwrap();
        let content = "# Test\n\nContent here.\n";
        let path = create_test_doc(&temp_dir, "test.md", content);

        let backup_path = create_backup(&path).unwrap();

        assert!(backup_path.exists());
        assert_eq!(backup_path.extension().unwrap(), "bak");
        assert_eq!(fs::read_to_string(&backup_path).unwrap(), content);
    }

    #[test]
    fn test_find_markdown_files() {
        let temp_dir = TempDir::new().unwrap();
        create_test_doc(&temp_dir, "docs/a.md", "# A");
        create_test_doc(&temp_dir, "docs/sub/b.md", "# B");
        create_test_doc(&temp_dir, "docs/node_modules/c.md", "# C");
        create_test_doc(&temp_dir, "docs/test.txt", "Not markdown");

        let files = find_markdown_files(&[temp_dir.path().join("docs")]).unwrap();

        assert_eq!(files.len(), 2);
        assert!(files.iter().any(|f| f.ends_with("a.md")));
        assert!(files.iter().any(|f| f.ends_with("b.md")));
        // c.md should be skipped (node_modules)
        // test.txt should be skipped (not .md)
    }

    #[test]
    fn test_migration_report_serialization() {
        let report = MigrationReport {
            files_scanned: 10,
            files_modified: 5,
            files_skipped: 3,
            files_failed: 2,
            dry_run: false,
            files: vec![FileResult {
                path: PathBuf::from("test.md"),
                status: MigrationStatus::Modified,
                message: "added 2 section(s)".to_string(),
                sections_added: vec!["Purpose".to_string(), "Verification".to_string()],
                backup_path: Some(PathBuf::from("test.md.bak")),
            }],
        };

        let json = serde_json::to_string(&report).unwrap();
        assert!(json.contains("\"files_scanned\":10"));
        assert!(json.contains("\"status\":\"modified\""));
    }

    #[test]
    fn test_insert_sections_skips_headings_in_code_blocks() {
        let content = r#"# Title

## Purpose
Content here.

```markdown
## Fake Section
This is inside a code block.
```

## Examples
Example content.
"#;
        let missing = vec![MissingSection {
            name: "Verification".to_string(),
            placeholder: "<!-- TODO: Verify -->".to_string(),
        }];

        let result = insert_sections(content, &missing);

        // Verification should be inserted between Purpose and Examples
        // NOT after the fake section inside the code block
        let verification_pos = result.find("## Verification").unwrap();
        let examples_pos = result.find("## Examples").unwrap();
        let purpose_pos = result.find("## Purpose").unwrap();

        assert!(verification_pos > purpose_pos);
        assert!(verification_pos < examples_pos);
    }
}
