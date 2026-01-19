use anyhow::{Context, Result, anyhow};
use std::fs;
use std::path::{Path, PathBuf};
use toml::Value;

const CONFIG_FILENAME: &str = ".paver.toml";

/// Find the config file by walking up from current directory.
pub fn find_config_path() -> Result<PathBuf> {
    let cwd = std::env::current_dir().context("Failed to get current directory")?;
    find_config_path_from(&cwd)
}

fn find_config_path_from(start: &Path) -> Result<PathBuf> {
    let mut current = start.to_path_buf();
    loop {
        let config_path = current.join(CONFIG_FILENAME);
        if config_path.exists() {
            return Ok(config_path);
        }
        if !current.pop() {
            return Err(anyhow!(
                "No {} found in current directory or any parent directory",
                CONFIG_FILENAME
            ));
        }
    }
}

/// Load the config file as a TOML Value.
fn load_config(path: &Path) -> Result<Value> {
    let content = fs::read_to_string(path)
        .with_context(|| format!("Failed to read config file: {}", path.display()))?;
    let value: Value = content
        .parse()
        .with_context(|| format!("Failed to parse config file: {}", path.display()))?;
    Ok(value)
}

/// Save a TOML Value to the config file.
fn save_config(path: &Path, value: &Value) -> Result<()> {
    let content = toml::to_string_pretty(value).context("Failed to serialize config")?;
    fs::write(path, content)
        .with_context(|| format!("Failed to write config file: {}", path.display()))?;
    Ok(())
}

/// Get a value from the config using dot notation (e.g., "docs.root").
pub fn get(key: &str) -> Result<()> {
    let config_path = find_config_path()?;
    let config = load_config(&config_path)?;

    let value = get_nested_value(&config, key)?;
    println!("{}", format_value(value));
    Ok(())
}

/// Set a value in the config using dot notation.
pub fn set(key: &str, value: &str) -> Result<()> {
    let config_path = find_config_path()?;
    let mut config = load_config(&config_path)?;

    let parsed_value = parse_value(value);
    set_nested_value(&mut config, key, parsed_value)?;

    save_config(&config_path, &config)?;
    Ok(())
}

/// List all config values.
pub fn list() -> Result<()> {
    let config_path = find_config_path()?;
    let config = load_config(&config_path)?;

    print_config_values(&config, "");
    Ok(())
}

/// Print the path to the config file.
pub fn path() -> Result<()> {
    let config_path = find_config_path()?;
    println!("{}", config_path.display());
    Ok(())
}

/// Get a nested value using dot notation.
fn get_nested_value<'a>(value: &'a Value, key: &str) -> Result<&'a Value> {
    let parts: Vec<&str> = key.split('.').collect();
    let mut current = value;

    for part in &parts {
        current = match current {
            Value::Table(table) => table
                .get(*part)
                .ok_or_else(|| anyhow!("Key '{}' not found in config", key))?,
            _ => return Err(anyhow!("Key '{}' not found in config", key)),
        };
    }

    Ok(current)
}

/// Set a nested value using dot notation.
fn set_nested_value(value: &mut Value, key: &str, new_value: Value) -> Result<()> {
    let parts: Vec<&str> = key.split('.').collect();

    if parts.is_empty() {
        return Err(anyhow!("Empty key is not allowed"));
    }

    let mut current = value;

    // Navigate to the parent, creating tables as needed
    for part in &parts[..parts.len() - 1] {
        current = match current {
            Value::Table(table) => {
                if !table.contains_key(*part) {
                    table.insert(part.to_string(), Value::Table(toml::map::Map::new()));
                }
                table.get_mut(*part).unwrap()
            }
            _ => return Err(anyhow!("Cannot set nested key: parent is not a table")),
        };
    }

    // Set the final value
    let last_key = parts.last().unwrap();
    match current {
        Value::Table(table) => {
            table.insert(last_key.to_string(), new_value);
            Ok(())
        }
        _ => Err(anyhow!("Cannot set key: parent is not a table")),
    }
}

/// Parse a string value into an appropriate TOML Value.
fn parse_value(s: &str) -> Value {
    // Try to parse as integer
    if let Ok(i) = s.parse::<i64>() {
        return Value::Integer(i);
    }

    // Try to parse as float
    if let Ok(f) = s.parse::<f64>() {
        return Value::Float(f);
    }

    // Try to parse as boolean
    match s.to_lowercase().as_str() {
        "true" => return Value::Boolean(true),
        "false" => return Value::Boolean(false),
        _ => {}
    }

    // Default to string
    Value::String(s.to_string())
}

/// Format a TOML Value for display.
fn format_value(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        Value::Integer(i) => i.to_string(),
        Value::Float(f) => f.to_string(),
        Value::Boolean(b) => b.to_string(),
        Value::Array(arr) => {
            let items: Vec<String> = arr.iter().map(format_value).collect();
            format!("[{}]", items.join(", "))
        }
        Value::Table(table) => {
            let items: Vec<String> = table
                .iter()
                .map(|(k, v)| format!("{} = {}", k, format_value_quoted(v)))
                .collect();
            format!("{{{}}}", items.join(", "))
        }
        Value::Datetime(dt) => dt.to_string(),
    }
}

/// Format a TOML Value with quotes for strings.
fn format_value_quoted(value: &Value) -> String {
    match value {
        Value::String(s) => format!("\"{}\"", s),
        _ => format_value(value),
    }
}

/// Recursively print all config values with their full key paths.
fn print_config_values(value: &Value, prefix: &str) {
    if let Value::Table(table) = value {
        for (key, val) in table {
            let full_key = if prefix.is_empty() {
                key.clone()
            } else {
                format!("{}.{}", prefix, key)
            };

            match val {
                Value::Table(_) => {
                    print_config_values(val, &full_key);
                }
                _ => {
                    println!("{} = {}", full_key, format_value_quoted(val));
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_nested_value() {
        let config: Value = r#"
            [docs]
            root = "docs"

            [rules]
            max_lines = 300
            require_verification = true
        "#
        .parse()
        .unwrap();

        assert_eq!(
            get_nested_value(&config, "docs.root").unwrap(),
            &Value::String("docs".to_string())
        );
        assert_eq!(
            get_nested_value(&config, "rules.max_lines").unwrap(),
            &Value::Integer(300)
        );
        assert_eq!(
            get_nested_value(&config, "rules.require_verification").unwrap(),
            &Value::Boolean(true)
        );
    }

    #[test]
    fn test_get_missing_key() {
        let config: Value = r#"
            [docs]
            root = "docs"
        "#
        .parse()
        .unwrap();

        let result = get_nested_value(&config, "docs.missing");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not found"));
    }

    #[test]
    fn test_set_nested_value() {
        let mut config: Value = r#"
            [docs]
            root = "docs"
        "#
        .parse()
        .unwrap();

        set_nested_value(
            &mut config,
            "docs.root",
            Value::String("documentation".to_string()),
        )
        .unwrap();
        assert_eq!(
            get_nested_value(&config, "docs.root").unwrap(),
            &Value::String("documentation".to_string())
        );
    }

    #[test]
    fn test_set_creates_intermediate_tables() {
        let mut config: Value = r#"
            [docs]
            root = "docs"
        "#
        .parse()
        .unwrap();

        set_nested_value(&mut config, "rules.max_lines", Value::Integer(500)).unwrap();
        assert_eq!(
            get_nested_value(&config, "rules.max_lines").unwrap(),
            &Value::Integer(500)
        );
    }

    #[test]
    fn test_parse_value_integer() {
        assert_eq!(parse_value("42"), Value::Integer(42));
        assert_eq!(parse_value("-10"), Value::Integer(-10));
    }

    #[test]
    fn test_parse_value_float() {
        assert_eq!(parse_value("1.23"), Value::Float(1.23));
    }

    #[test]
    fn test_parse_value_boolean() {
        assert_eq!(parse_value("true"), Value::Boolean(true));
        assert_eq!(parse_value("false"), Value::Boolean(false));
        assert_eq!(parse_value("TRUE"), Value::Boolean(true));
        assert_eq!(parse_value("False"), Value::Boolean(false));
    }

    #[test]
    fn test_parse_value_string() {
        assert_eq!(parse_value("hello"), Value::String("hello".to_string()));
        assert_eq!(parse_value("docs"), Value::String("docs".to_string()));
    }

    #[test]
    fn test_format_value() {
        assert_eq!(format_value(&Value::String("test".to_string())), "test");
        assert_eq!(format_value(&Value::Integer(42)), "42");
        assert_eq!(format_value(&Value::Boolean(true)), "true");
    }
}
