//! Configuration file handling for paver.
//!
//! This module defines the `.paver.toml` configuration schema and provides
//! functions for loading, validating, and saving configuration files.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// The default configuration filename.
pub const CONFIG_FILENAME: &str = ".paver.toml";

/// Root configuration structure for a paver project.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct PaverConfig {
    /// Paver tool settings.
    pub paver: PaverSection,
    /// Documentation location settings.
    pub docs: DocsSection,
    /// Validation rules.
    #[serde(default)]
    pub rules: RulesSection,
    /// Template configuration.
    #[serde(default)]
    pub templates: TemplatesSection,
}

/// Paver tool metadata section.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PaverSection {
    /// Configuration schema version.
    pub version: String,
}

/// Documentation paths section.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DocsSection {
    /// Root directory for documentation.
    pub root: PathBuf,
    /// Directory where templates are stored (optional).
    #[serde(default)]
    pub templates: Option<PathBuf>,
}

/// Validation rules section.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RulesSection {
    /// Maximum lines per document.
    #[serde(default = "default_max_lines")]
    pub max_lines: u32,
    /// Require Verification section in documents.
    #[serde(default = "default_true")]
    pub require_verification: bool,
    /// Require Examples section in documents.
    #[serde(default = "default_true")]
    pub require_examples: bool,
}

/// Template file mappings section.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct TemplatesSection {
    /// Filename for component template.
    #[serde(default)]
    pub component: Option<String>,
    /// Filename for runbook template.
    #[serde(default)]
    pub runbook: Option<String>,
    /// Filename for ADR template.
    #[serde(default)]
    pub adr: Option<String>,
}

fn default_max_lines() -> u32 {
    300
}

fn default_true() -> bool {
    true
}

impl Default for PaverSection {
    fn default() -> Self {
        Self {
            version: "0.1".to_string(),
        }
    }
}

impl Default for DocsSection {
    fn default() -> Self {
        Self {
            root: PathBuf::from("docs"),
            templates: None,
        }
    }
}

impl Default for RulesSection {
    fn default() -> Self {
        Self {
            max_lines: default_max_lines(),
            require_verification: true,
            require_examples: true,
        }
    }
}

impl PaverConfig {
    /// Load configuration from a file path.
    pub fn load(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("failed to read config file: {}", path.display()))?;
        Self::parse(&content)
    }

    /// Parse configuration from a TOML string.
    pub fn parse(content: &str) -> Result<Self> {
        let config: PaverConfig = toml::from_str(content).context("failed to parse config file")?;
        config.validate()?;
        Ok(config)
    }

    /// Save configuration to a file path.
    pub fn save(&self, path: impl AsRef<Path>) -> Result<()> {
        let path = path.as_ref();
        let content = toml::to_string_pretty(self).context("failed to serialize config")?;
        std::fs::write(path, content)
            .with_context(|| format!("failed to write config file: {}", path.display()))?;
        Ok(())
    }

    /// Validate the configuration values.
    pub fn validate(&self) -> Result<()> {
        if self.paver.version.is_empty() {
            anyhow::bail!("paver.version cannot be empty");
        }

        if self.docs.root.as_os_str().is_empty() {
            anyhow::bail!("docs.root cannot be empty");
        }

        if self.rules.max_lines == 0 {
            anyhow::bail!("rules.max_lines must be greater than 0");
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_valid_config() {
        let toml = r#"
[paver]
version = "0.1"

[docs]
root = "docs"
templates = "docs/templates"

[rules]
max_lines = 300
require_verification = true
require_examples = true

[templates]
component = "component.md"
runbook = "runbook.md"
adr = "adr.md"
"#;
        let config = PaverConfig::parse(toml).unwrap();
        assert_eq!(config.paver.version, "0.1");
        assert_eq!(config.docs.root, PathBuf::from("docs"));
        assert_eq!(config.docs.templates, Some(PathBuf::from("docs/templates")));
        assert_eq!(config.rules.max_lines, 300);
        assert!(config.rules.require_verification);
        assert!(config.rules.require_examples);
        assert_eq!(config.templates.component, Some("component.md".to_string()));
        assert_eq!(config.templates.runbook, Some("runbook.md".to_string()));
        assert_eq!(config.templates.adr, Some("adr.md".to_string()));
    }

    #[test]
    fn parse_config_with_missing_optional_fields() {
        let toml = r#"
[paver]
version = "0.1"

[docs]
root = "documentation"
"#;
        let config = PaverConfig::parse(toml).unwrap();
        assert_eq!(config.paver.version, "0.1");
        assert_eq!(config.docs.root, PathBuf::from("documentation"));
        assert_eq!(config.docs.templates, None);
        // Default values should be applied
        assert_eq!(config.rules.max_lines, 300);
        assert!(config.rules.require_verification);
        assert!(config.rules.require_examples);
        assert_eq!(config.templates.component, None);
    }

    #[test]
    fn reject_config_with_empty_version() {
        let toml = r#"
[paver]
version = ""

[docs]
root = "docs"
"#;
        let result = PaverConfig::parse(toml);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("version cannot be empty")
        );
    }

    #[test]
    fn reject_config_with_zero_max_lines() {
        let toml = r#"
[paver]
version = "0.1"

[docs]
root = "docs"

[rules]
max_lines = 0
"#;
        let result = PaverConfig::parse(toml);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("max_lines must be greater than 0")
        );
    }

    #[test]
    fn default_config_is_valid() {
        let config = PaverConfig::default();
        assert!(config.validate().is_ok());
        assert_eq!(config.paver.version, "0.1");
        assert_eq!(config.docs.root, PathBuf::from("docs"));
        assert_eq!(config.rules.max_lines, 300);
        assert!(config.rules.require_verification);
        assert!(config.rules.require_examples);
    }

    #[test]
    fn config_roundtrip() {
        let config = PaverConfig::default();
        let serialized = toml::to_string_pretty(&config).unwrap();
        let deserialized = PaverConfig::parse(&serialized).unwrap();
        assert_eq!(config, deserialized);
    }

    #[test]
    fn reject_config_with_empty_docs_root() {
        let toml = r#"
[paver]
version = "0.1"

[docs]
root = ""
"#;
        let result = PaverConfig::parse(toml);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("docs.root cannot be empty")
        );
    }

    #[test]
    fn parse_config_with_custom_rules() {
        let toml = r#"
[paver]
version = "0.1"

[docs]
root = "docs"

[rules]
max_lines = 500
require_verification = false
require_examples = false
"#;
        let config = PaverConfig::parse(toml).unwrap();
        assert_eq!(config.rules.max_lines, 500);
        assert!(!config.rules.require_verification);
        assert!(!config.rules.require_examples);
    }
}
