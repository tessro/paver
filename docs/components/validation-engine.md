# Validation Engine

## Purpose

The validation engine parses markdown files into a structured form and validates them against configurable rules from the PAVED framework. It enforces documentation quality by checking for required sections, code blocks, and document length limits.

Non-goals: This is not a general-purpose markdown parser. It does not check prose quality, grammar, or spelling.

## Interface

### Entry Point

The validation engine is invoked through the `pave check` command:

```bash
pave check [PATH...]
```

| Argument | Description |
|----------|-------------|
| `PATH` | Files or directories to check (defaults to docs root from config) |

### CLI Options

| Flag | Description |
|------|-------------|
| `--format <FORMAT>` | Output format: `text` (default), `json`, or `github` |
| `--strict` | Treat warnings as errors (exit non-zero if any warnings) |

### Exit Codes

| Code | Meaning |
|------|---------|
| 0 | All checks passed |
| 1 | Validation errors found |

### Output Formats

**Text (default)** - Human-readable with line numbers and hints:
```
docs/file.md:5: error: missing required section: Verification
  hint: add a '## Verification' section to the document
```

**JSON** - Structured output for programmatic parsing:
```json
{
  "files_checked": 1,
  "errors": [{"file": "docs/file.md", "line": 1, "message": "..."}],
  "warnings": []
}
```

**GitHub** - CI/CD annotations for GitHub Actions workflows:
```
::error file=docs/file.md,line=1::missing required section: Verification
```

## Configuration

Rules are configured in `.pave.toml` under the `[rules]` section:

```toml
[rules]
max_lines = 300              # Maximum lines per document
require_verification = true  # Require Verification section
require_examples = true      # Require Examples section with code blocks
```

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `max_lines` | integer | 300 | Maximum allowed lines per document |
| `require_verification` | boolean | true | Require a `## Verification` section |
| `require_examples` | boolean | true | Require a `## Examples` section with code blocks |

### Rules Applied

The engine always enforces:
- **Purpose section** - Every document must have a `## Purpose` section

When `require_verification = true`:
- **Verification section** - Document must have a `## Verification` section

When `require_examples = true`:
- **Examples section** - Document must have a `## Examples` section
- **Code blocks in Examples** - The Examples section must contain at least one code block

The **max_lines** rule produces an error if the document exceeds the configured limit.

## Verification

Test validation with a known-good document:

```bash
./target/release/pave check docs/components/validation-engine.md
```

Test that validation catches errors:

```bash
echo "# No sections" > /tmp/bad.md && ./target/release/pave check /tmp/bad.md; rm /tmp/bad.md
```

Run the unit tests:

```bash
cargo test parser && cargo test rules
```

## Examples

### Valid Document

A minimal document that passes validation:

````markdown
# My Component

## Purpose
This component handles user authentication.

## Verification
Run the tests:
$ cargo test

## Examples
Basic usage:
```rust
let auth = Auth::new();
```
````

### Invalid Document

A document missing required sections will fail:

````markdown
# Missing Sections

Just some text without proper sections.
````

Error output:
```
missing-sections.md:1: error: missing required section: Purpose
  hint: add a '## Purpose' section to the document
```

### Using JSON Output for CI

```bash
pave check --format json | jq '.errors | length'
```

## Gotchas

- **Section headings are case-insensitive**: `## Purpose`, `## PURPOSE`, and `## purpose` are all valid
- **Code blocks require triple backticks**: Indented code blocks are not detected, only fenced code blocks using ` ``` `
- **H3+ headings are not tracked**: Only H2 (`##`) headings are recognized as sections
- **Commands are detected heuristically**: The engine looks for shell prompts (`$`) or common command prefixes (`cargo`, `make`, `npm`, etc.)

## Decisions

**Why require specific sections?** The PAVED framework (Purpose, API, Verification, Examples, Decisions) provides a consistent structure that AI agents can reliably parse and execute. Required sections ensure documentation is actionable, not just descriptive.

**Why limit document length?** Long documents are harder for agents to process and often indicate the document should be split. The 300-line default encourages atomic, focused documentation.

**Why markdown over RST/AsciiDoc?** Markdown is the most widely supported format, requires no special tooling to read, and agents are well-trained on it.

**Why case-insensitive sections?** Reduces friction and validation failures from minor formatting differences while maintaining structural requirements.

## Paths

- `src/parser.rs`
- `src/rules.rs`
- `src/config.rs`
- `src/commands/check.rs`
