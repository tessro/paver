//! Generate prompts for AI agents to create PAVED-compliant documentation.
//!
//! This module provides functionality to generate structured prompts that help
//! AI agents produce documentation that passes `paver check`.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;

use crate::config::{CONFIG_FILENAME, PaverConfig, RulesSection};
use crate::templates::{TemplateType, get_template};

/// Output format for the generated prompt.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum OutputFormat {
    /// Plain text output (default).
    #[default]
    Text,
    /// JSON output for programmatic use.
    Json,
}

/// Options for generating a prompt.
#[derive(Debug, Clone)]
pub struct PromptOptions {
    /// The document type to generate.
    pub doc_type: TemplateType,
    /// Name of the thing being documented.
    pub name: Option<String>,
    /// Path to existing document to update.
    pub update_path: Option<String>,
    /// Additional context file paths.
    pub context_paths: Vec<String>,
    /// Output format.
    pub output_format: OutputFormat,
}

/// JSON output structure for programmatic use.
#[derive(Debug, Serialize, Deserialize)]
pub struct PromptOutput {
    /// The full generated prompt.
    pub prompt: String,
    /// The template content.
    pub template: String,
    /// Project rules that apply.
    pub rules: Vec<String>,
    /// Context files included.
    pub context_files: Vec<String>,
}

/// Generate a prompt for AI agents to create PAVED documentation.
pub fn generate_prompt(options: &PromptOptions) -> Result<String> {
    let config = load_config_or_default()?;
    let template = get_template(options.doc_type);
    let rules = format_rules(&config.rules);
    let paved_sections = get_paved_sections(options.doc_type);
    let doc_type_name = get_doc_type_name(options.doc_type);

    let mut prompt = String::new();

    // Header
    prompt.push_str(&format!(
        "You are documenting a software {} using the PAVED framework.\n\n",
        doc_type_name
    ));

    // PAVED Structure section
    prompt.push_str("## PAVED Structure\n");
    prompt.push_str("Your document MUST include these sections:\n");
    prompt.push_str(&paved_sections);
    prompt.push('\n');

    // Project Rules section
    prompt.push_str("## Project Rules\n");
    for rule in &rules {
        prompt.push_str(&format!("- {}\n", rule));
    }
    prompt.push('\n');

    // Template section
    prompt.push_str("## Template\n");
    prompt.push_str("Use this template as the starting structure:\n\n");
    prompt.push_str("```markdown\n");
    prompt.push_str(template);
    prompt.push_str("```\n\n");

    // Context section (if update or context files provided)
    let mut has_context = false;
    if options.update_path.is_some() || !options.context_paths.is_empty() {
        prompt.push_str("## Context\n");
        has_context = true;
    }

    // Include existing document content if updating
    if let Some(update_path) = &options.update_path {
        let existing_content = std::fs::read_to_string(update_path)
            .with_context(|| format!("failed to read existing document: {}", update_path))?;
        prompt.push_str("### Existing Document (to update)\n");
        prompt.push_str("```markdown\n");
        prompt.push_str(&existing_content);
        prompt.push_str("```\n\n");
    }

    // Include context files
    for path in &options.context_paths {
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("failed to read context file: {}", path))?;
        prompt.push_str(&format!("### Context: {}\n", path));
        prompt.push_str("```\n");
        prompt.push_str(&content);
        prompt.push_str("```\n\n");
    }

    if has_context {
        prompt.push('\n');
    }

    // Task section
    prompt.push_str("## Task\n");
    if let Some(name) = &options.name {
        if options.update_path.is_some() {
            prompt.push_str(&format!(
                "Update the PAVED {} document for: {}\n",
                doc_type_name, name
            ));
        } else {
            prompt.push_str(&format!(
                "Create a PAVED {} document for: {}\n",
                doc_type_name, name
            ));
        }
    } else if options.update_path.is_some() {
        prompt.push_str(&format!(
            "Update the PAVED {} document shown above.\n",
            doc_type_name
        ));
    } else {
        prompt.push_str(&format!("Create a PAVED {} document.\n", doc_type_name));
    }

    match options.output_format {
        OutputFormat::Text => Ok(prompt),
        OutputFormat::Json => {
            let output = PromptOutput {
                prompt,
                template: template.to_string(),
                rules,
                context_files: options.context_paths.clone(),
            };
            serde_json::to_string_pretty(&output).context("failed to serialize JSON output")
        }
    }
}

/// Load configuration from .paver.toml or return defaults if not found.
fn load_config_or_default() -> Result<PaverConfig> {
    if Path::new(CONFIG_FILENAME).exists() {
        PaverConfig::load(CONFIG_FILENAME)
    } else {
        Ok(PaverConfig::default())
    }
}

/// Format rules section from configuration.
fn format_rules(rules: &RulesSection) -> Vec<String> {
    let mut formatted = Vec::new();

    formatted.push(format!("Maximum {} lines per document", rules.max_lines));

    if rules.require_verification {
        formatted.push("Verification section must include runnable commands".to_string());
    }

    if rules.require_examples {
        formatted.push("Examples must include expected output".to_string());
    }

    formatted
}

/// Get PAVED section descriptions for a document type.
fn get_paved_sections(doc_type: TemplateType) -> String {
    match doc_type {
        TemplateType::Component => {
            "- **Purpose**: What is this? What problem does it solve? (1-3 sentences, include non-goals)\n\
             - **Interface**: How do you use it? (CLI commands, API endpoints, schemas)\n\
             - **Configuration**: Config keys, environment variables, file formats\n\
             - **Verification**: How do you know it's working? (test commands, health checks)\n\
             - **Examples**: Concrete copy/paste examples (happy path, realistic, failure case)\n\
             - **Gotchas**: Common pitfalls and how to avoid them\n\
             - **Decisions**: Why this design? What must not change?"
                .to_string()
        }
        TemplateType::Runbook => {
            "- **When to Use**: Circumstances that trigger this runbook\n\
             - **Preconditions**: What must be true before starting\n\
             - **Steps**: Numbered steps with commands that actually run\n\
             - **Rollback**: How to undo if something goes wrong\n\
             - **Verification**: How to confirm success\n\
             - **Escalation**: Who to contact if this doesn't work"
                .to_string()
        }
        TemplateType::Adr => {
            "- **Status**: Proposed | Accepted | Deprecated | Superseded\n\
             - **Context**: What is the issue we're deciding on?\n\
             - **Decision**: What did we decide?\n\
             - **Consequences**: What are the results of this decision?\n\
             - **Alternatives Considered**: What else did we consider and why not?"
                .to_string()
        }
    }
}

/// Get human-readable name for document type.
fn get_doc_type_name(doc_type: TemplateType) -> &'static str {
    match doc_type {
        TemplateType::Component => "component",
        TemplateType::Runbook => "runbook",
        TemplateType::Adr => "architecture decision record (ADR)",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prompt_includes_all_component_sections() {
        let options = PromptOptions {
            doc_type: TemplateType::Component,
            name: Some("auth service".to_string()),
            update_path: None,
            context_paths: vec![],
            output_format: OutputFormat::Text,
        };

        let prompt = generate_prompt(&options).unwrap();

        assert!(prompt.contains("## PAVED Structure"));
        assert!(prompt.contains("**Purpose**"));
        assert!(prompt.contains("**Interface**"));
        assert!(prompt.contains("**Configuration**"));
        assert!(prompt.contains("**Verification**"));
        assert!(prompt.contains("**Examples**"));
        assert!(prompt.contains("**Gotchas**"));
        assert!(prompt.contains("**Decisions**"));
    }

    #[test]
    fn prompt_includes_all_runbook_sections() {
        let options = PromptOptions {
            doc_type: TemplateType::Runbook,
            name: Some("deploy api".to_string()),
            update_path: None,
            context_paths: vec![],
            output_format: OutputFormat::Text,
        };

        let prompt = generate_prompt(&options).unwrap();

        assert!(prompt.contains("**When to Use**"));
        assert!(prompt.contains("**Preconditions**"));
        assert!(prompt.contains("**Steps**"));
        assert!(prompt.contains("**Rollback**"));
        assert!(prompt.contains("**Verification**"));
        assert!(prompt.contains("**Escalation**"));
    }

    #[test]
    fn prompt_includes_all_adr_sections() {
        let options = PromptOptions {
            doc_type: TemplateType::Adr,
            name: Some("use postgres".to_string()),
            update_path: None,
            context_paths: vec![],
            output_format: OutputFormat::Text,
        };

        let prompt = generate_prompt(&options).unwrap();

        assert!(prompt.contains("**Status**"));
        assert!(prompt.contains("**Context**"));
        assert!(prompt.contains("**Decision**"));
        assert!(prompt.contains("**Consequences**"));
        assert!(prompt.contains("**Alternatives Considered**"));
    }

    #[test]
    fn prompt_includes_project_rules() {
        let options = PromptOptions {
            doc_type: TemplateType::Component,
            name: Some("test".to_string()),
            update_path: None,
            context_paths: vec![],
            output_format: OutputFormat::Text,
        };

        let prompt = generate_prompt(&options).unwrap();

        assert!(prompt.contains("## Project Rules"));
        assert!(prompt.contains("Maximum 300 lines per document"));
        assert!(prompt.contains("Verification section must include runnable commands"));
        assert!(prompt.contains("Examples must include expected output"));
    }

    #[test]
    fn prompt_includes_template() {
        let options = PromptOptions {
            doc_type: TemplateType::Component,
            name: Some("test".to_string()),
            update_path: None,
            context_paths: vec![],
            output_format: OutputFormat::Text,
        };

        let prompt = generate_prompt(&options).unwrap();

        assert!(prompt.contains("## Template"));
        assert!(prompt.contains("{Component Name}"));
    }

    #[test]
    fn prompt_includes_name_in_task() {
        let options = PromptOptions {
            doc_type: TemplateType::Component,
            name: Some("auth service".to_string()),
            update_path: None,
            context_paths: vec![],
            output_format: OutputFormat::Text,
        };

        let prompt = generate_prompt(&options).unwrap();

        assert!(prompt.contains("## Task"));
        assert!(prompt.contains("Create a PAVED component document for: auth service"));
    }

    #[test]
    fn json_output_is_valid() {
        let options = PromptOptions {
            doc_type: TemplateType::Component,
            name: Some("test".to_string()),
            update_path: None,
            context_paths: vec![],
            output_format: OutputFormat::Json,
        };

        let output = generate_prompt(&options).unwrap();
        let parsed: PromptOutput = serde_json::from_str(&output).unwrap();

        assert!(!parsed.prompt.is_empty());
        assert!(!parsed.template.is_empty());
        assert!(!parsed.rules.is_empty());
    }

    #[test]
    fn update_mode_indicates_update_in_task() {
        use std::io::Write;
        let temp_dir = std::env::temp_dir();
        let temp_file = temp_dir.join("test_doc.md");
        {
            let mut f = std::fs::File::create(&temp_file).unwrap();
            writeln!(f, "# Test Doc\n\nSome content").unwrap();
        }

        let options = PromptOptions {
            doc_type: TemplateType::Component,
            name: Some("test".to_string()),
            update_path: Some(temp_file.to_string_lossy().to_string()),
            context_paths: vec![],
            output_format: OutputFormat::Text,
        };

        let prompt = generate_prompt(&options).unwrap();

        assert!(prompt.contains("Update the PAVED component document for: test"));
        assert!(prompt.contains("### Existing Document (to update)"));
        assert!(prompt.contains("# Test Doc"));

        std::fs::remove_file(&temp_file).ok();
    }

    #[test]
    fn format_rules_respects_config() {
        let rules = RulesSection {
            max_lines: 500,
            require_verification: false,
            require_examples: true,
            require_verification_commands: true,
            strict_output_matching: false,
            skip_output_matching: false,
            type_specific: Default::default(),
            validate_paths: false,
            warn_empty_paths: false,
        };

        let formatted = format_rules(&rules);

        assert!(formatted.iter().any(|r| r.contains("500")));
        assert!(!formatted.iter().any(|r| r.contains("Verification section")));
        assert!(formatted.iter().any(|r| r.contains("Examples")));
    }
}
