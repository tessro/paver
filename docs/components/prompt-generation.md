# Prompt Generation

## Purpose

The prompt generation system creates structured prompts for AI agents to produce PAVED-compliant documentation. It packages the PAVED framework requirements, project-specific rules, templates, and optional context into a single prompt that guides AI assistants to create documentation that passes `pave check`.

**Non-goals:**
- Not an AI agent itself (it generates prompts, doesn't execute them)
- Not a documentation generator (it produces prompts, the AI produces documentation)
- Doesn't validate the AI's output (use `pave check` for that)

## Interface

### CLI Usage

```bash
pave prompt <doc_type> [options]
```

| Argument | Description |
|----------|-------------|
| `doc_type` | Document type: `component`, `runbook`, or `adr` |

### CLI Options

| Option | Description |
|--------|-------------|
| `--for <name>` | Name of the thing being documented |
| `--update <path>` | Path to existing document to update |
| `--context <path>` | Include file as context (can be repeated) |
| `--output <format>` | Output format: `text` (default) or `json` |

### Output Formats

**Text (default)** - Ready to paste into an AI chat:
```
You are documenting a software component using the PAVED framework.

## PAVED Structure
Your document MUST include these sections:
- **Purpose**: What is this? What problem does it solve?
...

## Task
Create a PAVED component document for: auth service
```

**JSON** - Structured output for programmatic use:
```json
{
  "prompt": "You are documenting...",
  "template": "# {Component Name}\n\n## Purpose...",
  "rules": ["Maximum 300 lines per document", "Verification section must include runnable commands", "Examples must include expected output"],
  "context_files": ["src/auth.rs"]
}
```

### Generated Prompt Structure

The generated prompt includes:
1. **PAVED Structure** - Required sections for the document type
2. **Project Rules** - Configured limits and requirements from `.pave.toml`
3. **Template** - Starting structure from pave's built-in templates
4. **Context** (optional) - Existing document content or source files
5. **Task** - Clear instruction on what to create or update

## Configuration

Prompt generation uses the standard `.pave.toml` configuration to read project rules:

```toml
[rules]
max_lines = 300              # Included in prompt as line limit
require_verification = true  # Tells AI to include runnable commands
require_examples = true      # Tells AI to include output examples
```

No additional configuration is required. If `.pave.toml` doesn't exist, default rules are used.

## Verification

Generate a prompt for a new component:

```bash
./target/release/pave prompt component --for "auth service"
```

Verify the prompt includes required sections:

```bash
./target/release/pave prompt component --for "test" | grep -q "## PAVED Structure"
```

Generate JSON output and verify structure:

```bash
./target/release/pave prompt component --for "test" --output json | jq -e '.prompt and .template and .rules'
```

Run the unit tests:

```bash
cargo test prompt
```

## Examples

### Generate Prompt for New Component

```bash
pave prompt component --for "user authentication"
```

Output includes PAVED sections, project rules, template, and task instruction.

### Generate Prompt to Update Existing Doc

```bash
pave prompt component --update docs/components/auth.md --for "authentication"
```

The prompt includes the existing document content under "Context" so the AI can preserve existing content while making updates.

### Include Source Code as Context

```bash
pave prompt component --for "parser" --context src/parser.rs --context src/ast.rs
```

Source files are included in the prompt under "Context" sections, helping the AI understand the implementation.

### JSON Output for Automation

```bash
pave prompt component --for "api" --output json | jq '.prompt' > prompt.txt
```

Use JSON output to extract specific fields or integrate with scripts.

### Generate Runbook Prompt

```bash
pave prompt runbook --for "deploy to production"
```

Runbook prompts include different PAVED sections: When to Use, Preconditions, Steps, Rollback, Verification, and Escalation.

### Generate ADR Prompt

```bash
pave prompt adr --for "use PostgreSQL for storage"
```

ADR prompts include: Status, Context, Decision, Consequences, and Alternatives Considered.

## Gotchas

- **Context files must exist**: The command fails if `--context` paths don't exist. Verify file paths before running.
- **Large context may exceed token limits**: Including many or large source files can produce prompts that exceed AI context windows. Be selective about what context to include.
- **Update path must be readable**: When using `--update`, the file must exist and be readable.
- **Rules come from `.pave.toml`**: The prompt reflects your project's configured rules. Ensure `.pave.toml` is set up correctly for accurate prompts.
- **Template is embedded**: The prompt includes the full template, which may be verbose for simple tasks.

## Decisions

**Why generate prompts instead of documentation?** AI capabilities vary and improve over time. Generating prompts lets users choose their preferred AI tool and model, rather than coupling pave to a specific AI service.

**Why include templates in prompts?** Templates provide concrete structure that guides AI output format. Without them, AI assistants often produce inconsistent section structures.

**Why JSON output?** Enables integration with scripts, CI pipelines, and custom tooling that may want to extract specific parts of the prompt or add custom processing.

**Why read context files at generation time?** Including actual file content ensures the AI has accurate, current information rather than stale references. It also works offline without requiring API calls.

## Paths

- `src/commands/prompt.rs`
