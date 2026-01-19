---
layout: doc
title: Commands
---

# Commands Reference 📖

Complete CLI reference for pave.

## pave init

Initialize pave in your project.

```bash
pave init
```

**What it does:**
- Creates `.pave.toml` config file
- Sets up default `docs/` directory
- Configures sensible defaults for rules

---

## pave new

Scaffold a new document from templates.

```bash
pave new <type> <name>
```

**Arguments:**

| Argument | Description |
|----------|-------------|
| `type` | Document type: `component`, `runbook`, or `adr` |
| `name` | Name for the document (kebab-case recommended) |

**Examples:**

```bash
# Create a component doc
pave new component auth-service

# Create a runbook
pave new runbook deploy-production

# Create an ADR
pave new adr use-rust-for-cli
```

**Output locations:**
- Components: `docs/components/<name>.md`
- Runbooks: `docs/runbooks/<name>.md`
- ADRs: `docs/adrs/<name>.md`

---

## pave prompt

Generate AI agent prompts for documentation tasks.

```bash
pave prompt <type> [options]
```

**Arguments:**

| Argument | Description |
|----------|-------------|
| `type` | Prompt type: `create` or `update` |

**Options:**

| Option | Description |
|--------|-------------|
| `--context <file>` | Include file content as context |
| `--json` | Output in JSON format |

**Examples:**

```bash
# Generate a prompt for creating docs
pave prompt create

# Generate with context
pave prompt update --context src/auth.rs

# JSON output for programmatic use
pave prompt create --json
```

---

## pave index

Generate a documentation index.

```bash
pave index [options]
```

**Options:**

| Option | Description |
|--------|-------------|
| `--output <path>` | Output file path (default: `docs/index.md`) |
| `--update` | Preserve custom content sections |

**Examples:**

```bash
# Generate index with defaults
pave index

# Custom output location
pave index --output docs/README.md

# Update existing index, keeping custom notes
pave index --update
```

**What it does:**
- Scans `docs/` directory recursively
- Extracts titles and purpose summaries
- Categorizes by document type
- Generates Quick Links and sections
- Preserves custom content between markers

---

## pave check

Validate documentation against rules.

```bash
pave check [path]
```

**Arguments:**

| Argument | Description |
|----------|-------------|
| `path` | Optional path to check (default: `docs/`) |

**Rules enforced:**
- `max_lines`: Maximum lines per document (default: 300)
- `require_verification`: Must have Verification section
- `require_examples`: Must have Examples section

**Examples:**

```bash
# Check all docs
pave check

# Check specific file
pave check docs/components/auth.md
```

---

## pave config

Manage pave configuration.

```bash
pave config <subcommand>
```

**Subcommands:**

| Subcommand | Description |
|------------|-------------|
| `get <key>` | Get a config value |
| `set <key> <value>` | Set a config value |
| `list` | Show all configuration |
| `path` | Show config file path |

**Examples:**

```bash
# View all config
pave config list

# Get a specific value
pave config get rules.max_lines

# Set a value
pave config set rules.max_lines 500

# Find config file
pave config path
```

**Config keys:**

| Key | Description | Default |
|-----|-------------|---------|
| `pave.version` | Config schema version | `"0.1"` |
| `docs.root` | Documentation root directory | `"docs"` |
| `docs.templates` | Custom templates path | `"templates"` |
| `rules.max_lines` | Max lines per document | `300` |
| `rules.require_verification` | Require Verification section | `true` |
| `rules.require_examples` | Require Examples section | `true` |
