# paver

A CLI tool for creating, validating, and managing documentation optimized for human+agent pairs.

## What is PAVED?

PAVED is a documentation framework that treats docs like APIs:

- **P**urpose - What is this? What problem does it solve?
- **A**PI/Interface - How do you use it?
- **V**erification - How do you know it's working?
- **E**xamples - Concrete, copy-paste usage
- **D**ecisions - Why this design? What must not change?

## Quick Start

```bash
# Build from source
cargo build --release

# Initialize in your project
paver init

# Create a new document
paver new component my-service

# Validate your docs
paver check

# Generate index
paver index
```

## Commands

| Command | Description |
|---------|-------------|
| `paver init` | Initialize paver in your project |
| `paver new <type> <name>` | Scaffold a new document |
| `paver check` | Validate documents against rules |
| `paver index` | Generate documentation index |
| `paver prompt <type>` | Generate AI agent prompts |
| `paver config` | Manage configuration |

## Document Types

- **Components** - For services, libraries, and modules
- **Runbooks** - For operational procedures
- **ADRs** - Architecture Decision Records

## Learn More

- [Documentation](https://tessro.github.io/paver/)
- [Manifesto](https://tessro.github.io/paver/docs/manifesto/)
- [Getting Started](https://tessro.github.io/paver/docs/getting-started/)
