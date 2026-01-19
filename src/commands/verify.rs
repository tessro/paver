//! Implementation of the `paver verify` command for running verification commands.

use anyhow::{Context, Result};
use regex::Regex;
use serde::Serialize;
use std::env;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use crate::cli::OutputFormat;
use crate::config::{CONFIG_FILENAME, PaverConfig, RulesSection};
use crate::parser::ParsedDoc;
use crate::verification::{
    OutputMatcher, VerificationItem, VerificationSpec, extract_verification_spec,
};

/// Arguments for the `paver verify` command.
pub struct VerifyArgs {
    /// Specific files or directories to verify.
    pub paths: Vec<PathBuf>,
    /// Output format.
    pub format: OutputFormat,
    /// Path to write JSON report.
    pub report: Option<PathBuf>,
    /// Timeout per command in seconds.
    pub timeout: u32,
    /// Continue running after first failure.
    pub keep_going: bool,
}

/// Status of a verification command execution.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum VerifyStatus {
    Pass,
    Warn,
    Fail,
    Timeout,
    Skipped,
}

/// Details about an output mismatch.
#[derive(Debug, Clone, Serialize)]
pub struct OutputMismatch {
    /// The expected output pattern.
    pub expected: String,
    /// The match strategy used (contains, regex, exact).
    pub strategy: String,
    /// The actual output received.
    pub actual: String,
}

/// Result of running a single verification command.
#[derive(Debug, Clone, Serialize)]
pub struct CommandResult {
    /// The command that was run.
    pub command: String,
    /// Status of the command.
    pub status: VerifyStatus,
    /// Exit code if the command completed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
    /// Expected exit code.
    pub expected_exit_code: i32,
    /// Standard output.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stdout: Option<String>,
    /// Standard error.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stderr: Option<String>,
    /// Duration in milliseconds.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u64>,
    /// Output mismatch details (if any).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_mismatch: Option<OutputMismatch>,
    /// Working directory used for the command.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub working_dir: Option<PathBuf>,
    /// Environment variables set for the command.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub env_vars: Vec<(String, String)>,
}

/// Result of verifying a single document.
#[derive(Debug, Clone, Serialize)]
pub struct DocumentResult {
    /// Path to the document.
    pub file: PathBuf,
    /// Line number of the Verification section.
    pub section_line: usize,
    /// Results for each command.
    pub commands: Vec<CommandResult>,
    /// Overall status of the document.
    pub status: VerifyStatus,
}

impl DocumentResult {
    fn new(spec: &VerificationSpec) -> Self {
        Self {
            file: spec.source_file.clone(),
            section_line: spec.section_line,
            commands: Vec::new(),
            status: VerifyStatus::Pass,
        }
    }

    fn add_result(&mut self, result: CommandResult) {
        // Fail/Timeout always override other statuses
        // Warn only upgrades from Pass
        match result.status {
            VerifyStatus::Fail | VerifyStatus::Timeout => {
                self.status = result.status;
            }
            VerifyStatus::Warn => {
                if self.status == VerifyStatus::Pass {
                    self.status = VerifyStatus::Warn;
                }
            }
            VerifyStatus::Pass | VerifyStatus::Skipped => {}
        }
        self.commands.push(result);
    }

    fn is_success(&self) -> bool {
        // Pass and Warn are both considered success (warnings don't fail verification)
        self.status == VerifyStatus::Pass || self.status == VerifyStatus::Warn
    }
}

/// Aggregate results of running all verifications.
#[derive(Debug, Serialize)]
pub struct VerifyResults {
    /// Number of documents with verification sections.
    pub documents_verified: usize,
    /// Number of commands executed.
    pub commands_executed: usize,
    /// Number of commands that passed.
    pub commands_passed: usize,
    /// Number of commands that had warnings (output mismatch but not strict).
    pub commands_warned: usize,
    /// Number of commands that failed.
    pub commands_failed: usize,
    /// Results per document.
    pub documents: Vec<DocumentResult>,
}

impl VerifyResults {
    fn new() -> Self {
        Self {
            documents_verified: 0,
            commands_executed: 0,
            commands_passed: 0,
            commands_warned: 0,
            commands_failed: 0,
            documents: Vec::new(),
        }
    }

    fn add_document(&mut self, doc_result: DocumentResult) {
        for cmd in &doc_result.commands {
            self.commands_executed += 1;
            match cmd.status {
                VerifyStatus::Pass => self.commands_passed += 1,
                VerifyStatus::Warn => self.commands_warned += 1,
                VerifyStatus::Fail | VerifyStatus::Timeout => self.commands_failed += 1,
                VerifyStatus::Skipped => {}
            }
        }
        self.documents_verified += 1;
        self.documents.push(doc_result);
    }

    fn is_success(&self) -> bool {
        self.commands_failed == 0
    }
}

/// Execute the `paver verify` command.
pub fn execute(args: VerifyArgs) -> Result<()> {
    // Find and load config
    let config_path = find_config()?;
    let config = PaverConfig::load(&config_path)?;
    let config_dir = config_path.parent().unwrap_or_else(|| Path::new("."));

    // Determine paths to verify
    let paths = if args.paths.is_empty() {
        vec![config_dir.join(&config.docs.root)]
    } else {
        args.paths.clone()
    };

    // Find all markdown files
    let files = find_markdown_files(&paths)?;

    if files.is_empty() {
        eprintln!("No markdown files found to verify");
        return Ok(());
    }

    // Collect verification specs from all documents
    let mut specs: Vec<VerificationSpec> = Vec::new();
    for file in &files {
        let doc = ParsedDoc::parse(file)?;
        if let Some(spec) = extract_verification_spec(&doc) {
            specs.push(spec);
        }
    }

    if specs.is_empty() {
        eprintln!("No verification sections found in documents");
        return Ok(());
    }

    // Run verifications
    let mut results = VerifyResults::new();
    let timeout = Duration::from_secs(args.timeout as u64);

    for spec in &specs {
        let doc_result =
            run_verification(spec, timeout, args.keep_going, config_dir, &config.rules)?;
        let should_stop = !doc_result.is_success() && !args.keep_going;
        results.add_document(doc_result);

        if should_stop {
            break;
        }
    }

    // Output results in the requested format
    match args.format {
        OutputFormat::Text => output_text(&results),
        OutputFormat::Json => output_json(&results)?,
        OutputFormat::Github => output_github(&results),
    }

    // Write report file if requested
    if let Some(report_path) = &args.report {
        write_report(&results, report_path)?;
    }

    // Return error if verifications failed
    if results.is_success() {
        Ok(())
    } else {
        anyhow::bail!(
            "Verification failed: {} of {} command{} failed",
            results.commands_failed,
            results.commands_executed,
            if results.commands_executed == 1 {
                ""
            } else {
                "s"
            }
        );
    }
}

/// Run verification commands for a single document.
fn run_verification(
    spec: &VerificationSpec,
    timeout: Duration,
    keep_going: bool,
    working_dir: &Path,
    rules: &RulesSection,
) -> Result<DocumentResult> {
    let mut doc_result = DocumentResult::new(spec);

    for item in &spec.items {
        let cmd_result = run_command(item, timeout, working_dir, rules);
        // Fail/Timeout stop execution unless keep_going; Warn does not stop execution
        let is_failure =
            cmd_result.status == VerifyStatus::Fail || cmd_result.status == VerifyStatus::Timeout;
        doc_result.add_result(cmd_result);

        if is_failure && !keep_going {
            // Mark remaining commands as skipped
            for remaining in spec.items.iter().skip(doc_result.commands.len()) {
                doc_result.add_result(CommandResult {
                    command: remaining.command.clone(),
                    status: VerifyStatus::Skipped,
                    exit_code: None,
                    expected_exit_code: remaining.expected_exit_code.unwrap_or(0),
                    stdout: None,
                    stderr: None,
                    duration_ms: None,
                    output_mismatch: None,
                    working_dir: remaining.working_dir.clone(),
                    env_vars: remaining.env_vars.clone(),
                });
            }
            break;
        }
    }

    Ok(doc_result)
}

/// Check if the output matches the expected pattern.
/// Returns (matches, strategy_name) tuple.
fn check_output_match(matcher: &OutputMatcher, stdout: &str) -> (bool, &'static str) {
    match matcher {
        OutputMatcher::Contains(substring) => (stdout.contains(substring), "contains"),
        OutputMatcher::Regex(pattern) => {
            let matches = Regex::new(pattern)
                .map(|re| re.is_match(stdout))
                .unwrap_or(false);
            (matches, "regex")
        }
        OutputMatcher::Exact(expected) => (stdout.trim() == expected.trim(), "exact"),
        OutputMatcher::ExitCodeOnly => (true, "exit_code_only"),
    }
}

/// Get the expected string from an OutputMatcher.
fn get_expected_string(matcher: &OutputMatcher) -> String {
    match matcher {
        OutputMatcher::Contains(s) => s.clone(),
        OutputMatcher::Regex(s) => s.clone(),
        OutputMatcher::Exact(s) => s.clone(),
        OutputMatcher::ExitCodeOnly => String::new(),
    }
}

/// Run a single verification command.
fn run_command(
    item: &VerificationItem,
    timeout: Duration,
    working_dir: &Path,
    rules: &RulesSection,
) -> CommandResult {
    let expected_exit_code = item.expected_exit_code.unwrap_or(0);
    let start = std::time::Instant::now();

    // Use item's working_dir if specified, otherwise use config_dir
    let cmd_working_dir = item.working_dir.as_deref().unwrap_or(working_dir);

    // Build the command
    let mut cmd = Command::new("sh");
    cmd.arg("-c")
        .arg(&item.command)
        .current_dir(cmd_working_dir);

    // Set environment variables
    for (key, value) in &item.env_vars {
        cmd.env(key, value);
    }

    // Execute command via shell
    let output = cmd.output();

    let duration_ms = start.elapsed().as_millis() as u64;

    // Track the working dir and env vars for the result (only if non-default)
    let result_working_dir = item.working_dir.clone();
    let result_env_vars = item.env_vars.clone();

    match output {
        Ok(output) => {
            let exit_code = output.status.code();
            let stdout = String::from_utf8_lossy(&output.stdout).to_string();
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();

            // Check if timed out (heuristic: check if duration exceeds timeout)
            if duration_ms >= timeout.as_millis() as u64 {
                return CommandResult {
                    command: item.command.clone(),
                    status: VerifyStatus::Timeout,
                    exit_code,
                    expected_exit_code,
                    stdout: Some(stdout),
                    stderr: Some(stderr),
                    duration_ms: Some(duration_ms),
                    output_mismatch: None,
                    working_dir: result_working_dir,
                    env_vars: result_env_vars,
                };
            }

            // Check exit code first
            let exit_code_matches = exit_code == Some(expected_exit_code);

            // If exit code doesn't match, fail immediately
            if !exit_code_matches {
                return CommandResult {
                    command: item.command.clone(),
                    status: VerifyStatus::Fail,
                    exit_code,
                    expected_exit_code,
                    stdout: if stdout.is_empty() {
                        None
                    } else {
                        Some(stdout)
                    },
                    stderr: if stderr.is_empty() {
                        None
                    } else {
                        Some(stderr)
                    },
                    duration_ms: Some(duration_ms),
                    output_mismatch: None,
                    working_dir: result_working_dir,
                    env_vars: result_env_vars,
                };
            }

            // Check output matching if expected_output is specified and not skipped
            let (status, output_mismatch) = if rules.skip_output_matching {
                // Skip output matching entirely
                (VerifyStatus::Pass, None)
            } else if let Some(ref matcher) = item.expected_output {
                let (matches, strategy) = check_output_match(matcher, &stdout);
                if matches {
                    (VerifyStatus::Pass, None)
                } else {
                    // Output doesn't match
                    let mismatch = OutputMismatch {
                        expected: get_expected_string(matcher),
                        strategy: strategy.to_string(),
                        actual: stdout.clone(),
                    };
                    if rules.strict_output_matching {
                        // Strict mode: fail on mismatch
                        (VerifyStatus::Fail, Some(mismatch))
                    } else {
                        // Default mode: warn on mismatch
                        (VerifyStatus::Warn, Some(mismatch))
                    }
                }
            } else {
                // No expected output, just pass
                (VerifyStatus::Pass, None)
            };

            CommandResult {
                command: item.command.clone(),
                status,
                exit_code,
                expected_exit_code,
                stdout: if stdout.is_empty() {
                    None
                } else {
                    Some(stdout)
                },
                stderr: if stderr.is_empty() {
                    None
                } else {
                    Some(stderr)
                },
                duration_ms: Some(duration_ms),
                output_mismatch,
                working_dir: result_working_dir,
                env_vars: result_env_vars,
            }
        }
        Err(e) => CommandResult {
            command: item.command.clone(),
            status: VerifyStatus::Fail,
            exit_code: None,
            expected_exit_code,
            stdout: None,
            stderr: Some(format!("Failed to execute command: {}", e)),
            duration_ms: Some(duration_ms),
            output_mismatch: None,
            working_dir: result_working_dir,
            env_vars: result_env_vars,
        },
    }
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

/// Print a debugging suggestion for a failed command.
fn print_debug_suggestion(cmd: &CommandResult) {
    println!("    suggestion: Try running manually:");
    let mut suggestion = String::new();
    // Add env vars
    for (key, value) in &cmd.env_vars {
        suggestion.push_str(&format!("{}={} ", key, value));
    }
    // Add cd if working_dir is set
    if let Some(ref wd) = cmd.working_dir {
        suggestion.push_str(&format!("cd {} && ", wd.display()));
    }
    suggestion.push_str(&cmd.command);
    println!("      {}", suggestion);
}

/// Truncate a string to a maximum number of lines.
fn truncate_lines(s: &str, max_lines: usize) -> String {
    let lines: Vec<&str> = s.lines().collect();
    if lines.len() <= max_lines {
        s.to_string()
    } else {
        let mut result: String = lines[..max_lines].join("\n");
        result.push_str(&format!(
            "\n    ... ({} more lines)",
            lines.len() - max_lines
        ));
        result
    }
}

/// Output results in text format.
fn output_text(results: &VerifyResults) {
    for doc in &results.documents {
        println!("{}:{}", doc.file.display(), doc.section_line);

        for cmd in &doc.commands {
            let status_str = match cmd.status {
                VerifyStatus::Pass => "PASS",
                VerifyStatus::Warn => "WARN",
                VerifyStatus::Fail => "FAIL",
                VerifyStatus::Timeout => "TIMEOUT",
                VerifyStatus::Skipped => "SKIPPED",
            };

            let duration_str = cmd
                .duration_ms
                .map(|d| format!(" ({:.2}s)", d as f64 / 1000.0))
                .unwrap_or_default();

            println!("  [{}]{} {}", status_str, duration_str, cmd.command);

            // Show failure details
            if cmd.status == VerifyStatus::Fail || cmd.status == VerifyStatus::Timeout {
                // Show working directory if specified
                if let Some(ref wd) = cmd.working_dir {
                    println!("    working_dir: {}", wd.display());
                }
                // Show environment variables if any
                if !cmd.env_vars.is_empty() {
                    for (key, value) in &cmd.env_vars {
                        println!("    env: {}={}", key, value);
                    }
                }
                if let Some(code) = cmd.exit_code
                    && code != cmd.expected_exit_code
                {
                    println!(
                        "    exit code: {} (expected {})",
                        code, cmd.expected_exit_code
                    );
                }
                // Always show full stdout/stderr for failed commands to aid debugging
                if let Some(stdout) = &cmd.stdout
                    && !stdout.is_empty()
                {
                    println!("    stdout:");
                    for line in stdout.lines() {
                        println!("      {}", line);
                    }
                }
                if let Some(stderr) = &cmd.stderr
                    && !stderr.is_empty()
                {
                    println!("    stderr:");
                    for line in stderr.lines() {
                        println!("      {}", line);
                    }
                }
                // Print debugging suggestion
                print_debug_suggestion(cmd);
            }

            // Show output mismatch details for both warnings and failures
            if let Some(ref mismatch) = cmd.output_mismatch {
                println!("    output mismatch ({}):", mismatch.strategy);
                println!("      expected: {}", truncate_lines(&mismatch.expected, 3));
                println!(
                    "      actual:   {}",
                    truncate_lines(mismatch.actual.trim(), 5)
                );
            }
        }
        println!();
    }

    // Print summary
    print!(
        "Verified {} document{}: ",
        results.documents_verified,
        if results.documents_verified == 1 {
            ""
        } else {
            "s"
        }
    );

    if results.commands_failed == 0 && results.commands_warned == 0 {
        println!(
            "{} command{} passed",
            results.commands_passed,
            if results.commands_passed == 1 {
                ""
            } else {
                "s"
            }
        );
    } else if results.commands_failed == 0 {
        println!(
            "{} passed, {} warned",
            results.commands_passed, results.commands_warned
        );
    } else {
        println!(
            "{} passed, {} warned, {} failed",
            results.commands_passed, results.commands_warned, results.commands_failed
        );
    }
}

/// Output results in JSON format.
fn output_json(results: &VerifyResults) -> Result<()> {
    let json = serde_json::to_string_pretty(results).context("Failed to serialize results")?;
    println!("{}", json);
    Ok(())
}

/// Output results in GitHub Actions annotation format.
fn output_github(results: &VerifyResults) {
    for doc in &results.documents {
        for cmd in &doc.commands {
            if cmd.status != VerifyStatus::Pass {
                let level = match cmd.status {
                    VerifyStatus::Fail | VerifyStatus::Timeout => "error",
                    VerifyStatus::Warn | VerifyStatus::Skipped => "warning",
                    VerifyStatus::Pass => continue,
                };

                let message = match cmd.status {
                    VerifyStatus::Fail => {
                        if let Some(ref mismatch) = cmd.output_mismatch {
                            format!(
                                "Output mismatch ({}): expected '{}', got '{}'",
                                mismatch.strategy,
                                mismatch.expected.lines().next().unwrap_or(""),
                                mismatch.actual.trim().lines().next().unwrap_or("")
                            )
                        } else {
                            format!(
                                "Command failed: {} (exit code: {:?}, expected: {})",
                                cmd.command, cmd.exit_code, cmd.expected_exit_code
                            )
                        }
                    }
                    VerifyStatus::Warn => {
                        if let Some(ref mismatch) = cmd.output_mismatch {
                            format!(
                                "Output mismatch ({}): expected '{}', got '{}'",
                                mismatch.strategy,
                                mismatch.expected.lines().next().unwrap_or(""),
                                mismatch.actual.trim().lines().next().unwrap_or("")
                            )
                        } else {
                            format!("Command warning: {}", cmd.command)
                        }
                    }
                    VerifyStatus::Timeout => {
                        format!("Command timed out: {}", cmd.command)
                    }
                    VerifyStatus::Skipped => {
                        format!("Command skipped: {}", cmd.command)
                    }
                    VerifyStatus::Pass => continue,
                };

                println!(
                    "::{} file={},line={}::{}",
                    level,
                    doc.file.display(),
                    doc.section_line,
                    message
                );
            }
        }
    }
}

/// Write JSON report to file.
fn write_report(results: &VerifyResults, path: &Path) -> Result<()> {
    let json = serde_json::to_string_pretty(results).context("Failed to serialize results")?;
    let mut file = std::fs::File::create(path)
        .with_context(|| format!("Failed to create {}", path.display()))?;
    file.write_all(json.as_bytes())
        .with_context(|| format!("Failed to write to {}", path.display()))?;
    eprintln!("Report written to {}", path.display());
    Ok(())
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

[rules]
max_lines = 300
require_verification = true
require_examples = true
"#;
        let config_path = temp_dir.path().join(".paver.toml");
        fs::write(&config_path, config_content).unwrap();
        config_path
    }

    fn create_doc_with_verification(
        temp_dir: &TempDir,
        filename: &str,
        commands: &[&str],
    ) -> PathBuf {
        let docs_dir = temp_dir.path().join("docs");
        fs::create_dir_all(&docs_dir).unwrap();

        // Create separate code blocks for each command so they become separate verification items
        let commands_str = commands
            .iter()
            .map(|c| format!("```bash\n{}\n```", c))
            .collect::<Vec<_>>()
            .join("\n");

        let content = format!(
            r#"# Test Document

## Purpose
A test document.

## Verification
{}

## Examples
Example here.
"#,
            commands_str
        );

        let path = docs_dir.join(filename);
        fs::write(&path, content).unwrap();
        path
    }

    fn default_rules() -> RulesSection {
        RulesSection::default()
    }

    fn strict_rules() -> RulesSection {
        RulesSection {
            strict_output_matching: true,
            ..RulesSection::default()
        }
    }

    fn skip_output_rules() -> RulesSection {
        RulesSection {
            skip_output_matching: true,
            ..RulesSection::default()
        }
    }

    #[test]
    fn verify_status_serializes_lowercase() {
        let pass = serde_json::to_string(&VerifyStatus::Pass).unwrap();
        let warn = serde_json::to_string(&VerifyStatus::Warn).unwrap();
        let fail = serde_json::to_string(&VerifyStatus::Fail).unwrap();
        let timeout = serde_json::to_string(&VerifyStatus::Timeout).unwrap();
        let skipped = serde_json::to_string(&VerifyStatus::Skipped).unwrap();

        assert_eq!(pass, "\"pass\"");
        assert_eq!(warn, "\"warn\"");
        assert_eq!(fail, "\"fail\"");
        assert_eq!(timeout, "\"timeout\"");
        assert_eq!(skipped, "\"skipped\"");
    }

    #[test]
    fn document_result_tracks_status() {
        let spec = VerificationSpec {
            source_file: PathBuf::from("test.md"),
            section_line: 10,
            items: vec![],
        };

        let mut doc_result = DocumentResult::new(&spec);
        assert!(doc_result.is_success());

        doc_result.add_result(CommandResult {
            command: "echo ok".to_string(),
            status: VerifyStatus::Pass,
            exit_code: Some(0),
            expected_exit_code: 0,
            stdout: None,
            stderr: None,
            duration_ms: Some(10),
            output_mismatch: None,
            working_dir: None,
            env_vars: Vec::new(),
        });
        assert!(doc_result.is_success());

        doc_result.add_result(CommandResult {
            command: "false".to_string(),
            status: VerifyStatus::Fail,
            exit_code: Some(1),
            expected_exit_code: 0,
            stdout: None,
            stderr: None,
            duration_ms: Some(5),
            output_mismatch: None,
            working_dir: None,
            env_vars: Vec::new(),
        });
        assert!(!doc_result.is_success());
    }

    #[test]
    fn verify_results_aggregates_counts() {
        let spec = VerificationSpec {
            source_file: PathBuf::from("test.md"),
            section_line: 10,
            items: vec![],
        };

        let mut results = VerifyResults::new();
        let mut doc_result = DocumentResult::new(&spec);

        doc_result.add_result(CommandResult {
            command: "echo 1".to_string(),
            status: VerifyStatus::Pass,
            exit_code: Some(0),
            expected_exit_code: 0,
            stdout: None,
            stderr: None,
            duration_ms: Some(10),
            output_mismatch: None,
            working_dir: None,
            env_vars: Vec::new(),
        });

        doc_result.add_result(CommandResult {
            command: "false".to_string(),
            status: VerifyStatus::Fail,
            exit_code: Some(1),
            expected_exit_code: 0,
            stdout: None,
            stderr: None,
            duration_ms: Some(5),
            output_mismatch: None,
            working_dir: None,
            env_vars: Vec::new(),
        });

        results.add_document(doc_result);

        assert_eq!(results.documents_verified, 1);
        assert_eq!(results.commands_executed, 2);
        assert_eq!(results.commands_passed, 1);
        assert_eq!(results.commands_failed, 1);
        assert!(!results.is_success());
    }

    #[test]
    fn run_command_success() {
        let item = VerificationItem {
            command: "echo hello".to_string(),
            working_dir: None,
            expected_exit_code: Some(0),
            expected_output: None,
            timeout_secs: Some(30),
            env_vars: Vec::new(),
        };

        let result = run_command(
            &item,
            Duration::from_secs(30),
            Path::new("."),
            &default_rules(),
        );

        assert_eq!(result.status, VerifyStatus::Pass);
        assert_eq!(result.exit_code, Some(0));
        assert!(result.stdout.as_ref().is_some_and(|s| s.contains("hello")));
    }

    #[test]
    fn run_command_failure() {
        let item = VerificationItem {
            command: "exit 1".to_string(),
            working_dir: None,
            expected_exit_code: Some(0),
            expected_output: None,
            timeout_secs: Some(30),
            env_vars: Vec::new(),
        };

        let result = run_command(
            &item,
            Duration::from_secs(30),
            Path::new("."),
            &default_rules(),
        );

        assert_eq!(result.status, VerifyStatus::Fail);
        assert_eq!(result.exit_code, Some(1));
        assert_eq!(result.expected_exit_code, 0);
    }

    #[test]
    fn run_command_expected_nonzero_exit() {
        let item = VerificationItem {
            command: "exit 1".to_string(),
            working_dir: None,
            expected_exit_code: Some(1),
            expected_output: None,
            timeout_secs: Some(30),
            env_vars: Vec::new(),
        };

        let result = run_command(
            &item,
            Duration::from_secs(30),
            Path::new("."),
            &default_rules(),
        );

        assert_eq!(result.status, VerifyStatus::Pass);
        assert_eq!(result.exit_code, Some(1));
    }

    #[test]
    fn json_output_is_valid() {
        let spec = VerificationSpec {
            source_file: PathBuf::from("test.md"),
            section_line: 10,
            items: vec![],
        };

        let mut results = VerifyResults::new();
        let mut doc_result = DocumentResult::new(&spec);
        doc_result.add_result(CommandResult {
            command: "echo ok".to_string(),
            status: VerifyStatus::Pass,
            exit_code: Some(0),
            expected_exit_code: 0,
            stdout: Some("ok\n".to_string()),
            stderr: None,
            duration_ms: Some(10),
            output_mismatch: None,
            working_dir: None,
            env_vars: Vec::new(),
        });
        results.add_document(doc_result);

        let json = serde_json::to_string(&results).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed["documents_verified"], 1);
        assert_eq!(parsed["commands_passed"], 1);
        assert_eq!(parsed["documents"][0]["commands"][0]["status"], "pass");
    }

    #[test]
    fn find_markdown_files_collects_recursively() {
        let temp_dir = TempDir::new().unwrap();
        let docs_dir = temp_dir.path().join("docs");
        let nested_dir = docs_dir.join("nested");
        fs::create_dir_all(&nested_dir).unwrap();

        fs::write(docs_dir.join("doc1.md"), "# Doc 1").unwrap();
        fs::write(nested_dir.join("doc2.md"), "# Doc 2").unwrap();

        let files = find_markdown_files(&[docs_dir]).unwrap();

        assert_eq!(files.len(), 2);
    }

    #[test]
    fn integration_verify_passing_document() {
        let temp_dir = TempDir::new().unwrap();
        create_test_config(&temp_dir);
        let doc_path =
            create_doc_with_verification(&temp_dir, "passing.md", &["echo hello", "true"]);

        let doc = ParsedDoc::parse(&doc_path).unwrap();
        let spec = extract_verification_spec(&doc).unwrap();

        let doc_result = run_verification(
            &spec,
            Duration::from_secs(30),
            true,
            temp_dir.path(),
            &default_rules(),
        )
        .unwrap();

        assert!(doc_result.is_success());
        assert_eq!(doc_result.commands.len(), 2);
        assert!(
            doc_result
                .commands
                .iter()
                .all(|c| c.status == VerifyStatus::Pass)
        );
    }

    #[test]
    fn integration_verify_failing_document() {
        let temp_dir = TempDir::new().unwrap();
        create_test_config(&temp_dir);
        let doc_path =
            create_doc_with_verification(&temp_dir, "failing.md", &["echo hello", "false"]);

        let doc = ParsedDoc::parse(&doc_path).unwrap();
        let spec = extract_verification_spec(&doc).unwrap();

        let doc_result = run_verification(
            &spec,
            Duration::from_secs(30),
            true,
            temp_dir.path(),
            &default_rules(),
        )
        .unwrap();

        assert!(!doc_result.is_success());
        assert_eq!(doc_result.commands[0].status, VerifyStatus::Pass);
        assert_eq!(doc_result.commands[1].status, VerifyStatus::Fail);
    }

    #[test]
    fn integration_keep_going_false_skips_remaining() {
        let temp_dir = TempDir::new().unwrap();
        create_test_config(&temp_dir);
        let doc_path = create_doc_with_verification(
            &temp_dir,
            "skip.md",
            &["false", "echo should-be-skipped"],
        );

        let doc = ParsedDoc::parse(&doc_path).unwrap();
        let spec = extract_verification_spec(&doc).unwrap();

        let doc_result = run_verification(
            &spec,
            Duration::from_secs(30),
            false,
            temp_dir.path(),
            &default_rules(),
        )
        .unwrap();

        assert!(!doc_result.is_success());
        assert_eq!(doc_result.commands.len(), 2);
        assert_eq!(doc_result.commands[0].status, VerifyStatus::Fail);
        assert_eq!(doc_result.commands[1].status, VerifyStatus::Skipped);
    }

    #[test]
    fn integration_keep_going_true_runs_all() {
        let temp_dir = TempDir::new().unwrap();
        create_test_config(&temp_dir);
        let doc_path = create_doc_with_verification(
            &temp_dir,
            "continue.md",
            &["false", "echo second", "true"],
        );

        let doc = ParsedDoc::parse(&doc_path).unwrap();
        let spec = extract_verification_spec(&doc).unwrap();

        let doc_result = run_verification(
            &spec,
            Duration::from_secs(30),
            true,
            temp_dir.path(),
            &default_rules(),
        )
        .unwrap();

        assert!(!doc_result.is_success());
        assert_eq!(doc_result.commands.len(), 3);
        assert_eq!(doc_result.commands[0].status, VerifyStatus::Fail);
        assert_eq!(doc_result.commands[1].status, VerifyStatus::Pass);
        assert_eq!(doc_result.commands[2].status, VerifyStatus::Pass);
    }

    #[test]
    fn output_mismatch_produces_warning_by_default() {
        let item = VerificationItem {
            command: "echo actual".to_string(),
            working_dir: None,
            expected_exit_code: Some(0),
            expected_output: Some(OutputMatcher::Contains("expected".to_string())),
            timeout_secs: Some(30),
            env_vars: Vec::new(),
        };

        let result = run_command(
            &item,
            Duration::from_secs(30),
            Path::new("."),
            &default_rules(),
        );

        assert_eq!(result.status, VerifyStatus::Warn);
        assert!(result.output_mismatch.is_some());
        let mismatch = result.output_mismatch.unwrap();
        assert_eq!(mismatch.strategy, "contains");
        assert_eq!(mismatch.expected, "expected");
        assert!(mismatch.actual.contains("actual"));
    }

    #[test]
    fn output_mismatch_fails_with_strict_mode() {
        let item = VerificationItem {
            command: "echo actual".to_string(),
            working_dir: None,
            expected_exit_code: Some(0),
            expected_output: Some(OutputMatcher::Contains("expected".to_string())),
            timeout_secs: Some(30),
            env_vars: Vec::new(),
        };

        let result = run_command(
            &item,
            Duration::from_secs(30),
            Path::new("."),
            &strict_rules(),
        );

        assert_eq!(result.status, VerifyStatus::Fail);
        assert!(result.output_mismatch.is_some());
    }

    #[test]
    fn output_mismatch_ignored_with_skip_mode() {
        let item = VerificationItem {
            command: "echo actual".to_string(),
            working_dir: None,
            expected_exit_code: Some(0),
            expected_output: Some(OutputMatcher::Contains("expected".to_string())),
            timeout_secs: Some(30),
            env_vars: Vec::new(),
        };

        let result = run_command(
            &item,
            Duration::from_secs(30),
            Path::new("."),
            &skip_output_rules(),
        );

        assert_eq!(result.status, VerifyStatus::Pass);
        assert!(result.output_mismatch.is_none());
    }

    #[test]
    fn output_match_passes() {
        let item = VerificationItem {
            command: "echo hello world".to_string(),
            working_dir: None,
            expected_exit_code: Some(0),
            expected_output: Some(OutputMatcher::Contains("hello".to_string())),
            timeout_secs: Some(30),
            env_vars: Vec::new(),
        };

        let result = run_command(
            &item,
            Duration::from_secs(30),
            Path::new("."),
            &default_rules(),
        );

        assert_eq!(result.status, VerifyStatus::Pass);
        assert!(result.output_mismatch.is_none());
    }

    #[test]
    fn warn_status_is_success() {
        let spec = VerificationSpec {
            source_file: PathBuf::from("test.md"),
            section_line: 10,
            items: vec![],
        };

        let mut doc_result = DocumentResult::new(&spec);
        doc_result.add_result(CommandResult {
            command: "echo ok".to_string(),
            status: VerifyStatus::Warn,
            exit_code: Some(0),
            expected_exit_code: 0,
            stdout: Some("actual".to_string()),
            stderr: None,
            duration_ms: Some(10),
            output_mismatch: Some(OutputMismatch {
                expected: "expected".to_string(),
                strategy: "contains".to_string(),
                actual: "actual".to_string(),
            }),
            working_dir: None,
            env_vars: Vec::new(),
        });

        // Warn is still considered success
        assert!(doc_result.is_success());
        // Verify the document status is Warn
        assert_eq!(doc_result.status, VerifyStatus::Warn);
    }

    #[test]
    fn verify_results_tracks_warnings() {
        let spec = VerificationSpec {
            source_file: PathBuf::from("test.md"),
            section_line: 10,
            items: vec![],
        };

        let mut results = VerifyResults::new();
        let mut doc_result = DocumentResult::new(&spec);

        doc_result.add_result(CommandResult {
            command: "echo 1".to_string(),
            status: VerifyStatus::Pass,
            exit_code: Some(0),
            expected_exit_code: 0,
            stdout: None,
            stderr: None,
            duration_ms: Some(10),
            output_mismatch: None,
            working_dir: None,
            env_vars: Vec::new(),
        });

        doc_result.add_result(CommandResult {
            command: "echo 2".to_string(),
            status: VerifyStatus::Warn,
            exit_code: Some(0),
            expected_exit_code: 0,
            stdout: Some("actual".to_string()),
            stderr: None,
            duration_ms: Some(5),
            output_mismatch: Some(OutputMismatch {
                expected: "expected".to_string(),
                strategy: "contains".to_string(),
                actual: "actual".to_string(),
            }),
            working_dir: None,
            env_vars: Vec::new(),
        });

        results.add_document(doc_result);

        assert_eq!(results.commands_passed, 1);
        assert_eq!(results.commands_warned, 1);
        assert_eq!(results.commands_failed, 0);
        assert!(results.is_success());
    }
}
