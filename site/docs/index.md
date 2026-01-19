---
layout: doc
title: Documentation
---

# Documentation 📚

Welcome to the paver docs. You can read them, but they're not really for you - they're for the human+agent pair doing the work. 🤝

## Overview

paver is a CLI tool for creating, validating, and managing **PAVED** documentation - a framework optimized for AI agent workflows.

| Command | Description |
|---------|-------------|
| `paver init` | Initialize paver in your project |
| `paver new <type> <name>` | Scaffold a new document |
| `paver prompt <type>` | Generate AI agent prompts |
| `paver index` | Generate documentation index |
| `paver check` | Validate documents against rules |
| `paver config` | Manage configuration |

## Document Types 📑

paver supports three core document types:

### Components 🔧

For services, libraries, and modules. Includes Purpose, Interface, Configuration, Verification, Examples, Gotchas, and Decisions sections.

```bash
paver new component auth-service
```

### Runbooks 📋

For operational procedures. Includes When to Use, Preconditions, Steps, Rollback, Verification, and Escalation sections.

```bash
paver new runbook deploy-production
```

### ADRs 📝

Architecture Decision Records. Includes Status, Context, Decision, Consequences, and Alternatives Considered sections.

```bash
paver new adr use-rust
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
