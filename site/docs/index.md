---
layout: doc
title: Documentation
---

# Documentation 📚

Welcome to the pave docs. You can read them, but they're not really for you - they're for the human+agent pair doing the work. 🤝

## Overview

pave is a CLI tool for creating, validating, and managing **PAVED** documentation - a framework optimized for AI agent workflows.

| Command | Description |
|---------|-------------|
| `pave init` | Initialize pave in your project |
| `pave new <type> <name>` | Scaffold a new document |
| `pave prompt <type>` | Generate AI agent prompts |
| `pave index` | Generate documentation index |
| `pave check` | Validate documents against rules |
| `pave config` | Manage configuration |
| `pave adopt` | Scan existing docs to help onboard |

## Guides 📖

| Guide | Description |
|-------|-------------|
| [Getting Started](/docs/getting-started/) | Set up pave in a new project |
| [Onboarding Existing Projects](/docs/onboarding-existing-projects/) | Adopt pave in an existing codebase |
| [CI/CD Integration](/docs/ci-integration/) | Run pave in CI pipelines |

## Document Types 📑

pave supports three core document types:

### Components 🔧

For services, libraries, and modules. Includes Purpose, Interface, Configuration, Verification, Examples, Gotchas, and Decisions sections.

```bash
pave new component auth-service
```

### Runbooks 📋

For operational procedures. Includes When to Use, Preconditions, Steps, Rollback, Verification, and Escalation sections.

```bash
pave new runbook deploy-production
```

### ADRs 📝

Architecture Decision Records. Includes Status, Context, Decision, Consequences, and Alternatives Considered sections.

```bash
pave new adr use-rust
```

## Philosophy 🧠

> "Docs aren't for humans anymore; they're for a human+agent pair doing work."

The PAVED framework treats documentation like APIs:

- **Precise contracts** - not vague prose
- **Small surfaces** - one concept per doc
- **Versioned** - track changes
- **Validated** - enforce quality rules
- **Optimized for retrieval + execution** - agents need ground truth

Learn more in the [Manifesto](manifesto).
