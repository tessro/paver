---
layout: doc
title: Getting Started
---

# Getting Started 🚀

Get paver up and running in your project in under 5 minutes.

## Installation 📦

```bash
# Build from source
cargo build --release

# Or install directly
cargo install --path .
```

## Initialize Your Project 🎬

```bash
paver init
```

This creates a `.paver.toml` configuration file in your project root with sensible defaults.

## Your First Document 📝

Create a component document:

```bash
paver new component my-service
```

This scaffolds `docs/components/my-service.md` with all the PAVED sections ready to fill in:

- **Purpose** - What does this service do?
- **Interface** - How do you use it?
- **Configuration** - What settings are available?
- **Verification** - How do you know it works?
- **Examples** - Show me the code
- **Gotchas** - What trips people up?
- **Decisions** - Why these choices?

## Generate an Index 🗺️

Keep your docs navigable:

```bash
paver index
```

This scans your `docs/` directory and generates `docs/index.md` with:

- Quick links to top-level docs
- Categorized sections (Components, Runbooks, ADRs)
- Purpose summaries extracted from each doc

## Validate Your Docs ✅

Enforce quality rules:

```bash
paver check
```

This validates your documentation against configured rules:

- Max line limits (default: 300)
- Required sections (Verification, Examples)
- PAVED structure compliance

## Configuration ⚙️

View your config:

```bash
paver config list
```

Modify settings:

```bash
paver config set rules.max_lines 500
```

## Next Steps 🎯

- Read the [Manifesto](/docs/manifesto/) to understand the philosophy
- Explore [Commands](/docs/commands/) for full CLI reference
- Check out the templates in `templates/` for customization
