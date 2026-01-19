use clap::{Parser, Subcommand, ValueEnum};
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

#[derive(Subcommand)]
pub enum Command {
    /// Initialize a project with PAVED documentation
    Init,

    /// Validate PAVED documentation
    Check,

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
    Prompt,

    /// Manage git hooks for documentation validation
    Hooks,

    /// View or modify paver configuration
    #[command(subcommand)]
    Config(ConfigCommand),

    /// Generate an index document
    Index,
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

/// Document type for the `paver new` command.
#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum DocType {
    /// Component documentation for services, libraries, and modules
    Component,
    /// Runbook for operational procedures
    Runbook,
    /// Architecture Decision Record
    Adr,
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
