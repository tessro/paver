# Paver CLI

## Purpose

Paver is a command-line tool for creating, validating, and managing PAVED documentation. It enforces structured documentation practices optimized for AI agents by providing templates, validation rules, and indexing capabilities.

**Non-goals:**
- Not a static site generator (use separate tools like MkDocs or VitePress for that)
- Not a prose linter (doesn't check grammar or style)
- Not a documentation search engine (generates indexes, doesn't serve them)

## Interface

| Command | Description |
|---------|-------------|
| `paver init` | Initialize project with `.paver.toml` config and docs directory |
| `paver new <type> <name>` | Scaffold a new document from template |
| `paver check [path]` | Validate documentation against PAVED rules |
| `paver index` | Generate documentation index |
| `paver prompt <type>` | Generate AI prompts for documentation tasks |
| `paver config <subcommand>` | View or modify configuration |
| `paver hooks <subcommand>` | Manage git hooks for validation |

### Command Details

**paver init**
```bash
paver init [--docs-root <path>] [--hooks] [--force]
```
- `--docs-root`: Set docs directory (default: `docs`)
- `--hooks`: Also install git pre-commit hook
- `--force`: Overwrite existing files

**paver new**
```bash
paver new <type> <name> [--output <path>]
```
- `type`: `component`, `runbook`, or `adr`
- `name`: Document name (kebab-case recommended)
- `--output`: Custom output path

**paver check**
```bash
paver check [paths...] [--format <format>] [--strict]
```
- `paths`: Files or directories to check (default: docs root)
- `--format`: Output format (`text`, `json`, `github`)
- `--strict`: Treat warnings as errors

**paver index**
```bash
paver index [--output <path>] [--update]
```
- `--output`: Output file path (default: `docs/index.md`)
- `--update`: Preserve custom content sections when regenerating

**paver prompt**
```bash
paver prompt <type> [--for <name>] [--update <path>] [--context <file>] [--output <format>]
```
- `type`: `component`, `runbook`, or `adr`
- `--for`: Name of the thing being documented
- `--update`: Generate prompt to update existing doc at path
- `--context`: Include file content as context (repeatable)
- `--output`: Output format (`text` or `json`)

**paver config**
```bash
paver config get <key>
paver config set <key> <value>
paver config list
paver config path
```
- `get`: Retrieve a config value by key
- `set`: Update a config value
- `list`: Show all configuration
- `path`: Show config file path

**paver hooks**
```bash
paver hooks install [--hook <type>] [--force]
paver hooks uninstall [--hook <type>]
```
- `--hook`: `pre-commit` (default) or `pre-push`
- `--force`: Overwrite existing hooks

## Configuration

Configuration is stored in `.paver.toml` at the project root.

| Key | Description | Default |
|-----|-------------|---------|
| `paver.version` | Config schema version | `"0.1"` |
| `docs.root` | Documentation root directory | `"docs"` |
| `docs.templates` | Custom templates path | `"templates"` |
| `rules.max_lines` | Max lines per document | `300` |
| `rules.require_verification` | Require Verification section | `true` |
| `rules.require_examples` | Require Examples section | `true` |

Example `.paver.toml`:
```toml
[paver]
version = "0.1"

[docs]
root = "docs"
templates = "templates"

[rules]
max_lines = 300
require_verification = true
require_examples = true
```

## Verification

Confirm paver is installed and working:

```bash
# Check version
paver --version
# Expected: paver 0.1.0

# Validate documentation
paver check docs/
# Expected: Lists validation results, exits 0 if all pass

# Show configuration
paver config list
# Expected: Displays current configuration values
```

## Examples

### Initialize a new project
```bash
# Basic initialization
paver init

# With git hooks
paver init --hooks

# Custom docs location
paver init --docs-root documentation
```

### Create documentation
```bash
# Create a component doc
paver new component auth-service
# Creates: docs/components/auth-service.md

# Create a runbook
paver new runbook deploy-production
# Creates: docs/runbooks/deploy-production.md

# Create an ADR
paver new adr use-postgres
# Creates: docs/adrs/use-postgres.md
```

### Validate documentation
```bash
# Check all docs
paver check

# Check specific file
paver check docs/components/auth-service.md

# Check with strict mode (warnings become errors)
paver check --strict

# JSON output for CI
paver check --format json
```

### Generate index
```bash
# Generate new index
paver index

# Update existing index (preserves custom content)
paver index --update

# Custom output path
paver index --output docs/README.md
```

## Gotchas

- **Config not found**: Paver looks for `.paver.toml` in the current directory and parent directories. Run `paver init` to create one, or use `paver config path` to see where it's looking.
- **Template not found**: Custom templates must be in the path specified by `docs.templates`. The built-in templates are used if no custom templates exist.
- **Hook conflicts**: If a git hook already exists and wasn't installed by paver, use `--force` to overwrite or manually merge the hooks.
- **Max lines exceeded**: Documents over `rules.max_lines` (default 300) fail validation. Split large docs into smaller, focused documents.
- **Missing sections**: By default, `Verification` and `Examples` sections are required. Disable with `rules.require_verification = false` or `rules.require_examples = false` if needed.

## Decisions

**Why Rust?** Paver is written in Rust for fast startup time and easy distribution as a single binary. This matters for git hooks where slow tools delay commits.

**Why TOML for config?** TOML is human-readable, widely supported, and matches Cargo's config format (familiar to Rust users). It's also simple enough for agents to reliably edit.

**Why built-in templates?** Embedded templates ensure paver works out of the box. Custom templates can override them when teams need project-specific formats.

**Why strict section requirements?** The `Verification` and `Examples` requirements enforce documentation quality. Docs without these sections are less useful for both humans and AI agents.
