//! Implementation of the `paver coverage` command for analyzing code-to-doc coverage.
//!
//! This module analyzes which code files are covered by documentation patterns
//! defined in the `## Paths` sections of PAVED documents.

use anyhow::{Context, Result};
use glob::Pattern;
use serde::Serialize;
use std::collections::HashMap;
use std::env;
use std::path::{Path, PathBuf};

use crate::cli::CoverageOutputFormat;
use crate::config::{CONFIG_FILENAME, PaverConfig};

/// Arguments for the `paver coverage` command.
pub struct CoverageArgs {
    /// Path to analyze (defaults to project root).
    pub path: Option<PathBuf>,
    /// Output format.
    pub format: CoverageOutputFormat,
    /// Minimum coverage percentage to pass.
    pub threshold: Option<u32>,
    /// Patterns to include (only consider these code files).
    pub include: Vec<String>,
    /// Patterns to exclude (skip these code files).
    pub exclude: Vec<String>,
}

/// Coverage statistics for a directory.
#[derive(Debug, Clone, Serialize)]
pub struct DirectoryCoverage {
    /// Directory path.
    pub path: String,
    /// Number of covered files.
    pub covered: usize,
    /// Total number of files.
    pub total: usize,
    /// Coverage percentage.
    pub percentage: f64,
}

/// Information about an uncovered file.
#[derive(Debug, Clone, Serialize)]
pub struct UncoveredFile {
    /// Path to the uncovered file.
    pub path: PathBuf,
    /// Suggested documentation file that could cover this.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub suggested_doc: Option<String>,
}

/// A suggestion for improving coverage.
#[derive(Debug, Clone, Serialize)]
pub struct CoverageSuggestion {
    /// Description of the suggestion.
    pub description: String,
    /// Files that would be covered by this suggestion.
    pub files: Vec<PathBuf>,
}

/// Results of the coverage analysis.
#[derive(Debug, Serialize)]
pub struct CoverageResults {
    /// Number of covered files.
    pub covered_files: usize,
    /// Number of uncovered files.
    pub uncovered_files: usize,
    /// Total number of code files.
    pub total_files: usize,
    /// Overall coverage percentage.
    pub coverage_percentage: f64,
    /// Coverage by directory.
    pub by_directory: Vec<DirectoryCoverage>,
    /// List of uncovered files.
    pub uncovered: Vec<UncoveredFile>,
    /// Suggestions for improving coverage.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub suggestions: Vec<CoverageSuggestion>,
    /// Whether the threshold was met (if specified).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub threshold_met: Option<bool>,
    /// The threshold that was checked.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub threshold: Option<u32>,
}

/// A documentation file with its path mappings.
#[derive(Debug, Clone)]
struct DocMapping {
    /// Glob patterns for code paths this doc covers.
    patterns: Vec<String>,
}

/// Execute the `paver coverage` command.
pub fn execute(args: CoverageArgs) -> Result<()> {
    // Find and load config
    let config_path = find_config()?;
    let config = PaverConfig::load(&config_path)?;
    let config_dir = config_path.parent().unwrap_or_else(|| Path::new("."));
    let docs_root = config_dir.join(&config.docs.root);

    // Determine the path to analyze
    let analyze_path = args.path.unwrap_or_else(|| config_dir.to_path_buf());

    // Build exclude patterns (merge config + CLI)
    let mut exclude_patterns = config.mapping.exclude.clone();
    exclude_patterns.extend(args.exclude.clone());

    // Collect code files
    let code_files = collect_code_files(&analyze_path, &args.include, &exclude_patterns)?;

    if code_files.is_empty() {
        if args.format == CoverageOutputFormat::Text {
            println!("No code files found to analyze.");
            if !args.include.is_empty() {
                println!("Include patterns: {:?}", args.include);
            }
        } else {
            let results = CoverageResults {
                covered_files: 0,
                uncovered_files: 0,
                total_files: 0,
                coverage_percentage: 100.0,
                by_directory: vec![],
                uncovered: vec![],
                suggestions: vec![],
                threshold_met: args.threshold.map(|_| true),
                threshold: args.threshold,
            };
            output_json(&results)?;
        }
        return Ok(());
    }

    // Load all doc mappings
    let doc_mappings = load_doc_mappings(&docs_root, config_dir)?;

    // Determine coverage for each file
    let (covered, uncovered) = analyze_coverage(&code_files, &doc_mappings, config_dir);

    // Calculate directory-level coverage
    let by_directory = calculate_directory_coverage(&covered, &uncovered);

    // Generate suggestions
    let suggestions = generate_suggestions(&uncovered, config_dir);

    // Calculate percentages
    let total_files = code_files.len();
    let covered_count = covered.len();
    let uncovered_count = uncovered.len();
    let coverage_percentage = if total_files > 0 {
        (covered_count as f64 / total_files as f64) * 100.0
    } else {
        100.0
    };

    // Check threshold
    let threshold_met = args
        .threshold
        .map(|t| coverage_percentage >= t as f64);

    let results = CoverageResults {
        covered_files: covered_count,
        uncovered_files: uncovered_count,
        total_files,
        coverage_percentage,
        by_directory,
        uncovered: uncovered
            .iter()
            .map(|p| UncoveredFile {
                path: p.clone(),
                suggested_doc: suggest_doc_name(p),
            })
            .collect(),
        suggestions,
        threshold_met,
        threshold: args.threshold,
    };

    // Output results
    match args.format {
        CoverageOutputFormat::Text => output_text(&results),
        CoverageOutputFormat::Json => output_json(&results)?,
    }

    // Return error if threshold not met
    if let Some(false) = threshold_met {
        anyhow::bail!(
            "Coverage {:.1}% is below threshold {}%",
            coverage_percentage,
            args.threshold.unwrap()
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

/// Collect code files from the given path, applying include/exclude patterns.
fn collect_code_files(
    root: &Path,
    include: &[String],
    exclude: &[String],
) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    collect_code_files_recursive(root, root, include, exclude, &mut files)?;
    files.sort();
    Ok(files)
}

/// Recursively collect code files.
fn collect_code_files_recursive(
    root: &Path,
    current: &Path,
    include: &[String],
    exclude: &[String],
    files: &mut Vec<PathBuf>,
) -> Result<()> {
    let entries = match std::fs::read_dir(current) {
        Ok(entries) => entries,
        Err(_) => return Ok(()),
    };

    for entry in entries {
        let entry = entry?;
        let path = entry.path();
        let relative = path.strip_prefix(root).unwrap_or(&path);

        // Check exclusions first
        if matches_any_pattern(relative, exclude) {
            continue;
        }

        // Skip hidden directories and common non-code directories
        if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
            if name.starts_with('.') {
                continue;
            }
            if path.is_dir() {
                // Skip common non-code directories
                if matches!(
                    name,
                    "target" | "node_modules" | "dist" | "build" | "__pycache__" | ".git"
                ) {
                    continue;
                }
            }
        }

        if path.is_dir() {
            collect_code_files_recursive(root, &path, include, exclude, files)?;
        } else if is_code_file(&path) {
            // If include patterns specified, file must match at least one
            if !include.is_empty() && !matches_any_pattern(relative, include) {
                continue;
            }
            files.push(relative.to_path_buf());
        }
    }

    Ok(())
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
fn parse_doc_mapping(path: &Path, _config_dir: &Path) -> Result<Option<DocMapping>> {
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
    let mut in_code_block = false;

    for line in content.lines() {
        let trimmed = line.trim();

        // Track code blocks to skip headings inside them
        if trimmed.starts_with("```") {
            in_code_block = !in_code_block;
            continue;
        }

        // Skip processing if inside a code block
        if in_code_block {
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
fn analyze_coverage(
    code_files: &[PathBuf],
    doc_mappings: &[DocMapping],
    _config_dir: &Path,
) -> (Vec<PathBuf>, Vec<PathBuf>) {
    let mut covered = Vec::new();
    let mut uncovered = Vec::new();

    // Collect all patterns from all docs
    let all_patterns: Vec<&String> = doc_mappings
        .iter()
        .flat_map(|d| d.patterns.iter())
        .collect();

    for file in code_files {
        if matches_any_pattern(file, &all_patterns.iter().map(|s| s.as_str()).collect::<Vec<_>>())
        {
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

/// Calculate coverage statistics by directory.
fn calculate_directory_coverage(
    covered: &[PathBuf],
    uncovered: &[PathBuf],
) -> Vec<DirectoryCoverage> {
    let mut dir_stats: HashMap<String, (usize, usize)> = HashMap::new();

    // Count covered files per directory
    for file in covered {
        if let Some(parent) = file.parent() {
            let dir = parent.to_string_lossy().to_string();
            let dir = if dir.is_empty() { ".".to_string() } else { dir };
            let entry = dir_stats.entry(dir).or_insert((0, 0));
            entry.0 += 1; // covered
            entry.1 += 1; // total
        }
    }

    // Count uncovered files per directory
    for file in uncovered {
        if let Some(parent) = file.parent() {
            let dir = parent.to_string_lossy().to_string();
            let dir = if dir.is_empty() { ".".to_string() } else { dir };
            let entry = dir_stats.entry(dir).or_insert((0, 0));
            entry.1 += 1; // total only
        }
    }

    // Convert to DirectoryCoverage structs
    let mut result: Vec<DirectoryCoverage> = dir_stats
        .into_iter()
        .map(|(path, (covered, total))| {
            let percentage = if total > 0 {
                (covered as f64 / total as f64) * 100.0
            } else {
                100.0
            };
            DirectoryCoverage {
                path,
                covered,
                total,
                percentage,
            }
        })
        .collect();

    // Sort by path for consistent output
    result.sort_by(|a, b| a.path.cmp(&b.path));
    result
}

/// Generate suggestions for improving coverage.
fn generate_suggestions(uncovered: &[PathBuf], _config_dir: &Path) -> Vec<CoverageSuggestion> {
    // Group uncovered files by directory
    let mut by_dir: HashMap<String, Vec<PathBuf>> = HashMap::new();

    for file in uncovered {
        if let Some(parent) = file.parent() {
            let dir = parent.to_string_lossy().to_string();
            let dir = if dir.is_empty() { ".".to_string() } else { dir };
            by_dir.entry(dir).or_default().push(file.clone());
        }
    }

    // Create suggestions for directories with multiple uncovered files
    let mut suggestions: Vec<CoverageSuggestion> = by_dir
        .into_iter()
        .filter(|(_, files)| files.len() >= 2)
        .map(|(dir, files)| {
            let doc_name = suggest_doc_name_for_dir(&dir);
            CoverageSuggestion {
                description: format!("Create {} covering {}/", doc_name, dir),
                files,
            }
        })
        .collect();

    // Sort by number of files (most impactful first)
    suggestions.sort_by(|a, b| b.files.len().cmp(&a.files.len()));

    // Limit to top 5 suggestions
    suggestions.truncate(5);
    suggestions
}

/// Suggest a documentation file name for a code file.
fn suggest_doc_name(path: &Path) -> Option<String> {
    path.parent().and_then(|parent| {
        let dir_name = parent.file_name()?.to_str()?;
        Some(format!("docs/components/{}.md", dir_name))
    })
}

/// Suggest a documentation file name for a directory.
fn suggest_doc_name_for_dir(dir: &str) -> String {
    let parts: Vec<&str> = dir.split('/').collect();
    let name = parts.last().unwrap_or(&"component");
    format!("docs/components/{}.md", name)
}

/// Output results in text format.
fn output_text(results: &CoverageResults) {
    println!("Code Coverage Report");
    println!("====================");
    println!();
    println!(
        "Covered: {} file{} ({:.1}%)",
        results.covered_files,
        if results.covered_files == 1 { "" } else { "s" },
        results.coverage_percentage
    );
    println!(
        "Uncovered: {} file{} ({:.1}%)",
        results.uncovered_files,
        if results.uncovered_files == 1 { "" } else { "s" },
        100.0 - results.coverage_percentage
    );
    println!();

    if !results.by_directory.is_empty() {
        println!("By Directory:");
        for dir in &results.by_directory {
            println!(
                "  {:<30} {}/{} files ({:.0}%)",
                format!("{}/", dir.path),
                dir.covered,
                dir.total,
                dir.percentage
            );
        }
        println!();
    }

    if !results.uncovered.is_empty() {
        println!(
            "Uncovered Files ({}):",
            results.uncovered.len()
        );
        // Limit display to first 20 files
        let display_limit = 20;
        for file in results.uncovered.iter().take(display_limit) {
            println!("  {}", file.path.display());
        }
        if results.uncovered.len() > display_limit {
            println!(
                "  ... and {} more",
                results.uncovered.len() - display_limit
            );
        }
        println!();
    }

    if !results.suggestions.is_empty() {
        println!("Suggested Actions:");
        for (i, suggestion) in results.suggestions.iter().enumerate() {
            println!(
                "  {}. {} ({} files)",
                i + 1,
                suggestion.description,
                suggestion.files.len()
            );
        }
        println!();
    }

    if let Some(threshold) = results.threshold {
        let status = if results.threshold_met.unwrap_or(true) {
            "✓ PASS"
        } else {
            "✗ FAIL"
        };
        println!(
            "Threshold: {}% (actual: {:.1}%) {}",
            threshold, results.coverage_percentage, status
        );
    }
}

/// Output results in JSON format.
fn output_json(results: &CoverageResults) -> Result<()> {
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
    fn test_calculate_directory_coverage() {
        let covered = vec![
            PathBuf::from("src/commands/check.rs"),
            PathBuf::from("src/commands/verify.rs"),
            PathBuf::from("src/cli.rs"),
        ];
        let uncovered = vec![
            PathBuf::from("src/commands/new.rs"),
            PathBuf::from("src/utils.rs"),
        ];

        let coverage = calculate_directory_coverage(&covered, &uncovered);

        // Should have 2 directories: src/commands and src
        assert_eq!(coverage.len(), 2);

        // Find src/commands stats
        let cmd_stats = coverage.iter().find(|c| c.path == "src/commands").unwrap();
        assert_eq!(cmd_stats.covered, 2);
        assert_eq!(cmd_stats.total, 3);

        // Find src stats
        let src_stats = coverage.iter().find(|c| c.path == "src").unwrap();
        assert_eq!(src_stats.covered, 1);
        assert_eq!(src_stats.total, 2);
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

        let (covered, uncovered) = analyze_coverage(&code_files, &doc_mappings, Path::new("."));

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

        let mapping = parse_doc_mapping(&doc_path, temp_dir.path())
            .unwrap()
            .unwrap();

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
    fn test_generate_suggestions() {
        let uncovered = vec![
            PathBuf::from("src/utils/helper.rs"),
            PathBuf::from("src/utils/format.rs"),
            PathBuf::from("src/utils/parse.rs"),
            PathBuf::from("src/single.rs"),
        ];

        let suggestions = generate_suggestions(&uncovered, Path::new("."));

        // Should have 1 suggestion for src/utils (3 files) but not for src (only 1 file)
        assert_eq!(suggestions.len(), 1);
        assert!(suggestions[0].description.contains("src/utils"));
        assert_eq!(suggestions[0].files.len(), 3);
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

        let mappings = load_doc_mappings(&docs_dir, temp_dir.path()).unwrap();

        // Should only include the doc with paths, not the one without or index.md
        assert_eq!(mappings.len(), 1);
        assert!(mappings[0].patterns.contains(&"src/*.rs".to_string()));
    }
}
