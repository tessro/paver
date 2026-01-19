//! Implementation of the `pave coverage-changed` command for checking coverage of new files.
//!
//! This module analyzes git diffs to find newly added code files and checks if they
//! are covered by documentation patterns defined in the `## Paths` sections of PAVED documents.

use anyhow::{Context, Result};
use glob::Pattern;
use serde::Serialize;
use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::cli::CoverageOutputFormat;
use crate::config::{CONFIG_FILENAME, PaveConfig};
use crate::parser::CodeBlockTracker;

/// Arguments for the `pave coverage-changed` command.
pub struct CoverageChangedArgs {
    /// Git ref to compare against.
    pub base: Option<String>,
    /// Output format.
    pub format: CoverageOutputFormat,
    /// Patterns to include (only consider these code files).
    pub include: Vec<String>,
    /// Patterns to exclude (skip these code files).
    pub exclude: Vec<String>,
}

/// A documentation file with its path mappings.
#[derive(Debug, Clone)]
struct DocMapping {
    /// Glob patterns for code paths this doc covers.
    patterns: Vec<String>,
}

/// Information about an uncovered new file.
#[derive(Debug, Clone, Serialize)]
pub struct UncoveredNewFile {
    /// Path to the uncovered file.
    pub path: PathBuf,
    /// Suggested documentation file that could cover this.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub suggested_doc: Option<String>,
}

/// Results of the coverage-changed analysis.
#[derive(Debug, Serialize)]
pub struct CoverageChangedResults {
    /// Base ref that was compared against.
    pub base_ref: String,
    /// Total number of new files in the diff.
    pub new_files_count: usize,
    /// Number of new code files (after filtering).
    pub new_code_files_count: usize,
    /// Number of covered new files.
    pub covered_count: usize,
    /// Number of uncovered new files.
    pub uncovered_count: usize,
    /// List of uncovered new files.
    pub uncovered: Vec<UncoveredNewFile>,
    /// Whether all new code files are covered.
    pub all_covered: bool,
}

/// Execute the `pave coverage-changed` command.
pub fn execute(args: CoverageChangedArgs) -> Result<()> {
    // Find and load config
    let config_path = find_config()?;
    let config = PaveConfig::load(&config_path)?;
    let config_dir = config_path.parent().unwrap_or_else(|| Path::new("."));
    let docs_root = config_dir.join(&config.docs.root);

    // Build exclude patterns (merge config + CLI)
    let mut exclude_patterns = config.mapping.exclude.clone();
    exclude_patterns.extend(args.exclude.clone());

    // Determine base ref
    let base_ref = determine_base_ref(args.base.as_deref())?;

    // Get added files from git
    let added_files = get_added_files(&base_ref)?;
    let new_files_count = added_files.len();

    if added_files.is_empty() {
        if args.format == CoverageOutputFormat::Text {
            println!("No new files found compared to {}", base_ref);
        } else {
            let results = CoverageChangedResults {
                base_ref,
                new_files_count: 0,
                new_code_files_count: 0,
                covered_count: 0,
                uncovered_count: 0,
                uncovered: vec![],
                all_covered: true,
            };
            output_json(&results)?;
        }
        return Ok(());
    }

    // Filter to code files only, applying include/exclude patterns
    let new_code_files: Vec<PathBuf> = added_files
        .into_iter()
        .filter(|p| is_code_file(p))
        .filter(|p| {
            // Check exclusions
            !matches_any_pattern(p, &exclude_patterns)
        })
        .filter(|p| {
            // If include patterns specified, file must match at least one
            if args.include.is_empty() {
                true
            } else {
                matches_any_pattern(p, &args.include)
            }
        })
        .collect();

    if new_code_files.is_empty() {
        if args.format == CoverageOutputFormat::Text {
            println!(
                "No new code files found compared to {} (after filtering)",
                base_ref
            );
        } else {
            let results = CoverageChangedResults {
                base_ref,
                new_files_count: 0,
                new_code_files_count: 0,
                covered_count: 0,
                uncovered_count: 0,
                uncovered: vec![],
                all_covered: true,
            };
            output_json(&results)?;
        }
        return Ok(());
    }

    // Load all doc mappings
    let doc_mappings = load_doc_mappings(&docs_root)?;

    // Determine coverage for each new file
    let (covered, uncovered) = analyze_coverage(&new_code_files, &doc_mappings);

    let results = CoverageChangedResults {
        base_ref: base_ref.clone(),
        new_files_count,
        new_code_files_count: new_code_files.len(),
        covered_count: covered.len(),
        uncovered_count: uncovered.len(),
        uncovered: uncovered
            .iter()
            .map(|p| UncoveredNewFile {
                path: p.clone(),
                suggested_doc: suggest_doc_name(p),
            })
            .collect(),
        all_covered: uncovered.is_empty(),
    };

    // Output results
    match args.format {
        CoverageOutputFormat::Text => output_text(&results),
        CoverageOutputFormat::Json => output_json(&results)?,
    }

    // Return error if any new code files are uncovered
    if !results.all_covered {
        anyhow::bail!(
            "{} new code file{} not covered by documentation",
            results.uncovered_count,
            if results.uncovered_count == 1 { "" } else { "s" }
        );
    }

    Ok(())
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

/// Get the list of added files from git diff.
fn get_added_files(base_ref: &str) -> Result<Vec<PathBuf>> {
    // Use --diff-filter=A to only get added files
    let output = Command::new("git")
        .args([
            "diff",
            "--name-only",
            "--diff-filter=A",
            &format!("{}..HEAD", base_ref),
        ])
        .output()
        .context("Failed to run git diff")?;

    if !output.status.success() {
        // Try without ..HEAD for cases like HEAD~1
        let output = Command::new("git")
            .args(["diff", "--name-only", "--diff-filter=A", base_ref])
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

/// Parse git diff --name-only output into a list of paths.
fn parse_git_diff_output(output: &[u8]) -> Result<Vec<PathBuf>> {
    let stdout = String::from_utf8_lossy(output);
    let files: Vec<PathBuf> = stdout
        .lines()
        .filter(|line| !line.is_empty())
        .map(PathBuf::from)
        .collect();
    Ok(files)
}

/// Check if a file is a code file based on extension.
fn is_code_file(path: &Path) -> bool {
    let code_extensions = [
        "rs", "py", "js", "ts", "jsx", "tsx", "go", "java", "c", "cpp", "h", "hpp", "rb", "php",
        "swift", "kt", "scala", "sh", "bash", "zsh", "pl", "pm", "lua", "ex", "exs", "erl", "hrl",
        "hs", "ml", "mli", "fs", "fsi", "clj", "cljs", "lisp", "el", "vim", "sql",
    ];

    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| code_extensions.contains(&ext))
        .unwrap_or(false)
}

/// Load all documentation files with their path mappings.
fn load_doc_mappings(docs_root: &Path) -> Result<Vec<DocMapping>> {
    let mut mappings = Vec::new();
    load_doc_mappings_recursive(docs_root, &mut mappings)?;
    Ok(mappings)
}

/// Recursively load documentation files.
fn load_doc_mappings_recursive(
    current: &Path,
    mappings: &mut Vec<DocMapping>,
) -> Result<()> {
    let entries = match std::fs::read_dir(current) {
        Ok(entries) => entries,
        Err(_) => return Ok(()),
    };

    for entry in entries {
        let entry = entry?;
        let path = entry.path();

        if path.is_dir() {
            // Skip templates directory
            if path.file_name().is_some_and(|n| n == "templates") {
                continue;
            }
            load_doc_mappings_recursive(&path, mappings)?;
        } else if path.extension().is_some_and(|ext| ext == "md") {
            // Skip index.md
            if path.file_name().is_some_and(|n| n == "index.md") {
                continue;
            }

            if let Some(doc_mapping) = parse_doc_mapping(&path)? {
                mappings.push(doc_mapping);
            }
        }
    }

    Ok(())
}

/// Parse a documentation file to extract path mappings.
fn parse_doc_mapping(path: &Path) -> Result<Option<DocMapping>> {
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("Failed to read file: {}", path.display()))?;

    let patterns = extract_paths_patterns(&content);

    // Only include docs that have path mappings
    if patterns.is_empty() {
        return Ok(None);
    }

    Ok(Some(DocMapping { patterns }))
}

/// Extract path patterns from the ## Paths section.
fn extract_paths_patterns(content: &str) -> Vec<String> {
    let mut patterns = Vec::new();
    let mut in_paths_section = false;
    let mut tracker = CodeBlockTracker::new();

    for line in content.lines() {
        let trimmed = line.trim();

        // Track code blocks (handles language tags and nested fences)
        tracker.process_line(trimmed);

        // Skip processing if inside a code block
        if tracker.in_code_block() {
            continue;
        }

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

/// Analyze coverage of code files against doc patterns.
fn analyze_coverage(code_files: &[PathBuf], doc_mappings: &[DocMapping]) -> (Vec<PathBuf>, Vec<PathBuf>) {
    let mut covered = Vec::new();
    let mut uncovered = Vec::new();

    // Collect all patterns from all docs
    let all_patterns: Vec<&String> = doc_mappings
        .iter()
        .flat_map(|d| d.patterns.iter())
        .collect();

    for file in code_files {
        if matches_any_pattern(
            file,
            &all_patterns.iter().map(|s| s.as_str()).collect::<Vec<_>>(),
        ) {
            covered.push(file.clone());
        } else {
            uncovered.push(file.clone());
        }
    }

    (covered, uncovered)
}

/// Check if a path matches any of the glob patterns.
fn matches_any_pattern<S: AsRef<str>>(path: &Path, patterns: &[S]) -> bool {
    let path_str = path.to_string_lossy();

    for pattern_str in patterns {
        let pattern_str = pattern_str.as_ref();

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

/// Suggest a documentation file name for a code file.
fn suggest_doc_name(path: &Path) -> Option<String> {
    path.parent().and_then(|parent| {
        let dir_name = parent.file_name()?.to_str()?;
        Some(format!("docs/components/{}.md", dir_name))
    })
}

/// Output results in text format.
fn output_text(results: &CoverageChangedResults) {
    println!(
        "Comparing against: {} ({} new code file{})",
        results.base_ref,
        results.new_code_files_count,
        if results.new_code_files_count == 1 {
            ""
        } else {
            "s"
        }
    );
    println!();

    if results.new_code_files_count == 0 {
        println!("No new code files to check.");
        return;
    }

    println!(
        "Covered: {} file{} ({:.1}%)",
        results.covered_count,
        if results.covered_count == 1 { "" } else { "s" },
        if results.new_code_files_count > 0 {
            (results.covered_count as f64 / results.new_code_files_count as f64) * 100.0
        } else {
            100.0
        }
    );
    println!(
        "Uncovered: {} file{} ({:.1}%)",
        results.uncovered_count,
        if results.uncovered_count == 1 { "" } else { "s" },
        if results.new_code_files_count > 0 {
            (results.uncovered_count as f64 / results.new_code_files_count as f64) * 100.0
        } else {
            0.0
        }
    );
    println!();

    if !results.uncovered.is_empty() {
        println!("Uncovered New Files ({}):", results.uncovered.len());
        for file in &results.uncovered {
            println!("  {}", file.path.display());
            if let Some(ref suggested) = file.suggested_doc {
                println!("      suggested: {}", suggested);
            }
        }
        println!();
    }

    if results.all_covered {
        println!("All new code files are covered by documentation.");
    } else {
        println!(
            "{} new code file{} need{} documentation coverage.",
            results.uncovered_count,
            if results.uncovered_count == 1 { "" } else { "s" },
            if results.uncovered_count == 1 { "s" } else { "" }
        );
    }
}

/// Output results in JSON format.
fn output_json(results: &CoverageChangedResults) -> Result<()> {
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
    fn test_extract_paths_patterns_skips_code_blocks() {
        let content = r#"# Doc

## Setup

```markdown
## Paths
- `should/not/match`
```

## Paths
- `src/real/*.rs`
"#;

        let patterns = extract_paths_patterns(content);
        assert_eq!(patterns.len(), 1);
        assert_eq!(patterns[0], "src/real/*.rs");
    }

    #[test]
    fn test_matches_any_pattern_exact() {
        let path = PathBuf::from("src/cli.rs");
        let patterns = vec!["src/cli.rs"];
        assert!(matches_any_pattern(&path, &patterns));
    }

    #[test]
    fn test_matches_any_pattern_glob() {
        let path = PathBuf::from("src/commands/check.rs");
        let patterns = vec!["src/commands/*.rs"];
        assert!(matches_any_pattern(&path, &patterns));
    }

    #[test]
    fn test_matches_any_pattern_glob_recursive() {
        let path = PathBuf::from("src/commands/sub/deep.rs");
        let patterns = vec!["src/**/*.rs"];
        assert!(matches_any_pattern(&path, &patterns));
    }

    #[test]
    fn test_matches_any_pattern_no_match() {
        let path = PathBuf::from("tests/test.rs");
        let patterns = vec!["src/*.rs"];
        assert!(!matches_any_pattern(&path, &patterns));
    }

    #[test]
    fn test_matches_any_pattern_prefix() {
        let path = PathBuf::from("src/commands/check.rs");
        let patterns = vec!["src/commands/"];
        assert!(matches_any_pattern(&path, &patterns));
    }

    #[test]
    fn test_is_code_file() {
        assert!(is_code_file(Path::new("src/main.rs")));
        assert!(is_code_file(Path::new("lib/utils.py")));
        assert!(is_code_file(Path::new("app/index.ts")));
        assert!(!is_code_file(Path::new("docs/readme.md")));
        assert!(!is_code_file(Path::new("config.toml")));
        assert!(!is_code_file(Path::new("data.json")));
    }

    #[test]
    fn test_analyze_coverage() {
        let code_files = vec![
            PathBuf::from("src/cli.rs"),
            PathBuf::from("src/main.rs"),
            PathBuf::from("src/utils.rs"),
        ];

        let doc_mappings = vec![DocMapping {
            patterns: vec!["src/cli.rs".to_string(), "src/main.rs".to_string()],
        }];

        let (covered, uncovered) = analyze_coverage(&code_files, &doc_mappings);

        assert_eq!(covered.len(), 2);
        assert!(covered.contains(&PathBuf::from("src/cli.rs")));
        assert!(covered.contains(&PathBuf::from("src/main.rs")));
        assert_eq!(uncovered.len(), 1);
        assert!(uncovered.contains(&PathBuf::from("src/utils.rs")));
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

        let mapping = parse_doc_mapping(&doc_path).unwrap().unwrap();

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

        let mapping = parse_doc_mapping(&doc_path).unwrap();
        assert!(mapping.is_none());
    }

    #[test]
    fn test_suggest_doc_name() {
        let path = PathBuf::from("src/commands/check.rs");
        let suggested = suggest_doc_name(&path);
        assert_eq!(suggested, Some("docs/components/commands.md".to_string()));
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

        let mappings = load_doc_mappings(&docs_dir).unwrap();

        // Should only include the doc with paths, not the one without or index.md
        assert_eq!(mappings.len(), 1);
        assert!(mappings[0].patterns.contains(&"src/*.rs".to_string()));
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
}
