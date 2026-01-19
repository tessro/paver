//! Implementation of the `pave status` command for showing documentation health overview.

use anyhow::{Context, Result};
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::cli::StatusOutputFormat;
use crate::commands::hooks::{PAVE_HOOK_MARKER, find_git_hooks_dir_from};
use crate::config::{CONFIG_FILENAME, PaveConfig};
use crate::parser::ParsedDoc;
use crate::rules::{DocType, RulesEngine, detect_doc_type};

/// File analysis result: (is_compliant, has_warnings, error_count, warning_count, doc_type)
type FileAnalysisResult = (bool, bool, usize, usize, DocType);

/// Arguments for the `pave status` command.
pub struct StatusArgs {
    /// Specific files or directories to check.
    pub paths: Vec<PathBuf>,
    /// Output format.
    pub format: StatusOutputFormat,
    /// Only show status for docs changed since base ref.
    pub changed: bool,
    /// Base ref for --changed comparison.
    pub base: Option<String>,
}

/// Statistics about document compliance by type.
#[derive(Debug, Default, Serialize)]
pub struct TypeStats {
    /// Total documents of this type.
    pub total: usize,
    /// Number of compliant documents.
    pub compliant: usize,
}

/// Information about a changed document.
#[derive(Debug, Serialize)]
pub struct ChangedDoc {
    /// Path to the document.
    pub path: PathBuf,
    /// Change type (Added, Modified).
    pub change_type: String,
    /// Whether the document is compliant.
    pub is_compliant: bool,
    /// Number of errors.
    pub error_count: usize,
    /// Number of warnings.
    pub warning_count: usize,
    /// Brief summary of status.
    pub summary: String,
}

/// Results of the status command.
#[derive(Debug, Serialize)]
pub struct StatusResults {
    /// Root directory of documentation.
    pub docs_root: PathBuf,
    /// Total number of documents.
    pub total_docs: usize,
    /// Number of compliant documents.
    pub compliant_docs: usize,
    /// Number of documents with warnings only.
    pub warning_docs: usize,
    /// Number of documents with errors.
    pub error_docs: usize,
    /// Compliance percentage.
    pub compliance_percent: f64,
    /// Statistics by document type.
    pub type_stats: HashMap<String, TypeStats>,
    /// Recent changes (when in git repo with --changed).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recent_changes: Option<Vec<ChangedDoc>>,
    /// Whether gradual mode is in use (rules relaxed).
    pub gradual_mode: bool,
    /// Whether the project is ready for strict mode.
    pub strict_mode_ready: bool,
    /// Whether pre-commit hook is installed.
    pub hooks_installed: bool,
}

impl StatusResults {
    fn new(docs_root: PathBuf) -> Self {
        Self {
            docs_root,
            total_docs: 0,
            compliant_docs: 0,
            warning_docs: 0,
            error_docs: 0,
            compliance_percent: 0.0,
            type_stats: HashMap::new(),
            recent_changes: None,
            gradual_mode: false,
            strict_mode_ready: false,
            hooks_installed: false,
        }
    }

    fn update_compliance_percent(&mut self) {
        if self.total_docs > 0 {
            self.compliance_percent = (self.compliant_docs as f64 / self.total_docs as f64) * 100.0;
        }
    }

    fn add_doc(&mut self, doc_type: DocType, is_compliant: bool, has_warnings: bool) {
        self.total_docs += 1;

        let type_name = match doc_type {
            DocType::Component => "Components",
            DocType::Runbook => "Runbooks",
            DocType::Adr => "ADRs",
            DocType::Other => "Other",
        };

        let stats = self
            .type_stats
            .entry(type_name.to_string())
            .or_default();
        stats.total += 1;

        if is_compliant {
            self.compliant_docs += 1;
            stats.compliant += 1;
            if has_warnings {
                self.warning_docs += 1;
            }
        } else {
            self.error_docs += 1;
        }
    }
}

/// Execute the `pave status` command.
pub fn execute(args: StatusArgs) -> Result<()> {
    // Find and load config
    let config_path = find_config()?;
    let config = PaveConfig::load(&config_path)?;
    let config_dir = config_path.parent().unwrap_or_else(|| Path::new("."));

    // Determine paths to check
    let docs_root = config_dir.join(&config.docs.root);
    let paths = if args.paths.is_empty() {
        vec![docs_root.clone()]
    } else {
        args.paths.clone()
    };

    // Find all markdown files
    let mut files = find_markdown_files(&paths)?;

    // Initialize results
    let mut results = StatusResults::new(config.docs.root.clone());

    // Check if gradual mode is in use (verification/examples not required)
    results.gradual_mode = !config.rules.require_verification || !config.rules.require_examples;

    // Check if hooks are installed
    results.hooks_installed = check_hooks_installed(config_dir);

    // Handle --changed flag
    let changed_files = if args.changed {
        let base_ref = determine_base_ref(args.base.as_deref())?;
        let changed = get_changed_md_files(&base_ref, config_dir)?;

        if changed.is_empty() {
            eprintln!("No changed markdown files found compared to {}", base_ref);
        }

        // Filter files to only include those that changed
        files.retain(|f| {
            let relative = f.strip_prefix(config_dir).unwrap_or(f).to_path_buf();
            changed.contains(&relative) || changed.contains(f)
        });

        Some(changed)
    } else {
        None
    };

    if files.is_empty() && !args.changed {
        eprintln!("No markdown files found in documentation root");
        output_results(&results, args.format)?;
        return Ok(());
    }

    // Get list of newly added files for accurate change_type detection
    let added_files = if args.changed {
        get_added_files(args.base.as_deref(), config_dir).unwrap_or_default()
    } else {
        HashSet::new()
    };

    // Analyze each file
    let mut recent_changes: Vec<ChangedDoc> = Vec::new();

    for file in &files {
        // Skip files that shouldn't be counted (index.md, templates)
        let Some((is_compliant, has_warnings, error_count, warning_count, doc_type)) =
            analyze_file(file, &config, config_dir)?
        else {
            continue;
        };

        results.add_doc(doc_type, is_compliant, has_warnings);

        // Track changed docs for recent changes display
        if let Some(ref changed) = changed_files {
            let relative = file.strip_prefix(config_dir).unwrap_or(file).to_path_buf();
            if changed.contains(&relative) || changed.contains(file) {
                let change_type = if added_files.contains(&relative) || added_files.contains(file) {
                    "Added"
                } else {
                    "Modified"
                };
                let summary = if is_compliant && warning_count == 0 {
                    "compliant".to_string()
                } else if is_compliant {
                    format!(
                        "{} warning{}",
                        warning_count,
                        if warning_count == 1 { "" } else { "s" }
                    )
                } else {
                    format!(
                        "{} error{}",
                        error_count,
                        if error_count == 1 { "" } else { "s" }
                    )
                };

                recent_changes.push(ChangedDoc {
                    path: relative,
                    change_type: change_type.to_string(),
                    is_compliant,
                    error_count,
                    warning_count,
                    summary,
                });
            }
        }
    }

    // Update compliance percentage
    results.update_compliance_percent();

    // Determine if ready for strict mode (> 50% compliant)
    results.strict_mode_ready = results.compliance_percent >= 50.0;

    // Set recent changes if we tracked them
    if args.changed {
        results.recent_changes = Some(recent_changes);
    }

    // Output results
    output_results(&results, args.format)?;

    Ok(())
}

/// Check if a file should be skipped from compliance tracking.
fn should_skip_file(path: &Path) -> bool {
    // Skip index.md files - they are navigation documents
    if path.file_name().is_some_and(|f| f == "index.md") {
        return true;
    }

    // Skip template files - they are scaffolds, not actual documentation
    let path_str = path.to_string_lossy();
    if path_str.contains("/templates/") || path_str.contains("\\templates\\") {
        return true;
    }

    false
}

/// Analyze a single file and return compliance info.
/// Returns None for files that should be skipped (index.md, templates).
fn analyze_file(
    path: &Path,
    config: &PaveConfig,
    config_dir: &Path,
) -> Result<Option<FileAnalysisResult>> {
    // Skip index.md and template files (they don't count toward compliance)
    if should_skip_file(path) {
        return Ok(None);
    }

    // Read and parse the file
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("Failed to read file: {}", path.display()))?;
    let doc = ParsedDoc::parse_content(path.to_path_buf(), &content)?;

    // Detect document type
    let doc_type = detect_doc_type(path, &content);

    // Build rules engine with project root for path validation
    let engine = RulesEngine::from_config_with_root(&config.rules, config_dir);

    // Validate with type-specific rules
    let result = engine.validate_with_type(&doc, doc_type, &config.rules);

    // Also check base requirements (max lines, etc.)
    let error_count = result.errors.len();
    let mut warning_count = result.warnings.len();

    // Check max lines (produces warning)
    if doc.line_count > config.rules.max_lines as usize {
        warning_count += 1;
    }

    let is_compliant = error_count == 0;
    let has_warnings = warning_count > 0;

    Ok(Some((
        is_compliant,
        has_warnings,
        error_count,
        warning_count,
        doc_type,
    )))
}

/// Check if pre-commit hook is installed by pave.
fn check_hooks_installed(config_dir: &Path) -> bool {
    if let Ok(hooks_dir) = find_git_hooks_dir_from(config_dir) {
        let pre_commit = hooks_dir.join("pre-commit");
        if pre_commit.exists()
            && let Ok(content) = std::fs::read_to_string(&pre_commit)
        {
            return content.contains(PAVE_HOOK_MARKER);
        }
    }
    false
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
        } else if !path.exists() {
            // Path doesn't exist yet - could be a new docs folder
            continue;
        } else {
            anyhow::bail!("Path is not a file or directory: {}", path.display());
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

/// Get the list of newly added markdown files from git diff.
fn get_added_files(explicit_base: Option<&str>, config_dir: &Path) -> Result<HashSet<PathBuf>> {
    let base_ref = determine_base_ref(explicit_base)?;

    // Use --diff-filter=A to get only added files
    let output = Command::new("git")
        .args([
            "diff",
            "--name-only",
            "--diff-filter=A",
            &format!("{}..HEAD", base_ref),
        ])
        .current_dir(config_dir)
        .output()
        .context("Failed to run git diff")?;

    if !output.status.success() {
        // Try without ..HEAD for cases like HEAD~1
        let output = Command::new("git")
            .args(["diff", "--name-only", "--diff-filter=A", &base_ref])
            .current_dir(config_dir)
            .output()
            .context("Failed to run git diff")?;

        if !output.status.success() {
            return Ok(HashSet::new());
        }

        return parse_changed_md_files(&output.stdout);
    }

    parse_changed_md_files(&output.stdout)
}

/// Output results in the requested format.
fn output_results(results: &StatusResults, format: StatusOutputFormat) -> Result<()> {
    match format {
        StatusOutputFormat::Text => output_text(results),
        StatusOutputFormat::Json => output_json(results)?,
    }
    Ok(())
}

/// Output results in text format.
fn output_text(results: &StatusResults) {
    println!("Documentation: {}/", results.docs_root.display());
    println!(
        "  Total: {} document{}",
        results.total_docs,
        if results.total_docs == 1 { "" } else { "s" }
    );
    println!(
        "  Compliant: {} ({:.0}%)",
        results.compliant_docs, results.compliance_percent
    );
    if results.warning_docs > 0 {
        println!("  Warnings: {}", results.warning_docs);
    }
    if results.error_docs > 0 {
        println!("  Errors: {}", results.error_docs);
    }

    // Document types breakdown
    if !results.type_stats.is_empty() {
        println!();
        println!("Document Types:");

        // Sort by type name for consistent output
        let mut types: Vec<_> = results.type_stats.iter().collect();
        types.sort_by_key(|(name, _)| *name);

        for (type_name, stats) in types {
            if stats.total > 0 {
                println!(
                    "  {}: {} ({} compliant)",
                    type_name, stats.total, stats.compliant
                );
            }
        }
    }

    // Recent changes section
    if let Some(ref changes) = results.recent_changes
        && !changes.is_empty()
    {
        println!();
        println!("Recent Changes:");
        for change in changes {
            let status_indicator = if change.is_compliant {
                if change.warning_count > 0 { "!" } else { "✓" }
            } else {
                "✗"
            };
            println!(
                "  {}: {} ({} {})",
                change.change_type,
                change.path.display(),
                change.summary,
                status_indicator
            );
        }
    }

    // Mode and readiness info
    println!();
    if results.gradual_mode {
        let readiness = if results.strict_mode_ready {
            format!(
                "strict mode ready: {:.0}% compliant",
                results.compliance_percent
            )
        } else {
            format!(
                "need {:.0}% more for strict mode",
                50.0 - results.compliance_percent
            )
        };
        println!("Mode: gradual ({})", readiness);
    } else {
        println!("Mode: strict");
    }

    // Hooks status
    if results.hooks_installed {
        println!("Hooks: pre-commit installed");
    } else {
        println!("Hooks: not installed");
    }

    // Helpful footer
    println!();
    println!("Run 'pave check' for details or 'pave hooks install' to add git hooks.");
}

/// Output results in JSON format.
fn output_json(results: &StatusResults) -> Result<()> {
    let json = serde_json::to_string_pretty(results).context("Failed to serialize results")?;
    println!("{}", json);
    Ok(())
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
max_lines = 300
require_verification = true
require_examples = true
"#;
        let config_path = temp_dir.path().join(".pave.toml");
        fs::write(&config_path, config_content).unwrap();
        config_path
    }

    fn create_valid_doc(temp_dir: &TempDir, subpath: &str) -> PathBuf {
        let docs_dir = temp_dir.path().join("docs");
        let full_path = docs_dir.join(subpath);
        if let Some(parent) = full_path.parent() {
            fs::create_dir_all(parent).unwrap();
        }

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
```bash
$ example command
```
"#;
        fs::write(&full_path, content).unwrap();
        full_path
    }

    fn create_invalid_doc(temp_dir: &TempDir, subpath: &str) -> PathBuf {
        let docs_dir = temp_dir.path().join("docs");
        let full_path = docs_dir.join(subpath);
        if let Some(parent) = full_path.parent() {
            fs::create_dir_all(parent).unwrap();
        }

        let content = r#"# Incomplete Document

## Purpose
This document is missing required sections.
"#;
        fs::write(&full_path, content).unwrap();
        full_path
    }

    #[test]
    fn status_results_tracks_compliance() {
        let mut results = StatusResults::new(PathBuf::from("docs"));

        results.add_doc(DocType::Component, true, false);
        results.add_doc(DocType::Component, true, true);
        results.add_doc(DocType::Component, false, false);

        assert_eq!(results.total_docs, 3);
        assert_eq!(results.compliant_docs, 2);
        assert_eq!(results.error_docs, 1);
        assert_eq!(results.warning_docs, 1);

        results.update_compliance_percent();
        assert!((results.compliance_percent - 66.67).abs() < 0.1);
    }

    #[test]
    fn status_results_tracks_by_type() {
        let mut results = StatusResults::new(PathBuf::from("docs"));

        results.add_doc(DocType::Component, true, false);
        results.add_doc(DocType::Component, false, false);
        results.add_doc(DocType::Runbook, true, false);
        results.add_doc(DocType::Adr, true, false);
        results.add_doc(DocType::Other, false, false);

        assert_eq!(results.type_stats.get("Components").unwrap().total, 2);
        assert_eq!(results.type_stats.get("Components").unwrap().compliant, 1);
        assert_eq!(results.type_stats.get("Runbooks").unwrap().total, 1);
        assert_eq!(results.type_stats.get("ADRs").unwrap().total, 1);
        assert_eq!(results.type_stats.get("Other").unwrap().total, 1);
    }

    #[test]
    fn analyze_valid_file() {
        let temp_dir = TempDir::new().unwrap();
        let _config_path = create_test_config(&temp_dir);
        let doc_path = create_valid_doc(&temp_dir, "valid.md");

        let config = PaveConfig::load(temp_dir.path().join(".pave.toml")).unwrap();
        let result = analyze_file(&doc_path, &config, temp_dir.path()).unwrap();

        let (is_compliant, _, error_count, _, _) = result.expect("File should not be skipped");
        assert!(is_compliant);
        assert_eq!(error_count, 0);
    }

    #[test]
    fn analyze_invalid_file() {
        let temp_dir = TempDir::new().unwrap();
        let _config_path = create_test_config(&temp_dir);
        let doc_path = create_invalid_doc(&temp_dir, "invalid.md");

        let config = PaveConfig::load(temp_dir.path().join(".pave.toml")).unwrap();
        let result = analyze_file(&doc_path, &config, temp_dir.path()).unwrap();

        let (is_compliant, _, error_count, _, _) = result.expect("File should not be skipped");
        assert!(!is_compliant);
        assert!(error_count > 0);
    }

    #[test]
    fn analyze_skips_index_files() {
        let temp_dir = TempDir::new().unwrap();
        let _config_path = create_test_config(&temp_dir);

        // Create an index.md without required sections
        let docs_dir = temp_dir.path().join("docs");
        fs::create_dir_all(&docs_dir).unwrap();
        let index_path = docs_dir.join("index.md");
        fs::write(&index_path, "# Index\n\nJust links here.").unwrap();

        let config = PaveConfig::load(temp_dir.path().join(".pave.toml")).unwrap();
        let result = analyze_file(&index_path, &config, temp_dir.path()).unwrap();

        // index.md should be skipped (None returned)
        assert!(result.is_none(), "index.md should be skipped");
    }

    #[test]
    fn analyze_skips_template_files() {
        let temp_dir = TempDir::new().unwrap();
        let _config_path = create_test_config(&temp_dir);

        // Create a template file without required sections
        let templates_dir = temp_dir.path().join("docs").join("templates");
        fs::create_dir_all(&templates_dir).unwrap();
        let template_path = templates_dir.join("component.md");
        fs::write(&template_path, "# {Name}\n\n## Purpose\nDescribe.").unwrap();

        let config = PaveConfig::load(temp_dir.path().join(".pave.toml")).unwrap();
        let result = analyze_file(&template_path, &config, temp_dir.path()).unwrap();

        // Templates should be skipped (None returned)
        assert!(result.is_none(), "template files should be skipped");
    }

    #[test]
    fn json_output_is_valid() {
        let mut results = StatusResults::new(PathBuf::from("docs"));
        results.add_doc(DocType::Component, true, false);
        results.update_compliance_percent();

        let json = serde_json::to_string(&results).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed["total_docs"], 1);
        assert_eq!(parsed["compliant_docs"], 1);
        assert_eq!(parsed["compliance_percent"], 100.0);
    }

    #[test]
    fn find_markdown_files_in_dir() {
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
    fn strict_mode_ready_above_50_percent() {
        let mut results = StatusResults::new(PathBuf::from("docs"));

        // 3 out of 5 compliant = 60%
        results.add_doc(DocType::Component, true, false);
        results.add_doc(DocType::Component, true, false);
        results.add_doc(DocType::Component, true, false);
        results.add_doc(DocType::Component, false, false);
        results.add_doc(DocType::Component, false, false);

        results.update_compliance_percent();
        results.strict_mode_ready = results.compliance_percent >= 50.0;

        assert!(results.strict_mode_ready);
    }

    #[test]
    fn strict_mode_not_ready_below_50_percent() {
        let mut results = StatusResults::new(PathBuf::from("docs"));

        // 2 out of 5 compliant = 40%
        results.add_doc(DocType::Component, true, false);
        results.add_doc(DocType::Component, true, false);
        results.add_doc(DocType::Component, false, false);
        results.add_doc(DocType::Component, false, false);
        results.add_doc(DocType::Component, false, false);

        results.update_compliance_percent();
        results.strict_mode_ready = results.compliance_percent >= 50.0;

        assert!(!results.strict_mode_ready);
    }

    #[test]
    fn parse_changed_md_files_filters_correctly() {
        let output = b"src/cli.rs\ndocs/readme.md\nsrc/main.rs\ndocs/guide.md\n";
        let files = parse_changed_md_files(output).unwrap();

        assert_eq!(files.len(), 2);
        assert!(files.contains(&PathBuf::from("docs/readme.md")));
        assert!(files.contains(&PathBuf::from("docs/guide.md")));
    }

    #[test]
    fn determine_base_ref_uses_explicit() {
        let result = determine_base_ref(Some("custom-branch")).unwrap();
        assert_eq!(result, "custom-branch");
    }
}
