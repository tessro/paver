//! Implementation of the `paver changed` command for detecting impacted documentation.
//!
//! This module analyzes git diffs and reports which documentation should be reviewed
//! or updated based on code-to-doc mappings defined in the docs.

use anyhow::{Context, Result};
use glob::Pattern;
use serde::Serialize;
use std::collections::HashSet;
use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::cli::ChangedOutputFormat;
use crate::config::{CONFIG_FILENAME, PaverConfig};

/// Arguments for the `paver changed` command.
pub struct ChangedArgs {
    /// Git ref to compare against.
    pub base: Option<String>,
    /// Output format.
    pub format: ChangedOutputFormat,
    /// Fail if impacted docs weren't updated.
    pub strict: bool,
}

/// A documentation file with its path mappings.
#[derive(Debug, Clone)]
pub struct DocMapping {
    /// Path to the documentation file (relative to config dir).
    pub doc_path: PathBuf,
    /// Document title.
    pub title: Option<String>,
    /// Glob patterns for code paths this doc covers.
    pub patterns: Vec<String>,
}

/// Information about an impacted document.
#[derive(Debug, Clone, Serialize)]
pub struct ImpactedDoc {
    /// Path to the documentation file.
    pub doc_path: PathBuf,
    /// Document title, if available.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// Code files that matched this doc's patterns.
    pub matched_files: Vec<PathBuf>,
    /// Whether the doc was also modified in the diff.
    pub was_updated: bool,
}

/// Results of the changed analysis.
#[derive(Debug, Serialize)]
pub struct ChangedResults {
    /// Base ref that was compared against.
    pub base_ref: String,
    /// Number of changed files in the diff.
    pub changed_files_count: usize,
    /// Docs that were impacted by code changes.
    pub impacted_docs: Vec<ImpactedDoc>,
    /// Docs that were impacted but not updated.
    pub missing_updates: Vec<PathBuf>,
}

/// Execute the `paver changed` command.
pub fn execute(args: ChangedArgs) -> Result<()> {
    // Find and load config
    let config_path = find_config()?;
    let config = PaverConfig::load(&config_path)?;
    let config_dir = config_path.parent().unwrap_or_else(|| Path::new("."));
    let docs_root = config_dir.join(&config.docs.root);

    // Determine base ref
    let base_ref = determine_base_ref(args.base.as_deref())?;

    // Get changed files from git
    let changed_files = get_changed_files(&base_ref)?;

    if changed_files.is_empty() {
        if args.format == ChangedOutputFormat::Text {
            println!("No changed files found compared to {}", base_ref);
        } else {
            let results = ChangedResults {
                base_ref,
                changed_files_count: 0,
                impacted_docs: vec![],
                missing_updates: vec![],
            };
            output_json(&results)?;
        }
        return Ok(());
    }

    // Load all docs with path mappings
    let doc_mappings = load_doc_mappings(&docs_root, config_dir)?;

    // Find impacted docs
    let impacted_docs = find_impacted_docs(&doc_mappings, &changed_files, config_dir);

    // Collect missing updates
    let missing_updates: Vec<PathBuf> = impacted_docs
        .iter()
        .filter(|d| !d.was_updated)
        .map(|d| d.doc_path.clone())
        .collect();

    let results = ChangedResults {
        base_ref: base_ref.clone(),
        changed_files_count: changed_files.len(),
        impacted_docs,
        missing_updates: missing_updates.clone(),
    };

    // Output results
    match args.format {
        ChangedOutputFormat::Text => output_text(&results),
        ChangedOutputFormat::Json => output_json(&results)?,
    }

    // Return error if strict mode and missing updates
    if args.strict && !missing_updates.is_empty() {
        anyhow::bail!(
            "Strict mode: {} impacted doc{} not updated",
            missing_updates.len(),
            if missing_updates.len() == 1 { "" } else { "s" }
        );
    }

    Ok(())
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

/// Get the list of changed files from git diff.
fn get_changed_files(base_ref: &str) -> Result<HashSet<PathBuf>> {
    let output = Command::new("git")
        .args(["diff", "--name-only", &format!("{}..HEAD", base_ref)])
        .output()
        .context("Failed to run git diff")?;

    if !output.status.success() {
        // Try without ..HEAD for cases like HEAD~1
        let output = Command::new("git")
            .args(["diff", "--name-only", base_ref])
            .output()
            .context("Failed to run git diff")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("git diff failed: {}", stderr);
        }

        return parse_git_diff_output(&output.stdout);
    }

    parse_git_diff_output(&output.stdout)
}

/// Parse git diff --name-only output into a set of paths.
fn parse_git_diff_output(output: &[u8]) -> Result<HashSet<PathBuf>> {
    let stdout = String::from_utf8_lossy(output);
    let files: HashSet<PathBuf> = stdout
        .lines()
        .filter(|line| !line.is_empty())
        .map(PathBuf::from)
        .collect();
    Ok(files)
}

/// Load all documentation files with their path mappings.
fn load_doc_mappings(docs_root: &Path, config_dir: &Path) -> Result<Vec<DocMapping>> {
    let mut mappings = Vec::new();
    load_doc_mappings_recursive(docs_root, config_dir, &mut mappings)?;
    Ok(mappings)
}

/// Recursively load documentation files.
fn load_doc_mappings_recursive(
    current: &Path,
    config_dir: &Path,
    mappings: &mut Vec<DocMapping>,
) -> Result<()> {
    let entries = match std::fs::read_dir(current) {
        Ok(entries) => entries,
        Err(_) => return Ok(()), // Directory doesn't exist or can't be read
    };

    for entry in entries {
        let entry = entry?;
        let path = entry.path();

        if path.is_dir() {
            // Skip templates directory
            if path.file_name().is_some_and(|n| n == "templates") {
                continue;
            }
            load_doc_mappings_recursive(&path, config_dir, mappings)?;
        } else if path.extension().is_some_and(|ext| ext == "md") {
            // Skip index.md
            if path.file_name().is_some_and(|n| n == "index.md") {
                continue;
            }

            if let Some(doc_mapping) = parse_doc_mapping(&path, config_dir)? {
                mappings.push(doc_mapping);
            }
        }
    }

    Ok(())
}

/// Parse a documentation file to extract path mappings.
fn parse_doc_mapping(path: &Path, config_dir: &Path) -> Result<Option<DocMapping>> {
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("Failed to read file: {}", path.display()))?;

    let relative_path = path.strip_prefix(config_dir).unwrap_or(path).to_path_buf();

    let title = extract_title(&content);
    let patterns = extract_paths_patterns(&content);

    // Only include docs that have path mappings
    if patterns.is_empty() {
        return Ok(None);
    }

    Ok(Some(DocMapping {
        doc_path: relative_path,
        title,
        patterns,
    }))
}

/// Extract the title from the first # heading.
fn extract_title(content: &str) -> Option<String> {
    for line in content.lines() {
        let trimmed = line.trim();
        if let Some(title) = trimmed.strip_prefix("# ")
            && !title.starts_with('#')
        {
            return Some(title.trim().to_string());
        }
    }
    None
}

/// Extract path patterns from the ## Paths section.
fn extract_paths_patterns(content: &str) -> Vec<String> {
    let mut patterns = Vec::new();
    let mut in_paths_section = false;

    for line in content.lines() {
        let trimmed = line.trim();

        // Check if entering Paths section
        if trimmed.starts_with("## Paths") {
            in_paths_section = true;
            continue;
        }

        // Check if leaving Paths section (another ## heading)
        if in_paths_section && trimmed.starts_with("## ") {
            break;
        }

        // Collect patterns (lines starting with - or *)
        if in_paths_section
            && let Some(pattern) = trimmed
                .strip_prefix("- ")
                .or_else(|| trimmed.strip_prefix("* "))
        {
            let pattern = pattern.trim();
            // Remove backticks if present
            let pattern = pattern.trim_matches('`');
            if !pattern.is_empty() {
                patterns.push(pattern.to_string());
            }
        }
    }

    patterns
}

/// Find docs impacted by the changed files.
fn find_impacted_docs(
    doc_mappings: &[DocMapping],
    changed_files: &HashSet<PathBuf>,
    config_dir: &Path,
) -> Vec<ImpactedDoc> {
    let mut impacted = Vec::new();

    for doc in doc_mappings {
        let mut matched_files = Vec::new();

        for changed_file in changed_files {
            if matches_any_pattern(changed_file, &doc.patterns) {
                matched_files.push(changed_file.clone());
            }
        }

        if !matched_files.is_empty() {
            // Check if the doc itself was updated
            let doc_full_path = config_dir.join(&doc.doc_path);
            let doc_relative = doc_full_path
                .strip_prefix(std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")))
                .unwrap_or(&doc.doc_path);

            let was_updated = changed_files.contains(&doc.doc_path)
                || changed_files.contains(doc_relative)
                || changed_files
                    .iter()
                    .any(|f| f.ends_with(&doc.doc_path) || doc.doc_path.ends_with(f));

            matched_files.sort();
            impacted.push(ImpactedDoc {
                doc_path: doc.doc_path.clone(),
                title: doc.title.clone(),
                matched_files,
                was_updated,
            });
        }
    }

    // Sort by doc path for consistent output
    impacted.sort_by(|a, b| a.doc_path.cmp(&b.doc_path));
    impacted
}

/// Check if a path matches any of the glob patterns.
fn matches_any_pattern(path: &Path, patterns: &[String]) -> bool {
    let path_str = path.to_string_lossy();

    for pattern_str in patterns {
        // Try to compile as glob pattern
        if let Ok(pattern) = Pattern::new(pattern_str)
            && pattern.matches(&path_str)
        {
            return true;
        }

        // Also do simple prefix matching for patterns like "src/foo/"
        if pattern_str.ends_with('/') || pattern_str.ends_with('*') {
            let prefix = pattern_str.trim_end_matches('*').trim_end_matches('/');
            if path_str.starts_with(prefix) {
                return true;
            }
        }
    }

    false
}

/// Output results in text format.
fn output_text(results: &ChangedResults) {
    println!(
        "Comparing against: {} ({} file{} changed)",
        results.base_ref,
        results.changed_files_count,
        if results.changed_files_count == 1 {
            ""
        } else {
            "s"
        }
    );
    println!();

    if results.impacted_docs.is_empty() {
        println!("No impacted documentation found.");
        println!("(No docs have ## Paths sections matching the changed files)");
        return;
    }

    println!(
        "Impacted documentation ({} doc{}):",
        results.impacted_docs.len(),
        if results.impacted_docs.len() == 1 {
            ""
        } else {
            "s"
        }
    );
    println!();

    for doc in &results.impacted_docs {
        let status = if doc.was_updated { "✓" } else { "✗" };
        let title = doc
            .title
            .as_deref()
            .unwrap_or_else(|| doc.doc_path.to_str().unwrap_or("unknown"));
        println!("  {} {} ({})", status, title, doc.doc_path.display());
        for matched in &doc.matched_files {
            println!("      ← {}", matched.display());
        }
    }

    println!();

    if results.missing_updates.is_empty() {
        println!("All impacted docs were updated.");
    } else {
        println!(
            "{} doc{} need{} review:",
            results.missing_updates.len(),
            if results.missing_updates.len() == 1 {
                ""
            } else {
                "s"
            },
            if results.missing_updates.len() == 1 {
                "s"
            } else {
                ""
            }
        );
        for path in &results.missing_updates {
            println!("  - {}", path.display());
        }
    }
}

/// Output results in JSON format.
fn output_json(results: &ChangedResults) -> Result<()> {
    let json = serde_json::to_string_pretty(results).context("Failed to serialize results")?;
    println!("{}", json);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_extract_title() {
        let content = "# My Document\n\n## Purpose\nSome content.";
        assert_eq!(extract_title(content), Some("My Document".to_string()));

        let content = "No title here";
        assert_eq!(extract_title(content), None);

        let content = "## Not H1\n# Actual Title";
        assert_eq!(extract_title(content), Some("Actual Title".to_string()));
    }

    #[test]
    fn test_extract_paths_patterns() {
        let content = r#"# Doc

## Purpose
Does something.

## Paths
- `src/commands/*.rs`
- `src/cli.rs`

## Examples
Some examples.
"#;

        let patterns = extract_paths_patterns(content);
        assert_eq!(patterns.len(), 2);
        assert_eq!(patterns[0], "src/commands/*.rs");
        assert_eq!(patterns[1], "src/cli.rs");
    }

    #[test]
    fn test_extract_paths_patterns_with_asterisks() {
        let content = r#"# Doc

## Paths
* src/foo.rs
* src/bar/*.rs
"#;

        let patterns = extract_paths_patterns(content);
        assert_eq!(patterns.len(), 2);
        assert_eq!(patterns[0], "src/foo.rs");
        assert_eq!(patterns[1], "src/bar/*.rs");
    }

    #[test]
    fn test_extract_paths_patterns_empty() {
        let content = r#"# Doc

## Purpose
No paths section.

## Examples
"#;

        let patterns = extract_paths_patterns(content);
        assert!(patterns.is_empty());
    }

    #[test]
    fn test_matches_any_pattern_exact() {
        let path = PathBuf::from("src/cli.rs");
        let patterns = vec!["src/cli.rs".to_string()];
        assert!(matches_any_pattern(&path, &patterns));
    }

    #[test]
    fn test_matches_any_pattern_glob() {
        let path = PathBuf::from("src/commands/check.rs");
        let patterns = vec!["src/commands/*.rs".to_string()];
        assert!(matches_any_pattern(&path, &patterns));
    }

    #[test]
    fn test_matches_any_pattern_glob_recursive() {
        let path = PathBuf::from("src/commands/sub/deep.rs");
        let patterns = vec!["src/**/*.rs".to_string()];
        assert!(matches_any_pattern(&path, &patterns));
    }

    #[test]
    fn test_matches_any_pattern_no_match() {
        let path = PathBuf::from("tests/test.rs");
        let patterns = vec!["src/*.rs".to_string()];
        assert!(!matches_any_pattern(&path, &patterns));
    }

    #[test]
    fn test_matches_any_pattern_prefix() {
        let path = PathBuf::from("src/commands/check.rs");
        let patterns = vec!["src/commands/".to_string()];
        assert!(matches_any_pattern(&path, &patterns));
    }

    #[test]
    fn test_parse_doc_mapping() {
        let temp_dir = TempDir::new().unwrap();
        let doc_path = temp_dir.path().join("test.md");

        let content = r#"# Test Component

## Purpose
Test document.

## Paths
- `src/test.rs`
- `src/lib/*.rs`

## Examples
Example content.
"#;
        fs::write(&doc_path, content).unwrap();

        let mapping = parse_doc_mapping(&doc_path, temp_dir.path())
            .unwrap()
            .unwrap();

        assert_eq!(mapping.doc_path, PathBuf::from("test.md"));
        assert_eq!(mapping.title, Some("Test Component".to_string()));
        assert_eq!(mapping.patterns.len(), 2);
        assert_eq!(mapping.patterns[0], "src/test.rs");
        assert_eq!(mapping.patterns[1], "src/lib/*.rs");
    }

    #[test]
    fn test_parse_doc_mapping_no_paths() {
        let temp_dir = TempDir::new().unwrap();
        let doc_path = temp_dir.path().join("test.md");

        let content = r#"# Test

## Purpose
No paths section.
"#;
        fs::write(&doc_path, content).unwrap();

        let mapping = parse_doc_mapping(&doc_path, temp_dir.path()).unwrap();
        assert!(mapping.is_none());
    }

    #[test]
    fn test_find_impacted_docs() {
        let doc_mappings = vec![
            DocMapping {
                doc_path: PathBuf::from("docs/cli.md"),
                title: Some("CLI".to_string()),
                patterns: vec!["src/cli.rs".to_string(), "src/main.rs".to_string()],
            },
            DocMapping {
                doc_path: PathBuf::from("docs/commands.md"),
                title: Some("Commands".to_string()),
                patterns: vec!["src/commands/*.rs".to_string()],
            },
        ];

        let mut changed_files = HashSet::new();
        changed_files.insert(PathBuf::from("src/cli.rs"));
        changed_files.insert(PathBuf::from("src/commands/check.rs"));
        changed_files.insert(PathBuf::from("tests/test.rs"));

        let impacted = find_impacted_docs(&doc_mappings, &changed_files, Path::new("."));

        assert_eq!(impacted.len(), 2);

        let cli_doc = impacted
            .iter()
            .find(|d| d.doc_path.to_string_lossy().contains("cli"))
            .unwrap();
        assert_eq!(cli_doc.matched_files.len(), 1);
        assert!(cli_doc.matched_files.contains(&PathBuf::from("src/cli.rs")));

        let cmd_doc = impacted
            .iter()
            .find(|d| d.doc_path.to_string_lossy().contains("commands"))
            .unwrap();
        assert_eq!(cmd_doc.matched_files.len(), 1);
        assert!(
            cmd_doc
                .matched_files
                .contains(&PathBuf::from("src/commands/check.rs"))
        );
    }

    #[test]
    fn test_find_impacted_docs_with_update() {
        let doc_mappings = vec![DocMapping {
            doc_path: PathBuf::from("docs/cli.md"),
            title: Some("CLI".to_string()),
            patterns: vec!["src/cli.rs".to_string()],
        }];

        let mut changed_files = HashSet::new();
        changed_files.insert(PathBuf::from("src/cli.rs"));
        changed_files.insert(PathBuf::from("docs/cli.md")); // Doc was also updated

        let impacted = find_impacted_docs(&doc_mappings, &changed_files, Path::new("."));

        assert_eq!(impacted.len(), 1);
        assert!(impacted[0].was_updated);
    }

    #[test]
    fn test_parse_git_diff_output() {
        let output = b"src/cli.rs\nsrc/main.rs\ndocs/readme.md\n";
        let files = parse_git_diff_output(output).unwrap();

        assert_eq!(files.len(), 3);
        assert!(files.contains(&PathBuf::from("src/cli.rs")));
        assert!(files.contains(&PathBuf::from("src/main.rs")));
        assert!(files.contains(&PathBuf::from("docs/readme.md")));
    }

    #[test]
    fn test_parse_git_diff_output_empty() {
        let output = b"";
        let files = parse_git_diff_output(output).unwrap();
        assert!(files.is_empty());

        let output = b"\n\n";
        let files = parse_git_diff_output(output).unwrap();
        assert!(files.is_empty());
    }

    #[test]
    fn test_load_doc_mappings() {
        let temp_dir = TempDir::new().unwrap();
        let docs_dir = temp_dir.path().join("docs");
        fs::create_dir_all(&docs_dir).unwrap();

        // Create a doc with paths
        let doc_with_paths = r#"# Component

## Paths
- `src/*.rs`

## Purpose
Has paths.
"#;
        fs::write(docs_dir.join("component.md"), doc_with_paths).unwrap();

        // Create a doc without paths
        let doc_without_paths = r#"# Other

## Purpose
No paths section.
"#;
        fs::write(docs_dir.join("other.md"), doc_without_paths).unwrap();

        // Create an index.md (should be skipped)
        fs::write(docs_dir.join("index.md"), "# Index").unwrap();

        let mappings = load_doc_mappings(&docs_dir, temp_dir.path()).unwrap();

        // Should only include the doc with paths, not the one without or index.md
        assert_eq!(mappings.len(), 1);
        assert!(mappings[0].doc_path.to_string_lossy().contains("component"));
    }
}
