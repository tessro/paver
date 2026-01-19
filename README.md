<h1 align="center">🛣️ pave</h1>

<p align="center">A CLI tool for creating, validating, and managing documentation optimized for human+agent pairs.</p>

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
pave init

# Create a new document
pave new component my-service

# Validate your docs
pave check

# Run verification commands
pave verify

# Generate index
pave index
```

## Commands

| Command | Description |
|---------|-------------|
| `pave init` | Initialize pave in your project |
| `pave new <type> <name>` | Scaffold a new document |
| `pave check` | Validate documents against rules |
| `pave verify` | Run verification commands from docs |
| `pave changed` | Show docs impacted by code changes |
| `pave index` | Generate documentation index |
| `pave prompt <type>` | Generate AI agent prompts |
| `pave config` | Manage configuration |

## Document Types

- **Components** - For services, libraries, and modules
- **Runbooks** - For operational procedures
- **ADRs** - Architecture Decision Records

## Learn More

- [Documentation](https://tessro.github.io/pave/)
- [Manifesto](https://tessro.github.io/pave/docs/manifesto/)
- [Getting Started](https://tessro.github.io/pave/docs/getting-started/)
