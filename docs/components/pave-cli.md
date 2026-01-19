# Pave CLI

## Purpose

Pave is a command-line tool for creating, validating, and managing PAVED documentation. It enforces structured documentation practices optimized for AI agents by providing templates, validation rules, and indexing capabilities.

**Non-goals:**
- Not a static site generator (use separate tools like MkDocs or VitePress for that)
- Not a prose linter (doesn't check grammar or style)
- Not a documentation search engine (generates indexes, doesn't serve them)

## Interface

| Command | Description |
|---------|-------------|
| `pave init` | Initialize project with `.pave.toml` config and docs directory |
| `pave new <type> <name>` | Scaffold a new document from template |
| `pave check [path]` | Validate documentation against PAVED rules |
| `pave verify [path]` | Run verification commands from documentation |
| `pave index` | Generate documentation index |
| `pave prompt <type>` | Generate AI prompts for documentation tasks |
| `pave changed` | Show docs impacted by code changes |
| `pave config <subcommand>` | View or modify configuration |
| `pave hooks <subcommand>` | Manage git hooks for validation |

### Command Details

**pave init**
```bash
pave init [--docs-root <path>] [--hooks] [--force]
```
- `--docs-root`: Set docs directory (default: `docs`)
- `--hooks`: Also install git pre-commit hook
- `--force`: Overwrite existing files

**pave new**
```bash
pave new <type> <name> [--output <path>]
```
- `type`: `component`, `runbook`, or `adr`
- `name`: Document name (kebab-case recommended)
- `--output`: Custom output path

**pave check**
```bash
pave check [paths...] [--format <format>] [--strict]
```
- `paths`: Files or directories to check (default: docs root)
- `--format`: Output format (`text`, `json`, `github`)
- `--strict`: Treat warnings as errors

**pave index**
```bash
pave index [--output <path>] [--update]
```
- `--output`: Output file path (default: `docs/index.md`)
- `--update`: Preserve custom content sections when regenerating

**pave prompt**
```bash
pave prompt <type> [--for <name>] [--update <path>] [--context <file>] [--output <format>]
```
- `type`: `component`, `runbook`, or `adr`
- `--for`: Name of the thing being documented
- `--update`: Generate prompt to update existing doc at path
- `--context`: Include file content as context (repeatable)
- `--output`: Output format (`text` or `json`)

**pave config**
```bash
pave config get <key>
pave config set <key> <value>
pave config list
pave config path
```
- `get`: Retrieve a config value by key
- `set`: Update a config value
- `list`: Show all configuration
- `path`: Show config file path

**pave changed**
```bash
pave changed [--base <ref>] [--format <format>] [--strict]
```
- `--base`: Git ref to compare against (default: `origin/main`, `origin/master`, or `HEAD~1`)
- `--format`: Output format (`text` or `json`)
- `--strict`: Fail if impacted docs weren't updated

**pave verify**
```bash
pave verify [paths...] [--format <format>] [--timeout <seconds>] [--keep-going] [--report <path>]
```
- `paths`: Files or directories to verify (default: docs root)
- `--format`: Output format (`text`, `json`, or `github`)
- `--timeout`: Timeout per command in seconds (default: 30)
- `--keep-going`: Continue running after first failure
- `--report`: Write JSON report to file

**pave hooks**
```bash
pave hooks install [--hook <type>] [--force]
pave hooks uninstall [--hook <type>]
```
- `--hook`: `pre-commit` (default) or `pre-push`
- `--force`: Overwrite existing hooks

## Configuration

Configuration is stored in `.pave.toml` at the project root.

| Key | Description | Default |
|-----|-------------|---------|
| `pave.version` | Config schema version | `"0.1"` |
| `docs.root` | Documentation root directory | `"docs"` |
| `docs.templates` | Custom templates path | `"templates"` |
| `rules.max_lines` | Max lines per document | `300` |
| `rules.require_verification` | Require Verification section | `true` |
| `rules.require_examples` | Require Examples section | `true` |

Example `.pave.toml`:
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
```

## Verification

Confirm pave is built and working:

```bash
cargo build --release && ./target/release/pave --version
```

Validate documentation:

```bash
./target/release/pave check docs/
```

Show configuration:

```bash
./target/release/pave config list
```

## Examples

### Initialize a new project
```bash
# Basic initialization
pave init

# With git hooks
pave init --hooks

# Custom docs location
pave init --docs-root documentation
```

### Create documentation
```bash
# Create a component doc
pave new component auth-service
# Creates: docs/components/auth-service.md

# Create a runbook
pave new runbook deploy-production
# Creates: docs/runbooks/deploy-production.md

# Create an ADR
pave new adr use-postgres
# Creates: docs/adrs/use-postgres.md
```

### Validate documentation
```bash
# Check all docs
pave check

# Check specific file
pave check docs/components/auth-service.md

# Check with strict mode (warnings become errors)
pave check --strict

# JSON output for CI
pave check --format json
```

### Generate index
```bash
# Generate new index
pave index

# Update existing index (preserves custom content)
pave index --update

# Custom output path
pave index --output docs/README.md
```

### Check impacted docs
```bash
# Show docs impacted by code changes since origin/main
pave changed

# Compare against a specific branch or commit
pave changed --base feature-branch

# JSON output for CI integration
pave changed --format json

# Fail in CI if impacted docs weren't updated
pave changed --strict
```

### Run verification commands
```bash
# Run all verification commands from all docs
pave verify

# Verify a specific document
pave verify docs/components/auth-service.md

# Continue running after failures
pave verify --keep-going

# Write JSON report for CI
pave verify --format json --report verify-results.json

# GitHub Actions annotations
pave verify --format github
```

## Gotchas

- **Config not found**: Pave looks for `.pave.toml` in the current directory and parent directories. Run `pave init` to create one, or use `pave config path` to see where it's looking.
- **Template not found**: Custom templates must be in the path specified by `docs.templates`. The built-in templates are used if no custom templates exist.
- **Hook conflicts**: If a git hook already exists and wasn't installed by pave, use `--force` to overwrite or manually merge the hooks.
- **Max lines exceeded**: Documents over `rules.max_lines` (default 300) fail validation. Split large docs into smaller, focused documents.
- **Missing sections**: By default, `Verification` and `Examples` sections are required. Disable with `rules.require_verification = false` or `rules.require_examples = false` if needed.

## Decisions

**Why Rust?** Pave is written in Rust for fast startup time and easy distribution as a single binary. This matters for git hooks where slow tools delay commits.

**Why TOML for config?** TOML is human-readable, widely supported, and matches Cargo's config format (familiar to Rust users). It's also simple enough for agents to reliably edit.

**Why built-in templates?** Embedded templates ensure pave works out of the box. Custom templates can override them when teams need project-specific formats.

**Why strict section requirements?** The `Verification` and `Examples` requirements enforce documentation quality. Docs without these sections are less useful for both humans and AI agents.

## Paths

- `src/cli.rs`
- `src/main.rs`
- `src/commands/*.rs`
- `src/verification.rs`
