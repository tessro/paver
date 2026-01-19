//! Validation rules engine for PAVED documents.
//!
//! This module provides a rules engine that validates parsed PAVED documents
//! against configurable rules from `.paver.toml`.

use std::path::{Path, PathBuf};

use glob::Pattern;

use crate::config::RulesSection;
use crate::parser::ParsedDoc;

/// Document type for type-specific validation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DocType {
    Component,
    Runbook,
    Adr,
    Other,
}

/// A rule that can be applied to validate a PAVED document.
#[derive(Debug, Clone, PartialEq)]
pub enum Rule {
    /// Require a specific section to be present in the document.
    RequireSection { name: String },
    /// Enforce a maximum line count for the document.
    MaxLines { limit: usize },
    /// Require at least one code block in a specific section.
    RequireCodeBlock { in_section: String },
    /// Require a runnable command in a specific section.
    RequireCommand { in_section: String },
    /// Require at least one of the listed sections to be present.
    RequireOneOf { sections: Vec<String> },
    /// Require a section to contain a valid ADR status value.
    RequireValidAdrStatus,
    /// Validate that paths in the Paths section are valid glob patterns.
    /// If `warn_empty` is true, also warns when patterns match no files.
    ValidatePaths {
        /// The project root directory for checking if patterns match files.
        project_root: PathBuf,
        /// Whether to warn when patterns match no files.
        warn_empty: bool,
    },
}

impl Rule {
    /// Returns a human-readable name for this rule.
    pub fn name(&self) -> String {
        match self {
            Rule::RequireSection { name } => format!("require-section-{}", name.to_lowercase()),
            Rule::MaxLines { limit } => format!("max-lines-{}", limit),
            Rule::RequireCodeBlock { in_section } => {
                format!("require-code-block-in-{}", in_section.to_lowercase())
            }
            Rule::RequireCommand { in_section } => {
                format!("require-command-in-{}", in_section.to_lowercase())
            }
            Rule::RequireOneOf { sections } => {
                let names: Vec<_> = sections.iter().map(|s| s.to_lowercase()).collect();
                format!("require-one-of-{}", names.join("-or-"))
            }
            Rule::RequireValidAdrStatus => "require-valid-adr-status".to_string(),
            Rule::ValidatePaths { .. } => "validate-paths".to_string(),
        }
    }
}

/// Valid ADR status values.
const VALID_ADR_STATUSES: &[&str] = &["proposed", "accepted", "deprecated", "superseded"];

/// A validation error found in a document.
#[derive(Debug, Clone, PartialEq)]
pub struct ValidationError {
    /// The name of the rule that was violated.
    pub rule: String,
    /// A human-readable error message.
    pub message: String,
    /// The line number where the error was found, if applicable.
    pub line: Option<usize>,
    /// A suggestion for how to fix the error.
    pub suggestion: Option<String>,
}

/// A validation warning found in a document.
#[derive(Debug, Clone, PartialEq)]
pub struct ValidationWarning {
    /// The name of the rule that triggered the warning.
    pub rule: String,
    /// A human-readable warning message.
    pub message: String,
    /// The line number where the warning was found, if applicable.
    pub line: Option<usize>,
}

/// The result of validating a document.
#[derive(Debug, Clone, PartialEq)]
pub struct ValidationResult {
    /// The path to the document that was validated.
    pub path: PathBuf,
    /// Validation errors (rule violations).
    pub errors: Vec<ValidationError>,
    /// Validation warnings.
    pub warnings: Vec<ValidationWarning>,
}

impl ValidationResult {
    /// Creates a new validation result for the given path.
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            errors: Vec::new(),
            warnings: Vec::new(),
        }
    }

    /// Returns true if the document passed validation (no errors).
    pub fn is_valid(&self) -> bool {
        self.errors.is_empty()
    }

    /// Returns true if the document has any warnings.
    pub fn has_warnings(&self) -> bool {
        !self.warnings.is_empty()
    }
}

/// The rules engine that validates documents against a set of rules.
#[derive(Debug, Clone)]
pub struct RulesEngine {
    rules: Vec<Rule>,
}

impl RulesEngine {
    /// Creates a new rules engine with the given rules.
    pub fn new(rules: Vec<Rule>) -> Self {
        Self { rules }
    }

    /// Creates a rules engine from the configuration.
    ///
    /// Uses the current directory as the project root for ValidatePaths rule.
    pub fn from_config(config: &RulesSection) -> Self {
        Self::from_config_with_root(config, PathBuf::from("."))
    }

    /// Creates a rules engine from the configuration with a specified project root.
    ///
    /// The project root is used for ValidatePaths rule to check if patterns match files.
    pub fn from_config_with_root(config: &RulesSection, project_root: impl Into<PathBuf>) -> Self {
        let project_root = project_root.into();
        let mut rules = Vec::new();

        // Always require Purpose section (from PAVED framework)
        rules.push(Rule::RequireSection {
            name: "Purpose".to_string(),
        });

        // Require Verification section if enabled
        if config.require_verification {
            rules.push(Rule::RequireSection {
                name: "Verification".to_string(),
            });
            // Require executable commands in Verification section
            if config.require_verification_commands {
                rules.push(Rule::RequireCommand {
                    in_section: "Verification".to_string(),
                });
            }
        }

        // Require Examples section if enabled
        if config.require_examples {
            rules.push(Rule::RequireSection {
                name: "Examples".to_string(),
            });
            // When Examples is required, also require code blocks in it
            rules.push(Rule::RequireCodeBlock {
                in_section: "Examples".to_string(),
            });
        }

        // Max lines rule
        rules.push(Rule::MaxLines {
            limit: config.max_lines as usize,
        });

        // ValidatePaths rule
        if config.validate_paths {
            rules.push(Rule::ValidatePaths {
                project_root,
                warn_empty: config.warn_empty_paths,
            });
        }

        Self { rules }
    }

    /// Returns the default rules based on the PAVED manifesto.
    pub fn default_rules() -> Vec<Rule> {
        vec![
            Rule::RequireSection {
                name: "Purpose".to_string(),
            },
            Rule::RequireSection {
                name: "Verification".to_string(),
            },
            Rule::MaxLines { limit: 300 },
            Rule::RequireCodeBlock {
                in_section: "Examples".to_string(),
            },
        ]
    }

    /// Creates a rules engine with the default rules.
    pub fn with_defaults() -> Self {
        Self::new(Self::default_rules())
    }

    /// Validates a document against all rules.
    pub fn validate(&self, doc: &ParsedDoc) -> ValidationResult {
        let mut result = ValidationResult::new(&doc.path);

        for rule in &self.rules {
            self.apply_rule(rule, doc, &mut result);
        }

        result
    }

    /// Applies a single rule to a document.
    fn apply_rule(&self, rule: &Rule, doc: &ParsedDoc, result: &mut ValidationResult) {
        match rule {
            Rule::RequireSection { name } => {
                if !doc.has_section(name) {
                    result.errors.push(ValidationError {
                        rule: rule.name(),
                        message: format!("missing required section: {}", name),
                        line: None,
                        suggestion: Some(format!("add a '## {}' section to the document", name)),
                    });
                }
            }
            Rule::MaxLines { limit } => {
                if doc.line_count > *limit {
                    result.errors.push(ValidationError {
                        rule: rule.name(),
                        message: format!(
                            "document has {} lines, exceeds maximum of {}",
                            doc.line_count, limit
                        ),
                        line: Some(*limit + 1),
                        suggestion: Some(
                            "split this document into smaller, focused documents".to_string(),
                        ),
                    });
                }
            }
            Rule::RequireCodeBlock { in_section } => {
                if let Some(section) = doc.get_section(in_section)
                    && !section.has_code_blocks
                {
                    result.errors.push(ValidationError {
                        rule: rule.name(),
                        message: format!(
                            "section '{}' must contain at least one code block",
                            in_section
                        ),
                        line: Some(section.start_line),
                        suggestion: Some(format!(
                            "add a code block with an example in the '{}' section",
                            in_section
                        )),
                    });
                }
                // Note: If section doesn't exist, RequireSection rule will catch it
            }
            Rule::RequireCommand { in_section } => {
                if let Some(section) = doc.get_section(in_section)
                    && !section.has_commands
                {
                    result.errors.push(ValidationError {
                        rule: rule.name(),
                        message: format!(
                            "section '{}' should contain a runnable command",
                            in_section
                        ),
                        line: Some(section.start_line),
                        suggestion: Some(format!(
                            "add a shell command or script in a ```bash code block in '{}'",
                            in_section
                        )),
                    });
                }
            }
            Rule::RequireOneOf { sections } => {
                let has_any = sections.iter().any(|name| doc.has_section(name));
                if !has_any {
                    let section_list = sections.join("' or '");
                    result.errors.push(ValidationError {
                        rule: rule.name(),
                        message: format!(
                            "missing required section: must have '{}' section",
                            section_list
                        ),
                        line: None,
                        suggestion: Some(format!(
                            "add a '## {}' section to the document",
                            sections.first().unwrap_or(&String::new())
                        )),
                    });
                }
            }
            Rule::RequireValidAdrStatus => {
                if let Some(section) = doc.get_section("Status") {
                    let content_lower = section.content.to_lowercase();
                    let has_valid_status = VALID_ADR_STATUSES
                        .iter()
                        .any(|status| content_lower.contains(status));
                    if !has_valid_status {
                        result.errors.push(ValidationError {
                            rule: rule.name(),
                            message: "ADR Status section must contain a valid status value"
                                .to_string(),
                            line: Some(section.start_line),
                            suggestion: Some(
                                "set status to one of: Proposed, Accepted, Deprecated, Superseded"
                                    .to_string(),
                            ),
                        });
                    }
                }
            }
            Rule::ValidatePaths {
                project_root,
                warn_empty,
            } => {
                if let Some(section) = doc.get_section("Paths") {
                    let patterns = Self::extract_paths_patterns(&section.content);
                    for (line_offset, pattern) in patterns {
                        let line = section.start_line + line_offset;
                        // Check for absolute paths
                        if pattern.starts_with('/') {
                            result.errors.push(ValidationError {
                                rule: rule.name(),
                                message: format!(
                                    "path pattern '{}' is absolute; paths should be relative to project root",
                                    pattern
                                ),
                                line: Some(line),
                                suggestion: Some(format!(
                                    "remove the leading '/' to make the path relative: '{}'",
                                    pattern.trim_start_matches('/')
                                )),
                            });
                            continue;
                        }

                        // Check for valid glob syntax
                        if let Err(e) = Pattern::new(&pattern) {
                            result.errors.push(ValidationError {
                                rule: rule.name(),
                                message: format!("invalid glob pattern '{}': {}", pattern, e),
                                line: Some(line),
                                suggestion: Some(
                                    "check for unmatched brackets or invalid glob syntax"
                                        .to_string(),
                                ),
                            });
                            continue;
                        }

                        // Check if pattern matches any files (warning only)
                        if *warn_empty && !Self::pattern_matches_files(&pattern, project_root) {
                            result.warnings.push(ValidationWarning {
                                rule: rule.name(),
                                message: format!(
                                    "path pattern '{}' matches no files in the project",
                                    pattern
                                ),
                                line: Some(line),
                            });
                        }
                    }
                }
            }
        }
    }

    /// Extract path patterns from the Paths section content.
    /// Returns pairs of (line_offset, pattern).
    fn extract_paths_patterns(content: &str) -> Vec<(usize, String)> {
        let mut patterns = Vec::new();

        for (idx, line) in content.lines().enumerate() {
            let trimmed = line.trim();

            // Collect patterns (lines starting with - or *)
            if let Some(pattern) = trimmed
                .strip_prefix("- ")
                .or_else(|| trimmed.strip_prefix("* "))
            {
                let pattern = pattern.trim();
                // Remove backticks if present
                let pattern = pattern.trim_matches('`');
                if !pattern.is_empty() {
                    // Line offset is idx + 1 since content starts after the heading
                    patterns.push((idx + 1, pattern.to_string()));
                }
            }
        }

        patterns
    }

    /// Check if a glob pattern matches any files in the given directory.
    fn pattern_matches_files(pattern: &str, root: &Path) -> bool {
        // Build the full glob pattern from the root directory
        let full_pattern = root.join(pattern);
        let pattern_str = full_pattern.to_string_lossy();

        // Try to match files using glob
        if let Ok(paths) = glob::glob(&pattern_str) {
            return paths.flatten().next().is_some();
        }

        // Also try simple prefix matching for directory patterns
        if pattern.ends_with('/') || !pattern.contains('*') {
            let target = root.join(pattern.trim_end_matches('/'));
            return target.exists();
        }

        false
    }

    /// Returns the rules in this engine.
    pub fn rules(&self) -> &[Rule] {
        &self.rules
    }

    /// Validates a document against all rules, including type-specific rules.
    pub fn validate_with_type(
        &self,
        doc: &ParsedDoc,
        doc_type: DocType,
        config: &RulesSection,
    ) -> ValidationResult {
        let mut result = ValidationResult::new(&doc.path);

        // Apply base rules
        for rule in &self.rules {
            self.apply_rule(rule, doc, &mut result);
        }

        // Apply type-specific rules based on config and detected type
        let type_rules = get_type_specific_rules(doc_type, config);
        for rule in &type_rules {
            self.apply_rule(rule, doc, &mut result);
        }

        result
    }
}

/// Detects the document type from path and content.
pub fn detect_doc_type(path: &Path, content: &str) -> DocType {
    let path_str = path.to_string_lossy().to_lowercase();

    // Check path patterns
    if path_str.contains("component") {
        return DocType::Component;
    }
    if path_str.contains("runbook") {
        return DocType::Runbook;
    }
    if path_str.contains("adr") || path_str.contains("decision") {
        return DocType::Adr;
    }

    // Check content patterns
    let content_lower = content.to_lowercase();

    // ADRs typically have a Status section
    if content_lower.contains("## status")
        && (content_lower.contains("accepted")
            || content_lower.contains("proposed")
            || content_lower.contains("deprecated"))
    {
        return DocType::Adr;
    }

    // Runbooks have specific sections
    if content_lower.contains("## when to use")
        || content_lower.contains("## preconditions")
        || content_lower.contains("## steps")
    {
        return DocType::Runbook;
    }

    // Components have Interface/Configuration sections
    if content_lower.contains("## interface") || content_lower.contains("## configuration") {
        return DocType::Component;
    }

    DocType::Other
}

/// Returns the type-specific rules for a given document type.
pub fn get_type_specific_rules(doc_type: DocType, config: &RulesSection) -> Vec<Rule> {
    let mut rules = Vec::new();

    match doc_type {
        DocType::Runbook if config.type_specific.runbooks => {
            // Runbooks require: When to Use, Steps, Rollback, Verification
            rules.push(Rule::RequireSection {
                name: "When to Use".to_string(),
            });
            rules.push(Rule::RequireSection {
                name: "Steps".to_string(),
            });
            rules.push(Rule::RequireSection {
                name: "Rollback".to_string(),
            });
        }
        DocType::Adr if config.type_specific.adrs => {
            // ADRs require: Status (with valid value), Context, Decision, Consequences
            rules.push(Rule::RequireSection {
                name: "Status".to_string(),
            });
            rules.push(Rule::RequireValidAdrStatus);
            rules.push(Rule::RequireSection {
                name: "Context".to_string(),
            });
            rules.push(Rule::RequireSection {
                name: "Decision".to_string(),
            });
            rules.push(Rule::RequireSection {
                name: "Consequences".to_string(),
            });
        }
        DocType::Component if config.type_specific.components => {
            // Components require: Interface OR Configuration (at least one)
            rules.push(Rule::RequireOneOf {
                sections: vec!["Interface".to_string(), "Configuration".to_string()],
            });
        }
        _ => {}
    }

    rules
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn parse_doc(content: &str) -> ParsedDoc {
        ParsedDoc::parse_content(PathBuf::from("test.md"), content).unwrap()
    }

    #[test]
    fn validate_document_passes_all_rules() {
        let content = r#"# Complete Document

## Purpose
This document demonstrates validation.

## Verification
Run `cargo test` to verify.
```bash
$ cargo test
```

## Examples
```rust
fn example() {}
```
"#;
        let doc = parse_doc(content);
        let engine = RulesEngine::with_defaults();
        let result = engine.validate(&doc);

        assert!(result.is_valid(), "errors: {:?}", result.errors);
    }

    #[test]
    fn validate_missing_required_section() {
        let content = r#"# Incomplete Document

## Purpose
This is missing Verification and Examples.
"#;
        let doc = parse_doc(content);
        let engine = RulesEngine::with_defaults();
        let result = engine.validate(&doc);

        assert!(!result.is_valid());
        assert!(
            result
                .errors
                .iter()
                .any(|e| e.message.contains("Verification"))
        );
    }

    #[test]
    fn validate_document_exceeds_line_limit() {
        // Create a document with more than 300 lines
        let mut content =
            "# Long Document\n\n## Purpose\nThis is a very long document.\n\n".to_string();
        for i in 0..300 {
            content.push_str(&format!("Line {}\n", i));
        }

        let doc = parse_doc(&content);
        let engine = RulesEngine::new(vec![Rule::MaxLines { limit: 300 }]);
        let result = engine.validate(&doc);

        assert!(!result.is_valid());
        assert!(result.errors.iter().any(|e| e.message.contains("exceeds")));
    }

    #[test]
    fn validate_missing_code_block_in_examples() {
        let content = r#"# Document Without Code Examples

## Purpose
This document has an Examples section but no code.

## Verification
Run the tests.
```bash
$ cargo test
```

## Examples
This section has text but no code blocks.
"#;
        let doc = parse_doc(content);
        let engine = RulesEngine::with_defaults();
        let result = engine.validate(&doc);

        assert!(!result.is_valid());
        assert!(
            result
                .errors
                .iter()
                .any(|e| e.message.contains("code block"))
        );
    }

    #[test]
    fn validation_result_includes_suggestions() {
        let content = r#"# Incomplete Document

## Purpose
Missing verification.
"#;
        let doc = parse_doc(content);
        let engine = RulesEngine::new(vec![Rule::RequireSection {
            name: "Verification".to_string(),
        }]);
        let result = engine.validate(&doc);

        assert!(!result.is_valid());
        let error = &result.errors[0];
        assert!(error.suggestion.is_some());
        assert!(error.suggestion.as_ref().unwrap().contains("Verification"));
    }

    #[test]
    fn rules_engine_from_config() {
        let config = RulesSection {
            max_lines: 500,
            require_verification: true,
            require_examples: false,
            require_verification_commands: true,
            strict_output_matching: false,
            skip_output_matching: false,
            type_specific: Default::default(),
            validate_paths: false,
            warn_empty_paths: false,
            gradual: false,
            gradual_until: None,
        };
        let engine = RulesEngine::from_config(&config);

        // Should have: Purpose, Verification, RequireCommand(Verification), MaxLines
        assert_eq!(engine.rules().len(), 4);
        assert!(
            engine
                .rules()
                .iter()
                .any(|r| matches!(r, Rule::MaxLines { limit: 500 }))
        );
        assert!(engine.rules().iter().any(|r| matches!(
            r,
            Rule::RequireSection { name } if name == "Verification"
        )));
        assert!(engine.rules().iter().any(|r| matches!(
            r,
            Rule::RequireCommand { in_section } if in_section == "Verification"
        )));
        // Examples rule should not be present
        assert!(!engine.rules().iter().any(|r| matches!(
            r,
            Rule::RequireSection { name } if name == "Examples"
        )));
    }

    #[test]
    fn rules_engine_from_config_without_verification_commands() {
        let config = RulesSection {
            max_lines: 300,
            require_verification: true,
            require_examples: false,
            require_verification_commands: false,
            strict_output_matching: false,
            skip_output_matching: false,
            type_specific: Default::default(),
            validate_paths: false,
            warn_empty_paths: false,
            gradual: false,
            gradual_until: None,
        };
        let engine = RulesEngine::from_config(&config);

        // Should have: Purpose, Verification, MaxLines (no RequireCommand)
        assert_eq!(engine.rules().len(), 3);
        assert!(engine.rules().iter().any(|r| matches!(
            r,
            Rule::RequireSection { name } if name == "Verification"
        )));
        // RequireCommand rule should NOT be present
        assert!(
            !engine
                .rules()
                .iter()
                .any(|r| matches!(r, Rule::RequireCommand { .. }))
        );
    }

    #[test]
    fn rule_names_are_descriptive() {
        assert_eq!(
            Rule::RequireSection {
                name: "Purpose".to_string()
            }
            .name(),
            "require-section-purpose"
        );
        assert_eq!(Rule::MaxLines { limit: 300 }.name(), "max-lines-300");
        assert_eq!(
            Rule::RequireCodeBlock {
                in_section: "Examples".to_string()
            }
            .name(),
            "require-code-block-in-examples"
        );
    }

    #[test]
    fn validation_result_methods() {
        let mut result = ValidationResult::new("test.md");
        assert!(result.is_valid());
        assert!(!result.has_warnings());

        result.errors.push(ValidationError {
            rule: "test".to_string(),
            message: "test error".to_string(),
            line: None,
            suggestion: None,
        });
        assert!(!result.is_valid());

        result.warnings.push(ValidationWarning {
            rule: "test".to_string(),
            message: "test warning".to_string(),
            line: None,
        });
        assert!(result.has_warnings());
    }

    #[test]
    fn section_name_matching_is_case_insensitive() {
        let content = r#"# Document

## PURPOSE
This is uppercase.

## verification
This is lowercase.
"#;
        let doc = parse_doc(content);
        assert!(doc.has_section("Purpose"));
        assert!(doc.has_section("purpose"));
        assert!(doc.has_section("PURPOSE"));
        assert!(doc.has_section("Verification"));
        assert!(doc.has_section("VERIFICATION"));
    }

    #[test]
    fn require_command_rule_detects_shell_commands() {
        let content = r#"# Document

## Purpose
Test document.

## Verification
```bash
$ cargo test
```
"#;
        let doc = parse_doc(content);
        let engine = RulesEngine::new(vec![Rule::RequireCommand {
            in_section: "Verification".to_string(),
        }]);
        let result = engine.validate(&doc);

        assert!(result.is_valid());
    }

    #[test]
    fn require_command_rule_fails_without_command() {
        let content = r#"# Document

## Purpose
Test document.

## Verification
Just some text, no commands here.
"#;
        let doc = parse_doc(content);
        let engine = RulesEngine::new(vec![Rule::RequireCommand {
            in_section: "Verification".to_string(),
        }]);
        let result = engine.validate(&doc);

        assert!(!result.is_valid());
        assert!(
            result
                .errors
                .iter()
                .any(|e| e.message.contains("runnable command"))
        );
    }

    #[test]
    fn require_one_of_passes_with_first_section() {
        let content = r#"# Component

## Purpose
A test component.

## Interface
The API interface.
"#;
        let doc = parse_doc(content);
        let engine = RulesEngine::new(vec![Rule::RequireOneOf {
            sections: vec!["Interface".to_string(), "Configuration".to_string()],
        }]);
        let result = engine.validate(&doc);
        assert!(result.is_valid());
    }

    #[test]
    fn require_one_of_passes_with_second_section() {
        let content = r#"# Component

## Purpose
A test component.

## Configuration
The configuration options.
"#;
        let doc = parse_doc(content);
        let engine = RulesEngine::new(vec![Rule::RequireOneOf {
            sections: vec!["Interface".to_string(), "Configuration".to_string()],
        }]);
        let result = engine.validate(&doc);
        assert!(result.is_valid());
    }

    #[test]
    fn require_one_of_fails_without_any_section() {
        let content = r#"# Component

## Purpose
A test component without interface or config.
"#;
        let doc = parse_doc(content);
        let engine = RulesEngine::new(vec![Rule::RequireOneOf {
            sections: vec!["Interface".to_string(), "Configuration".to_string()],
        }]);
        let result = engine.validate(&doc);
        assert!(!result.is_valid());
        assert!(
            result
                .errors
                .iter()
                .any(|e| e.message.contains("Interface"))
        );
    }

    #[test]
    fn require_valid_adr_status_passes_with_accepted() {
        let content = r#"# ADR: Test Decision

## Status
Accepted

## Purpose
A test ADR.
"#;
        let doc = parse_doc(content);
        let engine = RulesEngine::new(vec![Rule::RequireValidAdrStatus]);
        let result = engine.validate(&doc);
        assert!(result.is_valid());
    }

    #[test]
    fn require_valid_adr_status_passes_with_proposed() {
        let content = r#"# ADR: Test Decision

## Status
Proposed

## Purpose
A test ADR.
"#;
        let doc = parse_doc(content);
        let engine = RulesEngine::new(vec![Rule::RequireValidAdrStatus]);
        let result = engine.validate(&doc);
        assert!(result.is_valid());
    }

    #[test]
    fn require_valid_adr_status_fails_with_invalid_status() {
        let content = r#"# ADR: Test Decision

## Status
Unknown

## Purpose
A test ADR.
"#;
        let doc = parse_doc(content);
        let engine = RulesEngine::new(vec![Rule::RequireValidAdrStatus]);
        let result = engine.validate(&doc);
        assert!(!result.is_valid());
        assert!(
            result
                .errors
                .iter()
                .any(|e| e.message.contains("valid status"))
        );
    }

    #[test]
    fn detect_doc_type_from_path_component() {
        let path = PathBuf::from("docs/components/auth.md");
        assert_eq!(detect_doc_type(&path, ""), DocType::Component);
    }

    #[test]
    fn detect_doc_type_from_path_runbook() {
        let path = PathBuf::from("docs/runbooks/deploy.md");
        assert_eq!(detect_doc_type(&path, ""), DocType::Runbook);
    }

    #[test]
    fn detect_doc_type_from_path_adr() {
        let path = PathBuf::from("docs/adr/001-use-rust.md");
        assert_eq!(detect_doc_type(&path, ""), DocType::Adr);
    }

    #[test]
    fn detect_doc_type_from_content_adr() {
        let path = PathBuf::from("docs/decisions/001.md");
        assert_eq!(detect_doc_type(&path, ""), DocType::Adr);

        let path = PathBuf::from("docs/other.md");
        let content = "## Status\nAccepted\n\n## Context\nSome context.";
        assert_eq!(detect_doc_type(&path, content), DocType::Adr);
    }

    #[test]
    fn detect_doc_type_from_content_runbook() {
        let path = PathBuf::from("docs/ops/deploy.md");
        let content = "## When to Use\nWhen deploying.";
        assert_eq!(detect_doc_type(&path, content), DocType::Runbook);

        let content = "## Steps\n1. First step.";
        assert_eq!(detect_doc_type(&path, content), DocType::Runbook);
    }

    #[test]
    fn detect_doc_type_from_content_component() {
        let path = PathBuf::from("docs/services/auth.md");
        let content = "## Interface\nThe API.";
        assert_eq!(detect_doc_type(&path, content), DocType::Component);

        let content = "## Configuration\nConfig options.";
        assert_eq!(detect_doc_type(&path, content), DocType::Component);
    }

    #[test]
    fn detect_doc_type_other() {
        let path = PathBuf::from("docs/misc/readme.md");
        assert_eq!(
            detect_doc_type(&path, "## Purpose\nJust a doc."),
            DocType::Other
        );
    }

    #[test]
    fn get_type_specific_rules_runbook() {
        let config = RulesSection {
            type_specific: crate::config::TypeSpecificRulesSection {
                runbooks: true,
                adrs: false,
                components: false,
            },
            ..Default::default()
        };
        let rules = get_type_specific_rules(DocType::Runbook, &config);
        assert_eq!(rules.len(), 3); // When to Use, Steps, Rollback
        assert!(rules.iter().any(|r| matches!(
            r,
            Rule::RequireSection { name } if name == "When to Use"
        )));
        assert!(rules.iter().any(|r| matches!(
            r,
            Rule::RequireSection { name } if name == "Steps"
        )));
        assert!(rules.iter().any(|r| matches!(
            r,
            Rule::RequireSection { name } if name == "Rollback"
        )));
    }

    #[test]
    fn get_type_specific_rules_adr() {
        let config = RulesSection {
            type_specific: crate::config::TypeSpecificRulesSection {
                runbooks: false,
                adrs: true,
                components: false,
            },
            ..Default::default()
        };
        let rules = get_type_specific_rules(DocType::Adr, &config);
        assert_eq!(rules.len(), 5); // Status, RequireValidAdrStatus, Context, Decision, Consequences
        assert!(rules.iter().any(|r| matches!(
            r,
            Rule::RequireSection { name } if name == "Status"
        )));
        assert!(
            rules
                .iter()
                .any(|r| matches!(r, Rule::RequireValidAdrStatus))
        );
        assert!(rules.iter().any(|r| matches!(
            r,
            Rule::RequireSection { name } if name == "Context"
        )));
        assert!(rules.iter().any(|r| matches!(
            r,
            Rule::RequireSection { name } if name == "Decision"
        )));
        assert!(rules.iter().any(|r| matches!(
            r,
            Rule::RequireSection { name } if name == "Consequences"
        )));
    }

    #[test]
    fn get_type_specific_rules_component() {
        let config = RulesSection {
            type_specific: crate::config::TypeSpecificRulesSection {
                runbooks: false,
                adrs: false,
                components: true,
            },
            ..Default::default()
        };
        let rules = get_type_specific_rules(DocType::Component, &config);
        assert_eq!(rules.len(), 1); // RequireOneOf Interface/Configuration
        assert!(
            matches!(&rules[0], Rule::RequireOneOf { sections } if sections.contains(&"Interface".to_string()))
        );
    }

    #[test]
    fn get_type_specific_rules_disabled() {
        let config = RulesSection::default(); // All type-specific rules disabled
        assert!(get_type_specific_rules(DocType::Runbook, &config).is_empty());
        assert!(get_type_specific_rules(DocType::Adr, &config).is_empty());
        assert!(get_type_specific_rules(DocType::Component, &config).is_empty());
        assert!(get_type_specific_rules(DocType::Other, &config).is_empty());
    }

    #[test]
    fn validate_with_type_applies_type_specific_rules() {
        let content = r#"# Runbook: Deploy

## Purpose
How to deploy.

## Verification
```bash
$ echo "deployed"
```

## Examples
```bash
$ deploy.sh
```
"#;
        let doc = parse_doc(content);
        let config = RulesSection {
            type_specific: crate::config::TypeSpecificRulesSection {
                runbooks: true,
                adrs: false,
                components: false,
            },
            ..Default::default()
        };
        let engine = RulesEngine::from_config(&config);
        let result = engine.validate_with_type(&doc, DocType::Runbook, &config);

        // Should fail because missing When to Use, Steps, Rollback
        assert!(!result.is_valid());
        assert!(
            result
                .errors
                .iter()
                .any(|e| e.message.contains("When to Use"))
        );
        assert!(result.errors.iter().any(|e| e.message.contains("Steps")));
        assert!(result.errors.iter().any(|e| e.message.contains("Rollback")));
    }

    #[test]
    fn validate_with_type_passes_complete_runbook() {
        let content = r#"# Runbook: Deploy

## Purpose
How to deploy.

## When to Use
When deploying.

## Steps
1. Run deploy.

## Rollback
Revert the deploy.

## Verification
```bash
$ echo "deployed"
```

## Examples
```bash
$ deploy.sh
```
"#;
        let doc = parse_doc(content);
        let config = RulesSection {
            type_specific: crate::config::TypeSpecificRulesSection {
                runbooks: true,
                adrs: false,
                components: false,
            },
            ..Default::default()
        };
        let engine = RulesEngine::from_config(&config);
        let result = engine.validate_with_type(&doc, DocType::Runbook, &config);
        assert!(result.is_valid(), "errors: {:?}", result.errors);
    }

    #[test]
    fn rule_names_for_new_rules() {
        assert_eq!(
            Rule::RequireOneOf {
                sections: vec!["Interface".to_string(), "Configuration".to_string()]
            }
            .name(),
            "require-one-of-interface-or-configuration"
        );
        assert_eq!(
            Rule::RequireValidAdrStatus.name(),
            "require-valid-adr-status"
        );
    }

    #[test]
    fn validate_paths_rule_name() {
        let rule = Rule::ValidatePaths {
            project_root: PathBuf::from("."),
            warn_empty: false,
        };
        assert_eq!(rule.name(), "validate-paths");
    }

    #[test]
    fn validate_paths_valid_patterns_pass() {
        let content = r#"# Component

## Purpose
This is a component.

## Paths
- `src/*.rs`
- `src/commands/*.rs`
- `docs/**/*.md`
"#;
        let doc = parse_doc(content);
        let engine = RulesEngine::new(vec![Rule::ValidatePaths {
            project_root: PathBuf::from("."),
            warn_empty: false,
        }]);
        let result = engine.validate(&doc);

        assert!(result.is_valid(), "errors: {:?}", result.errors);
    }

    #[test]
    fn validate_paths_invalid_glob_pattern_fails() {
        let content = r#"# Component

## Purpose
This is a component.

## Paths
- `src/[broken`
- `src/*.rs`
"#;
        let doc = parse_doc(content);
        let engine = RulesEngine::new(vec![Rule::ValidatePaths {
            project_root: PathBuf::from("."),
            warn_empty: false,
        }]);
        let result = engine.validate(&doc);

        assert!(!result.is_valid());
        assert_eq!(result.errors.len(), 1);
        assert!(result.errors[0].message.contains("invalid glob pattern"));
        assert!(result.errors[0].message.contains("src/[broken"));
    }

    #[test]
    fn validate_paths_absolute_path_fails() {
        let content = r#"# Component

## Purpose
This is a component.

## Paths
- `/usr/local/bin/foo`
- `src/*.rs`
"#;
        let doc = parse_doc(content);
        let engine = RulesEngine::new(vec![Rule::ValidatePaths {
            project_root: PathBuf::from("."),
            warn_empty: false,
        }]);
        let result = engine.validate(&doc);

        assert!(!result.is_valid());
        assert_eq!(result.errors.len(), 1);
        assert!(result.errors[0].message.contains("absolute"));
        assert!(
            result.errors[0]
                .suggestion
                .as_ref()
                .unwrap()
                .contains("usr/local/bin/foo")
        );
    }

    #[test]
    fn validate_paths_document_without_paths_section_passes() {
        let content = r#"# Component

## Purpose
This is a component without paths.
"#;
        let doc = parse_doc(content);
        let engine = RulesEngine::new(vec![Rule::ValidatePaths {
            project_root: PathBuf::from("."),
            warn_empty: false,
        }]);
        let result = engine.validate(&doc);

        assert!(result.is_valid());
    }

    #[test]
    fn validate_paths_warn_empty_creates_warning() {
        let content = r#"# Component

## Purpose
This is a component.

## Paths
- `nonexistent/path/that/does/not/exist/*.xyz`
"#;
        let doc = parse_doc(content);
        let engine = RulesEngine::new(vec![Rule::ValidatePaths {
            project_root: PathBuf::from("."),
            warn_empty: true,
        }]);
        let result = engine.validate(&doc);

        // Should still be valid (warning, not error)
        assert!(result.is_valid());
        // Should have a warning
        assert!(result.has_warnings());
        assert!(result.warnings[0].message.contains("matches no files"));
    }

    #[test]
    fn validate_paths_warn_empty_false_no_warning() {
        let content = r#"# Component

## Purpose
This is a component.

## Paths
- `nonexistent/path/that/does/not/exist/*.xyz`
"#;
        let doc = parse_doc(content);
        let engine = RulesEngine::new(vec![Rule::ValidatePaths {
            project_root: PathBuf::from("."),
            warn_empty: false,
        }]);
        let result = engine.validate(&doc);

        assert!(result.is_valid());
        assert!(!result.has_warnings());
    }

    #[test]
    fn validate_paths_multiple_errors() {
        let content = r#"# Component

## Purpose
This is a component.

## Paths
- `/absolute/path`
- `src/[broken`
- `src/*.rs`
"#;
        let doc = parse_doc(content);
        let engine = RulesEngine::new(vec![Rule::ValidatePaths {
            project_root: PathBuf::from("."),
            warn_empty: false,
        }]);
        let result = engine.validate(&doc);

        assert!(!result.is_valid());
        assert_eq!(result.errors.len(), 2);
        // One for absolute path, one for invalid glob
        assert!(result.errors.iter().any(|e| e.message.contains("absolute")));
        assert!(
            result
                .errors
                .iter()
                .any(|e| e.message.contains("invalid glob"))
        );
    }

    #[test]
    fn validate_paths_with_asterisk_prefix() {
        let content = r#"# Component

## Purpose
This is a component.

## Paths
* src/*.rs
* docs/
"#;
        let doc = parse_doc(content);
        let engine = RulesEngine::new(vec![Rule::ValidatePaths {
            project_root: PathBuf::from("."),
            warn_empty: false,
        }]);
        let result = engine.validate(&doc);

        assert!(result.is_valid(), "errors: {:?}", result.errors);
    }

    #[test]
    fn validate_paths_line_numbers_are_correct() {
        let content = r#"# Component

## Purpose
This is a component.

## Paths
- `valid/*.rs`
- `/invalid/absolute`
"#;
        let doc = parse_doc(content);
        let engine = RulesEngine::new(vec![Rule::ValidatePaths {
            project_root: PathBuf::from("."),
            warn_empty: false,
        }]);
        let result = engine.validate(&doc);

        assert!(!result.is_valid());
        assert_eq!(result.errors.len(), 1);
        // The error should have a line number
        assert!(result.errors[0].line.is_some());
    }

    #[test]
    fn rules_engine_from_config_with_validate_paths() {
        let config = RulesSection {
            max_lines: 300,
            require_verification: false,
            require_examples: false,
            require_verification_commands: false,
            strict_output_matching: false,
            skip_output_matching: false,
            type_specific: Default::default(),
            validate_paths: true,
            warn_empty_paths: true,
            gradual: false,
            gradual_until: None,
        };
        let engine = RulesEngine::from_config_with_root(&config, "/project/root");

        // Should have: Purpose, MaxLines, ValidatePaths
        assert_eq!(engine.rules().len(), 3);
        assert!(engine.rules().iter().any(|r| matches!(
            r,
            Rule::ValidatePaths {
                warn_empty: true,
                ..
            }
        )));
    }

    #[test]
    fn rules_engine_from_config_without_validate_paths() {
        let config = RulesSection {
            max_lines: 300,
            require_verification: false,
            require_examples: false,
            require_verification_commands: false,
            strict_output_matching: false,
            skip_output_matching: false,
            type_specific: Default::default(),
            validate_paths: false,
            warn_empty_paths: false,
            gradual: false,
            gradual_until: None,
        };
        let engine = RulesEngine::from_config(&config);

        // Should have: Purpose, MaxLines (no ValidatePaths)
        assert_eq!(engine.rules().len(), 2);
        assert!(
            !engine
                .rules()
                .iter()
                .any(|r| matches!(r, Rule::ValidatePaths { .. }))
        );
    }

    #[test]
    fn extract_paths_patterns_helper() {
        let content = r#"Some intro text.
- `src/commands/*.rs`
- `src/cli.rs`
* docs/
"#;
        let patterns = RulesEngine::extract_paths_patterns(content);

        assert_eq!(patterns.len(), 3);
        assert_eq!(patterns[0], (2, "src/commands/*.rs".to_string()));
        assert_eq!(patterns[1], (3, "src/cli.rs".to_string()));
        assert_eq!(patterns[2], (4, "docs/".to_string()));
    }
}
