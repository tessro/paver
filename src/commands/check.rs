//! Implementation of the `pave check` command for validating PAVED documents.

use anyhow::{Context, Result};
use serde::Serialize;
use std::collections::HashSet;
use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::cli::OutputFormat;
use crate::config::{CONFIG_FILENAME, PaveConfig};
use crate::parser::ParsedDoc;
use crate::rules::{RulesEngine, detect_doc_type, get_type_specific_rules};

/// Arguments for the `pave check` command.
pub struct CheckArgs {
    /// Specific files or directories to check.
    pub paths: Vec<PathBuf>,
    /// Output format.
    pub format: OutputFormat,
    /// Treat warnings as errors (overrides gradual mode).
    pub strict: bool,
    /// Force gradual mode (treat errors as warnings, exit 0).
    pub gradual: bool,
    /// Only check docs changed since base ref.
    pub changed: bool,
    /// Base ref for --changed comparison.
    pub base: Option<String>,
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
    /// Whether this issue was converted from an error (in gradual mode).
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    #[serde(default)]
    pub converted_from_error: bool,
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
    /// Number of issues that would be errors outside gradual mode.
    /// Only populated when gradual mode is active.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub would_fail_count: Option<usize>,
}

impl CheckResults {
    fn new() -> Self {
        Self {
            files_checked: 0,
            errors: Vec::new(),
            warnings: Vec::new(),
            would_fail_count: None,
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

/// Check if the gradual deadline has passed.
/// Returns true if the deadline has passed (gradual mode should be disabled).
fn is_gradual_deadline_passed(deadline: &str) -> bool {
    // Parse the deadline date (YYYY-MM-DD format)
    let parts: Vec<&str> = deadline.split('-').collect();
    if parts.len() != 3 {
        eprintln!(
            "Warning: Invalid gradual_until format '{}'. Expected YYYY-MM-DD. Ignoring deadline.",
            deadline
        );
        return false;
    }

    let year: i32 = match parts[0].parse() {
        Ok(y) => y,
        Err(_) => {
            eprintln!(
                "Warning: Invalid year in gradual_until '{}'. Ignoring deadline.",
                deadline
            );
            return false;
        }
    };

    let month: u32 = match parts[1].parse() {
        Ok(m) if (1..=12).contains(&m) => m,
        _ => {
            eprintln!(
                "Warning: Invalid month in gradual_until '{}'. Ignoring deadline.",
                deadline
            );
            return false;
        }
    };

    let day: u32 = match parts[2].parse() {
        Ok(d) if (1..=31).contains(&d) => d,
        _ => {
            eprintln!(
                "Warning: Invalid day in gradual_until '{}'. Ignoring deadline.",
                deadline
            );
            return false;
        }
    };

    // Get current date using std::time
    // We need to compare dates, so we'll get the current date from SystemTime
    use std::time::{SystemTime, UNIX_EPOCH};
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    // Convert deadline to seconds since epoch (approximate, days since epoch * seconds per day)
    // Days from year 1970 to deadline year
    let mut deadline_days: i64 = 0;
    for y in 1970..year {
        deadline_days += if is_leap_year(y) { 366 } else { 365 };
    }

    // Add days for months in deadline year
    let days_in_month = [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    for m in 1..month {
        deadline_days += days_in_month[(m - 1) as usize] as i64;
        if m == 2 && is_leap_year(year) {
            deadline_days += 1;
        }
    }
    deadline_days += day as i64;

    let deadline_secs = deadline_days * 86400;
    let now_days = now as i64 / 86400;
    let deadline_days_from_epoch = deadline_secs / 86400;

    now_days > deadline_days_from_epoch
}

fn is_leap_year(year: i32) -> bool {
    (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0)
}

/// Determine if gradual mode is active based on config, CLI flags, and deadline.
fn is_gradual_mode_active(config: &PaveConfig, args: &CheckArgs) -> bool {
    // --strict always disables gradual mode
    if args.strict {
        return false;
    }

    // --gradual flag always enables gradual mode
    if args.gradual {
        return true;
    }

    // Check config
    if config.rules.gradual {
        // Check deadline if specified
        if let Some(ref deadline) = config.rules.gradual_until
            && is_gradual_deadline_passed(deadline)
        {
            eprintln!(
                "Note: Gradual mode deadline ({}) has passed. Running in strict mode.",
                deadline
            );
            return false;
        }
        return true;
    }

    false
}

/// Execute the `pave check` command.
pub fn execute(args: CheckArgs) -> Result<()> {
    // Find and load config
    let config_path = find_config()?;
    let config = PaveConfig::load(&config_path)?;
    let config_dir = config_path.parent().unwrap_or_else(|| Path::new("."));

    // Determine paths to check
    let paths = if args.paths.is_empty() {
        // Use docs root from config, relative to config file location
        vec![config_dir.join(&config.docs.root)]
    } else {
        args.paths.clone()
    };

    // Find all markdown files
    let mut files = find_markdown_files(&paths)?;

    // Filter to only changed files if --changed flag is set
    if args.changed {
        let base_ref = determine_base_ref(args.base.as_deref())?;
        let changed_files = get_changed_md_files(&base_ref, config_dir)?;

        if changed_files.is_empty() {
            eprintln!("No changed markdown files found compared to {}", base_ref);
            return Ok(());
        }

        // Filter files to only include those that changed
        files.retain(|f| {
            // Normalize path for comparison
            let relative = f.strip_prefix(config_dir).unwrap_or(f).to_path_buf();
            changed_files.contains(&relative) || changed_files.contains(f)
        });
    }

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

    // Determine if gradual mode is active
    let gradual_mode = is_gradual_mode_active(&config, &args);

    // In gradual mode, convert errors to warnings
    if gradual_mode && !results.errors.is_empty() {
        let error_count = results.errors.len();
        results.would_fail_count = Some(error_count);

        // Convert all errors to warnings and mark them as converted
        for mut error in results.errors.drain(..) {
            error.severity = Severity::Warning;
            error.converted_from_error = true;
            results.warnings.push(error);
        }
    }

    // Output results in the requested format
    match args.format {
        OutputFormat::Text => output_text(&results, gradual_mode),
        OutputFormat::Json => output_json(&results)?,
        OutputFormat::Github => output_github(&results, gradual_mode),
    }

    // Return error if checks failed
    // In gradual mode, always return success (exit 0)
    if gradual_mode || results.is_success(args.strict) {
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

/// Find the .pave.toml config file by walking up from the current directory.
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

/// Determine the base ref to compare against.
fn determine_base_ref(explicit_base: Option<&str>) -> Result<String> {
    if let Some(base) = explicit_base {
        return Ok(base.to_string());
    }

    // Try origin/main first
    if ref_exists("origin/main") {
        return Ok("origin/main".to_string());
    }

    // Try origin/master
    if ref_exists("origin/master") {
        return Ok("origin/master".to_string());
    }

    // Fall back to HEAD~1
    Ok("HEAD~1".to_string())
}

/// Check if a git ref exists.
fn ref_exists(ref_name: &str) -> bool {
    Command::new("git")
        .args(["rev-parse", "--verify", "--quiet", ref_name])
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

/// Get the list of changed markdown files from git diff.
fn get_changed_md_files(base_ref: &str, config_dir: &Path) -> Result<HashSet<PathBuf>> {
    let output = Command::new("git")
        .args(["diff", "--name-only", &format!("{}..HEAD", base_ref)])
        .current_dir(config_dir)
        .output()
        .context("Failed to run git diff")?;

    if !output.status.success() {
        // Try without ..HEAD for cases like HEAD~1
        let output = Command::new("git")
            .args(["diff", "--name-only", base_ref])
            .current_dir(config_dir)
            .output()
            .context("Failed to run git diff")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("git diff failed: {}", stderr);
        }

        return parse_changed_md_files(&output.stdout);
    }

    parse_changed_md_files(&output.stdout)
}

/// Parse git diff --name-only output into a set of markdown file paths.
fn parse_changed_md_files(output: &[u8]) -> Result<HashSet<PathBuf>> {
    let stdout = String::from_utf8_lossy(output);
    let files: HashSet<PathBuf> = stdout
        .lines()
        .filter(|line| !line.is_empty())
        .filter(|line| line.ends_with(".md"))
        .map(PathBuf::from)
        .collect();
    Ok(files)
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
fn check_file(path: &Path, config: &PaveConfig, results: &mut CheckResults) -> Result<()> {
    // Skip validation of index.md files - they are navigation documents
    // that don't need Verification and Examples sections
    if path.file_name().is_some_and(|f| f == "index.md") {
        return Ok(());
    }

    // Skip template files - they are scaffolds, not actual documentation
    let path_str = path.to_string_lossy();
    if path_str.contains("/templates/") || path_str.contains("\\templates\\") {
        return Ok(());
    }

    // Read file content once for parsing and type detection
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("Failed to read file: {}", path.display()))?;
    let doc = ParsedDoc::parse_content(path.to_path_buf(), &content)?;

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
            converted_from_error: false,
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
            converted_from_error: false,
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
            converted_from_error: false,
        });
    }

    // Apply document-type-specific validation rules
    let doc_type = detect_doc_type(path, &content);
    let type_rules = get_type_specific_rules(doc_type, &config.rules);

    if !type_rules.is_empty() {
        let engine = RulesEngine::new(type_rules);
        let validation_result = engine.validate(&doc);

        for error in validation_result.errors {
            results.add_issue(Issue {
                file: path.to_path_buf(),
                line: error.line.unwrap_or(1),
                severity: Severity::Error,
                message: error.message,
                hint: error.suggestion,
                converted_from_error: false,
            });
        }

        for warning in validation_result.warnings {
            results.add_issue(Issue {
                file: path.to_path_buf(),
                line: warning.line.unwrap_or(1),
                severity: Severity::Warning,
                message: warning.message,
                hint: None,
                converted_from_error: false,
            });
        }
    }

    Ok(())
}

/// Output results in text format.
fn output_text(results: &CheckResults, gradual_mode: bool) {
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
        // Show note only for issues converted from errors in gradual mode
        if issue.converted_from_error {
            println!("  note: This would be an error outside gradual mode");
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
    } else if gradual_mode {
        println!(
            "{} error{}, {} warning{} (gradual mode active)",
            error_count,
            if error_count == 1 { "" } else { "s" },
            warning_count,
            if warning_count == 1 { "" } else { "s" }
        );
    } else {
        println!(
            "{} error{}, {} warning{}",
            error_count,
            if error_count == 1 { "" } else { "s" },
            warning_count,
            if warning_count == 1 { "" } else { "s" }
        );
    }

    // In gradual mode, show how many issues would fail in strict mode
    if let Some(would_fail) = results.would_fail_count {
        println!(
            "Note: {} issue{} would fail in strict mode. Run 'pave check --strict' to see.",
            would_fail,
            if would_fail == 1 { "" } else { "s" }
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
fn output_github(results: &CheckResults, _gradual_mode: bool) {
    for issue in results.errors.iter().chain(results.warnings.iter()) {
        let level = match issue.severity {
            Severity::Error => "error",
            Severity::Warning => "warning",
        };
        let message = if issue.converted_from_error {
            format!("{} (would be error outside gradual mode)", issue.message)
        } else {
            issue.message.clone()
        };
        println!(
            "::{} file={},line={}::{}",
            level,
            issue.file.display(),
            issue.line,
            message
        );
    }

    // Print summary notice in gradual mode
    if let Some(would_fail) = results.would_fail_count {
        println!(
            "::notice::Gradual mode active: {} issue(s) would fail in strict mode",
            would_fail
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
[pave]
version = "0.1"

[docs]
root = "docs"

[rules]
max_lines = 50
require_verification = true
require_examples = true
"#;
        let config_path = temp_dir.path().join(".pave.toml");
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

        let config = PaveConfig::load(&config_path).unwrap();
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

        let config = PaveConfig::load(&config_path).unwrap();
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

        let config = PaveConfig::load(&config_path).unwrap();
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
            converted_from_error: false,
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
            converted_from_error: false,
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
            converted_from_error: false,
        });

        let json = serde_json::to_string(&results).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed["files_checked"], 1);
        assert_eq!(parsed["errors"].as_array().unwrap().len(), 1);
        assert_eq!(parsed["errors"][0]["severity"], "error");
        assert_eq!(parsed["errors"][0]["message"], "Test error");
    }

    #[test]
    fn check_skips_index_md_files() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = create_test_config(&temp_dir);
        let docs_dir = temp_dir.path().join("docs");
        fs::create_dir_all(&docs_dir).unwrap();

        // Create an index.md without required sections
        let index_content = "# Documentation Index\n\nJust navigation links here.\n";
        fs::write(docs_dir.join("index.md"), index_content).unwrap();

        let config = PaveConfig::load(&config_path).unwrap();
        let mut results = CheckResults::new();
        check_file(&docs_dir.join("index.md"), &config, &mut results).unwrap();

        // index.md should be skipped - no errors reported
        assert!(results.errors.is_empty());
        assert!(results.warnings.is_empty());
    }

    #[test]
    fn check_skips_template_files() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = create_test_config(&temp_dir);
        let templates_dir = temp_dir.path().join("docs").join("templates");
        fs::create_dir_all(&templates_dir).unwrap();

        // Create a template without required sections
        let template_content = "# {Component Name}\n\n## Purpose\n\nDescribe here.\n";
        fs::write(templates_dir.join("component.md"), template_content).unwrap();

        let config = PaveConfig::load(&config_path).unwrap();
        let mut results = CheckResults::new();
        check_file(&templates_dir.join("component.md"), &config, &mut results).unwrap();

        // Template files should be skipped - no errors reported
        assert!(results.errors.is_empty());
        assert!(results.warnings.is_empty());
    }

    #[test]
    fn parse_changed_md_files_filters_to_markdown() {
        let output = b"src/cli.rs\ndocs/readme.md\nsrc/main.rs\ndocs/guide.md\n";
        let files = parse_changed_md_files(output).unwrap();

        assert_eq!(files.len(), 2);
        assert!(files.contains(&PathBuf::from("docs/readme.md")));
        assert!(files.contains(&PathBuf::from("docs/guide.md")));
    }

    #[test]
    fn parse_changed_md_files_empty_output() {
        let output = b"";
        let files = parse_changed_md_files(output).unwrap();
        assert!(files.is_empty());

        let output = b"\n\n";
        let files = parse_changed_md_files(output).unwrap();
        assert!(files.is_empty());
    }

    #[test]
    fn parse_changed_md_files_no_markdown() {
        let output = b"src/cli.rs\nsrc/main.rs\nCargo.toml\n";
        let files = parse_changed_md_files(output).unwrap();
        assert!(files.is_empty());
    }

    #[test]
    fn determine_base_ref_uses_explicit() {
        let result = determine_base_ref(Some("custom-branch")).unwrap();
        assert_eq!(result, "custom-branch");
    }

    fn create_test_config_with_type_rules(temp_dir: &TempDir) -> PathBuf {
        let config_content = r#"
[pave]
version = "0.1"

[docs]
root = "docs"

[rules]
max_lines = 300
require_verification = true
require_examples = true

[rules.type_specific]
runbooks = true
adrs = true
components = true
"#;
        let config_path = temp_dir.path().join(".pave.toml");
        fs::write(&config_path, config_content).unwrap();
        config_path
    }

    #[test]
    fn check_runbook_missing_required_sections() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = create_test_config_with_type_rules(&temp_dir);
        let runbooks_dir = temp_dir.path().join("docs").join("runbooks");
        fs::create_dir_all(&runbooks_dir).unwrap();

        let content = r#"# Runbook: Deploy

## Purpose
How to deploy.

## Verification
```bash
$ echo "ok"
```

## Examples
```bash
$ deploy.sh
```
"#;
        let doc_path = runbooks_dir.join("deploy.md");
        fs::write(&doc_path, content).unwrap();

        let config = PaveConfig::load(&config_path).unwrap();
        let mut results = CheckResults::new();
        check_file(&doc_path, &config, &mut results).unwrap();

        // Should fail because missing When to Use, Steps, Rollback
        assert!(!results.errors.is_empty());
        assert!(
            results
                .errors
                .iter()
                .any(|e| e.message.contains("When to Use"))
        );
        assert!(results.errors.iter().any(|e| e.message.contains("Steps")));
        assert!(
            results
                .errors
                .iter()
                .any(|e| e.message.contains("Rollback"))
        );
    }

    #[test]
    fn check_complete_runbook_passes() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = create_test_config_with_type_rules(&temp_dir);
        let runbooks_dir = temp_dir.path().join("docs").join("runbooks");
        fs::create_dir_all(&runbooks_dir).unwrap();

        let content = r#"# Runbook: Deploy

## Purpose
How to deploy.

## When to Use
When deploying the application.

## Steps
1. Build the app
2. Deploy

## Rollback
Revert the deployment.

## Verification
```bash
$ echo "ok"
```

## Examples
```bash
$ deploy.sh
```
"#;
        let doc_path = runbooks_dir.join("deploy.md");
        fs::write(&doc_path, content).unwrap();

        let config = PaveConfig::load(&config_path).unwrap();
        let mut results = CheckResults::new();
        check_file(&doc_path, &config, &mut results).unwrap();

        assert!(results.errors.is_empty(), "errors: {:?}", results.errors);
    }

    #[test]
    fn check_adr_missing_required_sections() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = create_test_config_with_type_rules(&temp_dir);
        let adr_dir = temp_dir.path().join("docs").join("adr");
        fs::create_dir_all(&adr_dir).unwrap();

        let content = r#"# ADR: Use Rust

## Purpose
We decided to use Rust.

## Verification
```bash
$ cargo --version
```

## Examples
```rust
fn main() {}
```
"#;
        let doc_path = adr_dir.join("001-use-rust.md");
        fs::write(&doc_path, content).unwrap();

        let config = PaveConfig::load(&config_path).unwrap();
        let mut results = CheckResults::new();
        check_file(&doc_path, &config, &mut results).unwrap();

        // Should fail because missing Status, Context, Decision, Consequences
        assert!(!results.errors.is_empty());
        assert!(results.errors.iter().any(|e| e.message.contains("Status")));
        assert!(results.errors.iter().any(|e| e.message.contains("Context")));
        assert!(
            results
                .errors
                .iter()
                .any(|e| e.message.contains("Decision"))
        );
        assert!(
            results
                .errors
                .iter()
                .any(|e| e.message.contains("Consequences"))
        );
    }

    #[test]
    fn check_complete_adr_passes() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = create_test_config_with_type_rules(&temp_dir);
        let adr_dir = temp_dir.path().join("docs").join("adr");
        fs::create_dir_all(&adr_dir).unwrap();

        let content = r#"# ADR: Use Rust

## Purpose
We decided to use Rust.

## Status
Accepted

## Context
We need a systems language.

## Decision
We chose Rust.

## Consequences
Better safety.

## Verification
```bash
$ cargo --version
```

## Examples
```rust
fn main() {}
```
"#;
        let doc_path = adr_dir.join("001-use-rust.md");
        fs::write(&doc_path, content).unwrap();

        let config = PaveConfig::load(&config_path).unwrap();
        let mut results = CheckResults::new();
        check_file(&doc_path, &config, &mut results).unwrap();

        assert!(results.errors.is_empty(), "errors: {:?}", results.errors);
    }

    #[test]
    fn check_adr_invalid_status() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = create_test_config_with_type_rules(&temp_dir);
        let adr_dir = temp_dir.path().join("docs").join("adr");
        fs::create_dir_all(&adr_dir).unwrap();

        let content = r#"# ADR: Use Rust

## Purpose
We decided to use Rust.

## Status
Unknown

## Context
We need a systems language.

## Decision
We chose Rust.

## Consequences
Better safety.

## Verification
```bash
$ cargo --version
```

## Examples
```rust
fn main() {}
```
"#;
        let doc_path = adr_dir.join("001-use-rust.md");
        fs::write(&doc_path, content).unwrap();

        let config = PaveConfig::load(&config_path).unwrap();
        let mut results = CheckResults::new();
        check_file(&doc_path, &config, &mut results).unwrap();

        // Should fail because of invalid status
        assert!(
            results
                .errors
                .iter()
                .any(|e| e.message.contains("valid status"))
        );
    }

    #[test]
    fn check_component_missing_interface_and_config() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = create_test_config_with_type_rules(&temp_dir);
        let components_dir = temp_dir.path().join("docs").join("components");
        fs::create_dir_all(&components_dir).unwrap();

        let content = r#"# Auth Component

## Purpose
Handles authentication.

## Verification
```bash
$ cargo test
```

## Examples
```rust
fn main() {}
```
"#;
        let doc_path = components_dir.join("auth.md");
        fs::write(&doc_path, content).unwrap();

        let config = PaveConfig::load(&config_path).unwrap();
        let mut results = CheckResults::new();
        check_file(&doc_path, &config, &mut results).unwrap();

        // Should fail because missing Interface OR Configuration
        assert!(
            results
                .errors
                .iter()
                .any(|e| e.message.contains("Interface"))
        );
    }

    #[test]
    fn check_component_with_interface_passes() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = create_test_config_with_type_rules(&temp_dir);
        let components_dir = temp_dir.path().join("docs").join("components");
        fs::create_dir_all(&components_dir).unwrap();

        let content = r#"# Auth Component

## Purpose
Handles authentication.

## Interface
The API endpoints.

## Verification
```bash
$ cargo test
```

## Examples
```rust
fn main() {}
```
"#;
        let doc_path = components_dir.join("auth.md");
        fs::write(&doc_path, content).unwrap();

        let config = PaveConfig::load(&config_path).unwrap();
        let mut results = CheckResults::new();
        check_file(&doc_path, &config, &mut results).unwrap();

        assert!(results.errors.is_empty(), "errors: {:?}", results.errors);
    }

    #[test]
    fn check_component_with_configuration_passes() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = create_test_config_with_type_rules(&temp_dir);
        let components_dir = temp_dir.path().join("docs").join("components");
        fs::create_dir_all(&components_dir).unwrap();

        let content = r#"# Auth Component

## Purpose
Handles authentication.

## Configuration
The config options.

## Verification
```bash
$ cargo test
```

## Examples
```rust
fn main() {}
```
"#;
        let doc_path = components_dir.join("auth.md");
        fs::write(&doc_path, content).unwrap();

        let config = PaveConfig::load(&config_path).unwrap();
        let mut results = CheckResults::new();
        check_file(&doc_path, &config, &mut results).unwrap();

        assert!(results.errors.is_empty(), "errors: {:?}", results.errors);
    }

    #[test]
    fn check_generic_doc_no_type_specific_rules() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = create_test_config_with_type_rules(&temp_dir);
        let docs_dir = temp_dir.path().join("docs");
        fs::create_dir_all(&docs_dir).unwrap();

        // A generic doc that doesn't match any specific type
        let content = r#"# General Guide

## Purpose
A general guide.

## Verification
```bash
$ echo "ok"
```

## Examples
```bash
$ run.sh
```
"#;
        let doc_path = docs_dir.join("guide.md");
        fs::write(&doc_path, content).unwrap();

        let config = PaveConfig::load(&config_path).unwrap();
        let mut results = CheckResults::new();
        check_file(&doc_path, &config, &mut results).unwrap();

        // Should pass - generic docs don't need type-specific sections
        assert!(results.errors.is_empty(), "errors: {:?}", results.errors);
    }

    fn create_test_config_with_gradual_mode(temp_dir: &TempDir, gradual: bool) -> PathBuf {
        let config_content = format!(
            r#"
[pave]
version = "0.1"

[docs]
root = "docs"

[rules]
max_lines = 50
require_verification = true
require_examples = true
gradual = {}
"#,
            gradual
        );
        let config_path = temp_dir.path().join(".pave.toml");
        fs::write(&config_path, config_content).unwrap();
        config_path
    }

    fn create_test_config_with_gradual_until(temp_dir: &TempDir, gradual_until: &str) -> PathBuf {
        let config_content = format!(
            r#"
[pave]
version = "0.1"

[docs]
root = "docs"

[rules]
max_lines = 50
require_verification = true
require_examples = true
gradual = true
gradual_until = "{}"
"#,
            gradual_until
        );
        let config_path = temp_dir.path().join(".pave.toml");
        fs::write(&config_path, config_content).unwrap();
        config_path
    }

    #[test]
    fn gradual_mode_converts_errors_to_warnings() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = create_test_config_with_gradual_mode(&temp_dir, true);
        let doc_path = create_invalid_doc(&temp_dir, "invalid.md");

        let config = PaveConfig::load(&config_path).unwrap();
        let mut results = CheckResults::new();
        check_file(&doc_path, &config, &mut results).unwrap();

        // Should have errors initially
        let error_count = results.errors.len();
        assert!(error_count > 0, "Expected errors for invalid doc");

        // Simulate gradual mode conversion
        let args = CheckArgs {
            paths: vec![],
            format: OutputFormat::Text,
            strict: false,
            gradual: false,
            changed: false,
            base: None,
        };

        assert!(is_gradual_mode_active(&config, &args));

        // Convert errors to warnings (simulating execute behavior)
        results.would_fail_count = Some(error_count);
        for mut error in results.errors.drain(..) {
            error.severity = Severity::Warning;
            results.warnings.push(error);
        }

        assert!(results.errors.is_empty());
        assert!(!results.warnings.is_empty());
        assert_eq!(results.would_fail_count, Some(error_count));
    }

    #[test]
    fn strict_flag_overrides_gradual_mode() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = create_test_config_with_gradual_mode(&temp_dir, true);

        let config = PaveConfig::load(&config_path).unwrap();
        let args = CheckArgs {
            paths: vec![],
            format: OutputFormat::Text,
            strict: true, // This should override gradual
            gradual: false,
            changed: false,
            base: None,
        };

        assert!(!is_gradual_mode_active(&config, &args));
    }

    #[test]
    fn gradual_flag_enables_gradual_mode() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = create_test_config_with_gradual_mode(&temp_dir, false); // Config has gradual=false

        let config = PaveConfig::load(&config_path).unwrap();
        let args = CheckArgs {
            paths: vec![],
            format: OutputFormat::Text,
            strict: false,
            gradual: true, // CLI flag should enable gradual mode
            changed: false,
            base: None,
        };

        assert!(is_gradual_mode_active(&config, &args));
    }

    #[test]
    fn gradual_until_future_date_enables_gradual_mode() {
        let temp_dir = TempDir::new().unwrap();
        // Use a date far in the future
        let config_path = create_test_config_with_gradual_until(&temp_dir, "2099-12-31");

        let config = PaveConfig::load(&config_path).unwrap();
        let args = CheckArgs {
            paths: vec![],
            format: OutputFormat::Text,
            strict: false,
            gradual: false,
            changed: false,
            base: None,
        };

        assert!(is_gradual_mode_active(&config, &args));
    }

    #[test]
    fn gradual_until_past_date_disables_gradual_mode() {
        let temp_dir = TempDir::new().unwrap();
        // Use a date in the past
        let config_path = create_test_config_with_gradual_until(&temp_dir, "2020-01-01");

        let config = PaveConfig::load(&config_path).unwrap();
        let args = CheckArgs {
            paths: vec![],
            format: OutputFormat::Text,
            strict: false,
            gradual: false,
            changed: false,
            base: None,
        };

        // Should be disabled due to past deadline
        assert!(!is_gradual_mode_active(&config, &args));
    }

    #[test]
    fn is_leap_year_works_correctly() {
        assert!(is_leap_year(2000)); // Divisible by 400
        assert!(is_leap_year(2024)); // Divisible by 4, not by 100
        assert!(!is_leap_year(1900)); // Divisible by 100, not by 400
        assert!(!is_leap_year(2023)); // Not divisible by 4
    }

    #[test]
    fn gradual_deadline_invalid_format_ignores_deadline() {
        // Invalid formats should not disable gradual mode
        assert!(!is_gradual_deadline_passed("not-a-date"));
        assert!(!is_gradual_deadline_passed("2024/01/01")); // Wrong separator
        assert!(!is_gradual_deadline_passed("2024-13-01")); // Invalid month
        assert!(!is_gradual_deadline_passed("2024-01-32")); // Invalid day
    }
}
