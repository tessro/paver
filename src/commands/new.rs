//! Implementation of the `pave new` command for scaffolding documents.

use anyhow::{Context, Result, bail};
use std::fs;
use std::path::{Path, PathBuf};

use crate::templates::{TemplateType, get_template};

/// Arguments for the `pave new` command.
pub struct NewArgs {
    /// Document type: component, runbook, adr
    pub doc_type: TemplateType,
    /// Name for the document (used in filename and title)
    pub name: String,
    /// Where to create the file (optional, uses default if not specified)
    pub output: Option<PathBuf>,
}

/// Execute the `pave new` command.
pub fn execute(args: NewArgs) -> Result<()> {
    // Determine output path
    let output_path = args
        .output
        .unwrap_or_else(|| default_output_path(&args.doc_type, &args.name));

    // Check if file already exists
    if output_path.exists() {
        bail!("File already exists: {}", output_path.display());
    }

    // Get template and replace placeholders
    let template = get_template(args.doc_type);
    let content = substitute_placeholders(template, &args.name, args.doc_type);

    // Create parent directories if needed
    if let Some(parent) = output_path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create directory: {}", parent.display()))?;
    }

    // Write the file
    fs::write(&output_path, content)
        .with_context(|| format!("Failed to write file: {}", output_path.display()))?;

    // Print success message
    println!(
        "Created {} at {}",
        type_name(args.doc_type),
        output_path.display()
    );
    println!("\nNext steps:");
    println!("  1. Open the file and fill in the sections");
    println!("  2. Run `pave check` to validate the document");

    Ok(())
}

/// Returns the default output path for a given document type and name.
fn default_output_path(doc_type: &TemplateType, name: &str) -> PathBuf {
    let subdir = match doc_type {
        TemplateType::Component => "components",
        TemplateType::Runbook => "runbooks",
        TemplateType::Adr => "adrs",
    };
    Path::new("docs").join(subdir).join(format!("{}.md", name))
}

/// Substitutes placeholders in the template.
fn substitute_placeholders(template: &str, name: &str, doc_type: TemplateType) -> String {
    let title = to_title_case(name);

    // Replace the specific placeholder used in each template
    match doc_type {
        TemplateType::Component => template.replace("{Component Name}", &title),
        TemplateType::Runbook => template.replace("{Task Name}", &title),
        TemplateType::Adr => template.replace("{Title}", &title),
    }
}

/// Converts a kebab-case or snake_case name to Title Case.
fn to_title_case(name: &str) -> String {
    name.split(['-', '_'])
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                None => String::new(),
                Some(first) => first.to_uppercase().chain(chars).collect(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Returns the human-readable name for a template type.
fn type_name(doc_type: TemplateType) -> &'static str {
    match doc_type {
        TemplateType::Component => "component",
        TemplateType::Runbook => "runbook",
        TemplateType::Adr => "ADR",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn to_title_case_converts_kebab_case() {
        assert_eq!(to_title_case("auth-service"), "Auth Service");
        assert_eq!(to_title_case("deploy-production"), "Deploy Production");
        assert_eq!(to_title_case("use-postgresql"), "Use Postgresql");
    }

    #[test]
    fn to_title_case_converts_snake_case() {
        assert_eq!(to_title_case("auth_service"), "Auth Service");
        assert_eq!(to_title_case("deploy_production"), "Deploy Production");
    }

    #[test]
    fn to_title_case_handles_single_word() {
        assert_eq!(to_title_case("auth"), "Auth");
    }

    #[test]
    fn default_output_path_component() {
        let path = default_output_path(&TemplateType::Component, "auth-service");
        assert_eq!(path, Path::new("docs/components/auth-service.md"));
    }

    #[test]
    fn default_output_path_runbook() {
        let path = default_output_path(&TemplateType::Runbook, "deploy-production");
        assert_eq!(path, Path::new("docs/runbooks/deploy-production.md"));
    }

    #[test]
    fn default_output_path_adr() {
        let path = default_output_path(&TemplateType::Adr, "use-postgresql");
        assert_eq!(path, Path::new("docs/adrs/use-postgresql.md"));
    }

    #[test]
    fn substitute_placeholders_component() {
        let template = "# {Component Name}\n\nSome content";
        let result = substitute_placeholders(template, "auth-service", TemplateType::Component);
        assert!(result.starts_with("# Auth Service\n"));
    }

    #[test]
    fn substitute_placeholders_runbook() {
        let template = "# Runbook: {Task Name}\n\nSome content";
        let result = substitute_placeholders(template, "deploy-production", TemplateType::Runbook);
        assert!(result.starts_with("# Runbook: Deploy Production\n"));
    }

    #[test]
    fn substitute_placeholders_adr() {
        let template = "# ADR: {Title}\n\nSome content";
        let result = substitute_placeholders(template, "use-postgresql", TemplateType::Adr);
        assert!(result.starts_with("# ADR: Use Postgresql\n"));
    }

    #[test]
    fn execute_creates_component_file() {
        let temp_dir = TempDir::new().unwrap();
        let output_path = temp_dir.path().join("test-component.md");

        let args = NewArgs {
            doc_type: TemplateType::Component,
            name: "test-component".to_string(),
            output: Some(output_path.clone()),
        };

        execute(args).unwrap();

        assert!(output_path.exists());
        let content = fs::read_to_string(&output_path).unwrap();
        assert!(content.contains("# Test Component"));
        assert!(content.contains("## Purpose"));
    }

    #[test]
    fn execute_creates_runbook_file() {
        let temp_dir = TempDir::new().unwrap();
        let output_path = temp_dir.path().join("test-runbook.md");

        let args = NewArgs {
            doc_type: TemplateType::Runbook,
            name: "test-runbook".to_string(),
            output: Some(output_path.clone()),
        };

        execute(args).unwrap();

        assert!(output_path.exists());
        let content = fs::read_to_string(&output_path).unwrap();
        assert!(content.contains("# Runbook: Test Runbook"));
        assert!(content.contains("## Steps"));
    }

    #[test]
    fn execute_creates_adr_file() {
        let temp_dir = TempDir::new().unwrap();
        let output_path = temp_dir.path().join("test-adr.md");

        let args = NewArgs {
            doc_type: TemplateType::Adr,
            name: "test-adr".to_string(),
            output: Some(output_path.clone()),
        };

        execute(args).unwrap();

        assert!(output_path.exists());
        let content = fs::read_to_string(&output_path).unwrap();
        assert!(content.contains("# ADR: Test Adr"));
        assert!(content.contains("## Status"));
    }

    #[test]
    fn execute_creates_parent_directories() {
        let temp_dir = TempDir::new().unwrap();
        let output_path = temp_dir.path().join("nested").join("dir").join("doc.md");

        let args = NewArgs {
            doc_type: TemplateType::Component,
            name: "test".to_string(),
            output: Some(output_path.clone()),
        };

        execute(args).unwrap();
        assert!(output_path.exists());
    }

    #[test]
    fn execute_errors_if_file_exists() {
        let temp_dir = TempDir::new().unwrap();
        let output_path = temp_dir.path().join("existing.md");
        fs::write(&output_path, "existing content").unwrap();

        let args = NewArgs {
            doc_type: TemplateType::Component,
            name: "existing".to_string(),
            output: Some(output_path),
        };

        let result = execute(args);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("already exists"));
    }
}
