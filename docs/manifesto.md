# PAVED Framework

## Purpose

PAVED is a documentation framework optimized for human+agent pairs doing software engineering work. It structures documentation as precise interfaces rather than prose, making docs easy for agents to author, maintain, and verify.

**Non-goals:**
- Replacing narrative documentation entirely
- Requiring every file to follow PAVED format
- Supporting documentation outside software engineering contexts

## Interface

The PAVED acronym defines five required elements for agent-native documentation:

| Section | Purpose | Key Content |
|---------|---------|-------------|
| **P**urpose | What is this? What problem does it solve? | 1-3 sentences + non-goals |
| **A**PI/Interface | How do you use it? | Commands, endpoints, schemas, config keys |
| **V**erification | How do you know it's working? | Test commands, health checks, expected outputs |
| **E**xamples | Concrete, copy-paste usage | Happy path, realistic case, failure case |
| **D**ecisions | Why this design? What must not change? | Rationale, constraints, tradeoffs |

### Document Types

PAVED supports three document types, each with specific sections:

**Component docs** (`component.md`):
- Purpose, Interface, Configuration, Verification, Examples, Gotchas, Decisions

**Runbooks** (`runbook_<task>.md`):
- When to use, Preconditions, Steps, Rollback, Verification, Escalation

**ADRs** (`adr_<title>.md`):
- Context, Decision, Consequences, Alternatives considered

### CLI Commands

| Command | Description |
|---------|-------------|
| `paver init` | Initialize PAVED documentation in a project |
| `paver new <type> <name>` | Create a new document from template |
| `paver check [paths]` | Validate documents against PAVED rules |
| `paver prompt <path>` | Generate context for AI agents |

## Configuration

Configuration lives in `.paver.toml` at the project root:

```toml
[paver]
version = "0.1"

[docs]
root = "docs"
templates = "docs/templates"

[rules]
max_lines = 300
require_verification = true
require_examples = true
```

| Key | Default | Description |
|-----|---------|-------------|
| `docs.root` | `docs` | Root directory for documentation |
| `docs.templates` | `docs/templates` | Template directory |
| `rules.max_lines` | `300` | Maximum lines per document |
| `rules.require_verification` | `true` | Require Verification section |
| `rules.require_examples` | `true` | Require Examples section |

## Verification

Validate documents pass PAVED requirements:

```bash
# Check all docs
paver check

# Check specific file
paver check docs/manifesto.md

# Check with strict mode (warnings become errors)
paver check --strict

# JSON output for CI integration
paver check --format json
```

Expected output for a valid document:
```
Checked 1 document: all checks passed
```

Common validation errors:
```
docs/example.md:1: error: Missing required section 'Verification'
  hint: Add a '## Verification' section with test commands
```

## Examples

### Creating a new component doc

```bash
paver new component auth-service
```

Creates `docs/auth-service.md`:

```markdown
# Auth Service

## Purpose
<!-- What is this? What problem does it solve? 1-3 sentences. -->

## Interface
<!-- How do you use it? Entry points, commands, schemas. -->

## Configuration
<!-- Config keys, environment variables, file formats. -->

## Verification
<!-- How do you know it's working? -->

## Examples
<!-- Concrete, copy-paste examples. -->

## Gotchas
<!-- Common pitfalls and how to avoid them. -->

## Decisions
<!-- Why does this design exist? What must not change? -->
```

### Before and after: Converting prose to PAVED

**Before** (traditional docs):
```markdown
# Background Jobs

Our background job system handles scheduled tasks. It uses Redis
for the queue and runs workers on each server. Jobs retry 3 times
before failing. You can check the dashboard at /admin/jobs.
```

**After** (PAVED format):
```markdown
# Background Jobs

## Purpose
Handles scheduled background tasks using Redis-backed queues.

**Non-goals:** Real-time event processing, cron job scheduling.

## Interface
- Queue: Redis `jobs:*` keys
- Dashboard: `/admin/jobs`
- CLI: `rake jobs:work`, `rake jobs:clear`

## Verification
```bash
# Check worker status
curl localhost:3000/admin/jobs/health
# Expected: {"status":"ok","workers":4}
```

## Examples
```ruby
BackgroundJob.perform_later(user_id: 123)
```

## Decisions
- 3 retries before failure (balances reliability vs. queue throughput)
- Redis over Postgres (lower latency for high-frequency jobs)
```

### Failure case: Document too long

```bash
$ paver check docs/monolith.md
docs/monolith.md:350: warning: Document exceeds 300 line limit (350 lines)
  hint: Consider splitting into smaller, focused documents

Checked 1 document: 0 errors, 1 warning
```

## Gotchas

**Agents don't need prose.** They need ground truth, constraints, examples, and verification steps. Avoid narrative explanations.

**Keep docs small.** The 300-line limit exists because agents retrieve docs as context. Large docs waste context window and reduce accuracy.

**Examples must be runnable.** If an example requires setup not shown, the agent will produce broken code. Show complete, working examples.

**Verification is everything.** Without verification steps, agents hallucinate correctness. Every doc needs a way to prove it's right.

**Don't nest docs.** Prefer flat structure over deep hierarchies. Agents retrieve docs individually, not as trees.

## Decisions

**Structure over prose.** Traditional docs optimize for human reading. PAVED optimizes for agent retrieval and action. Tables and code blocks beat paragraphs.

**Small, atomic docs.** "Leaf docs" cover one concept, one workflow, one component. "Index docs" provide routing. This mirrors how agents retrieve: map first, then target chunk.

**Verification is required.** The #1 missing element in traditional docs. Without it, agents can't self-correct.

**Max 300 lines.** Forces splitting, which improves retrieval accuracy. A focused doc beats a comprehensive one.

**Three document types.** Components (what), runbooks (how), ADRs (why). Covers 90% of engineering documentation needs without complexity.
