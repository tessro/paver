//! Implementation of the `paver init` command.

use anyhow::{Context, Result, bail};
use std::fs;
use std::path::Path;

use crate::cli::HookType;
use crate::commands::hooks;
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
    /// Skip installing git pre-commit hook
    pub skip_hooks: bool,
    /// Overwrite existing files
    pub force: bool,
    /// Working directory (for testing; uses current dir if None)
    pub working_dir: Option<std::path::PathBuf>,
}

impl Default for InitArgs {
    fn default() -> Self {
        Self {
            docs_root: "docs".to_string(),
            skip_hooks: false,
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

    // Install git pre-commit hook by default (unless skipped)
    if !args.skip_hooks {
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
    // Use the shared hook installation from the hooks module
    // init_mode=true means: silently skip if paver hook exists, warn for foreign hooks
    // run_verify=false by default; users can enable via config or reinstall with --verify
    hooks::install_at(base, HookType::PreCommit, true, false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::hooks::PAVER_HOOK_MARKER;
    use std::fs;
    use tempfile::TempDir;

    /// Helper to create a fake git repo structure for testing.
    fn setup_git_repo(temp_dir: &TempDir) {
        let git_dir = temp_dir.path().join(".git");
        let hooks_dir = git_dir.join("hooks");
        fs::create_dir_all(&hooks_dir).unwrap();
    }

    #[test]
    fn init_creates_expected_files() {
        let temp_dir = TempDir::new().unwrap();
        let args = InitArgs {
            skip_hooks: true,
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
            skip_hooks: true,
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
            skip_hooks: true,
            working_dir: Some(temp_dir.path().to_path_buf()),
            ..Default::default()
        };
        run(args).unwrap();

        // Second init should fail
        let args = InitArgs {
            skip_hooks: true,
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
            skip_hooks: true,
            working_dir: Some(temp_dir.path().to_path_buf()),
            ..Default::default()
        };
        run(args).unwrap();

        // Modify a file
        fs::write(temp_dir.path().join("docs/index.md"), "modified content").unwrap();

        // Second init with force should succeed and overwrite
        let args = InitArgs {
            force: true,
            skip_hooks: true,
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

    #[test]
    fn init_installs_hook_by_default_in_git_repo() {
        let temp_dir = TempDir::new().unwrap();
        setup_git_repo(&temp_dir);

        let args = InitArgs {
            working_dir: Some(temp_dir.path().to_path_buf()),
            ..Default::default()
        };
        run(args).unwrap();

        // Verify hook was installed
        let hook_path = temp_dir.path().join(".git/hooks/pre-commit");
        assert!(hook_path.exists());

        let content = fs::read_to_string(&hook_path).unwrap();
        assert!(content.contains(PAVER_HOOK_MARKER));
        assert!(content.contains("paver check"));
    }

    #[test]
    fn init_skip_hooks_does_not_install_hook() {
        let temp_dir = TempDir::new().unwrap();
        setup_git_repo(&temp_dir);

        let args = InitArgs {
            skip_hooks: true,
            working_dir: Some(temp_dir.path().to_path_buf()),
            ..Default::default()
        };
        run(args).unwrap();

        // Verify hook was NOT installed
        let hook_path = temp_dir.path().join(".git/hooks/pre-commit");
        assert!(!hook_path.exists());
    }

    #[test]
    fn init_fails_without_git_repo_when_hooks_enabled() {
        let temp_dir = TempDir::new().unwrap();
        // No git repo setup

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
                .contains("Not a git repository")
        );
    }

    #[cfg(unix)]
    #[test]
    fn init_makes_hook_executable() {
        use std::os::unix::fs::PermissionsExt;

        let temp_dir = TempDir::new().unwrap();
        setup_git_repo(&temp_dir);

        let args = InitArgs {
            working_dir: Some(temp_dir.path().to_path_buf()),
            ..Default::default()
        };
        run(args).unwrap();

        let hook_path = temp_dir.path().join(".git/hooks/pre-commit");
        let perms = fs::metadata(&hook_path).unwrap().permissions();
        assert_eq!(perms.mode() & 0o755, 0o755);
    }

    #[test]
    fn init_does_not_overwrite_existing_foreign_hook() {
        let temp_dir = TempDir::new().unwrap();
        setup_git_repo(&temp_dir);

        // Create a foreign hook
        let hook_path = temp_dir.path().join(".git/hooks/pre-commit");
        fs::write(&hook_path, "#!/bin/sh\necho 'custom hook'").unwrap();

        let args = InitArgs {
            working_dir: Some(temp_dir.path().to_path_buf()),
            ..Default::default()
        };
        run(args).unwrap();

        // Verify hook was NOT overwritten
        let content = fs::read_to_string(&hook_path).unwrap();
        assert!(content.contains("custom hook"));
        assert!(!content.contains(PAVER_HOOK_MARKER));
    }

    #[test]
    fn init_does_not_reinstall_paver_hook() {
        let temp_dir = TempDir::new().unwrap();
        setup_git_repo(&temp_dir);

        // First init
        let args = InitArgs {
            force: true, // Use force to allow re-init
            working_dir: Some(temp_dir.path().to_path_buf()),
            ..Default::default()
        };
        run(args).unwrap();

        let hook_path = temp_dir.path().join(".git/hooks/pre-commit");
        let first_content = fs::read_to_string(&hook_path).unwrap();

        // Second init should succeed without error (paver hook already installed)
        let args = InitArgs {
            force: true,
            working_dir: Some(temp_dir.path().to_path_buf()),
            ..Default::default()
        };
        run(args).unwrap();

        // Hook should still exist with same content
        let second_content = fs::read_to_string(&hook_path).unwrap();
        assert_eq!(first_content, second_content);
    }
}
