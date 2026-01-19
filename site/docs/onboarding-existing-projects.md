---
layout: doc
title: Onboarding Existing Projects
---

# Onboarding Existing Projects

Adopting paver in an existing codebase? This guide walks you through the process step by step.

## Overview

Adopting paver incrementally is the recommended approach for existing projects. Rather than converting all documentation at once, you'll:

1. Assess your current documentation landscape
2. Configure paver with gradual mode to avoid blocking CI
3. Convert high-value docs first
4. Expand coverage over time

**What you'll get:**
- Documentation that AI agents can reliably use
- Verification commands that catch drift early
- A clear path from legacy docs to PAVED compliance

## Step 1: Assess Your Current Docs

Run `paver adopt` to scan your existing documentation:

```bash
paver adopt
```

This command analyzes your project and reports:
- Where documentation lives (detected paths)
- Document count and types found
- Which docs could become components, runbooks, or ADRs
- Gaps in verification and examples

For a suggested configuration based on your project:

```bash
paver adopt --suggest-config
```

To see what `paver init` would create without making changes:

```bash
paver adopt --dry-run
```

## Step 2: Configure for Gradual Adoption

Initialize paver with gradual mode enabled:

```bash
paver init
```

Then edit `.paver.toml` to enable gradual mode:

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
gradual = true
gradual_until = "2026-04-18"  # 3 months from adoption
```

**Why gradual mode?**

With `gradual = true`, paver treats validation errors as warnings. This lets you:
- Add paver to CI without breaking builds
- Fix docs incrementally as you touch them
- Track progress toward full compliance

Set `gradual_until` to a realistic date (typically 2-3 months out) to create accountability.

## Step 3: Add PAVED Sections to Key Docs

Start with 3-5 high-impact documents. Good candidates are:
- Getting started guides
- Core API/service documentation
- Frequently referenced runbooks

For each document, add:

### Verification Section

Add commands that prove the documentation is accurate:

```markdown
## Verification

Confirm the service is running:

\`\`\`bash
curl -s http://localhost:8080/health | grep "ok"
\`\`\`

Verify configuration is loaded:

\`\`\`bash
./my-service config list | grep "database_url"
\`\`\`
```

### Examples Section

Add copy-paste ready examples:

```markdown
## Examples

### Basic usage

\`\`\`bash
./my-service start --port 8080
\`\`\`

### With custom configuration

\`\`\`bash
./my-service start --config /etc/my-service/prod.toml
\`\`\`
```

After updating docs, validate them:

```bash
paver check
```

## Step 4: Set Up CI Integration

Add paver validation to your CI pipeline. Example for GitHub Actions:

```yaml
# .github/workflows/docs.yml
name: Documentation

on: [push, pull_request]

jobs:
  validate:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - name: Install paver
        run: cargo install --path .

      - name: Validate documentation
        run: paver check --format github

      - name: Run verification commands
        run: paver verify --keep-going
```

The `--format github` flag outputs annotations that appear inline on PRs.

During gradual mode, validation passes even with errors (they're reported as warnings).

For more CI patterns including GitLab CI, JSON output, and troubleshooting, see the [CI/CD Integration Guide](/docs/ci-integration/).

## Step 5: Install Git Hooks

Install pre-commit hooks to catch issues before they're pushed:

```bash
paver hooks install
```

To also run verification commands in the hook:

```bash
paver hooks install --verify
```

To overwrite existing hooks:

```bash
paver hooks install --force
```

**Why hooks help:**

Hooks provide immediate feedback when editing docs. You'll catch:
- Missing required sections
- Documents exceeding line limits
- Broken verification commands (with `--verify`)

## Step 6: Track Progress Toward Strict Mode

Monitor your documentation health with:

```bash
paver check
```

This shows:
- Documents passing validation
- Documents with warnings (in gradual mode)
- Specific issues to fix

Check code-to-documentation coverage:

```bash
paver coverage
```

This reports which code paths have corresponding documentation and highlights gaps.

**When to disable gradual mode:**

Remove `gradual = true` from `.paver.toml` when:
- All existing docs pass `paver check`
- CI is stable
- Team is comfortable with the workflow

At that point, validation errors will fail CI, preventing regression.

## Migration Patterns

### Converting README-style docs

READMEs often mix multiple concerns. Split them into PAVED documents:

| README Section | PAVED Document Type |
|----------------|---------------------|
| "What is this?" | Component (Purpose) |
| "How to install" | Component (Interface) |
| "How to deploy" | Runbook |
| "Why we chose X" | ADR |

Create focused documents:

```bash
paver new component my-service
paver new runbook deploy-my-service
paver new adr why-we-chose-x
```

### Converting API documentation

API docs map well to components:

1. Create a component for each service/module:
   ```bash
   paver new component user-api
   ```

2. Move endpoint documentation to **Interface** section
3. Add authentication details to **Configuration**
4. Convert code samples to **Examples**
5. Add health check commands to **Verification**

### Converting runbooks/playbooks

Existing runbooks usually need minimal changes:

1. Add **Verification** section with commands to confirm success
2. Add **Rollback** section if missing
3. Ensure steps are numbered and unambiguous
4. Add **Preconditions** listing required access/tools

### Converting architecture decision records

ADRs are often close to PAVED format already:

1. Add **Verification** section (can reference docs or code that implement the decision)
2. Add **Examples** showing the decision in practice
3. Ensure **Status** is clearly marked (proposed, accepted, deprecated, superseded)

## Common Questions

### How long does adoption take?

It depends on your documentation volume and quality:

- **Small project (< 20 docs)**: 1-2 weeks for full compliance
- **Medium project (20-100 docs)**: 1-2 months with gradual adoption
- **Large project (100+ docs)**: 2-3 months, consider prioritizing by doc traffic

Start with gradual mode and chip away at warnings over time.

### What if our docs don't fit PAVED?

PAVED is flexible. The three document types cover most needs:

- **Components**: Anything you build and maintain
- **Runbooks**: Any procedure someone follows
- **ADRs**: Any significant decision worth recording

If you have documentation that doesn't fit (e.g., user tutorials, marketing content), you can exclude it from paver validation using `.paver.toml`:

```toml
[mapping]
exclude = ["docs/tutorials/", "docs/marketing/"]
```

### Can we customize required sections?

Yes. In `.paver.toml`, you can relax requirements:

```toml
[rules]
require_verification = false  # Allow docs without verification
require_examples = false      # Allow docs without examples
max_lines = 500              # Increase line limit
```

However, Verification and Examples are what make PAVED documentation valuable for agents. Consider keeping them required and using gradual mode instead.

### How do we handle legacy docs we don't want to update?

Exclude them from validation:

```toml
[mapping]
exclude = ["docs/legacy/", "docs/archive/"]
```

Alternatively, move them to a location outside `docs.root`:

```bash
mkdir archive
mv docs/old-stuff archive/
```

The `paver check` command will only validate documents within the configured docs root.

## Next Steps

- [Commands Reference](/docs/commands/) - Full CLI documentation
- [Getting Started](/docs/getting-started/) - New project setup
- [Manifesto](/docs/manifesto/) - The PAVED philosophy
