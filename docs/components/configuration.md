# Configuration

## Purpose

The configuration system manages pave's `.pave.toml` file, which controls documentation paths, validation rules, and template settings. It provides a consistent way to customize pave's behavior per-project.

**Non-goals:**
- Not a global user configuration (each project has its own `.pave.toml`)
- Not environment variable overrides (all config is in the TOML file)
- Not configuration inheritance (no layered or merged configs)

## Interface

### Config File Location

Pave discovers configuration by searching for `.pave.toml` starting from the current directory and walking up to parent directories. The first file found is used.

```
project/
├── .pave.toml      <- Found here
├── src/
│   └── main.rs
└── docs/
    └── components/
        └── foo.md   <- Running from here still finds root config
```

### PaveConfig Structure

The configuration is divided into sections:

| Section | Required | Description |
|---------|----------|-------------|
| `[pave]` | Yes | Tool metadata (version) |
| `[docs]` | Yes | Documentation paths |
| `[rules]` | No | Validation rules |
| `[templates]` | No | Template file mappings |
| `[mapping]` | No | Code-to-doc mapping settings |
| `[hooks]` | No | Git hooks configuration |

### CLI Commands

```bash
pave config get <key>      # Get a config value by dot notation
pave config set <key> <value>  # Set a config value
pave config list           # Show all configuration values
pave config path           # Show path to config file
```

## Configuration

### [pave] Section

| Key | Type | Required | Default | Description |
|-----|------|----------|---------|-------------|
| `version` | string | Yes | `"0.1"` | Configuration schema version |

### [docs] Section

| Key | Type | Required | Default | Description |
|-----|------|----------|---------|-------------|
| `root` | path | Yes | `"docs"` | Root directory for documentation |
| `templates` | path | No | None | Directory where custom templates are stored |

### [rules] Section

| Key | Type | Required | Default | Description |
|-----|------|----------|---------|-------------|
| `max_lines` | integer | No | `300` | Maximum lines per document |
| `require_verification` | boolean | No | `true` | Require Verification section in documents |
| `require_examples` | boolean | No | `true` | Require Examples section in documents |
| `strict_output_matching` | boolean | No | `false` | Fail verification if output doesn't match expected patterns |

### [templates] Section

| Key | Type | Required | Default | Description |
|-----|------|----------|---------|-------------|
| `component` | string | No | None | Filename for component template |
| `runbook` | string | No | None | Filename for runbook template |
| `adr` | string | No | None | Filename for ADR template |

When set, pave looks for templates at `{docs.templates}/{templates.<type>}`. If not found, built-in templates are used.

### [mapping] Section

| Key | Type | Required | Default | Description |
|-----|------|----------|---------|-------------|
| `exclude` | string[] | No | `[]` | Glob patterns to exclude from code-to-doc mapping |

Exclude patterns support glob syntax:
- `target/` - excludes the target directory
- `*.generated.rs` - excludes generated Rust files
- `node_modules/` - excludes node_modules

### [hooks] Section

| Key | Type | Required | Default | Description |
|-----|------|----------|---------|-------------|
| `run_verify` | boolean | No | `false` | Run `pave verify` in git hooks |

## Verification

Verify configuration is loaded correctly:

```bash
./target/release/pave config path
```

Check that configuration is used by other commands:

```bash
./target/release/pave check docs/index.md
```

## Examples

### Minimal Configuration

The minimum viable `.pave.toml`:

```toml
[pave]
version = "0.1"

[docs]
root = "docs"
```

### Full Configuration

A complete configuration with all options:

```toml
[pave]
version = "0.1"

[docs]
root = "docs"
templates = "templates"

[rules]
max_lines = 300
require_verification = true
require_examples = true
strict_output_matching = false

[templates]
component = "component.md"
runbook = "runbook.md"
adr = "adr.md"

[mapping]
exclude = ["target/", "node_modules/", "*.generated.rs"]

[hooks]
run_verify = true
```

### Custom Docs Location

For projects with non-standard documentation paths:

```toml
[pave]
version = "0.1"

[docs]
root = "documentation"
templates = "documentation/templates"
```

### Relaxed Validation Rules

For projects that don't need strict validation:

```toml
[pave]
version = "0.1"

[docs]
root = "docs"

[rules]
max_lines = 500
require_verification = false
require_examples = false
```

### Using Config Commands

```bash
# Show where config file is located
$ pave config path
/path/to/project/.pave.toml

# List all configuration
$ pave config list
docs.root = "docs"
docs.templates = "templates"
pave.version = "0.1"
rules.max_lines = 300
rules.require_examples = true
rules.require_verification = true

# Get a specific value
$ pave config get rules.max_lines
300

# Change a value
$ pave config set rules.max_lines 500
```

## Gotchas

- **Config not found**: Pave searches from the current directory up to the filesystem root. If no `.pave.toml` is found, commands fail with an error. Run `pave init` to create one.
- **Empty values rejected**: `pave.version` and `docs.root` cannot be empty strings. Validation fails if they are.
- **Zero max_lines invalid**: `rules.max_lines` must be greater than 0.
- **Template path is relative**: `docs.templates` is relative to the project root, not to `docs.root`.
- **Dot notation for nested keys**: Use `docs.root` not `[docs] root` when using `pave config get/set`.
- **Type coercion**: `pave config set` auto-detects types. `"300"` becomes integer `300`, `"true"` becomes boolean `true`. Quote strings if needed.

## Decisions

**Why TOML?** TOML is human-readable, widely supported, and matches Cargo's config format. It's simple enough for both humans and AI agents to reliably edit.

**Why no environment variable overrides?** Simplicity. All configuration is in one place, making it easier to understand and debug. Projects needing env-based config can use wrapper scripts.

**Why no config inheritance?** Each project should be self-contained. Config inheritance creates implicit dependencies that are hard to reason about, especially for AI agents.

**Why strict defaults for sections?** `require_verification` and `require_examples` default to `true` because these sections are essential for useful documentation. Projects can opt out explicitly.

**Why per-project config?** Documentation standards vary by project. A monorepo might have different rules for different packages. Global config would be too inflexible.

## Paths

- `src/config.rs`
- `src/commands/config.rs`
