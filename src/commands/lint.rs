//! Implementation of the `paver lint` command for prose quality checks.

use anyhow::{Context, Result};
use regex::Regex;
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::env;
use std::path::{Path, PathBuf};

use crate::cli::OutputFormat;
use crate::config::{CONFIG_FILENAME, LintSection, PaverConfig};
use crate::parser::ParsedDoc;

/// Arguments for the `paver lint` command.
pub struct LintArgs {
    /// Specific files or directories to lint.
    pub paths: Vec<PathBuf>,
    /// Output format.
    pub format: OutputFormat,
    /// Auto-fix simple issues.
    pub fix: bool,
    /// Only run these rules (comma-separated).
    pub rules: Option<String>,
    /// Check external link validity (slow).
    pub external_links: bool,
}

/// All available lint rules.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LintRule {
    /// Links to docs that don't exist.
    BrokenInternalLinks,
    /// Links to sections that don't exist.
    DeadAnchors,
    /// Code file references that don't exist.
    StaleCodeRefs,
    /// Mixed heading styles.
    InconsistentHeadings,
    /// Images without alt text.
    MissingAltText,
    /// Paragraphs over N words.
    LongParagraphs,
    /// Same heading text at same level.
    DuplicateHeadings,
    /// Trailing spaces on lines.
    TrailingWhitespace,
}

impl LintRule {
    /// Get the rule name as a string.
    pub fn name(&self) -> &'static str {
        match self {
            LintRule::BrokenInternalLinks => "broken-internal-links",
            LintRule::DeadAnchors => "dead-anchors",
            LintRule::StaleCodeRefs => "stale-code-refs",
            LintRule::InconsistentHeadings => "inconsistent-headings",
            LintRule::MissingAltText => "missing-alt-text",
            LintRule::LongParagraphs => "long-paragraphs",
            LintRule::DuplicateHeadings => "duplicate-headings",
            LintRule::TrailingWhitespace => "trailing-whitespace",
        }
    }

    /// Parse a rule name into a LintRule.
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "broken-internal-links" => Some(LintRule::BrokenInternalLinks),
            "dead-anchors" => Some(LintRule::DeadAnchors),
            "stale-code-refs" => Some(LintRule::StaleCodeRefs),
            "inconsistent-headings" => Some(LintRule::InconsistentHeadings),
            "missing-alt-text" => Some(LintRule::MissingAltText),
            "long-paragraphs" => Some(LintRule::LongParagraphs),
            "duplicate-headings" => Some(LintRule::DuplicateHeadings),
            "trailing-whitespace" => Some(LintRule::TrailingWhitespace),
            _ => None,
        }
    }

    /// Get all available rules.
    pub fn all() -> Vec<Self> {
        vec![
            LintRule::BrokenInternalLinks,
            LintRule::DeadAnchors,
            LintRule::StaleCodeRefs,
            LintRule::InconsistentHeadings,
            LintRule::MissingAltText,
            LintRule::LongParagraphs,
            LintRule::DuplicateHeadings,
            LintRule::TrailingWhitespace,
        ]
    }

    /// Check if this rule is auto-fixable.
    pub fn is_fixable(&self) -> bool {
        matches!(self, LintRule::TrailingWhitespace)
    }
}

/// A lint issue found in a document.
#[derive(Debug, Clone, Serialize)]
pub struct LintIssue {
    /// Path to the file with the issue.
    pub file: PathBuf,
    /// Line number where the issue was found (1-indexed).
    pub line: usize,
    /// The rule that triggered this issue.
    pub rule: String,
    /// Description of the issue.
    pub message: String,
    /// Whether this issue can be auto-fixed.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub fixable: bool,
}

/// Results of linting documents.
#[derive(Debug, Serialize)]
pub struct LintResults {
    /// Number of files linted.
    pub files_linted: usize,
    /// List of issues found.
    pub issues: Vec<LintIssue>,
    /// Number of issues that were auto-fixed.
    #[serde(skip_serializing_if = "is_zero")]
    pub fixed_count: usize,
}

fn is_zero(n: &usize) -> bool {
    *n == 0
}

impl LintResults {
    fn new() -> Self {
        Self {
            files_linted: 0,
            issues: Vec::new(),
            fixed_count: 0,
        }
    }

    fn add_issue(&mut self, issue: LintIssue) {
        self.issues.push(issue);
    }

    /// Group issues by file for display.
    fn issues_by_file(&self) -> HashMap<&Path, Vec<&LintIssue>> {
        let mut map: HashMap<&Path, Vec<&LintIssue>> = HashMap::new();
        for issue in &self.issues {
            map.entry(issue.file.as_path()).or_default().push(issue);
        }
        map
    }
}

/// Execute the `paver lint` command.
pub fn execute(args: LintArgs) -> Result<()> {
    // Find and load config
    let config_path = find_config()?;
    let config = PaverConfig::load(&config_path)?;
    let config_dir = config_path.parent().unwrap_or_else(|| Path::new("."));

    // Determine paths to lint
    let paths = if args.paths.is_empty() {
        vec![config_dir.join(&config.docs.root)]
    } else {
        args.paths.clone()
    };

    // Find all markdown files
    let files = find_markdown_files(&paths)?;

    if files.is_empty() {
        eprintln!("No markdown files found to lint");
        return Ok(());
    }

    // Determine which rules to run
    let rules = determine_rules(&args, &config.lint)?;

    // Check external links setting
    let check_external = args.external_links || config.lint.external_links;

    // Lint each file
    let mut results = LintResults::new();
    for file in &files {
        lint_file(
            file,
            &rules,
            &config.lint,
            config_dir,
            check_external,
            args.fix,
            &mut results,
        )?;
    }
    results.files_linted = files.len();

    // Output results in the requested format
    match args.format {
        OutputFormat::Text => output_text(&results, args.fix),
        OutputFormat::Json => output_json(&results)?,
        OutputFormat::Github => output_github(&results),
    }

    // Return error if there are unfixed issues
    let unfixed = results.issues.len() - results.fixed_count;
    if unfixed > 0 {
        anyhow::bail!(
            "Lint failed: {} issue{}",
            unfixed,
            if unfixed == 1 { "" } else { "s" }
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

/// Determine which rules to run based on CLI args and config.
fn determine_rules(args: &LintArgs, config: &LintSection) -> Result<HashSet<LintRule>> {
    let mut rules: HashSet<LintRule> = if let Some(ref rules_str) = args.rules {
        // Only run specified rules
        rules_str
            .split(',')
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .map(|name| {
                LintRule::from_name(name)
                    .ok_or_else(|| anyhow::anyhow!("Unknown lint rule: {}", name))
            })
            .collect::<Result<HashSet<_>>>()?
    } else if !config.enable.is_empty() {
        // Use enabled rules from config
        config
            .enable
            .iter()
            .map(|name| {
                LintRule::from_name(name)
                    .ok_or_else(|| anyhow::anyhow!("Unknown lint rule in config: {}", name))
            })
            .collect::<Result<HashSet<_>>>()?
    } else {
        // Default: all rules
        LintRule::all().into_iter().collect()
    };

    // Remove disabled rules from config
    for name in &config.disable {
        if let Some(rule) = LintRule::from_name(name) {
            rules.remove(&rule);
        }
    }

    Ok(rules)
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

/// Lint a single file against the enabled rules.
fn lint_file(
    path: &Path,
    rules: &HashSet<LintRule>,
    config: &LintSection,
    project_root: &Path,
    _check_external: bool,
    fix: bool,
    results: &mut LintResults,
) -> Result<()> {
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("Failed to read file: {}", path.display()))?;

    let lines: Vec<&str> = content.lines().collect();
    let doc = ParsedDoc::parse_content(path.to_path_buf(), &content)?;

    // Track fixes to apply
    let mut fixed_lines: Option<Vec<String>> = if fix {
        Some(lines.iter().map(|s| s.to_string()).collect())
    } else {
        None
    };

    // Run each enabled rule
    if rules.contains(&LintRule::BrokenInternalLinks) {
        check_broken_internal_links(path, &lines, project_root, results)?;
    }

    if rules.contains(&LintRule::DeadAnchors) {
        check_dead_anchors(path, &content, &lines, results);
    }

    if rules.contains(&LintRule::StaleCodeRefs) {
        check_stale_code_refs(path, &lines, project_root, results);
    }

    if rules.contains(&LintRule::InconsistentHeadings) {
        check_inconsistent_headings(path, &lines, results);
    }

    if rules.contains(&LintRule::MissingAltText) {
        check_missing_alt_text(path, &lines, results);
    }

    if rules.contains(&LintRule::LongParagraphs) {
        check_long_paragraphs(path, &doc, config.max_paragraph_words, results);
    }

    if rules.contains(&LintRule::DuplicateHeadings) {
        check_duplicate_headings(path, &lines, results);
    }

    if rules.contains(&LintRule::TrailingWhitespace) {
        check_trailing_whitespace(path, &lines, fix, &mut fixed_lines, results);
    }

    // Apply fixes if any
    if let Some(fixed) = fixed_lines {
        let original: Vec<String> = lines.iter().map(|s| s.to_string()).collect();
        if fixed != original {
            let new_content = fixed.join("\n");
            // Preserve trailing newline if original had one
            let new_content = if content.ends_with('\n') {
                format!("{}\n", new_content)
            } else {
                new_content
            };
            std::fs::write(path, new_content)
                .with_context(|| format!("Failed to write fixed file: {}", path.display()))?;
        }
    }

    Ok(())
}

/// Check for broken internal links (links to docs that don't exist).
fn check_broken_internal_links(
    path: &Path,
    lines: &[&str],
    project_root: &Path,
    results: &mut LintResults,
) -> Result<()> {
    let link_re = Regex::new(r"\[([^\]]*)\]\(([^)]+)\)").unwrap();

    let mut in_code_block = false;

    for (line_num, line) in lines.iter().enumerate() {
        // Track code blocks
        if line.trim_start().starts_with("```") {
            in_code_block = !in_code_block;
            continue;
        }

        if in_code_block {
            continue;
        }

        for cap in link_re.captures_iter(line) {
            let target = &cap[2];

            // Skip external links, anchors only, and special protocols
            if target.starts_with("http://")
                || target.starts_with("https://")
                || target.starts_with('#')
                || target.starts_with("mailto:")
            {
                continue;
            }

            // Extract the file path (strip anchor if present)
            let file_path = target.split('#').next().unwrap_or(target);

            // Skip empty paths (anchor-only links are handled separately)
            if file_path.is_empty() {
                continue;
            }

            // Resolve relative to the current file's directory
            let base_dir = path.parent().unwrap_or_else(|| Path::new("."));
            let resolved = if file_path.starts_with('/') {
                // Absolute path relative to project root
                project_root.join(file_path.trim_start_matches('/'))
            } else {
                base_dir.join(file_path)
            };

            if !resolved.exists() {
                results.add_issue(LintIssue {
                    file: path.to_path_buf(),
                    line: line_num + 1,
                    rule: LintRule::BrokenInternalLinks.name().to_string(),
                    message: format!("broken link to '{}' (file not found)", file_path),
                    fixable: false,
                });
            }
        }
    }

    Ok(())
}

/// Check for dead anchors (links to sections that don't exist).
fn check_dead_anchors(path: &Path, content: &str, lines: &[&str], results: &mut LintResults) {
    // Build set of valid anchors from headings
    let heading_re = Regex::new(r"^#{1,6}\s+(.+)$").unwrap();
    let mut valid_anchors: HashSet<String> = HashSet::new();

    for line in lines {
        if let Some(cap) = heading_re.captures(line) {
            let heading = &cap[1];
            // Convert heading to anchor format (lowercase, spaces to hyphens)
            let anchor = heading
                .to_lowercase()
                .replace(' ', "-")
                .chars()
                .filter(|c| c.is_alphanumeric() || *c == '-')
                .collect::<String>();
            valid_anchors.insert(anchor);
        }
    }

    // Find all anchor links
    let anchor_link_re = Regex::new(r"\[([^\]]*)\]\(#([^)]+)\)").unwrap();

    for (line_num, line) in lines.iter().enumerate() {
        for cap in anchor_link_re.captures_iter(line) {
            let anchor = &cap[2];

            // Normalize the anchor for comparison
            let normalized = anchor
                .to_lowercase()
                .chars()
                .filter(|c| c.is_alphanumeric() || *c == '-')
                .collect::<String>();

            if !valid_anchors.contains(&normalized) {
                results.add_issue(LintIssue {
                    file: path.to_path_buf(),
                    line: line_num + 1,
                    rule: LintRule::DeadAnchors.name().to_string(),
                    message: format!("dead anchor '#{}' (section not found)", anchor),
                    fixable: false,
                });
            }
        }
    }

    // Also check anchors in file links (e.g., file.md#section)
    let file_anchor_re = Regex::new(r"\[([^\]]*)\]\(([^)#]+)#([^)]+)\)").unwrap();

    for (line_num, line) in lines.iter().enumerate() {
        for cap in file_anchor_re.captures_iter(line) {
            let target_file = &cap[2];
            let anchor = &cap[3];

            // Skip external links
            if target_file.starts_with("http://") || target_file.starts_with("https://") {
                continue;
            }

            // For same-file anchors, we already checked above
            // For cross-file anchors, we need to read the target file
            let base_dir = path.parent().unwrap_or_else(|| Path::new("."));
            let resolved = base_dir.join(target_file);

            if resolved.exists()
                && let Ok(target_content) = std::fs::read_to_string(&resolved)
            {
                let target_lines: Vec<&str> = target_content.lines().collect();
                let mut target_anchors: HashSet<String> = HashSet::new();

                for tline in &target_lines {
                    if let Some(cap) = heading_re.captures(tline) {
                        let heading = &cap[1];
                        let anchor_id = heading
                            .to_lowercase()
                            .replace(' ', "-")
                            .chars()
                            .filter(|c| c.is_alphanumeric() || *c == '-')
                            .collect::<String>();
                        target_anchors.insert(anchor_id);
                    }
                }

                let normalized = anchor
                    .to_lowercase()
                    .chars()
                    .filter(|c| c.is_alphanumeric() || *c == '-')
                    .collect::<String>();

                if !target_anchors.contains(&normalized) {
                    results.add_issue(LintIssue {
                        file: path.to_path_buf(),
                        line: line_num + 1,
                        rule: LintRule::DeadAnchors.name().to_string(),
                        message: format!(
                            "dead anchor '{}#{}' (section not found in target file)",
                            target_file, anchor
                        ),
                        fixable: false,
                    });
                }
            }
        }
    }

    // Also handle HTML-style anchors
    let html_anchor_re = Regex::new(r#"<a\s+[^>]*id\s*=\s*["']([^"']+)["'][^>]*>"#).unwrap();
    for cap in html_anchor_re.captures_iter(content) {
        valid_anchors.insert(cap[1].to_string());
    }
}

/// Check for stale code references (references to code files that don't exist).
fn check_stale_code_refs(
    path: &Path,
    lines: &[&str],
    project_root: &Path,
    results: &mut LintResults,
) {
    // Pattern for code file references like `src/foo.rs` or in links
    let code_ref_re =
        Regex::new(r"(?:`([^`]+\.(rs|py|js|ts|go|java|rb|c|cpp|h|hpp))`|\[([^\]]*)\]\(([^)]+\.(rs|py|js|ts|go|java|rb|c|cpp|h|hpp))\))").unwrap();

    let mut in_code_block = false;

    for (line_num, line) in lines.iter().enumerate() {
        // Track code blocks
        if line.trim_start().starts_with("```") {
            in_code_block = !in_code_block;
            continue;
        }

        if in_code_block {
            continue;
        }

        for cap in code_ref_re.captures_iter(line) {
            // Extract the path from either backtick or link format
            let code_path = cap
                .get(1)
                .or_else(|| cap.get(4))
                .map(|m| m.as_str())
                .unwrap_or("");

            if code_path.is_empty() {
                continue;
            }

            // Skip paths that look like examples, URLs, or glob patterns
            if code_path.contains("example")
                || code_path.starts_with("http")
                || code_path.contains('*')
                || code_path.contains('?')
                || code_path.contains('{')
                || code_path.contains('<')
            {
                continue;
            }

            // Try to resolve the path relative to project root
            let resolved = project_root.join(code_path);

            if !resolved.exists() {
                results.add_issue(LintIssue {
                    file: path.to_path_buf(),
                    line: line_num + 1,
                    rule: LintRule::StaleCodeRefs.name().to_string(),
                    message: format!("reference to '{}' (file not found)", code_path),
                    fixable: false,
                });
            }
        }
    }
}

/// Check for inconsistent heading styles (ATX vs Setext, spacing variations).
fn check_inconsistent_headings(path: &Path, lines: &[&str], results: &mut LintResults) {
    let atx_re = Regex::new(r"^(#{1,6})\s").unwrap();

    let mut first_style: Option<bool> = None; // true = ATX with space, false = ATX without space

    for (line_num, line) in lines.iter().enumerate() {
        // Check for ATX headings
        if line.starts_with('#') {
            let has_space = atx_re.is_match(line);

            match first_style {
                None => first_style = Some(has_space),
                Some(expected) if expected != has_space => {
                    results.add_issue(LintIssue {
                        file: path.to_path_buf(),
                        line: line_num + 1,
                        rule: LintRule::InconsistentHeadings.name().to_string(),
                        message: if expected {
                            "inconsistent heading style (missing space after #)".to_string()
                        } else {
                            "inconsistent heading style (unexpected space after #)".to_string()
                        },
                        fixable: false,
                    });
                }
                _ => {}
            }
        }

        // Check for Setext-style headings (underlines)
        if line_num > 0 && (line.starts_with("===") || line.starts_with("---")) {
            let prev_line = lines[line_num - 1];
            if !prev_line.is_empty() && !prev_line.starts_with('#') {
                // This looks like a Setext heading
                if first_style.is_some() {
                    results.add_issue(LintIssue {
                        file: path.to_path_buf(),
                        line: line_num + 1,
                        rule: LintRule::InconsistentHeadings.name().to_string(),
                        message: "mixed ATX and Setext heading styles".to_string(),
                        fixable: false,
                    });
                }
            }
        }
    }
}

/// Check for images without alt text.
fn check_missing_alt_text(path: &Path, lines: &[&str], results: &mut LintResults) {
    let img_re = Regex::new(r"!\[([^\]]*)\]\([^)]+\)").unwrap();
    let html_img_re = Regex::new(r#"<img\s+[^>]*>"#).unwrap();
    let alt_attr_re = Regex::new(r#"alt\s*=\s*["']([^"']*)["']"#).unwrap();

    let mut in_code_block = false;

    for (line_num, line) in lines.iter().enumerate() {
        if line.trim_start().starts_with("```") {
            in_code_block = !in_code_block;
            continue;
        }

        if in_code_block {
            continue;
        }

        // Check markdown images
        for cap in img_re.captures_iter(line) {
            let alt_text = &cap[1];
            if alt_text.trim().is_empty() {
                results.add_issue(LintIssue {
                    file: path.to_path_buf(),
                    line: line_num + 1,
                    rule: LintRule::MissingAltText.name().to_string(),
                    message: "missing alt text for image".to_string(),
                    fixable: false,
                });
            }
        }

        // Check HTML images
        for cap in html_img_re.captures_iter(line) {
            let img_tag = &cap[0];
            let has_alt = alt_attr_re
                .captures(img_tag)
                .map(|c| !c[1].trim().is_empty())
                .unwrap_or(false);

            if !has_alt {
                results.add_issue(LintIssue {
                    file: path.to_path_buf(),
                    line: line_num + 1,
                    rule: LintRule::MissingAltText.name().to_string(),
                    message: "missing alt text for image".to_string(),
                    fixable: false,
                });
            }
        }
    }
}

/// Check for paragraphs that are too long.
fn check_long_paragraphs(path: &Path, doc: &ParsedDoc, max_words: u32, results: &mut LintResults) {
    // Process each section's content
    for section in &doc.sections {
        let content = &section.content;
        let paragraph_start_line = section.start_line;
        let mut paragraph_words = 0;
        let mut in_code_block = false;
        let mut paragraph_line_offset = 0;

        for (offset, line) in content.lines().enumerate() {
            // Track code blocks
            if line.trim_start().starts_with("```") {
                in_code_block = !in_code_block;
                continue;
            }

            if in_code_block {
                continue;
            }

            // Empty line ends a paragraph
            if line.trim().is_empty() {
                if paragraph_words > max_words as usize {
                    results.add_issue(LintIssue {
                        file: path.to_path_buf(),
                        line: paragraph_start_line + paragraph_line_offset,
                        rule: LintRule::LongParagraphs.name().to_string(),
                        message: format!(
                            "long paragraph ({} words, max {})",
                            paragraph_words, max_words
                        ),
                        fixable: false,
                    });
                }
                paragraph_words = 0;
                paragraph_line_offset = offset + 1;
            } else {
                // Skip headings and list items for word counting
                if !line.starts_with('#')
                    && !line.trim_start().starts_with('-')
                    && !line.trim_start().starts_with('*')
                    && !line.trim_start().starts_with(|c: char| c.is_ascii_digit())
                {
                    paragraph_words += line.split_whitespace().count();
                }
            }
        }

        // Check final paragraph
        if paragraph_words > max_words as usize {
            results.add_issue(LintIssue {
                file: path.to_path_buf(),
                line: paragraph_start_line + paragraph_line_offset,
                rule: LintRule::LongParagraphs.name().to_string(),
                message: format!(
                    "long paragraph ({} words, max {})",
                    paragraph_words, max_words
                ),
                fixable: false,
            });
        }
    }
}

/// Check for duplicate headings at the same level.
fn check_duplicate_headings(path: &Path, lines: &[&str], results: &mut LintResults) {
    let heading_re = Regex::new(r"^(#{1,6})\s+(.+)$").unwrap();

    // Track headings by level: level -> (text -> line_number)
    let mut headings_by_level: HashMap<usize, HashMap<String, usize>> = HashMap::new();

    let mut in_code_block = false;

    for (line_num, line) in lines.iter().enumerate() {
        if line.trim_start().starts_with("```") {
            in_code_block = !in_code_block;
            continue;
        }

        if in_code_block {
            continue;
        }

        if let Some(cap) = heading_re.captures(line) {
            let level = cap[1].len();
            let text = cap[2].trim().to_lowercase();

            let level_headings = headings_by_level.entry(level).or_default();

            if let Some(&first_line) = level_headings.get(&text) {
                results.add_issue(LintIssue {
                    file: path.to_path_buf(),
                    line: line_num + 1,
                    rule: LintRule::DuplicateHeadings.name().to_string(),
                    message: format!(
                        "duplicate heading '{}' (also at line {})",
                        cap[2].trim(),
                        first_line
                    ),
                    fixable: false,
                });
            } else {
                level_headings.insert(text, line_num + 1);
            }
        }
    }
}

/// Check for trailing whitespace.
fn check_trailing_whitespace(
    path: &Path,
    lines: &[&str],
    fix: bool,
    fixed_lines: &mut Option<Vec<String>>,
    results: &mut LintResults,
) {
    for (line_num, line) in lines.iter().enumerate() {
        if line.ends_with(' ') || line.ends_with('\t') {
            if fix {
                if let Some(fixed) = fixed_lines {
                    fixed[line_num] = line.trim_end().to_string();
                    results.fixed_count += 1;
                }
            } else {
                results.add_issue(LintIssue {
                    file: path.to_path_buf(),
                    line: line_num + 1,
                    rule: LintRule::TrailingWhitespace.name().to_string(),
                    message: "trailing whitespace".to_string(),
                    fixable: true,
                });
            }
        }
    }
}

/// Output results in text format.
fn output_text(results: &LintResults, fix_mode: bool) {
    let issues_by_file = results.issues_by_file();

    // Sort files for consistent output
    let mut files: Vec<_> = issues_by_file.keys().collect();
    files.sort();

    for file in files {
        let issues = &issues_by_file[file];
        println!("{}", file.display());

        // Sort issues by line number
        let mut sorted_issues: Vec<_> = issues.iter().collect();
        sorted_issues.sort_by_key(|i| i.line);

        for issue in sorted_issues {
            println!("  line {}: {}", issue.line, issue.message);
        }
        println!();
    }

    // Print summary
    let issue_count = results.issues.len();
    let fixable_count = results.issues.iter().filter(|i| i.fixable).count();

    if issue_count == 0 {
        println!(
            "Linted {} file{}: no issues found",
            results.files_linted,
            if results.files_linted == 1 { "" } else { "s" }
        );
    } else {
        println!(
            "Found {} issue{} in {} file{}.",
            issue_count,
            if issue_count == 1 { "" } else { "s" },
            issues_by_file.len(),
            if issues_by_file.len() == 1 { "" } else { "s" }
        );

        if results.fixed_count > 0 {
            println!(
                "Auto-fixed {} issue{}.",
                results.fixed_count,
                if results.fixed_count == 1 { "" } else { "s" }
            );
        } else if fixable_count > 0 && !fix_mode {
            println!(
                "Run 'paver lint --fix' to auto-fix {} issue{}.",
                fixable_count,
                if fixable_count == 1 { "" } else { "s" }
            );
        }
    }
}

/// Output results in JSON format.
fn output_json(results: &LintResults) -> Result<()> {
    let json = serde_json::to_string_pretty(results).context("Failed to serialize results")?;
    println!("{}", json);
    Ok(())
}

/// Output results in GitHub Actions annotation format.
fn output_github(results: &LintResults) {
    for issue in &results.issues {
        println!(
            "::warning file={},line={}::{}",
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
"#;
        let config_path = temp_dir.path().join(".paver.toml");
        fs::write(&config_path, config_content).unwrap();
        config_path
    }

    fn create_test_doc(temp_dir: &TempDir, filename: &str, content: &str) -> PathBuf {
        let docs_dir = temp_dir.path().join("docs");
        fs::create_dir_all(&docs_dir).unwrap();
        let path = docs_dir.join(filename);
        fs::write(&path, content).unwrap();
        path
    }

    #[test]
    fn test_broken_internal_links() {
        let temp_dir = TempDir::new().unwrap();
        create_test_config(&temp_dir);
        let path = create_test_doc(
            &temp_dir,
            "test.md",
            r#"# Test
See [other doc](missing.md) for details.
"#,
        );

        let content = fs::read_to_string(&path).unwrap();
        let lines: Vec<&str> = content.lines().collect();
        let mut results = LintResults::new();

        check_broken_internal_links(&path, &lines, temp_dir.path(), &mut results).unwrap();

        assert_eq!(results.issues.len(), 1);
        assert!(results.issues[0].message.contains("missing.md"));
    }

    #[test]
    fn test_valid_internal_links() {
        let temp_dir = TempDir::new().unwrap();
        create_test_config(&temp_dir);
        let _other = create_test_doc(&temp_dir, "other.md", "# Other\n");
        let path = create_test_doc(
            &temp_dir,
            "test.md",
            r#"# Test
See [other doc](other.md) for details.
"#,
        );

        let content = fs::read_to_string(&path).unwrap();
        let lines: Vec<&str> = content.lines().collect();
        let mut results = LintResults::new();

        check_broken_internal_links(&path, &lines, temp_dir.path(), &mut results).unwrap();

        assert!(results.issues.is_empty());
    }

    #[test]
    fn test_dead_anchors() {
        let temp_dir = TempDir::new().unwrap();
        let path = create_test_doc(
            &temp_dir,
            "test.md",
            r#"# Test
## Introduction
See [missing section](#nonexistent) for details.
"#,
        );

        let content = fs::read_to_string(&path).unwrap();
        let lines: Vec<&str> = content.lines().collect();
        let mut results = LintResults::new();

        check_dead_anchors(&path, &content, &lines, &mut results);

        assert_eq!(results.issues.len(), 1);
        assert!(results.issues[0].message.contains("nonexistent"));
    }

    #[test]
    fn test_valid_anchors() {
        let temp_dir = TempDir::new().unwrap();
        let path = create_test_doc(
            &temp_dir,
            "test.md",
            r#"# Test
## Introduction
See [intro](#introduction) for details.
"#,
        );

        let content = fs::read_to_string(&path).unwrap();
        let lines: Vec<&str> = content.lines().collect();
        let mut results = LintResults::new();

        check_dead_anchors(&path, &content, &lines, &mut results);

        assert!(results.issues.is_empty());
    }

    #[test]
    fn test_stale_code_refs() {
        let temp_dir = TempDir::new().unwrap();
        let path = create_test_doc(
            &temp_dir,
            "test.md",
            r#"# Test
See `src/missing.rs` for the implementation.
"#,
        );

        let content = fs::read_to_string(&path).unwrap();
        let lines: Vec<&str> = content.lines().collect();
        let mut results = LintResults::new();

        check_stale_code_refs(&path, &lines, temp_dir.path(), &mut results);

        assert_eq!(results.issues.len(), 1);
        assert!(results.issues[0].message.contains("src/missing.rs"));
    }

    #[test]
    fn test_valid_code_refs() {
        let temp_dir = TempDir::new().unwrap();
        let src_dir = temp_dir.path().join("src");
        fs::create_dir_all(&src_dir).unwrap();
        fs::write(src_dir.join("main.rs"), "fn main() {}").unwrap();

        let path = create_test_doc(
            &temp_dir,
            "test.md",
            r#"# Test
See `src/main.rs` for the implementation.
"#,
        );

        let content = fs::read_to_string(&path).unwrap();
        let lines: Vec<&str> = content.lines().collect();
        let mut results = LintResults::new();

        check_stale_code_refs(&path, &lines, temp_dir.path(), &mut results);

        assert!(results.issues.is_empty());
    }

    #[test]
    fn test_stale_code_refs_skips_glob_patterns() {
        let temp_dir = TempDir::new().unwrap();
        let path = create_test_doc(
            &temp_dir,
            "test.md",
            r#"# Test
Glob patterns like `src/**/*.rs` and `src/commands/*.rs` should be skipped.
Placeholders like `src/commands/<newcmd>.rs` should also be skipped.
Optional patterns like `*.generated.rs` should be skipped.
"#,
        );

        let content = fs::read_to_string(&path).unwrap();
        let lines: Vec<&str> = content.lines().collect();
        let mut results = LintResults::new();

        check_stale_code_refs(&path, &lines, temp_dir.path(), &mut results);

        assert!(
            results.issues.is_empty(),
            "Glob patterns should not be flagged as stale refs"
        );
    }

    #[test]
    fn test_missing_alt_text() {
        let temp_dir = TempDir::new().unwrap();
        let path = create_test_doc(
            &temp_dir,
            "test.md",
            r#"# Test
![](image.png)
"#,
        );

        let content = fs::read_to_string(&path).unwrap();
        let lines: Vec<&str> = content.lines().collect();
        let mut results = LintResults::new();

        check_missing_alt_text(&path, &lines, &mut results);

        assert_eq!(results.issues.len(), 1);
        assert!(results.issues[0].message.contains("alt text"));
    }

    #[test]
    fn test_valid_alt_text() {
        let temp_dir = TempDir::new().unwrap();
        let path = create_test_doc(
            &temp_dir,
            "test.md",
            r#"# Test
![Screenshot of the app](image.png)
"#,
        );

        let content = fs::read_to_string(&path).unwrap();
        let lines: Vec<&str> = content.lines().collect();
        let mut results = LintResults::new();

        check_missing_alt_text(&path, &lines, &mut results);

        assert!(results.issues.is_empty());
    }

    #[test]
    fn test_duplicate_headings() {
        let temp_dir = TempDir::new().unwrap();
        let path = create_test_doc(
            &temp_dir,
            "test.md",
            r#"# Test
## Prerequisites
First section.
## Prerequisites
Second section.
"#,
        );

        let content = fs::read_to_string(&path).unwrap();
        let lines: Vec<&str> = content.lines().collect();
        let mut results = LintResults::new();

        check_duplicate_headings(&path, &lines, &mut results);

        assert_eq!(results.issues.len(), 1);
        assert!(results.issues[0].message.contains("Prerequisites"));
        assert!(results.issues[0].message.contains("line 2"));
    }

    #[test]
    fn test_trailing_whitespace() {
        let temp_dir = TempDir::new().unwrap();
        let path = create_test_doc(&temp_dir, "test.md", "# Test \nSome text.  \n");

        let content = fs::read_to_string(&path).unwrap();
        let lines: Vec<&str> = content.lines().collect();
        let mut results = LintResults::new();
        let mut fixed_lines: Option<Vec<String>> = None;

        check_trailing_whitespace(&path, &lines, false, &mut fixed_lines, &mut results);

        assert_eq!(results.issues.len(), 2);
        assert!(results.issues[0].fixable);
    }

    #[test]
    fn test_trailing_whitespace_fix() {
        let temp_dir = TempDir::new().unwrap();
        let path = create_test_doc(&temp_dir, "test.md", "# Test \nSome text.  \n");

        let content = fs::read_to_string(&path).unwrap();
        let lines: Vec<&str> = content.lines().collect();
        let mut results = LintResults::new();
        let mut fixed_lines: Option<Vec<String>> =
            Some(lines.iter().map(|s| s.to_string()).collect());

        check_trailing_whitespace(&path, &lines, true, &mut fixed_lines, &mut results);

        assert_eq!(results.fixed_count, 2);
        let fixed = fixed_lines.unwrap();
        assert_eq!(fixed[0], "# Test");
        assert_eq!(fixed[1], "Some text.");
    }

    #[test]
    fn test_lint_rule_from_name() {
        assert_eq!(
            LintRule::from_name("broken-internal-links"),
            Some(LintRule::BrokenInternalLinks)
        );
        assert_eq!(LintRule::from_name("unknown-rule"), None);
    }

    #[test]
    fn test_determine_rules_all_by_default() {
        let config = LintSection::default();
        let args = LintArgs {
            paths: vec![],
            format: OutputFormat::Text,
            fix: false,
            rules: None,
            external_links: false,
        };

        let rules = determine_rules(&args, &config).unwrap();
        assert_eq!(rules.len(), LintRule::all().len());
    }

    #[test]
    fn test_determine_rules_with_cli_filter() {
        let config = LintSection::default();
        let args = LintArgs {
            paths: vec![],
            format: OutputFormat::Text,
            fix: false,
            rules: Some("broken-internal-links,trailing-whitespace".to_string()),
            external_links: false,
        };

        let rules = determine_rules(&args, &config).unwrap();
        assert_eq!(rules.len(), 2);
        assert!(rules.contains(&LintRule::BrokenInternalLinks));
        assert!(rules.contains(&LintRule::TrailingWhitespace));
    }

    #[test]
    fn test_determine_rules_with_disabled() {
        let config = LintSection {
            disable: vec!["long-paragraphs".to_string()],
            ..Default::default()
        };

        let args = LintArgs {
            paths: vec![],
            format: OutputFormat::Text,
            fix: false,
            rules: None,
            external_links: false,
        };

        let rules = determine_rules(&args, &config).unwrap();
        assert!(!rules.contains(&LintRule::LongParagraphs));
    }

    #[test]
    fn test_json_output() {
        let mut results = LintResults::new();
        results.files_linted = 1;
        results.add_issue(LintIssue {
            file: PathBuf::from("test.md"),
            line: 5,
            rule: "broken-internal-links".to_string(),
            message: "broken link".to_string(),
            fixable: false,
        });

        let json = serde_json::to_string(&results).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed["files_linted"], 1);
        assert_eq!(parsed["issues"].as_array().unwrap().len(), 1);
    }
}
