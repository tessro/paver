---
layout: doc
title: Commands
---

# Commands Reference 📖

Complete CLI reference for paver.

## paver init

Initialize paver in your project.

```bash
paver init
```

**What it does:**
- Creates `.paver.toml` config file
- Sets up default `docs/` directory
- Configures sensible defaults for rules

---

## paver new

Scaffold a new document from templates.

```bash
paver new <type> <name>
```

**Arguments:**

| Argument | Description |
|----------|-------------|
| `type` | Document type: `component`, `runbook`, or `adr` |
| `name` | Name for the document (kebab-case recommended) |

**Examples:**

```bash
# Create a component doc
paver new component auth-service

# Create a runbook
paver new runbook deploy-production

# Create an ADR
paver new adr use-rust-for-cli
```

**Output locations:**
- Components: `docs/components/<name>.md`
- Runbooks: `docs/runbooks/<name>.md`
- ADRs: `docs/adrs/<name>.md`

---

## paver prompt

Generate AI agent prompts for documentation tasks.

```bash
paver prompt <type> [options]
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
paver prompt create

# Generate with context
paver prompt update --context src/auth.rs

# JSON output for programmatic use
paver prompt create --json
```

---

## paver index

Generate a documentation index.

```bash
paver index [options]
```

**Options:**

| Option | Description |
|--------|-------------|
| `--output <path>` | Output file path (default: `docs/index.md`) |
| `--update` | Preserve custom content sections |

**Examples:**

```bash
# Generate index with defaults
paver index

# Custom output location
paver index --output docs/README.md

# Update existing index, keeping custom notes
paver index --update
```

**What it does:**
- Scans `docs/` directory recursively
- Extracts titles and purpose summaries
- Categorizes by document type
- Generates Quick Links and sections
- Preserves custom content between markers

---

## paver check

Validate documentation against rules.

```bash
paver check [path]
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
paver check

# Check specific file
paver check docs/components/auth.md
```

---

## paver config

Manage paver configuration.

```bash
paver config <subcommand>
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
paver config list

# Get a specific value
paver config get rules.max_lines

# Set a value
paver config set rules.max_lines 500

# Find config file
paver config path
```

**Config keys:**

| Key | Description | Default |
|-----|-------------|---------|
| `paver.version` | Config schema version | `"0.1"` |
| `docs.root` | Documentation root directory | `"docs"` |
| `docs.templates` | Custom templates path | `"templates"` |
| `rules.max_lines` | Max lines per document | `300` |
| `rules.require_verification` | Require Verification section | `true` |
| `rules.require_examples` | Require Examples section | `true` |
