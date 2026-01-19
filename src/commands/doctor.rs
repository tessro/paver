//! Implementation of the `paver doctor` command for diagnosing documentation issues.
//!
//! The doctor command provides comprehensive diagnostics and recommendations for:
//! - Configuration health
//! - Documentation structure
//! - Verification command health
//! - Code-to-documentation mapping

use anyhow::{Context, Result};
use serde::Serialize;
use std::collections::HashSet;
use std::env;
use std::path::{Path, PathBuf};

use crate::cli::OutputFormat;
use crate::config::{CONFIG_FILENAME, PaverConfig};
use crate::parser::ParsedDoc;
use crate::verification::extract_verification_spec;

/// Arguments for the `paver doctor` command.
pub struct DoctorArgs {
    /// Specific files or directories to analyze.
    pub paths: Vec<PathBuf>,
    /// Output format.
    pub format: OutputFormat,
}

/// Status of a diagnostic check.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum CheckStatus {
    Pass,
    Warning,
    Error,
}

/// A single diagnostic check result.
#[derive(Debug, Clone, Serialize)]
pub struct DiagnosticCheck {
    /// Name of the check.
    pub name: String,
    /// Status of the check.
    pub status: CheckStatus,
    /// Message explaining the result.
    pub message: String,
    /// Optional suggestion for how to fix the issue.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub suggestion: Option<String>,
    /// Optional list of files affected.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub affected_files: Vec<PathBuf>,
}

/// A category of diagnostic checks.
#[derive(Debug, Clone, Serialize)]
pub struct DiagnosticCategory {
    /// Name of the category.
    pub name: String,
    /// Checks in this category.
    pub checks: Vec<DiagnosticCheck>,
}

/// Overall results from the doctor command.
#[derive(Debug, Serialize)]
pub struct DoctorResults {
    /// Categories of diagnostic checks.
    pub categories: Vec<DiagnosticCategory>,
    /// Total number of errors.
    pub error_count: usize,
    /// Total number of warnings.
    pub warning_count: usize,
    /// Total number of passing checks.
    pub pass_count: usize,
}

impl DoctorResults {
    fn new() -> Self {
        Self {
            categories: Vec::new(),
            error_count: 0,
            warning_count: 0,
            pass_count: 0,
        }
    }

    fn add_category(&mut self, category: DiagnosticCategory) {
        for check in &category.checks {
            match check.status {
                CheckStatus::Pass => self.pass_count += 1,
                CheckStatus::Warning => self.warning_count += 1,
                CheckStatus::Error => self.error_count += 1,
            }
        }
        self.categories.push(category);
    }

    fn is_healthy(&self) -> bool {
        self.error_count == 0
    }
}

/// Execute the `paver doctor` command.
pub fn execute(args: DoctorArgs) -> Result<()> {
    // Find and load config
    let config_result = find_config();
    let mut results = DoctorResults::new();

    // Run configuration checks
    let config_category = run_config_checks(&config_result);
    results.add_category(config_category);

    // If config exists and is valid, run further checks
    if let Ok(ref config_path) = config_result {
        let config = PaverConfig::load(config_path)?;
        let config_dir = config_path.parent().unwrap_or_else(|| Path::new("."));

        // Determine paths to check
        let paths = if args.paths.is_empty() {
            vec![config_dir.join(&config.docs.root)]
        } else {
            args.paths.clone()
        };

        // Run documentation structure checks
        let docs_category = run_docs_checks(&paths, &config, config_dir)?;
        results.add_category(docs_category);

        // Run verification checks
        let verify_category = run_verification_checks(&paths, config_dir)?;
        results.add_category(verify_category);

        // Run code coverage checks
        let coverage_category = run_coverage_checks(&paths, &config, config_dir)?;
        results.add_category(coverage_category);
    }

    // Output results
    match args.format {
        OutputFormat::Text => output_text(&results),
        OutputFormat::Json => output_json(&results)?,
        OutputFormat::Github => output_github(&results),
    }

    if results.is_healthy() {
        Ok(())
    } else {
        anyhow::bail!(
            "Doctor found issues: {} error{}, {} warning{}",
            results.error_count,
            if results.error_count == 1 { "" } else { "s" },
            results.warning_count,
            if results.warning_count == 1 { "" } else { "s" }
        )
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

/// Run configuration health checks.
fn run_config_checks(config_result: &Result<PathBuf>) -> DiagnosticCategory {
    let mut checks = Vec::new();

    // Check if config file exists
    match config_result {
        Ok(config_path) => {
            checks.push(DiagnosticCheck {
                name: "Config file exists".to_string(),
                status: CheckStatus::Pass,
                message: format!("{} found", CONFIG_FILENAME),
                suggestion: None,
                affected_files: vec![config_path.clone()],
            });

            // Check if config is valid
            match PaverConfig::load(config_path) {
                Ok(config) => {
                    checks.push(DiagnosticCheck {
                        name: "Config file valid".to_string(),
                        status: CheckStatus::Pass,
                        message: format!("{} is valid TOML", CONFIG_FILENAME),
                        suggestion: None,
                        affected_files: vec![],
                    });

                    // Check for recommended settings
                    if !config.rules.require_verification {
                        checks.push(DiagnosticCheck {
                            name: "Verification sections".to_string(),
                            status: CheckStatus::Warning,
                            message: "require_verification is disabled".to_string(),
                            suggestion: Some(
                                "Consider enabling require_verification for testable documentation"
                                    .to_string(),
                            ),
                            affected_files: vec![],
                        });
                    }

                    if !config.rules.require_examples {
                        checks.push(DiagnosticCheck {
                            name: "Examples sections".to_string(),
                            status: CheckStatus::Warning,
                            message: "require_examples is disabled".to_string(),
                            suggestion: Some(
                                "Consider enabling require_examples for practical documentation"
                                    .to_string(),
                            ),
                            affected_files: vec![],
                        });
                    }

                    // Check docs root exists
                    let config_dir = config_path.parent().unwrap_or_else(|| Path::new("."));
                    let docs_root = config_dir.join(&config.docs.root);
                    if !docs_root.exists() {
                        checks.push(DiagnosticCheck {
                            name: "Docs root exists".to_string(),
                            status: CheckStatus::Error,
                            message: format!(
                                "Docs root '{}' does not exist",
                                config.docs.root.display()
                            ),
                            suggestion: Some(format!(
                                "Create the docs directory or update docs.root in {}",
                                CONFIG_FILENAME
                            )),
                            affected_files: vec![],
                        });
                    } else {
                        checks.push(DiagnosticCheck {
                            name: "Docs root exists".to_string(),
                            status: CheckStatus::Pass,
                            message: format!("Docs root '{}' exists", config.docs.root.display()),
                            suggestion: None,
                            affected_files: vec![],
                        });
                    }

                    // Check templates directory if configured
                    if let Some(ref templates_path) = config.docs.templates {
                        let templates_dir = config_dir.join(templates_path);
                        if !templates_dir.exists() {
                            checks.push(DiagnosticCheck {
                                name: "Templates directory".to_string(),
                                status: CheckStatus::Warning,
                                message: format!(
                                    "Templates directory '{}' does not exist",
                                    templates_path.display()
                                ),
                                suggestion: Some(
                                    "Create the templates directory or remove docs.templates from config"
                                        .to_string(),
                                ),
                                affected_files: vec![],
                            });
                        }
                    }
                }
                Err(e) => {
                    checks.push(DiagnosticCheck {
                        name: "Config file valid".to_string(),
                        status: CheckStatus::Error,
                        message: format!("Failed to parse {}: {}", CONFIG_FILENAME, e),
                        suggestion: Some("Check the config file for syntax errors".to_string()),
                        affected_files: vec![config_path.clone()],
                    });
                }
            }
        }
        Err(_) => {
            checks.push(DiagnosticCheck {
                name: "Config file exists".to_string(),
                status: CheckStatus::Error,
                message: format!("No {} found", CONFIG_FILENAME),
                suggestion: Some("Run 'paver init' to create a configuration file".to_string()),
                affected_files: vec![],
            });
        }
    }

    DiagnosticCategory {
        name: "Configuration".to_string(),
        checks,
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
        }
        // Skip non-existent paths silently for doctor command
    }

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

/// Check if a file should be skipped for validation (index.md, templates).
fn should_skip_file(path: &Path) -> bool {
    if path.file_name().is_some_and(|f| f == "index.md") {
        return true;
    }
    let path_str = path.to_string_lossy();
    path_str.contains("/templates/") || path_str.contains("\\templates\\")
}

/// Run documentation structure checks.
fn run_docs_checks(
    paths: &[PathBuf],
    config: &PaverConfig,
    _config_dir: &Path,
) -> Result<DiagnosticCategory> {
    let mut checks = Vec::new();

    let files = find_markdown_files(paths)?;
    let validatable_files: Vec<_> = files.iter().filter(|f| !should_skip_file(f)).collect();

    if validatable_files.is_empty() {
        checks.push(DiagnosticCheck {
            name: "Documentation files".to_string(),
            status: CheckStatus::Warning,
            message: "No documentation files found".to_string(),
            suggestion: Some(
                "Create markdown documentation files in the docs directory".to_string(),
            ),
            affected_files: vec![],
        });

        return Ok(DiagnosticCategory {
            name: "Documentation Structure".to_string(),
            checks,
        });
    }

    checks.push(DiagnosticCheck {
        name: "Documentation files".to_string(),
        status: CheckStatus::Pass,
        message: format!("Found {} documentation file(s)", validatable_files.len()),
        suggestion: None,
        affected_files: vec![],
    });

    // Check for missing Verification sections
    let mut missing_verification = Vec::new();
    let mut missing_examples = Vec::new();
    let mut exceeds_line_limit = Vec::new();

    for file in &validatable_files {
        if let Ok(doc) = ParsedDoc::parse(file) {
            if config.rules.require_verification && !doc.has_section("Verification") {
                missing_verification.push((*file).clone());
            }

            if config.rules.require_examples && !doc.has_section("Examples") {
                missing_examples.push((*file).clone());
            }

            if doc.line_count > config.rules.max_lines as usize {
                exceeds_line_limit.push(((*file).clone(), doc.line_count));
            }
        }
    }

    // Report missing Verification sections
    if missing_verification.is_empty() {
        if config.rules.require_verification {
            checks.push(DiagnosticCheck {
                name: "Verification sections".to_string(),
                status: CheckStatus::Pass,
                message: "All documents have Verification sections".to_string(),
                suggestion: None,
                affected_files: vec![],
            });
        }
    } else {
        checks.push(DiagnosticCheck {
            name: "Verification sections".to_string(),
            status: CheckStatus::Error,
            message: format!(
                "{} document(s) missing Verification section",
                missing_verification.len()
            ),
            suggestion: Some("Add '## Verification' section with test commands".to_string()),
            affected_files: missing_verification,
        });
    }

    // Report missing Examples sections
    if missing_examples.is_empty() {
        if config.rules.require_examples {
            checks.push(DiagnosticCheck {
                name: "Examples sections".to_string(),
                status: CheckStatus::Pass,
                message: "All documents have Examples sections".to_string(),
                suggestion: None,
                affected_files: vec![],
            });
        }
    } else {
        checks.push(DiagnosticCheck {
            name: "Examples sections".to_string(),
            status: CheckStatus::Error,
            message: format!(
                "{} document(s) missing Examples section",
                missing_examples.len()
            ),
            suggestion: Some("Add '## Examples' section with concrete usage examples".to_string()),
            affected_files: missing_examples,
        });
    }

    // Report documents exceeding line limit
    if exceeds_line_limit.is_empty() {
        checks.push(DiagnosticCheck {
            name: "Line limits".to_string(),
            status: CheckStatus::Pass,
            message: format!("All documents under {} line limit", config.rules.max_lines),
            suggestion: None,
            affected_files: vec![],
        });
    } else {
        let affected: Vec<PathBuf> = exceeds_line_limit.iter().map(|(p, _)| p.clone()).collect();
        let details: Vec<String> = exceeds_line_limit
            .iter()
            .map(|(p, lines)| format!("{} ({} lines)", p.display(), lines))
            .collect();
        checks.push(DiagnosticCheck {
            name: "Line limits".to_string(),
            status: CheckStatus::Warning,
            message: format!(
                "{} document(s) exceed {} line limit: {}",
                exceeds_line_limit.len(),
                config.rules.max_lines,
                details.join(", ")
            ),
            suggestion: Some(
                "Consider splitting large documents into smaller, focused ones".to_string(),
            ),
            affected_files: affected,
        });
    }

    Ok(DiagnosticCategory {
        name: "Documentation Structure".to_string(),
        checks,
    })
}

/// Run verification command checks.
fn run_verification_checks(paths: &[PathBuf], _config_dir: &Path) -> Result<DiagnosticCategory> {
    let mut checks = Vec::new();

    let files = find_markdown_files(paths)?;
    let validatable_files: Vec<_> = files.iter().filter(|f| !should_skip_file(f)).collect();

    let mut docs_with_commands = 0;
    let mut empty_verification_sections = Vec::new();
    let mut potentially_broken_commands = Vec::new();

    for file in &validatable_files {
        let Ok(doc) = ParsedDoc::parse(file) else {
            continue;
        };
        if doc.has_section("Verification") {
            let spec = extract_verification_spec(&doc);
            match spec {
                Some(spec) if !spec.items.is_empty() => {
                    docs_with_commands += 1;

                    // Check for potentially broken commands
                    for item in &spec.items {
                        // Check for hardcoded paths that might not exist
                        if item.command.contains("/home/")
                            || item.command.contains("/Users/")
                            || item.command.contains("C:\\")
                        {
                            potentially_broken_commands
                                .push(((*file).clone(), item.command.clone()));
                        }
                    }
                }
                _ => {
                    empty_verification_sections.push((*file).clone());
                }
            }
        }
    }

    // Report verification command presence
    if docs_with_commands > 0 {
        checks.push(DiagnosticCheck {
            name: "Verification commands".to_string(),
            status: CheckStatus::Pass,
            message: format!(
                "{} document(s) have executable verification commands",
                docs_with_commands
            ),
            suggestion: None,
            affected_files: vec![],
        });
    }

    // Report empty verification sections
    if !empty_verification_sections.is_empty() {
        checks.push(DiagnosticCheck {
            name: "Empty verification sections".to_string(),
            status: CheckStatus::Warning,
            message: format!(
                "{} document(s) have Verification section but no executable commands",
                empty_verification_sections.len()
            ),
            suggestion: Some("Add executable code blocks (```bash) with test commands".to_string()),
            affected_files: empty_verification_sections,
        });
    }

    // Report potentially broken commands
    if !potentially_broken_commands.is_empty() {
        let affected: Vec<PathBuf> = potentially_broken_commands
            .iter()
            .map(|(p, _)| p.clone())
            .collect::<HashSet<_>>()
            .into_iter()
            .collect();
        checks.push(DiagnosticCheck {
            name: "Hardcoded paths".to_string(),
            status: CheckStatus::Warning,
            message: format!(
                "{} command(s) contain hardcoded paths that may not be portable",
                potentially_broken_commands.len()
            ),
            suggestion: Some("Use relative paths or environment variables instead".to_string()),
            affected_files: affected,
        });
    }

    // If no checks were added, add a neutral one
    if checks.is_empty() {
        checks.push(DiagnosticCheck {
            name: "Verification commands".to_string(),
            status: CheckStatus::Warning,
            message: "No verification commands found in any documents".to_string(),
            suggestion: Some("Add executable verification commands to documentation".to_string()),
            affected_files: vec![],
        });
    }

    Ok(DiagnosticCategory {
        name: "Verification Commands".to_string(),
        checks,
    })
}

/// Run code coverage checks.
fn run_coverage_checks(
    paths: &[PathBuf],
    config: &PaverConfig,
    config_dir: &Path,
) -> Result<DiagnosticCategory> {
    let mut checks = Vec::new();

    let files = find_markdown_files(paths)?;
    let validatable_files: Vec<_> = files.iter().filter(|f| !should_skip_file(f)).collect();

    // Collect paths mentioned in documentation
    let mut documented_patterns: HashSet<String> = HashSet::new();

    for file in &validatable_files {
        let Ok(doc) = ParsedDoc::parse(file) else {
            continue;
        };
        let Some(section) = doc.get_section("Paths") else {
            continue;
        };
        // Extract path patterns from the Paths section
        for line in section.content.lines() {
            let trimmed = line.trim();
            // Skip empty lines, headings, and list markers
            if trimmed.is_empty()
                || trimmed.starts_with('#')
                || trimmed.starts_with('-')
                || trimmed.starts_with('*')
            {
                // If it's a list item, extract the path part
                if let Some(path_part) = trimmed.strip_prefix("- ") {
                    let path = path_part.trim_matches('`');
                    if !path.is_empty() {
                        documented_patterns.insert(path.to_string());
                    }
                } else if let Some(path_part) = trimmed.strip_prefix("* ") {
                    let path = path_part.trim_matches('`');
                    if !path.is_empty() {
                        documented_patterns.insert(path.to_string());
                    }
                }
            } else if trimmed.starts_with('`') && trimmed.ends_with('`') {
                let path = trimmed.trim_matches('`');
                if !path.is_empty() {
                    documented_patterns.insert(path.to_string());
                }
            }
        }
    }

    // Find common source directories that might need documentation
    let common_src_dirs = ["src", "lib", "pkg", "internal", "cmd"];
    let mut undocumented_dirs = Vec::new();

    for dir_name in &common_src_dirs {
        let dir_path = config_dir.join(dir_name);
        if dir_path.exists() && dir_path.is_dir() {
            // Check if any documentation references this directory
            let is_documented = documented_patterns
                .iter()
                .any(|p| p.contains(dir_name) || p.starts_with(dir_name));

            if !is_documented && !config.mapping.exclude.iter().any(|e| e.contains(dir_name)) {
                undocumented_dirs.push(dir_path);
            }
        }
    }

    if documented_patterns.is_empty() {
        checks.push(DiagnosticCheck {
            name: "Code-to-doc mapping".to_string(),
            status: CheckStatus::Warning,
            message: "No ## Paths sections found in documentation".to_string(),
            suggestion: Some(
                "Add '## Paths' sections to documents to map code to documentation".to_string(),
            ),
            affected_files: vec![],
        });
    } else {
        checks.push(DiagnosticCheck {
            name: "Code-to-doc mapping".to_string(),
            status: CheckStatus::Pass,
            message: format!(
                "Found {} path pattern(s) in documentation",
                documented_patterns.len()
            ),
            suggestion: None,
            affected_files: vec![],
        });
    }

    if !undocumented_dirs.is_empty() {
        checks.push(DiagnosticCheck {
            name: "Source directories".to_string(),
            status: CheckStatus::Warning,
            message: format!(
                "{} source director{} may not have associated documentation",
                undocumented_dirs.len(),
                if undocumented_dirs.len() == 1 {
                    "y"
                } else {
                    "ies"
                }
            ),
            suggestion: Some(
                "Consider adding documentation for these directories or exclude them in mapping.exclude"
                    .to_string(),
            ),
            affected_files: undocumented_dirs,
        });
    }

    Ok(DiagnosticCategory {
        name: "Code Coverage".to_string(),
        checks,
    })
}

/// Output results in text format.
fn output_text(results: &DoctorResults) {
    for category in &results.categories {
        println!("{}", category.name);

        for check in &category.checks {
            let symbol = match check.status {
                CheckStatus::Pass => "\u{2713}", // checkmark
                CheckStatus::Warning => "!",
                CheckStatus::Error => "\u{2717}", // X mark
            };

            let status_label = match check.status {
                CheckStatus::Pass => "",
                CheckStatus::Warning => " (warning)",
                CheckStatus::Error => " (error)",
            };

            println!("  {} {}{}", symbol, check.message, status_label);

            // Show affected files for warnings and errors
            if !check.affected_files.is_empty()
                && (check.status == CheckStatus::Warning || check.status == CheckStatus::Error)
            {
                for file in &check.affected_files {
                    println!("    -> {}", file.display());
                }
            }

            // Show suggestion
            if let Some(ref suggestion) = check.suggestion
                && check.status != CheckStatus::Pass
            {
                println!("    hint: {}", suggestion);
            }
        }
        println!();
    }

    // Print summary
    println!(
        "Summary: {} error{}, {} warning{}",
        results.error_count,
        if results.error_count == 1 { "" } else { "s" },
        results.warning_count,
        if results.warning_count == 1 { "" } else { "s" }
    );

    if results.error_count > 0 || results.warning_count > 0 {
        println!("Run 'paver check' for detailed validation");
    }
}

/// Output results in JSON format.
fn output_json(results: &DoctorResults) -> Result<()> {
    let json = serde_json::to_string_pretty(results).context("Failed to serialize results")?;
    println!("{}", json);
    Ok(())
}

/// Output results in GitHub Actions annotation format.
fn output_github(results: &DoctorResults) {
    for category in &results.categories {
        for check in &category.checks {
            if check.status != CheckStatus::Pass {
                let level = match check.status {
                    CheckStatus::Error => "error",
                    CheckStatus::Warning => "warning",
                    CheckStatus::Pass => continue,
                };

                let message = if let Some(ref suggestion) = check.suggestion {
                    format!("{}: {} - {}", check.name, check.message, suggestion)
                } else {
                    format!("{}: {}", check.name, check.message)
                };

                if check.affected_files.is_empty() {
                    println!("::{}::{}", level, message);
                } else {
                    for file in &check.affected_files {
                        println!("::{}file={}::{}", level, file.display(), message);
                    }
                }
            }
        }
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
max_lines = 300
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
```bash
echo "test"
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
Missing required sections.
"#;
        let path = docs_dir.join(filename);
        fs::write(&path, content).unwrap();
        path
    }

    #[test]
    fn doctor_results_counts_statuses() {
        let mut results = DoctorResults::new();

        let category = DiagnosticCategory {
            name: "Test".to_string(),
            checks: vec![
                DiagnosticCheck {
                    name: "Pass".to_string(),
                    status: CheckStatus::Pass,
                    message: "OK".to_string(),
                    suggestion: None,
                    affected_files: vec![],
                },
                DiagnosticCheck {
                    name: "Warn".to_string(),
                    status: CheckStatus::Warning,
                    message: "Warn".to_string(),
                    suggestion: None,
                    affected_files: vec![],
                },
                DiagnosticCheck {
                    name: "Error".to_string(),
                    status: CheckStatus::Error,
                    message: "Error".to_string(),
                    suggestion: None,
                    affected_files: vec![],
                },
            ],
        };

        results.add_category(category);

        assert_eq!(results.pass_count, 1);
        assert_eq!(results.warning_count, 1);
        assert_eq!(results.error_count, 1);
        assert!(!results.is_healthy());
    }

    #[test]
    fn doctor_results_healthy_when_no_errors() {
        let mut results = DoctorResults::new();

        let category = DiagnosticCategory {
            name: "Test".to_string(),
            checks: vec![
                DiagnosticCheck {
                    name: "Pass".to_string(),
                    status: CheckStatus::Pass,
                    message: "OK".to_string(),
                    suggestion: None,
                    affected_files: vec![],
                },
                DiagnosticCheck {
                    name: "Warn".to_string(),
                    status: CheckStatus::Warning,
                    message: "Warn".to_string(),
                    suggestion: None,
                    affected_files: vec![],
                },
            ],
        };

        results.add_category(category);

        assert!(results.is_healthy());
    }

    #[test]
    fn config_check_reports_missing_config() {
        let category = run_config_checks(&Err(anyhow::anyhow!("No config")));

        assert!(!category.checks.is_empty());
        assert!(
            category
                .checks
                .iter()
                .any(|c| c.status == CheckStatus::Error)
        );
        assert!(
            category
                .checks
                .iter()
                .any(|c| c.message.contains("No .paver.toml found"))
        );
    }

    #[test]
    fn config_check_reports_valid_config() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = create_test_config(&temp_dir);

        // Create docs directory
        fs::create_dir_all(temp_dir.path().join("docs")).unwrap();

        let category = run_config_checks(&Ok(config_path));

        assert!(
            category
                .checks
                .iter()
                .any(|c| c.status == CheckStatus::Pass && c.name == "Config file exists")
        );
        assert!(
            category
                .checks
                .iter()
                .any(|c| c.status == CheckStatus::Pass && c.name == "Config file valid")
        );
    }

    #[test]
    fn docs_check_reports_missing_sections() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = create_test_config(&temp_dir);
        let _doc_path = create_invalid_doc(&temp_dir, "invalid.md");

        let config = PaverConfig::load(&config_path).unwrap();
        let docs_dir = temp_dir.path().join("docs");

        let category = run_docs_checks(&[docs_dir], &config, temp_dir.path()).unwrap();

        assert!(
            category
                .checks
                .iter()
                .any(|c| c.status == CheckStatus::Error && c.name == "Verification sections")
        );
        assert!(
            category
                .checks
                .iter()
                .any(|c| c.status == CheckStatus::Error && c.name == "Examples sections")
        );
    }

    #[test]
    fn docs_check_passes_with_valid_docs() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = create_test_config(&temp_dir);
        let _doc_path = create_valid_doc(&temp_dir, "valid.md");

        let config = PaverConfig::load(&config_path).unwrap();
        let docs_dir = temp_dir.path().join("docs");

        let category = run_docs_checks(&[docs_dir], &config, temp_dir.path()).unwrap();

        // All checks should pass
        assert!(
            category
                .checks
                .iter()
                .all(|c| c.status == CheckStatus::Pass)
        );
    }

    #[test]
    fn verification_check_detects_empty_sections() {
        let temp_dir = TempDir::new().unwrap();
        let docs_dir = temp_dir.path().join("docs");
        fs::create_dir_all(&docs_dir).unwrap();

        // Create doc with empty Verification section
        let content = r#"# Test

## Verification
Just text, no commands.

## Examples
Example here.
"#;
        fs::write(docs_dir.join("empty-verify.md"), content).unwrap();

        let category = run_verification_checks(&[docs_dir], temp_dir.path()).unwrap();

        assert!(
            category
                .checks
                .iter()
                .any(|c| c.name == "Empty verification sections")
        );
    }

    #[test]
    fn should_skip_index_and_templates() {
        assert!(should_skip_file(Path::new("docs/index.md")));
        assert!(should_skip_file(Path::new("docs/templates/component.md")));
        assert!(!should_skip_file(Path::new("docs/guide.md")));
    }

    #[test]
    fn json_output_is_valid() {
        let mut results = DoctorResults::new();
        results.add_category(DiagnosticCategory {
            name: "Test".to_string(),
            checks: vec![DiagnosticCheck {
                name: "Check".to_string(),
                status: CheckStatus::Pass,
                message: "OK".to_string(),
                suggestion: None,
                affected_files: vec![],
            }],
        });

        let json = serde_json::to_string(&results).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed["pass_count"], 1);
        assert_eq!(parsed["error_count"], 0);
        assert_eq!(parsed["categories"][0]["name"], "Test");
    }

    #[test]
    fn check_status_serializes_lowercase() {
        let pass = serde_json::to_string(&CheckStatus::Pass).unwrap();
        let warning = serde_json::to_string(&CheckStatus::Warning).unwrap();
        let error = serde_json::to_string(&CheckStatus::Error).unwrap();

        assert_eq!(pass, "\"pass\"");
        assert_eq!(warning, "\"warning\"");
        assert_eq!(error, "\"error\"");
    }

    #[test]
    fn coverage_check_detects_paths_sections() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = create_test_config(&temp_dir);
        let docs_dir = temp_dir.path().join("docs");
        fs::create_dir_all(&docs_dir).unwrap();

        // Create doc with Paths section
        let content = r#"# Component

## Purpose
Test component.

## Paths
- `src/lib.rs`
- `src/main.rs`

## Verification
```bash
echo "test"
```

## Examples
Example here.
"#;
        fs::write(docs_dir.join("component.md"), content).unwrap();

        let config = PaverConfig::load(&config_path).unwrap();
        let category = run_coverage_checks(&[docs_dir], &config, temp_dir.path()).unwrap();

        // Should find the path patterns
        assert!(
            category
                .checks
                .iter()
                .any(|c| c.name == "Code-to-doc mapping" && c.status == CheckStatus::Pass)
        );
    }

    #[test]
    fn coverage_check_warns_on_no_paths() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = create_test_config(&temp_dir);
        let docs_dir = temp_dir.path().join("docs");
        fs::create_dir_all(&docs_dir).unwrap();

        // Create doc without Paths section
        let content = r#"# Component

## Purpose
Test component.

## Verification
```bash
echo "test"
```

## Examples
Example here.
"#;
        fs::write(docs_dir.join("component.md"), content).unwrap();

        let config = PaverConfig::load(&config_path).unwrap();
        let category = run_coverage_checks(&[docs_dir], &config, temp_dir.path()).unwrap();

        // Should warn about missing Paths sections
        assert!(
            category
                .checks
                .iter()
                .any(|c| c.name == "Code-to-doc mapping" && c.status == CheckStatus::Warning)
        );
    }
}
