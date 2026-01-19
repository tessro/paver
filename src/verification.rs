//! Verification module for executing and validating commands from documentation.
//!
//! This module provides functionality to:
//! - Extract verification specifications from parsed markdown documents
//! - Execute verification commands with timeout and output capture
//! - Report results including pass/fail status, timing, and error details

use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use crate::parser::{CodeBlock, ExpectMatchStrategy, ParsedDoc};

/// Default timeout for command execution in seconds.
pub const DEFAULT_TIMEOUT_SECS: u32 = 30;

/// Specifies how to match command output.
#[derive(Debug, Clone, PartialEq)]
pub enum OutputMatcher {
    /// Match if stdout contains the given substring.
    Contains(String),
    /// Match if stdout matches the given regex pattern.
    Regex(String),
    /// Match if stdout matches exactly (after trimming whitespace).
    Exact(String),
    /// Only check the exit code, ignore output.
    ExitCodeOnly,
}

/// A single verification item representing a command to execute.
#[derive(Debug, Clone, PartialEq)]
pub struct VerificationItem {
    /// The shell command to run.
    pub command: String,
    /// Optional working directory for command execution.
    pub working_dir: Option<PathBuf>,
    /// Expected exit code (default: 0).
    pub expected_exit_code: Option<i32>,
    /// How to validate command output.
    pub expected_output: Option<OutputMatcher>,
    /// Timeout in seconds (default: 30).
    pub timeout_secs: Option<u32>,
    /// Environment variables to set for this command.
    pub env_vars: Vec<(String, String)>,
}

impl Default for VerificationItem {
    fn default() -> Self {
        Self {
            command: String::new(),
            working_dir: None,
            expected_exit_code: Some(0),
            expected_output: None,
            timeout_secs: Some(DEFAULT_TIMEOUT_SECS),
            env_vars: Vec::new(),
        }
    }
}

/// A verification specification extracted from a document.
#[derive(Debug, Clone, PartialEq)]
pub struct VerificationSpec {
    /// Path to the source markdown file.
    pub source_file: PathBuf,
    /// Line number where the verification section starts.
    pub section_line: usize,
    /// List of verification items to execute.
    pub items: Vec<VerificationItem>,
}

/// Result of executing a single verification item.
#[derive(Debug)]
pub struct VerificationResult {
    /// The verification item that was executed.
    pub item: VerificationItem,
    /// Whether the verification passed.
    pub passed: bool,
    /// Exit code returned by the command (None if command didn't complete).
    pub exit_code: Option<i32>,
    /// Captured stdout from the command.
    pub stdout: String,
    /// Captured stderr from the command.
    pub stderr: String,
    /// Execution duration in milliseconds.
    pub duration_ms: u64,
    /// Error message if execution failed (e.g., "timeout", "command not found").
    pub error: Option<String>,
}

/// Extract a verification specification from a parsed document.
///
/// Looks for a "Verification" section and extracts executable code blocks
/// as verification items.
///
/// # Arguments
/// * `doc` - The parsed markdown document
///
/// # Returns
/// `Some(VerificationSpec)` if a Verification section with commands exists,
/// `None` otherwise.
pub fn extract_verification_spec(doc: &ParsedDoc) -> Option<VerificationSpec> {
    let section = doc.get_section("Verification")?;

    let executable_blocks: Vec<&CodeBlock> = section.executable_commands();

    if executable_blocks.is_empty() {
        return None;
    }

    // Get default working_dir from frontmatter
    let default_working_dir = doc
        .frontmatter
        .as_ref()
        .and_then(|fm| fm.working_dir.as_ref())
        .map(PathBuf::from);

    let items: Vec<VerificationItem> = executable_blocks
        .into_iter()
        .map(|block| {
            let command = extract_command_from_block(&block.content);
            let expected_output = convert_expected_output(block);
            // Per-block working_dir overrides frontmatter default
            let working_dir = block
                .working_dir
                .as_ref()
                .map(PathBuf::from)
                .or_else(|| default_working_dir.clone());
            VerificationItem {
                command,
                working_dir,
                expected_exit_code: Some(0),
                expected_output,
                timeout_secs: Some(DEFAULT_TIMEOUT_SECS),
                env_vars: block.env_vars.clone(),
            }
        })
        .collect();

    Some(VerificationSpec {
        source_file: doc.path.clone(),
        section_line: section.start_line,
        items,
    })
}

/// Convert parsed expected output to an OutputMatcher.
fn convert_expected_output(block: &CodeBlock) -> Option<OutputMatcher> {
    let expected = block.expected_output.as_ref()?;

    let matcher = match expected.strategy {
        ExpectMatchStrategy::Contains => OutputMatcher::Contains(expected.content.clone()),
        ExpectMatchStrategy::Regex => OutputMatcher::Regex(expected.content.clone()),
        ExpectMatchStrategy::Exact => OutputMatcher::Exact(expected.content.clone()),
    };

    Some(matcher)
}

/// Extract the command string from a code block's content.
///
/// Handles various formats:
/// - Lines starting with `$ ` (shell prompt) - strips the prompt
/// - Lines starting with `> ` (REPL prompt) - strips the prompt
/// - Plain commands without prompts
/// - Skips empty lines and comment lines (starting with #)
fn extract_command_from_block(content: &str) -> String {
    let mut commands = Vec::new();

    for line in content.lines() {
        let trimmed = line.trim();

        // Skip empty lines and comment-only lines
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        // Strip shell prompt prefixes
        let cmd = if let Some(rest) = trimmed.strip_prefix("$ ") {
            rest.to_string()
        } else if let Some(rest) = trimmed.strip_prefix("> ") {
            rest.to_string()
        } else {
            trimmed.to_string()
        };

        if !cmd.is_empty() {
            commands.push(cmd);
        }
    }

    commands.join(" && ")
}

/// Execute all verification items in a specification.
///
/// Runs each command and collects results including:
/// - Pass/fail status based on exit code comparison
/// - Captured stdout and stderr
/// - Execution timing
/// - Error details for failures
///
/// # Arguments
/// * `spec` - The verification specification to execute
///
/// # Returns
/// A vector of `VerificationResult` for each item in the spec.
pub fn run_verification(spec: &VerificationSpec) -> Vec<VerificationResult> {
    spec.items.iter().map(run_single_verification).collect()
}

/// Execute a single verification item.
fn run_single_verification(item: &VerificationItem) -> VerificationResult {
    let timeout = Duration::from_secs(item.timeout_secs.unwrap_or(DEFAULT_TIMEOUT_SECS) as u64);
    let start = Instant::now();

    // Clone item for the result
    let item_clone = item.clone();

    // Spawn the command
    let mut cmd = Command::new("sh");
    cmd.arg("-c").arg(&item.command);

    if let Some(ref working_dir) = item.working_dir {
        cmd.current_dir(working_dir);
    }

    // Set environment variables
    for (key, value) in &item.env_vars {
        cmd.env(key, value);
    }

    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());

    let child = match cmd.spawn() {
        Ok(child) => child,
        Err(e) => {
            return VerificationResult {
                item: item_clone,
                passed: false,
                exit_code: None,
                stdout: String::new(),
                stderr: String::new(),
                duration_ms: start.elapsed().as_millis() as u64,
                error: Some(format!("failed to spawn command: {}", e)),
            };
        }
    };

    // Use a channel to receive the result with timeout
    let (tx, rx) = mpsc::channel();

    let handle = thread::spawn(move || {
        let output = child.wait_with_output();
        let _ = tx.send(output);
    });

    match rx.recv_timeout(timeout) {
        Ok(Ok(output)) => {
            let _ = handle.join();
            let duration_ms = start.elapsed().as_millis() as u64;
            let exit_code = output.status.code();
            let stdout = String::from_utf8_lossy(&output.stdout).to_string();
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();

            let expected_code = item.expected_exit_code.unwrap_or(0);
            let code_matches = exit_code == Some(expected_code);

            let output_matches = match &item.expected_output {
                None => true,
                Some(OutputMatcher::ExitCodeOnly) => true,
                Some(OutputMatcher::Contains(substring)) => stdout.contains(substring),
                Some(OutputMatcher::Regex(pattern)) => regex::Regex::new(pattern)
                    .map(|re| re.is_match(&stdout))
                    .unwrap_or(false),
                Some(OutputMatcher::Exact(expected)) => stdout.trim() == expected.trim(),
            };

            let passed = code_matches && output_matches;

            VerificationResult {
                item: item_clone,
                passed,
                exit_code,
                stdout,
                stderr,
                duration_ms,
                error: None,
            }
        }
        Ok(Err(e)) => {
            let _ = handle.join();
            VerificationResult {
                item: item_clone,
                passed: false,
                exit_code: None,
                stdout: String::new(),
                stderr: String::new(),
                duration_ms: start.elapsed().as_millis() as u64,
                error: Some(format!("command execution failed: {}", e)),
            }
        }
        Err(mpsc::RecvTimeoutError::Timeout) => {
            // Command timed out - we can't easily kill the process from here,
            // but we report the timeout
            VerificationResult {
                item: item_clone,
                passed: false,
                exit_code: None,
                stdout: String::new(),
                stderr: String::new(),
                duration_ms: start.elapsed().as_millis() as u64,
                error: Some("timeout".to_string()),
            }
        }
        Err(mpsc::RecvTimeoutError::Disconnected) => VerificationResult {
            item: item_clone,
            passed: false,
            exit_code: None,
            stdout: String::new(),
            stderr: String::new(),
            duration_ms: start.elapsed().as_millis() as u64,
            error: Some("command thread disconnected unexpectedly".to_string()),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_successful_command_returns_passed_true() {
        let item = VerificationItem {
            command: "echo hello".to_string(),
            working_dir: None,
            expected_exit_code: Some(0),
            expected_output: None,
            timeout_secs: Some(5),
            env_vars: Vec::new(),
        };

        let result = run_single_verification(&item);

        assert!(result.passed);
        assert_eq!(result.exit_code, Some(0));
        assert!(result.stdout.contains("hello"));
        assert!(result.error.is_none());
    }

    #[test]
    fn test_failed_command_returns_passed_false() {
        let item = VerificationItem {
            command: "exit 1".to_string(),
            working_dir: None,
            expected_exit_code: Some(0),
            expected_output: None,
            timeout_secs: Some(5),
            env_vars: Vec::new(),
        };

        let result = run_single_verification(&item);

        assert!(!result.passed);
        assert_eq!(result.exit_code, Some(1));
    }

    #[test]
    fn test_timeout_handling() {
        let item = VerificationItem {
            command: "sleep 10".to_string(),
            working_dir: None,
            expected_exit_code: Some(0),
            expected_output: None,
            timeout_secs: Some(1),
            env_vars: Vec::new(),
        };

        let result = run_single_verification(&item);

        assert!(!result.passed);
        assert_eq!(result.error, Some("timeout".to_string()));
    }

    #[test]
    fn test_output_capture_stdout() {
        let item = VerificationItem {
            command: "echo 'test output'".to_string(),
            working_dir: None,
            expected_exit_code: Some(0),
            expected_output: None,
            timeout_secs: Some(5),
            env_vars: Vec::new(),
        };

        let result = run_single_verification(&item);

        assert!(result.passed);
        assert!(result.stdout.contains("test output"));
    }

    #[test]
    fn test_output_capture_stderr() {
        let item = VerificationItem {
            command: "echo 'error message' >&2".to_string(),
            working_dir: None,
            expected_exit_code: Some(0),
            expected_output: None,
            timeout_secs: Some(5),
            env_vars: Vec::new(),
        };

        let result = run_single_verification(&item);

        assert!(result.passed);
        assert!(result.stderr.contains("error message"));
    }

    #[test]
    fn test_command_not_found() {
        let item = VerificationItem {
            command: "nonexistent_command_12345".to_string(),
            working_dir: None,
            expected_exit_code: Some(0),
            expected_output: None,
            timeout_secs: Some(5),
            env_vars: Vec::new(),
        };

        let result = run_single_verification(&item);

        assert!(!result.passed);
        // Command not found typically returns 127
        assert!(result.exit_code == Some(127) || result.error.is_some());
    }

    #[test]
    fn test_expected_exit_code_matching() {
        let item = VerificationItem {
            command: "exit 42".to_string(),
            working_dir: None,
            expected_exit_code: Some(42),
            expected_output: None,
            timeout_secs: Some(5),
            env_vars: Vec::new(),
        };

        let result = run_single_verification(&item);

        assert!(result.passed);
        assert_eq!(result.exit_code, Some(42));
    }

    #[test]
    fn test_output_contains_matcher() {
        let item = VerificationItem {
            command: "echo 'hello world'".to_string(),
            working_dir: None,
            expected_exit_code: Some(0),
            expected_output: Some(OutputMatcher::Contains("world".to_string())),
            timeout_secs: Some(5),
            env_vars: Vec::new(),
        };

        let result = run_single_verification(&item);

        assert!(result.passed);
    }

    #[test]
    fn test_output_contains_matcher_fails() {
        let item = VerificationItem {
            command: "echo 'hello world'".to_string(),
            working_dir: None,
            expected_exit_code: Some(0),
            expected_output: Some(OutputMatcher::Contains("foo".to_string())),
            timeout_secs: Some(5),
            env_vars: Vec::new(),
        };

        let result = run_single_verification(&item);

        assert!(!result.passed);
    }

    #[test]
    fn test_duration_is_recorded() {
        let item = VerificationItem {
            command: "sleep 0.1".to_string(),
            working_dir: None,
            expected_exit_code: Some(0),
            expected_output: None,
            timeout_secs: Some(5),
            env_vars: Vec::new(),
        };

        let result = run_single_verification(&item);

        assert!(result.duration_ms >= 100);
    }

    #[test]
    fn test_extract_command_from_block_with_dollar_prefix() {
        let content = "$ echo hello\n$ echo world";
        let cmd = extract_command_from_block(content);
        assert_eq!(cmd, "echo hello && echo world");
    }

    #[test]
    fn test_extract_command_from_block_without_prefix() {
        let content = "echo hello";
        let cmd = extract_command_from_block(content);
        assert_eq!(cmd, "echo hello");
    }

    #[test]
    fn test_extract_command_from_block_mixed() {
        let content = "$ cargo build\ncargo test";
        let cmd = extract_command_from_block(content);
        assert_eq!(cmd, "cargo build && cargo test");
    }

    #[test]
    fn test_extract_command_from_block_skips_comments() {
        let content = "# This is a comment\necho hello\n# Another comment\necho world";
        let cmd = extract_command_from_block(content);
        assert_eq!(cmd, "echo hello && echo world");
    }

    #[test]
    fn test_extract_command_from_block_skips_empty_lines() {
        let content = "echo hello\n\n\necho world";
        let cmd = extract_command_from_block(content);
        assert_eq!(cmd, "echo hello && echo world");
    }

    #[test]
    fn test_extract_verification_spec_from_doc() {
        let content = r#"# Test Doc

## Verification
Run the test:
```bash
echo "test"
```
"#;

        let doc = ParsedDoc::parse_content(PathBuf::from("test.md"), content).unwrap();
        let spec = extract_verification_spec(&doc);

        assert!(spec.is_some());
        let spec = spec.unwrap();
        assert_eq!(spec.source_file, PathBuf::from("test.md"));
        assert_eq!(spec.items.len(), 1);
        assert_eq!(spec.items[0].command, "echo \"test\"");
    }

    #[test]
    fn test_extract_verification_spec_no_verification_section() {
        let content = r#"# Test Doc

## Purpose
Just a purpose section.
"#;

        let doc = ParsedDoc::parse_content(PathBuf::from("test.md"), content).unwrap();
        let spec = extract_verification_spec(&doc);

        assert!(spec.is_none());
    }

    #[test]
    fn test_extract_verification_spec_empty_verification_section() {
        let content = r#"# Test Doc

## Verification
No code blocks here, just text.
"#;

        let doc = ParsedDoc::parse_content(PathBuf::from("test.md"), content).unwrap();
        let spec = extract_verification_spec(&doc);

        assert!(spec.is_none());
    }

    #[test]
    fn test_extract_verification_spec_multiple_commands() {
        let content = r#"# Test Doc

## Verification
```bash
echo "first"
```
```sh
echo "second"
```
"#;

        let doc = ParsedDoc::parse_content(PathBuf::from("test.md"), content).unwrap();
        let spec = extract_verification_spec(&doc);

        assert!(spec.is_some());
        let spec = spec.unwrap();
        assert_eq!(spec.items.len(), 2);
    }

    #[test]
    fn test_run_verification_executes_all_items() {
        let spec = VerificationSpec {
            source_file: PathBuf::from("test.md"),
            section_line: 1,
            items: vec![
                VerificationItem {
                    command: "echo 'first'".to_string(),
                    working_dir: None,
                    expected_exit_code: Some(0),
                    expected_output: None,
                    timeout_secs: Some(5),
                    env_vars: Vec::new(),
                },
                VerificationItem {
                    command: "echo 'second'".to_string(),
                    working_dir: None,
                    expected_exit_code: Some(0),
                    expected_output: None,
                    timeout_secs: Some(5),
                    env_vars: Vec::new(),
                },
            ],
        };

        let results = run_verification(&spec);

        assert_eq!(results.len(), 2);
        assert!(results[0].passed);
        assert!(results[1].passed);
        assert!(results[0].stdout.contains("first"));
        assert!(results[1].stdout.contains("second"));
    }

    #[test]
    fn test_integration_actual_echo_command() {
        let item = VerificationItem {
            command: "echo 'Hello, World!'".to_string(),
            working_dir: None,
            expected_exit_code: Some(0),
            expected_output: Some(OutputMatcher::Contains("Hello, World!".to_string())),
            timeout_secs: Some(5),
            env_vars: Vec::new(),
        };

        let result = run_single_verification(&item);

        assert!(result.passed);
        assert_eq!(result.exit_code, Some(0));
        assert!(result.stdout.contains("Hello, World!"));
        assert!(result.error.is_none());
        // Note: duration_ms may be 0 on very fast systems where echo completes in under 1ms
    }

    #[test]
    fn test_output_regex_matcher() {
        let item = VerificationItem {
            command: "echo 'test 123 passed'".to_string(),
            working_dir: None,
            expected_exit_code: Some(0),
            expected_output: Some(OutputMatcher::Regex(r"test \d+ passed".to_string())),
            timeout_secs: Some(5),
            env_vars: Vec::new(),
        };

        let result = run_single_verification(&item);

        assert!(result.passed);
    }

    #[test]
    fn test_output_regex_matcher_fails() {
        let item = VerificationItem {
            command: "echo 'test abc passed'".to_string(),
            working_dir: None,
            expected_exit_code: Some(0),
            expected_output: Some(OutputMatcher::Regex(r"test \d+ passed".to_string())),
            timeout_secs: Some(5),
            env_vars: Vec::new(),
        };

        let result = run_single_verification(&item);

        assert!(!result.passed);
    }

    #[test]
    fn test_output_exact_matcher() {
        let item = VerificationItem {
            command: "echo 'hello'".to_string(),
            working_dir: None,
            expected_exit_code: Some(0),
            expected_output: Some(OutputMatcher::Exact("hello".to_string())),
            timeout_secs: Some(5),
            env_vars: Vec::new(),
        };

        let result = run_single_verification(&item);

        assert!(result.passed);
    }

    #[test]
    fn test_output_exact_matcher_fails() {
        let item = VerificationItem {
            command: "echo 'hello world'".to_string(),
            working_dir: None,
            expected_exit_code: Some(0),
            expected_output: Some(OutputMatcher::Exact("hello".to_string())),
            timeout_secs: Some(5),
            env_vars: Vec::new(),
        };

        let result = run_single_verification(&item);

        assert!(!result.passed);
    }

    #[test]
    fn test_extract_verification_spec_with_inline_output() {
        let content = r#"# Test Doc

## Verification
```bash
$ echo hello
hello
```
"#;

        let doc = ParsedDoc::parse_content(PathBuf::from("test.md"), content).unwrap();
        let spec = extract_verification_spec(&doc);

        assert!(spec.is_some());
        let spec = spec.unwrap();
        assert_eq!(spec.items.len(), 1);

        let item = &spec.items[0];
        assert_eq!(item.command, "echo hello");
        assert!(item.expected_output.is_some());
        match &item.expected_output {
            Some(OutputMatcher::Contains(s)) => assert!(s.contains("hello")),
            _ => panic!("Expected Contains matcher"),
        }
    }

    #[test]
    fn test_extract_verification_spec_with_explicit_output_block() {
        let content = r#"# Test Doc

## Verification
```bash
cargo test
```
<!-- pave:expect:regex -->
```
test result: ok\. \d+ passed
```
"#;

        let doc = ParsedDoc::parse_content(PathBuf::from("test.md"), content).unwrap();
        let spec = extract_verification_spec(&doc);

        assert!(spec.is_some());
        let spec = spec.unwrap();
        assert_eq!(spec.items.len(), 1);

        let item = &spec.items[0];
        assert!(item.expected_output.is_some());
        match &item.expected_output {
            Some(OutputMatcher::Regex(s)) => assert!(s.contains(r"\d+")),
            _ => panic!("Expected Regex matcher"),
        }
    }

    #[test]
    fn test_extract_verification_spec_with_working_dir_from_frontmatter() {
        let content = r#"---
pave:
  working_dir: packages/api
---
# API Tests

## Verification
```bash
npm test
```
"#;

        let doc = ParsedDoc::parse_content(PathBuf::from("test.md"), content).unwrap();
        let spec = extract_verification_spec(&doc);

        assert!(spec.is_some());
        let spec = spec.unwrap();
        assert_eq!(spec.items.len(), 1);

        let item = &spec.items[0];
        assert_eq!(item.working_dir, Some(PathBuf::from("packages/api")));
    }

    #[test]
    fn test_extract_verification_spec_with_inline_working_dir() {
        let content = r#"# API Tests

## Verification
<!-- pave:working_dir src/tests -->
```bash
cargo test
```
"#;

        let doc = ParsedDoc::parse_content(PathBuf::from("test.md"), content).unwrap();
        let spec = extract_verification_spec(&doc);

        assert!(spec.is_some());
        let spec = spec.unwrap();
        assert_eq!(spec.items.len(), 1);

        let item = &spec.items[0];
        assert_eq!(item.working_dir, Some(PathBuf::from("src/tests")));
    }

    #[test]
    fn test_extract_verification_spec_inline_overrides_frontmatter() {
        let content = r#"---
pave:
  working_dir: default/path
---
# API Tests

## Verification
<!-- pave:working_dir override/path -->
```bash
npm test
```
"#;

        let doc = ParsedDoc::parse_content(PathBuf::from("test.md"), content).unwrap();
        let spec = extract_verification_spec(&doc);

        assert!(spec.is_some());
        let spec = spec.unwrap();
        assert_eq!(spec.items.len(), 1);

        // Inline working_dir should override frontmatter
        let item = &spec.items[0];
        assert_eq!(item.working_dir, Some(PathBuf::from("override/path")));
    }

    #[test]
    fn test_extract_verification_spec_with_env_vars() {
        let content = r#"# API Tests

## Verification
<!-- pave:env TEST_DB=sqlite -->
<!-- pave:env DEBUG=true -->
```bash
cargo test
```
"#;

        let doc = ParsedDoc::parse_content(PathBuf::from("test.md"), content).unwrap();
        let spec = extract_verification_spec(&doc);

        assert!(spec.is_some());
        let spec = spec.unwrap();
        assert_eq!(spec.items.len(), 1);

        let item = &spec.items[0];
        assert_eq!(item.env_vars.len(), 2);
        assert!(
            item.env_vars
                .contains(&("TEST_DB".to_string(), "sqlite".to_string()))
        );
        assert!(
            item.env_vars
                .contains(&("DEBUG".to_string(), "true".to_string()))
        );
    }

    #[test]
    fn test_run_verification_with_env_vars() {
        let item = VerificationItem {
            command: "echo $MY_VAR".to_string(),
            working_dir: None,
            expected_exit_code: Some(0),
            expected_output: Some(OutputMatcher::Contains("hello_from_env".to_string())),
            timeout_secs: Some(5),
            env_vars: vec![("MY_VAR".to_string(), "hello_from_env".to_string())],
        };

        let result = run_single_verification(&item);

        assert!(result.passed);
        assert!(result.stdout.contains("hello_from_env"));
    }

    #[test]
    fn test_frontmatter_working_dir_applies_to_all_blocks() {
        let content = r#"---
pave:
  working_dir: packages/shared
---
# Shared Tests

## Verification
```bash
echo first
```
```bash
echo second
```
"#;

        let doc = ParsedDoc::parse_content(PathBuf::from("test.md"), content).unwrap();
        let spec = extract_verification_spec(&doc);

        assert!(spec.is_some());
        let spec = spec.unwrap();
        assert_eq!(spec.items.len(), 2);

        // Both items should have the frontmatter working_dir
        assert_eq!(
            spec.items[0].working_dir,
            Some(PathBuf::from("packages/shared"))
        );
        assert_eq!(
            spec.items[1].working_dir,
            Some(PathBuf::from("packages/shared"))
        );
    }
}
