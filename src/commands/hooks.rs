//! Implementation of the `pave hooks` command.

use anyhow::{Context, Result, bail};
use std::fs;
use std::path::Path;

use crate::cli::HookType;

/// Marker comment to identify pave-installed hooks.
pub const PAVE_HOOK_MARKER: &str = "# Installed by pave";

/// Generate the hook script content for the given hook type.
///
/// If `run_verify` is true, the hook will also run `pave verify --keep-going`
/// after `pave check` passes.
fn generate_hook_script(hook_type: HookType, run_verify: bool) -> String {
    let hook_name = hook_type.filename();
    let verify_section = if run_verify {
        r#"
    echo ""
    echo "Running PAVED verification..."
    echo "$CHANGED_DOCS" | xargs pave verify --keep-going
    exit $?
"#
    } else {
        ""
    };

    match hook_type {
        HookType::PreCommit => format!(
            r#"#!/bin/sh
{PAVE_HOOK_MARKER}
# PAVED documentation validation hook ({hook_name})

# Get docs root from pave config, default to "docs"
DOCS_ROOT=$(pave config get docs.root 2>/dev/null || echo "docs")

# Get list of changed .md files in docs directory (staged files)
CHANGED_DOCS=$(git diff --cached --name-only --diff-filter=ACM | grep "^$DOCS_ROOT/.*\.md$")

if [ -n "$CHANGED_DOCS" ]; then
    echo "Validating PAVED documentation..."
    echo "$CHANGED_DOCS" | xargs pave check
    CHECK_STATUS=$?
    if [ $CHECK_STATUS -ne 0 ]; then
        exit $CHECK_STATUS
    fi{verify_section}
fi
"#
        ),
        HookType::PrePush => format!(
            r#"#!/bin/sh
{PAVE_HOOK_MARKER}
# PAVED documentation validation hook ({hook_name})

# Get docs root from pave config, default to "docs"
DOCS_ROOT=$(pave config get docs.root 2>/dev/null || echo "docs")

# Pre-push receives: remote_name remote_url on stdin: local_ref local_sha remote_ref remote_sha
# We check docs changed between remote ref and local ref
while read local_ref local_sha remote_ref remote_sha; do
    if [ "$local_sha" = "0000000000000000000000000000000000000000" ]; then
        # Branch is being deleted, nothing to check
        continue
    fi

    if [ "$remote_sha" = "0000000000000000000000000000000000000000" ]; then
        # New branch, check all docs in the branch
        CHANGED_DOCS=$(git diff --name-only --diff-filter=ACM "$local_sha" | grep "^$DOCS_ROOT/.*\.md$")
    else
        # Existing branch, check docs changed since remote
        CHANGED_DOCS=$(git diff --name-only --diff-filter=ACM "$remote_sha".."$local_sha" | grep "^$DOCS_ROOT/.*\.md$")
    fi

    if [ -n "$CHANGED_DOCS" ]; then
        echo "Validating PAVED documentation..."
        echo "$CHANGED_DOCS" | xargs pave check
        CHECK_STATUS=$?
        if [ $CHECK_STATUS -ne 0 ]; then
            exit $CHECK_STATUS
        fi{verify_section}
    fi
done

exit 0
"#
        ),
    }
}

/// Find the git hooks directory starting from the given base path.
/// Supports both regular git repositories and git worktrees.
pub fn find_git_hooks_dir_from(base: &Path) -> Result<std::path::PathBuf> {
    let git_path = base.join(".git");

    // Check if .git is a directory (regular repo)
    if git_path.is_dir() {
        let hooks_dir = git_path.join("hooks");
        // Create hooks directory if it doesn't exist (bare .git may not have it)
        if !hooks_dir.exists() {
            fs::create_dir_all(&hooks_dir).context("Failed to create .git/hooks directory")?;
        }
        return Ok(hooks_dir);
    }

    // Check if .git is a file (worktree) - contains "gitdir: <path>"
    if git_path.is_file() {
        let content = fs::read_to_string(&git_path).context("Failed to read .git file")?;
        if let Some(gitdir) = content.strip_prefix("gitdir: ") {
            let gitdir = gitdir.trim();
            let hooks_dir = std::path::PathBuf::from(gitdir).join("hooks");
            // Create hooks directory if it doesn't exist
            if !hooks_dir.exists() {
                fs::create_dir_all(&hooks_dir)
                    .context("Failed to create hooks directory in worktree")?;
            }
            return Ok(hooks_dir);
        }
    }

    bail!("Not a git repository (no .git directory found)")
}

/// Find the git hooks directory, searching up from the current directory.
/// Supports both regular git repositories and git worktrees.
fn find_git_hooks_dir() -> Result<std::path::PathBuf> {
    let mut current = std::env::current_dir()?;

    loop {
        if let Ok(hooks_dir) = find_git_hooks_dir_from(&current) {
            return Ok(hooks_dir);
        }

        if !current.pop() {
            break;
        }
    }

    bail!("Not a git repository (no .git directory found)")
}

/// Check if a hook file was installed by pave.
fn is_pave_hook(path: &Path) -> bool {
    if let Ok(content) = fs::read_to_string(path) {
        content.contains(PAVE_HOOK_MARKER)
    } else {
        false
    }
}

/// Install a git hook for documentation validation.
///
/// If `run_verify` is true, the hook will also run `pave verify --keep-going`
/// after `pave check` passes.
pub fn install(hook_type: HookType, force: bool, run_verify: bool) -> Result<()> {
    let hooks_dir = find_git_hooks_dir()?;
    install_hook_in_dir(&hooks_dir, hook_type, force, run_verify)
}

/// Install a git hook at a specific base path (for use by init command).
///
/// Options for `init_mode`:
/// - If `true` (init mode): silently skips if pave hook exists, warns for foreign hooks
/// - If `false` (explicit install): follows normal install behavior with messages
///
/// If `run_verify` is true, the hook will also run `pave verify --keep-going`
/// after `pave check` passes.
pub fn install_at(
    base: &Path,
    hook_type: HookType,
    init_mode: bool,
    run_verify: bool,
) -> Result<()> {
    let hooks_dir = find_git_hooks_dir_from(base)?;
    let hook_path = hooks_dir.join(hook_type.filename());

    // Check if hook already exists
    if hook_path.exists() {
        if is_pave_hook(&hook_path) {
            // Already installed by pave, nothing to do
            return Ok(());
        } else {
            // Foreign hook exists
            if init_mode {
                println!("Warning: pre-commit hook already exists, skipping hook installation.");
                println!(
                    "Run 'pave hooks install --force' to overwrite, or add 'pave check' manually."
                );
                return Ok(());
            } else {
                bail!(
                    "Hook '{}' already exists (not installed by pave). Use --force to overwrite.",
                    hook_type.filename()
                );
            }
        }
    }

    let hook_content = generate_hook_script(hook_type, run_verify);
    fs::write(&hook_path, &hook_content)
        .with_context(|| format!("Failed to write {} hook", hook_type.filename()))?;

    // Make the hook executable on Unix
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(&hook_path)?.permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&hook_path, perms)?;
    }

    println!(
        "Installed git {} hook for documentation validation.",
        hook_type.filename()
    );

    Ok(())
}

/// Internal function to install a hook in a specific hooks directory.
fn install_hook_in_dir(
    hooks_dir: &Path,
    hook_type: HookType,
    force: bool,
    run_verify: bool,
) -> Result<()> {
    let hook_path = hooks_dir.join(hook_type.filename());

    // Check if hook already exists
    if hook_path.exists() {
        if is_pave_hook(&hook_path) {
            if !force {
                println!(
                    "Hook '{}' already installed by pave. Use --force to reinstall.",
                    hook_type.filename()
                );
                return Ok(());
            }
        } else if !force {
            bail!(
                "Hook '{}' already exists (not installed by pave). Use --force to overwrite.",
                hook_type.filename()
            );
        }
    }

    let hook_content = generate_hook_script(hook_type, run_verify);
    fs::write(&hook_path, hook_content)
        .with_context(|| format!("Failed to write {} hook", hook_type.filename()))?;

    // Make the hook executable on Unix
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(&hook_path)?.permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&hook_path, perms)?;
    }

    println!(
        "Installed git {} hook for documentation validation.",
        hook_type.filename()
    );

    Ok(())
}

/// Uninstall a git hook.
pub fn uninstall(hook_type: HookType) -> Result<()> {
    let hooks_dir = find_git_hooks_dir()?;
    let hook_path = hooks_dir.join(hook_type.filename());

    if !hook_path.exists() {
        println!("Hook '{}' is not installed.", hook_type.filename());
        return Ok(());
    }

    // Safety check: only remove hooks installed by pave
    if !is_pave_hook(&hook_path) {
        bail!(
            "Hook '{}' was not installed by pave. Remove it manually if needed.",
            hook_type.filename()
        );
    }

    fs::remove_file(&hook_path)
        .with_context(|| format!("Failed to remove {} hook", hook_type.filename()))?;

    println!("Uninstalled git {} hook.", hook_type.filename());

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::sync::Mutex;
    use tempfile::TempDir;

    // Mutex to ensure tests that change working directory don't run in parallel
    static WORKING_DIR_MUTEX: Mutex<()> = Mutex::new(());

    /// Helper to create a fake git repo structure.
    fn setup_git_repo(temp_dir: &TempDir) {
        let git_dir = temp_dir.path().join(".git");
        let hooks_dir = git_dir.join("hooks");
        fs::create_dir_all(&hooks_dir).unwrap();
    }

    /// Helper to run tests in a specific directory.
    /// Uses a mutex to prevent parallel execution of tests that change working dir.
    fn with_working_dir<F, R>(path: &Path, f: F) -> R
    where
        F: FnOnce() -> R,
    {
        let _guard = WORKING_DIR_MUTEX.lock().unwrap();
        let original = std::env::current_dir().unwrap();
        std::env::set_current_dir(path).unwrap();
        let result = f();
        std::env::set_current_dir(original).unwrap();
        result
    }

    #[test]
    fn install_creates_hook() {
        let temp_dir = TempDir::new().unwrap();
        setup_git_repo(&temp_dir);

        with_working_dir(temp_dir.path(), || {
            install(HookType::PreCommit, false, false).unwrap();
        });

        let hook_path = temp_dir.path().join(".git/hooks/pre-commit");
        assert!(hook_path.exists());

        let content = fs::read_to_string(&hook_path).unwrap();
        assert!(content.contains(PAVE_HOOK_MARKER));
        assert!(content.contains("pave check"));
    }

    #[test]
    fn install_pre_push_hook() {
        let temp_dir = TempDir::new().unwrap();
        setup_git_repo(&temp_dir);

        with_working_dir(temp_dir.path(), || {
            install(HookType::PrePush, false, false).unwrap();
        });

        let hook_path = temp_dir.path().join(".git/hooks/pre-push");
        assert!(hook_path.exists());

        let content = fs::read_to_string(&hook_path).unwrap();
        assert!(content.contains(PAVE_HOOK_MARKER));
        assert!(content.contains("pre-push"));
    }

    #[test]
    fn install_fails_without_git_repo() {
        let temp_dir = TempDir::new().unwrap();
        // No .git directory created

        let result = with_working_dir(temp_dir.path(), || {
            install(HookType::PreCommit, false, false)
        });

        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Not a git repository")
        );
    }

    #[test]
    fn install_warns_if_pave_hook_exists() {
        let temp_dir = TempDir::new().unwrap();
        setup_git_repo(&temp_dir);

        // Install once
        with_working_dir(temp_dir.path(), || {
            install(HookType::PreCommit, false, false).unwrap();
        });

        // Install again - should succeed with warning (not error)
        let result = with_working_dir(temp_dir.path(), || {
            install(HookType::PreCommit, false, false)
        });
        assert!(result.is_ok());
    }

    #[test]
    fn install_fails_if_foreign_hook_exists() {
        let temp_dir = TempDir::new().unwrap();
        setup_git_repo(&temp_dir);

        // Create a non-pave hook
        let hook_path = temp_dir.path().join(".git/hooks/pre-commit");
        fs::write(&hook_path, "#!/bin/sh\necho 'custom hook'").unwrap();

        let result = with_working_dir(temp_dir.path(), || {
            install(HookType::PreCommit, false, false)
        });

        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("not installed by pave")
        );
    }

    #[test]
    fn install_force_overwrites_foreign_hook() {
        let temp_dir = TempDir::new().unwrap();
        setup_git_repo(&temp_dir);

        // Create a non-pave hook
        let hook_path = temp_dir.path().join(".git/hooks/pre-commit");
        fs::write(&hook_path, "#!/bin/sh\necho 'custom hook'").unwrap();

        with_working_dir(temp_dir.path(), || {
            install(HookType::PreCommit, true, false).unwrap();
        });

        let content = fs::read_to_string(&hook_path).unwrap();
        assert!(content.contains(PAVE_HOOK_MARKER));
    }

    #[test]
    fn uninstall_removes_hook() {
        let temp_dir = TempDir::new().unwrap();
        setup_git_repo(&temp_dir);

        // Install first
        with_working_dir(temp_dir.path(), || {
            install(HookType::PreCommit, false, false).unwrap();
        });

        let hook_path = temp_dir.path().join(".git/hooks/pre-commit");
        assert!(hook_path.exists());

        // Uninstall
        with_working_dir(temp_dir.path(), || {
            uninstall(HookType::PreCommit).unwrap();
        });

        assert!(!hook_path.exists());
    }

    #[test]
    fn uninstall_does_nothing_if_not_installed() {
        let temp_dir = TempDir::new().unwrap();
        setup_git_repo(&temp_dir);

        let result = with_working_dir(temp_dir.path(), || uninstall(HookType::PreCommit));
        assert!(result.is_ok());
    }

    #[test]
    fn uninstall_fails_for_foreign_hook() {
        let temp_dir = TempDir::new().unwrap();
        setup_git_repo(&temp_dir);

        // Create a non-pave hook
        let hook_path = temp_dir.path().join(".git/hooks/pre-commit");
        fs::write(&hook_path, "#!/bin/sh\necho 'custom hook'").unwrap();

        let result = with_working_dir(temp_dir.path(), || uninstall(HookType::PreCommit));

        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("not installed by pave")
        );
    }

    #[cfg(unix)]
    #[test]
    fn install_makes_hook_executable() {
        use std::os::unix::fs::PermissionsExt;

        let temp_dir = TempDir::new().unwrap();
        setup_git_repo(&temp_dir);

        with_working_dir(temp_dir.path(), || {
            install(HookType::PreCommit, false, false).unwrap();
        });

        let hook_path = temp_dir.path().join(".git/hooks/pre-commit");
        let perms = fs::metadata(&hook_path).unwrap().permissions();
        assert_eq!(perms.mode() & 0o755, 0o755);
    }

    #[test]
    fn generated_pre_commit_hook_uses_cached_diff() {
        let script = generate_hook_script(HookType::PreCommit, false);
        assert!(script.contains("DOCS_ROOT=$(pave config get docs.root"));
        assert!(script.contains("git diff --cached"));
        assert!(script.contains("grep \"^$DOCS_ROOT/.*\\.md$\""));
    }

    #[test]
    fn generated_pre_push_hook_uses_ref_diff() {
        let script = generate_hook_script(HookType::PrePush, false);
        assert!(script.contains("DOCS_ROOT=$(pave config get docs.root"));
        assert!(script.contains("while read local_ref local_sha"));
        assert!(script.contains("$remote_sha\"..\"$local_sha"));
        assert!(script.contains("grep \"^$DOCS_ROOT/.*\\.md$\""));
    }

    /// Helper to create a fake git worktree structure.
    fn setup_git_worktree(temp_dir: &TempDir) -> TempDir {
        // Create the "main" git repo directory
        let main_repo = TempDir::new().unwrap();
        let worktree_git_dir = main_repo.path().join(".git/worktrees/test-worktree");
        fs::create_dir_all(&worktree_git_dir).unwrap();

        // Create .git file in the worktree pointing to the gitdir
        let git_file = temp_dir.path().join(".git");
        fs::write(&git_file, format!("gitdir: {}", worktree_git_dir.display())).unwrap();

        main_repo
    }

    #[test]
    fn install_works_in_worktree() {
        let temp_dir = TempDir::new().unwrap();
        let main_repo = setup_git_worktree(&temp_dir);

        with_working_dir(temp_dir.path(), || {
            install(HookType::PreCommit, false, false).unwrap();
        });

        // Hook should be in the worktree's git dir, not the main .git
        let hook_path = main_repo
            .path()
            .join(".git/worktrees/test-worktree/hooks/pre-commit");
        assert!(hook_path.exists());

        let content = fs::read_to_string(&hook_path).unwrap();
        assert!(content.contains(PAVE_HOOK_MARKER));
    }

    #[test]
    fn uninstall_works_in_worktree() {
        let temp_dir = TempDir::new().unwrap();
        let main_repo = setup_git_worktree(&temp_dir);

        // Install first
        with_working_dir(temp_dir.path(), || {
            install(HookType::PreCommit, false, false).unwrap();
        });

        let hook_path = main_repo
            .path()
            .join(".git/worktrees/test-worktree/hooks/pre-commit");
        assert!(hook_path.exists());

        // Uninstall
        with_working_dir(temp_dir.path(), || {
            uninstall(HookType::PreCommit).unwrap();
        });

        assert!(!hook_path.exists());
    }

    #[test]
    fn generated_hook_without_verify_omits_verify() {
        let script = generate_hook_script(HookType::PreCommit, false);
        assert!(script.contains("pave check"));
        assert!(!script.contains("pave verify"));
    }

    #[test]
    fn generated_hook_with_verify_includes_verify() {
        let script = generate_hook_script(HookType::PreCommit, true);
        assert!(script.contains("pave check"));
        assert!(script.contains("pave verify --keep-going"));
        assert!(script.contains("Running PAVED verification"));
    }

    #[test]
    fn generated_pre_push_hook_with_verify() {
        let script = generate_hook_script(HookType::PrePush, true);
        assert!(script.contains("pave check"));
        assert!(script.contains("pave verify --keep-going"));
    }

    #[test]
    fn install_with_verify_includes_verify_in_hook() {
        let temp_dir = TempDir::new().unwrap();
        setup_git_repo(&temp_dir);

        with_working_dir(temp_dir.path(), || {
            install(HookType::PreCommit, false, true).unwrap();
        });

        let hook_path = temp_dir.path().join(".git/hooks/pre-commit");
        let content = fs::read_to_string(&hook_path).unwrap();
        assert!(content.contains("pave check"));
        assert!(content.contains("pave verify --keep-going"));
    }
}
