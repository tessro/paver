use clap::{Args, Parser, Subcommand, ValueEnum};
use std::path::PathBuf;

use crate::templates::TemplateType;

/// PAVED documentation tool - structured docs optimized for AI agents
#[derive(Parser)]
#[command(name = "paver")]
#[command(version, about, long_about = None)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

/// Document type for PAVED documentation.
#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum DocType {
    /// Component documentation for services, libraries, and modules.
    Component,
    /// Runbook for operational procedures.
    Runbook,
    /// Architecture Decision Record.
    Adr,
}

/// Output format for prompt command.
#[derive(Debug, Clone, Copy, ValueEnum, Default)]
pub enum PromptOutputFormat {
    /// Plain text output.
    #[default]
    Text,
    /// JSON output for programmatic use.
    Json,
}

/// Output format for adopt command.
#[derive(Debug, Clone, Copy, ValueEnum, Default)]
pub enum AdoptOutputFormat {
    /// Plain text output.
    #[default]
    Text,
    /// JSON output for programmatic use.
    Json,
}

#[derive(Subcommand)]
pub enum Command {
    /// Scan existing documentation to help onboard paver
    Adopt {
        /// Path to scan for documentation [default: auto-detect docs/, documentation/, or README.md]
        #[arg()]
        path: Option<PathBuf>,

        /// Output format: text, json
        #[arg(long, default_value = "text", value_enum)]
        format: AdoptOutputFormat,

        /// Print suggested .paver.toml settings
        #[arg(long)]
        suggest_config: bool,

        /// Show what paver init would create (without creating)
        #[arg(long)]
        dry_run: bool,
    },

    /// Initialize a project with PAVED documentation
    Init(InitArgs),

    /// Validate PAVED documentation
    Check {
        /// Specific files or directories to check [default: docs root from config]
        #[arg()]
        paths: Vec<PathBuf>,

        /// Output format: text, json, github
        #[arg(long, default_value = "text", value_enum)]
        format: OutputFormat,

        /// Treat warnings as errors
        #[arg(long)]
        strict: bool,

        /// Only check docs changed since base ref
        #[arg(long)]
        changed: bool,

        /// Base ref for --changed comparison [default: origin/main]
        #[arg(long)]
        base: Option<String>,
    },

    /// Create a new document from template
    New {
        /// Document type: component, runbook, adr
        #[arg(value_enum)]
        doc_type: DocType,

        /// Name for the document (used in filename and title)
        name: String,

        /// Where to create the file [default: docs/{type}s/{name}.md]
        #[arg(long)]
        output: Option<PathBuf>,
    },

    /// Generate prompts for AI agents
    Prompt {
        /// Document type: component, runbook, adr
        #[arg(value_enum)]
        doc_type: DocType,

        /// Name of the thing being documented
        #[arg(long = "for")]
        name: Option<String>,

        /// Generate prompt to update existing doc
        #[arg(long)]
        update: Option<PathBuf>,

        /// Include these files as context (can be specified multiple times)
        #[arg(long, value_name = "PATH")]
        context: Vec<PathBuf>,

        /// Output format: text, json
        #[arg(long, value_enum, default_value = "text")]
        output: PromptOutputFormat,
    },

    /// Manage git hooks for documentation validation
    #[command(subcommand)]
    Hooks(HooksCommand),

    /// View or modify paver configuration
    #[command(subcommand)]
    Config(ConfigCommand),

    /// Generate an index document mapping all PAVED documentation
    Index {
        /// Where to write the index document
        #[arg(short, long, default_value = "docs/index.md")]
        output: PathBuf,

        /// Update existing index (preserve custom content)
        #[arg(short, long)]
        update: bool,
    },

    /// Show docs impacted by code changes
    Changed {
        /// Git ref to compare against [default: HEAD~1 or origin/main]
        #[arg(long)]
        base: Option<String>,

        /// Output format: text, json
        #[arg(long, default_value = "text", value_enum)]
        format: ChangedOutputFormat,

        /// Fail if impacted docs weren't updated
        #[arg(long)]
        strict: bool,
    },

    /// Run verification commands from PAVED documents
    Verify {
        /// Specific files or directories to verify [default: docs root from config]
        #[arg()]
        paths: Vec<PathBuf>,

        /// Output format: text, json, github
        #[arg(long, default_value = "text", value_enum)]
        format: OutputFormat,

        /// Write JSON report to file
        #[arg(long)]
        report: Option<PathBuf>,

        /// Timeout per command in seconds
        #[arg(long, default_value = "30")]
        timeout: u32,

        /// Continue running after first failure
        #[arg(long)]
        keep_going: bool,
    },

    /// Build static documentation site
    Build {
        /// Output directory for the built site
        #[arg(short, long, default_value = "_site")]
        output: PathBuf,
    },

    /// Show code-to-documentation coverage
    Coverage {
        /// Path to analyze [default: project root]
        #[arg()]
        path: Option<PathBuf>,

        /// Output format: text, json
        #[arg(long, default_value = "text", value_enum)]
        format: CoverageOutputFormat,

        /// Fail if coverage below this percentage
        #[arg(long)]
        threshold: Option<u32>,

        /// Only consider these code patterns (can be specified multiple times)
        #[arg(long = "include", value_name = "PATTERN")]
        include: Vec<String>,

        /// Exclude these code patterns (can be specified multiple times)
        #[arg(long = "exclude", value_name = "PATTERN")]
        exclude: Vec<String>,
    },
}

/// Output format for the `paver changed` command.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum, Default)]
pub enum ChangedOutputFormat {
    /// Human-readable text output
    #[default]
    Text,
    /// JSON output for programmatic use
    Json,
}

/// Output format for the `paver coverage` command.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum, Default)]
pub enum CoverageOutputFormat {
    /// Human-readable text output
    #[default]
    Text,
    /// JSON output for programmatic use
    Json,
}

#[derive(Subcommand)]
pub enum ConfigCommand {
    /// Get a config value
    Get {
        /// The key to get (e.g., docs.root, rules.max_lines)
        key: String,
    },

    /// Set a config value
    Set {
        /// The key to set (e.g., docs.root, rules.max_lines)
        key: String,
        /// The value to set
        value: String,
    },

    /// List all config values
    List,

    /// Print path to config file
    Path,
}

/// Output format for the `paver check` command.
#[derive(Debug, Clone, Copy, ValueEnum, Default)]
pub enum OutputFormat {
    /// Human-readable text output
    #[default]
    Text,
    /// JSON output for programmatic use
    Json,
    /// GitHub Actions annotation format
    Github,
}

/// Type of git hook to install.
#[derive(Debug, Clone, Copy, ValueEnum, Default)]
pub enum HookType {
    /// Run validation before commits.
    #[default]
    PreCommit,
    /// Run validation before pushes.
    PrePush,
}

impl HookType {
    /// Returns the git hook filename.
    pub fn filename(&self) -> &'static str {
        match self {
            HookType::PreCommit => "pre-commit",
            HookType::PrePush => "pre-push",
        }
    }
}

#[derive(Subcommand)]
pub enum HooksCommand {
    /// Install git hooks for documentation validation
    Install {
        /// Which hook to install: pre-commit, pre-push
        #[arg(long, value_enum, default_value = "pre-commit")]
        hook: HookType,

        /// Overwrite existing hooks
        #[arg(long)]
        force: bool,

        /// Also run paver verify in the hook
        #[arg(long)]
        verify: bool,
    },

    /// Uninstall git hooks
    Uninstall {
        /// Which hook to uninstall
        #[arg(long, value_enum, default_value = "pre-commit")]
        hook: HookType,
    },
}

impl From<DocType> for TemplateType {
    fn from(doc_type: DocType) -> Self {
        match doc_type {
            DocType::Component => TemplateType::Component,
            DocType::Runbook => TemplateType::Runbook,
            DocType::Adr => TemplateType::Adr,
        }
    }
}

/// Arguments for the init command.
#[derive(Args)]
pub struct InitArgs {
    /// Where to create docs directory
    #[arg(long, default_value = "docs")]
    pub docs_root: String,

    /// Skip installing git pre-commit hook
    #[arg(long)]
    pub skip_hooks: bool,

    /// Overwrite existing files
    #[arg(long)]
    pub force: bool,
}
