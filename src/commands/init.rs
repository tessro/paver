//! Implementation of the `paver init` command.

use anyhow::{Context, Result, bail};
use std::fs;
use std::path::Path;

use crate::templates::{TemplateType, get_template};

/// Default content for the .paver.toml configuration file.
fn default_config(docs_root: &str) -> String {
    format!(
        r#"# Paver configuration file
# See https://github.com/tessro/paver for documentation

[docs]
# Root directory for documentation
root = "{docs_root}"

# Directory containing document templates
templates = "{docs_root}/templates"

[validation]
# Enable strict validation mode
strict = false
"#
    )
}

/// Returns the content for the index.md file.
fn get_index_template() -> &'static str {
    include_str!("../../templates/index.md")
}

/// Arguments for the init command.
pub struct InitArgs {
    /// Where to create the docs directory (default: "docs")
    pub docs_root: String,
    /// Also install git hooks for validation
    pub hooks: bool,
    /// Overwrite existing files
    pub force: bool,
    /// Working directory (for testing; uses current dir if None)
    pub working_dir: Option<std::path::PathBuf>,
}

impl Default for InitArgs {
    fn default() -> Self {
        Self {
            docs_root: "docs".to_string(),
            hooks: false,
            force: false,
            working_dir: None,
        }
    }
}

/// Execute the init command.
pub fn run(args: InitArgs) -> Result<()> {
    let base = args
        .working_dir
        .clone()
        .unwrap_or_else(|| std::path::PathBuf::from("."));
    let config_path = base.join(".paver.toml");

    // Check if already initialized
    if config_path.exists() && !args.force {
        bail!("Project already initialized (.paver.toml exists). Use --force to overwrite.");
    }

    let docs_root = base.join(&args.docs_root);
    let templates_dir = docs_root.join("templates");

    // Create directories
    fs::create_dir_all(&templates_dir).with_context(|| {
        format!(
            "Failed to create templates directory: {}",
            templates_dir.display()
        )
    })?;

    // Write .paver.toml
    fs::write(&config_path, default_config(&args.docs_root))
        .context("Failed to write .paver.toml")?;

    // Write index.md
    let index_path = docs_root.join("index.md");
    if !index_path.exists() || args.force {
        fs::write(&index_path, get_index_template())
            .with_context(|| format!("Failed to write {}", index_path.display()))?;
    }

    // Write template files
    for template_type in TemplateType::all() {
        let template_path = templates_dir.join(template_type.default_filename());
        if !template_path.exists() || args.force {
            fs::write(&template_path, get_template(*template_type))
                .with_context(|| format!("Failed to write {}", template_path.display()))?;
        }
    }

    // Handle git hooks if requested
    if args.hooks {
        install_git_hooks(&base)?;
    }

    // Print success message
    println!("Initialized PAVED documentation in {}/", args.docs_root);
    println!();
    println!("Created:");
    println!("  .paver.toml              - Configuration file");
    println!(
        "  {}/index.md          - Documentation index",
        args.docs_root
    );
    println!(
        "  {}/templates/        - Document templates",
        args.docs_root
    );
    println!();
    println!("Next steps:");
    println!("  paver new component <name>  - Create a component doc");
    println!("  paver new runbook <name>    - Create a runbook");
    println!("  paver new adr <name>        - Create an ADR");

    Ok(())
}

/// Install git hooks for documentation validation.
fn install_git_hooks(base: &Path) -> Result<()> {
    let git_hooks_dir = base.join(".git/hooks");

    if !git_hooks_dir.exists() {
        bail!("Not a git repository (no .git/hooks directory found)");
    }

    let pre_commit_path = git_hooks_dir.join("pre-commit");

    // Check if pre-commit hook already exists
    if pre_commit_path.exists() {
        println!("Warning: pre-commit hook already exists, skipping hook installation.");
        println!("Add 'paver check' to your existing hook manually.");
        return Ok(());
    }

    let hook_content = r#"#!/bin/sh
# PAVED documentation validation hook
# Installed by: paver init --hooks

paver check
"#;

    fs::write(&pre_commit_path, hook_content).context("Failed to write pre-commit hook")?;

    // Make the hook executable on Unix
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(&pre_commit_path)?.permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&pre_commit_path, perms)?;
    }

    println!("Installed git pre-commit hook for documentation validation.");

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn init_creates_expected_files() {
        let temp_dir = TempDir::new().unwrap();
        let args = InitArgs {
            working_dir: Some(temp_dir.path().to_path_buf()),
            ..Default::default()
        };
        run(args).unwrap();

        assert!(temp_dir.path().join(".paver.toml").exists());
        assert!(temp_dir.path().join("docs/index.md").exists());
        assert!(temp_dir.path().join("docs/templates/component.md").exists());
        assert!(temp_dir.path().join("docs/templates/runbook.md").exists());
        assert!(temp_dir.path().join("docs/templates/adr.md").exists());
    }

    #[test]
    fn init_with_custom_docs_root() {
        let temp_dir = TempDir::new().unwrap();
        let args = InitArgs {
            docs_root: "custom/path".to_string(),
            working_dir: Some(temp_dir.path().to_path_buf()),
            ..Default::default()
        };
        run(args).unwrap();

        assert!(temp_dir.path().join(".paver.toml").exists());
        assert!(temp_dir.path().join("custom/path/index.md").exists());
        assert!(
            temp_dir
                .path()
                .join("custom/path/templates/component.md")
                .exists()
        );

        // Verify config contains correct path
        let config = fs::read_to_string(temp_dir.path().join(".paver.toml")).unwrap();
        assert!(config.contains("root = \"custom/path\""));
    }

    #[test]
    fn init_fails_if_already_initialized() {
        let temp_dir = TempDir::new().unwrap();

        // First init should succeed
        let args = InitArgs {
            working_dir: Some(temp_dir.path().to_path_buf()),
            ..Default::default()
        };
        run(args).unwrap();

        // Second init should fail
        let args = InitArgs {
            working_dir: Some(temp_dir.path().to_path_buf()),
            ..Default::default()
        };
        let result = run(args);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("already initialized")
        );
    }

    #[test]
    fn init_with_force_overwrites() {
        let temp_dir = TempDir::new().unwrap();

        // First init
        let args = InitArgs {
            working_dir: Some(temp_dir.path().to_path_buf()),
            ..Default::default()
        };
        run(args).unwrap();

        // Modify a file
        fs::write(temp_dir.path().join("docs/index.md"), "modified content").unwrap();

        // Second init with force should succeed and overwrite
        let args = InitArgs {
            force: true,
            working_dir: Some(temp_dir.path().to_path_buf()),
            ..Default::default()
        };
        run(args).unwrap();

        // Verify file was overwritten
        let content = fs::read_to_string(temp_dir.path().join("docs/index.md")).unwrap();
        assert!(content.contains("Project Documentation"));
    }

    #[test]
    fn config_is_valid_toml() {
        let config = default_config("docs");
        let parsed: Result<toml::Value, _> = toml::from_str(&config);
        assert!(parsed.is_ok(), "Generated config should be valid TOML");
    }
}
