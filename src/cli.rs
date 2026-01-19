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

#[derive(Subcommand)]
pub enum Command {
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

    /// Also install git hooks for validation
    #[arg(long)]
    pub hooks: bool,

    /// Overwrite existing files
    #[arg(long)]
    pub force: bool,
}
