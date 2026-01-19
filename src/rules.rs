//! Validation rules engine for PAVED documents.
//!
//! This module provides a rules engine that validates parsed PAVED documents
//! against configurable rules from `.paver.toml`.

use std::path::PathBuf;

use crate::config::RulesSection;
use crate::parser::ParsedDoc;

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
        }
    }
}

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
    pub fn from_config(config: &RulesSection) -> Self {
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
        }
    }

    /// Returns the rules in this engine.
    pub fn rules(&self) -> &[Rule] {
        &self.rules
    }
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
        };
        let engine = RulesEngine::from_config(&config);

        // Should have: Purpose, Verification, MaxLines (no Examples or RequireCodeBlock)
        assert_eq!(engine.rules().len(), 3);
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
        // Examples rule should not be present
        assert!(!engine.rules().iter().any(|r| matches!(
            r,
            Rule::RequireSection { name } if name == "Examples"
        )));
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
}
