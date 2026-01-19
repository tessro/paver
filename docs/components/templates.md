# Templates

## Purpose

The templates system provides scaffolding for new PAVED documents. When you run `pave new`, templates ensure that new documents start with the correct structure, sections, and guidance comments for their document type.

**Non-goals:**
- Not a static site generator (templates are for scaffolding, not rendering)
- Not a document transformation tool (no Jinja/Handlebars-style templating)
- Doesn't support arbitrary custom template variables

## Interface

### TemplateType Enum

Three document types are supported:

| Type | Default Filename | Placeholder Variable |
|------|------------------|---------------------|
| `Component` | `component.md` | `{Component Name}` |
| `Runbook` | `runbook.md` | `{Task Name}` |
| `Adr` | `adr.md` | `{Title}` |

### get_template() Function

```rust
pub fn get_template(template_type: TemplateType) -> &'static str
```

Returns the template content for a given type. Built-in templates are embedded in the binary at compile time using `include_str!`.

### pave new Command

```bash
pave new <type> <name> [--output <path>]
```

| Argument | Description |
|----------|-------------|
| `type` | `component`, `runbook`, or `adr` |
| `name` | Name for the document (kebab-case recommended) |
| `--output` | Custom output path (optional) |

Default output paths:
- Components: `docs/components/<name>.md`
- Runbooks: `docs/runbooks/<name>.md`
- ADRs: `docs/adrs/<name>.md`

## Configuration

The templates system uses built-in templates embedded in the pave binary. No configuration is required to use templates.

The config schema includes fields for future custom template support (`docs.templates`, `templates.component`, `templates.runbook`, `templates.adr`), but custom template loading is not yet implemented. Currently, `pave new` always uses the built-in templates.

## Verification

Verify templates are working:

```bash
./target/release/pave new component test-template --output /tmp/test-component.md && grep "## Purpose" /tmp/test-component.md && rm /tmp/test-component.md
```

Verify all template types have required sections:

```bash
cargo test templates
```

## Examples

### Create a Component Doc

```bash
pave new component auth-service
# Creates: docs/components/auth-service.md
# Title: "# Auth Service"
```

### Create a Runbook

```bash
pave new runbook deploy-production
# Creates: docs/runbooks/deploy-production.md
# Title: "# Runbook: Deploy Production"
```

### Create an ADR

```bash
pave new adr use-postgres
# Creates: docs/adrs/use-postgres.md
# Title: "# ADR: Use Postgres"
```

### Use Custom Output Path

```bash
pave new component my-service --output docs/services/my-service.md
# Creates: docs/services/my-service.md
```

## Gotchas

- **Placeholder replacement is exact**: Only the specific placeholder for each type is replaced (`{Component Name}`, `{Task Name}`, `{Title}`). Other `{...}` patterns are left unchanged.
- **Name conversion**: Names are converted from kebab-case or snake_case to Title Case. `auth-service` becomes `Auth Service`.
- **No nested directories**: The default output paths don't create nested subdirectories beyond `docs/components/`, `docs/runbooks/`, and `docs/adrs/`.
- **File already exists**: `pave new` will fail if the output file already exists. Use `--output` to specify a different path.

## Decisions

**Why embedded templates?** Built-in templates are embedded at compile time using `include_str!`. This ensures pave works out of the box without external dependencies or installation steps.

**Why simple placeholder substitution?** Complex templating engines (Tera, Handlebars) add dependencies and learning curves. Simple string replacement covers the common case (document title) and keeps templates easy to read.

**Why three document types?** Components, runbooks, and ADRs cover the primary documentation needs: what exists (components), how to operate it (runbooks), and why it was built that way (ADRs).

**Why kebab-case to Title Case?** Filenames work best in kebab-case (URL-friendly, easy to type), but document titles should be human-readable. The automatic conversion bridges both needs.

## Paths

- `src/templates.rs`
- `src/commands/new.rs`
- `templates/component.md`
- `templates/runbook.md`
- `templates/adr.md`
