//! PAVED document templates for component, runbook, and ADR documentation.
//!
//! These templates follow the PAVED structure optimized for AI agents to author and consume.

/// The types of PAVED document templates available.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TemplateType {
    /// Component documentation for services, libraries, and modules.
    Component,
    /// Runbook for operational procedures.
    Runbook,
    /// Architecture Decision Record.
    Adr,
}

impl TemplateType {
    /// Returns all available template types.
    pub fn all() -> &'static [TemplateType] {
        &[
            TemplateType::Component,
            TemplateType::Runbook,
            TemplateType::Adr,
        ]
    }

    /// Returns the default filename for this template type.
    pub fn default_filename(&self) -> &'static str {
        match self {
            TemplateType::Component => "component.md",
            TemplateType::Runbook => "runbook.md",
            TemplateType::Adr => "adr.md",
        }
    }
}

/// Returns the template content for the given template type.
pub fn get_template(template_type: TemplateType) -> &'static str {
    match template_type {
        TemplateType::Component => include_str!("../templates/component.md"),
        TemplateType::Runbook => include_str!("../templates/runbook.md"),
        TemplateType::Adr => include_str!("../templates/adr.md"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn component_template_has_required_sections() {
        let template = get_template(TemplateType::Component);
        assert!(template.contains("## Purpose"));
        assert!(template.contains("## Interface"));
        assert!(template.contains("## Configuration"));
        assert!(template.contains("## Verification"));
        assert!(template.contains("## Examples"));
        assert!(template.contains("## Gotchas"));
        assert!(template.contains("## Decisions"));
    }

    #[test]
    fn runbook_template_has_required_sections() {
        let template = get_template(TemplateType::Runbook);
        assert!(template.contains("## When to Use"));
        assert!(template.contains("## Preconditions"));
        assert!(template.contains("## Steps"));
        assert!(template.contains("## Rollback"));
        assert!(template.contains("## Verification"));
        assert!(template.contains("## Escalation"));
    }

    #[test]
    fn adr_template_has_required_sections() {
        let template = get_template(TemplateType::Adr);
        assert!(template.contains("## Status"));
        assert!(template.contains("## Context"));
        assert!(template.contains("## Decision"));
        assert!(template.contains("## Consequences"));
        assert!(template.contains("## Alternatives Considered"));
    }

    #[test]
    fn all_templates_returns_all_types() {
        let all = TemplateType::all();
        assert_eq!(all.len(), 3);
        assert!(all.contains(&TemplateType::Component));
        assert!(all.contains(&TemplateType::Runbook));
        assert!(all.contains(&TemplateType::Adr));
    }

    #[test]
    fn default_filenames_are_correct() {
        assert_eq!(TemplateType::Component.default_filename(), "component.md");
        assert_eq!(TemplateType::Runbook.default_filename(), "runbook.md");
        assert_eq!(TemplateType::Adr.default_filename(), "adr.md");
    }
}
