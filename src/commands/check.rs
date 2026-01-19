//! Implementation of the `paver check` command for validating PAVED documents.

use anyhow::{Context, Result};
use serde::Serialize;
use std::env;
use std::path::{Path, PathBuf};

use crate::cli::OutputFormat;
use crate::config::{CONFIG_FILENAME, PaverConfig};
use crate::parser::ParsedDoc;

/// Arguments for the `paver check` command.
pub struct CheckArgs {
    /// Specific files or directories to check.
    pub paths: Vec<PathBuf>,
    /// Output format.
    pub format: OutputFormat,
    /// Treat warnings as errors.
    pub strict: bool,
}

/// Severity of a validation issue.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Error,
    Warning,
}

/// A validation issue found in a document.
#[derive(Debug, Clone, Serialize)]
pub struct Issue {
    /// Path to the file with the issue.
    pub file: PathBuf,
    /// Line number where the issue was found (1-indexed).
    pub line: usize,
    /// Severity of the issue.
    pub severity: Severity,
    /// Description of the issue.
    pub message: String,
    /// Hint for fixing the issue.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hint: Option<String>,
}

/// Results of checking documents.
#[derive(Debug, Serialize)]
pub struct CheckResults {
    /// Number of files checked.
    pub files_checked: usize,
    /// List of errors found.
    pub errors: Vec<Issue>,
    /// List of warnings found.
    pub warnings: Vec<Issue>,
}

impl CheckResults {
    fn new() -> Self {
        Self {
            files_checked: 0,
            errors: Vec::new(),
            warnings: Vec::new(),
        }
    }

    fn add_issue(&mut self, issue: Issue) {
        match issue.severity {
            Severity::Error => self.errors.push(issue),
            Severity::Warning => self.warnings.push(issue),
        }
    }

    /// Returns true if there are no errors (and no warnings if strict mode).
    fn is_success(&self, strict: bool) -> bool {
        if strict {
            self.errors.is_empty() && self.warnings.is_empty()
        } else {
            self.errors.is_empty()
        }
    }
}

/// Execute the `paver check` command.
pub fn execute(args: CheckArgs) -> Result<()> {
    // Find and load config
    let config_path = find_config()?;
    let config = PaverConfig::load(&config_path)?;

    // Determine paths to check
    let paths = if args.paths.is_empty() {
        // Use docs root from config, relative to config file location
        let config_dir = config_path.parent().unwrap_or_else(|| Path::new("."));
        vec![config_dir.join(&config.docs.root)]
    } else {
        args.paths.clone()
    };

    // Find all markdown files
    let files = find_markdown_files(&paths)?;

    if files.is_empty() {
        eprintln!("No markdown files found to check");
        return Ok(());
    }

    // Check each file
    let mut results = CheckResults::new();
    for file in &files {
        check_file(file, &config, &mut results)?;
    }
    results.files_checked = files.len();

    // Output results in the requested format
    match args.format {
        OutputFormat::Text => output_text(&results),
        OutputFormat::Json => output_json(&results)?,
        OutputFormat::Github => output_github(&results),
    }

    // Return error if checks failed
    if results.is_success(args.strict) {
        Ok(())
    } else {
        let error_count = results.errors.len();
        let warning_count = results.warnings.len();
        if args.strict && error_count == 0 {
            anyhow::bail!(
                "Check failed: {} warning{} (strict mode)",
                warning_count,
                if warning_count == 1 { "" } else { "s" }
            );
        } else {
            anyhow::bail!(
                "Check failed: {} error{}",
                error_count,
                if error_count == 1 { "" } else { "s" }
            );
        }
    }
}

/// Find the .paver.toml config file by walking up from the current directory.
fn find_config() -> Result<PathBuf> {
    let current_dir = env::current_dir().context("Failed to get current directory")?;
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

/// Find all markdown files in the given paths.
fn find_markdown_files(paths: &[PathBuf]) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();

    for path in paths {
        if path.is_file() {
            if path.extension().is_some_and(|ext| ext == "md") {
                files.push(path.clone());
            }
        } else if path.is_dir() {
            collect_markdown_files_recursive(path, &mut files)?;
        } else {
            anyhow::bail!("Path does not exist: {}", path.display());
        }
    }

    // Sort for consistent output
    files.sort();
    Ok(files)
}

/// Recursively collect markdown files from a directory.
fn collect_markdown_files_recursive(dir: &Path, files: &mut Vec<PathBuf>) -> Result<()> {
    let entries = std::fs::read_dir(dir)
        .with_context(|| format!("Failed to read directory: {}", dir.display()))?;

    for entry in entries {
        let entry = entry?;
        let path = entry.path();

        if path.is_dir() {
            collect_markdown_files_recursive(&path, files)?;
        } else if path.extension().is_some_and(|ext| ext == "md") {
            files.push(path);
        }
    }

    Ok(())
}

/// Check a single file against the validation rules.
fn check_file(path: &Path, config: &PaverConfig, results: &mut CheckResults) -> Result<()> {
    let doc = ParsedDoc::parse(path)?;

    // Check max lines
    if doc.line_count > config.rules.max_lines as usize {
        results.add_issue(Issue {
            file: path.to_path_buf(),
            line: doc.line_count,
            severity: Severity::Warning,
            message: format!(
                "Document exceeds {} line limit ({} lines)",
                config.rules.max_lines, doc.line_count
            ),
            hint: Some("Consider splitting into smaller, focused documents".to_string()),
        });
    }

    // Check for required Verification section
    if config.rules.require_verification && !doc.has_section("Verification") {
        results.add_issue(Issue {
            file: path.to_path_buf(),
            line: 1,
            severity: Severity::Error,
            message: "Missing required section 'Verification'".to_string(),
            hint: Some("Add a '## Verification' section with test commands".to_string()),
        });
    }

    // Check for required Examples section
    if config.rules.require_examples && !doc.has_section("Examples") {
        results.add_issue(Issue {
            file: path.to_path_buf(),
            line: 1,
            severity: Severity::Error,
            message: "Missing required section 'Examples'".to_string(),
            hint: Some("Add an '## Examples' section with concrete usage examples".to_string()),
        });
    }

    Ok(())
}

/// Output results in text format.
fn output_text(results: &CheckResults) {
    // Print all issues
    for issue in results.errors.iter().chain(results.warnings.iter()) {
        let severity = match issue.severity {
            Severity::Error => "error",
            Severity::Warning => "warning",
        };
        println!(
            "{}:{}: {}: {}",
            issue.file.display(),
            issue.line,
            severity,
            issue.message
        );
        if let Some(hint) = &issue.hint {
            println!("  hint: {}", hint);
        }
        println!();
    }

    // Print summary
    let error_count = results.errors.len();
    let warning_count = results.warnings.len();

    print!(
        "Checked {} document{}: ",
        results.files_checked,
        if results.files_checked == 1 { "" } else { "s" }
    );

    if error_count == 0 && warning_count == 0 {
        println!("all checks passed");
    } else {
        println!(
            "{} error{}, {} warning{}",
            error_count,
            if error_count == 1 { "" } else { "s" },
            warning_count,
            if warning_count == 1 { "" } else { "s" }
        );
    }
}

/// Output results in JSON format.
fn output_json(results: &CheckResults) -> Result<()> {
    let json = serde_json::to_string_pretty(results).context("Failed to serialize results")?;
    println!("{}", json);
    Ok(())
}

/// Output results in GitHub Actions annotation format.
fn output_github(results: &CheckResults) {
    for issue in results.errors.iter().chain(results.warnings.iter()) {
        let level = match issue.severity {
            Severity::Error => "error",
            Severity::Warning => "warning",
        };
        println!(
            "::{} file={},line={}::{}",
            level,
            issue.file.display(),
            issue.line,
            issue.message
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn create_test_config(temp_dir: &TempDir) -> PathBuf {
        let config_content = r#"
[paver]
version = "0.1"

[docs]
root = "docs"

[rules]
max_lines = 50
require_verification = true
require_examples = true
"#;
        let config_path = temp_dir.path().join(".paver.toml");
        fs::write(&config_path, config_content).unwrap();
        config_path
    }

    fn create_valid_doc(temp_dir: &TempDir, filename: &str) -> PathBuf {
        let docs_dir = temp_dir.path().join("docs");
        fs::create_dir_all(&docs_dir).unwrap();

        let content = r#"# Test Document

## Purpose
This is a test document.

## Verification
Run the tests:
```bash
$ cargo test
```

## Examples
Example usage here.
"#;
        let path = docs_dir.join(filename);
        fs::write(&path, content).unwrap();
        path
    }

    fn create_invalid_doc(temp_dir: &TempDir, filename: &str) -> PathBuf {
        let docs_dir = temp_dir.path().join("docs");
        fs::create_dir_all(&docs_dir).unwrap();

        let content = r#"# Incomplete Document

## Purpose
This document is missing required sections.
"#;
        let path = docs_dir.join(filename);
        fs::write(&path, content).unwrap();
        path
    }

    fn create_long_doc(temp_dir: &TempDir, filename: &str, lines: usize) -> PathBuf {
        let docs_dir = temp_dir.path().join("docs");
        fs::create_dir_all(&docs_dir).unwrap();

        let mut content =
            String::from("# Long Document\n\n## Verification\nTest\n\n## Examples\nExample\n");
        for i in 0..lines {
            content.push_str(&format!("Line {}\n", i));
        }

        let path = docs_dir.join(filename);
        fs::write(&path, content).unwrap();
        path
    }

    #[test]
    fn check_valid_document_passes() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = create_test_config(&temp_dir);
        let _doc_path = create_valid_doc(&temp_dir, "valid.md");

        let config = PaverConfig::load(&config_path).unwrap();
        let docs_dir = temp_dir.path().join("docs");
        let files = find_markdown_files(&[docs_dir]).unwrap();

        let mut results = CheckResults::new();
        for file in &files {
            check_file(file, &config, &mut results).unwrap();
        }

        assert!(results.errors.is_empty());
        assert!(results.warnings.is_empty());
    }

    #[test]
    fn check_missing_verification_reports_error() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = create_test_config(&temp_dir);
        let doc_path = create_invalid_doc(&temp_dir, "invalid.md");

        let config = PaverConfig::load(&config_path).unwrap();
        let mut results = CheckResults::new();
        check_file(&doc_path, &config, &mut results).unwrap();

        assert_eq!(results.errors.len(), 2); // Missing Verification and Examples
        assert!(
            results
                .errors
                .iter()
                .any(|e| e.message.contains("Verification"))
        );
        assert!(
            results
                .errors
                .iter()
                .any(|e| e.message.contains("Examples"))
        );
    }

    #[test]
    fn check_long_document_reports_warning() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = create_test_config(&temp_dir);
        // Config has max_lines = 50, so 100 lines should trigger warning
        let doc_path = create_long_doc(&temp_dir, "long.md", 100);

        let config = PaverConfig::load(&config_path).unwrap();
        let mut results = CheckResults::new();
        check_file(&doc_path, &config, &mut results).unwrap();

        assert!(
            results
                .warnings
                .iter()
                .any(|w| w.message.contains("line limit"))
        );
    }

    #[test]
    fn find_markdown_files_collects_recursively() {
        let temp_dir = TempDir::new().unwrap();
        let docs_dir = temp_dir.path().join("docs");
        let nested_dir = docs_dir.join("nested");
        fs::create_dir_all(&nested_dir).unwrap();

        fs::write(docs_dir.join("doc1.md"), "# Doc 1").unwrap();
        fs::write(nested_dir.join("doc2.md"), "# Doc 2").unwrap();
        fs::write(docs_dir.join("readme.txt"), "Not markdown").unwrap();

        let files = find_markdown_files(&[docs_dir]).unwrap();

        assert_eq!(files.len(), 2);
        assert!(files.iter().any(|f| f.file_name().unwrap() == "doc1.md"));
        assert!(files.iter().any(|f| f.file_name().unwrap() == "doc2.md"));
    }

    #[test]
    fn check_results_success_without_strict() {
        let mut results = CheckResults::new();
        results.add_issue(Issue {
            file: PathBuf::from("test.md"),
            line: 1,
            severity: Severity::Warning,
            message: "A warning".to_string(),
            hint: None,
        });

        assert!(results.is_success(false)); // Warnings OK without strict
        assert!(!results.is_success(true)); // Warnings fail with strict
    }

    #[test]
    fn check_results_fail_with_errors() {
        let mut results = CheckResults::new();
        results.add_issue(Issue {
            file: PathBuf::from("test.md"),
            line: 1,
            severity: Severity::Error,
            message: "An error".to_string(),
            hint: None,
        });

        assert!(!results.is_success(false));
        assert!(!results.is_success(true));
    }

    #[test]
    fn json_output_is_valid() {
        let mut results = CheckResults::new();
        results.files_checked = 1;
        results.add_issue(Issue {
            file: PathBuf::from("test.md"),
            line: 5,
            severity: Severity::Error,
            message: "Test error".to_string(),
            hint: Some("Fix it".to_string()),
        });

        let json = serde_json::to_string(&results).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed["files_checked"], 1);
        assert_eq!(parsed["errors"].as_array().unwrap().len(), 1);
        assert_eq!(parsed["errors"][0]["severity"], "error");
        assert_eq!(parsed["errors"][0]["message"], "Test error");
    }
}
