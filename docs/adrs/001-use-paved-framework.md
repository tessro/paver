# ADR: Use PAVED Framework for Documentation

## Status

Accepted

## Context

Traditional documentation approaches fail in AI agent workflows:

- Large narrative documents are hard for agents to parse and retrieve from
- Docs without verification steps lead to hallucinated correctness
- Prose-heavy documentation lacks the structure agents need for accurate output
- No enforcement mechanisms mean documentation quality degrades over time

We need a documentation framework optimized for human+agent pair work that provides structure, verifiability, and maintainability.

## Decision

Adopt the PAVED framework for all project documentation:

- **P**urpose: What is this? What problem does it solve? (1-3 sentences, include non-goals)
- **A**PI/Interface: Entry points, commands, schemas, config keys
- **V**erification: How to know it's working - test commands, health checks, expected outputs
- **E**xamples: Concrete, copy-paste examples (happy path, realistic, failure cases)
- **D**ecisions: Why this design exists, what must not change, tradeoffs

Additionally:

- Use atomic "leaf docs" (one concept per document) with index routing
- Enforce validation rules via `paver check`
- Require Verification and Examples sections in all documents
- Limit documents to 300 lines (split if larger)

## Consequences

**Positive:**

- Structured docs are easier for agents to parse and work with
- Verification sections enable agents to validate their changes
- Small, atomic docs improve retrieval accuracy
- Enforced rules maintain quality over time
- Examples provide "shape matching" for accurate agent output

**Negative:**

- Higher upfront effort to create documentation
- All existing docs must be migrated to PAVED format
- Requires tooling (`paver`) to enforce rules
- Some docs may feel overly structured for simple concepts

**Neutral:**

- Docs become more like code: versioned, validated, linted
- Team must learn the PAVED structure

## Alternatives Considered

### Diataxis Framework

A documentation system with four quadrants: tutorials, how-to guides, technical reference, and explanation.

**Why not chosen:** Too narrative-focused. Diataxis optimizes for human learning journeys but doesn't address agent retrieval or verification. The four-quadrant model creates larger documents that are harder to parse.

### Plain Markdown

Simple, unstructured markdown files without enforced conventions.

**Why not chosen:** No structure enforcement means quality degrades over time. No verification requirements mean agents can't validate their work. Lacks the consistent format agents need for reliable retrieval.

### AsciiDoc

A text document format with richer semantics than Markdown.

**Why not chosen:** More complex syntax without proportional benefits for our use case. Smaller ecosystem of tools. The additional features (complex tables, includes, conditional content) aren't needed for agent-native docs.

### Sphinx/RST

Python documentation generator using reStructuredText.

**Why not chosen:** Python ecosystem specific. Heavy tooling for what we need. RST syntax is less familiar to developers. Optimized for API documentation generation rather than agent workflows.

## Verification

Prerequisites: Build paver with `cargo build --release`

Verify the PAVED framework is properly configured:

```bash
# Check paver is available and configured
./target/release/paver config list

# Validate all documentation follows PAVED rules
./target/release/paver check
```

Expected: All checks pass with no errors.

## Examples

### Creating a New Component Doc

```bash
# Generate a new component document
./target/release/paver new component "Authentication Service"

# This creates docs/authentication-service.md with PAVED sections
```

### Validating Documentation

```bash
# Check a specific file
./target/release/paver check docs/adrs/001-use-paved-framework.md

# Check all documentation
./target/release/paver check

# Strict mode (warnings as errors)
./target/release/paver check --strict
```

### PAVED Section Example

A minimal PAVED document structure:

```markdown
# Component Name

## Purpose
Brief description of what this component does.
Non-goals: what it explicitly does NOT do.

## Interface
- `command`: Description of the command

## Verification
Run `make test` to verify functionality.

## Examples
Example usage of the component.

## Decisions
Key architectural decisions and constraints.
```
