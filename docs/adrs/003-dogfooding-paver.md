# ADR: Dogfooding Paver for Self-Compliance

## Status
Accepted

## Context

Paver is a documentation validation tool designed to ensure documentation follows the PAVED framework. For paver to be credible as a documentation quality tool, it must demonstrate that its own documentation meets the standards it enforces for others.

Self-compliance ("dogfooding") provides several benefits:

- **Credibility**: Users trust tools that follow their own advice
- **Quality assurance**: Validation rules are tested against real documentation
- **Bug discovery**: Issues in paver are caught early when running against its own docs
- **Living example**: Paver's documentation serves as a reference implementation

Without self-compliance enforcement, paver could drift into a state where it validates external projects but fails its own checks.

## Decision

All paver documentation must:

1. Pass `paver check --strict` with no errors or warnings
2. Pass `paver verify` with no failures
3. Follow the appropriate template for its document type (ADR, component, runbook)
4. Include all required sections as configured in `.paver.toml`

Enforcement mechanisms:

- **CI**: GitHub Actions workflow runs `paver check --strict` on every PR and push to main
- **Development**: Developers should run `paver check` before committing
- **Documentation gaps**: Treated as bugs, not tech debt

## Consequences

**Positive:**

- Paver documentation is always valid and follows PAVED
- New features requiring documentation changes are caught before merge
- Users can reference paver's own docs as examples of proper PAVED structure
- Validation rules are continuously tested against real content

**Negative:**

- Documentation updates are required when adding features
- Stricter enforcement may slow down initial development
- Template changes require updating all existing docs

## Alternatives Considered

### No Self-Enforcement

Allow paver documentation to exist outside the validation framework.

**Why not chosen:** Undermines the tool's credibility. "Do as I say, not as I do" erodes user trust and means the tool isn't tested against real documentation.

### Optional Self-Enforcement

Make self-compliance a best-effort goal without CI enforcement.

**Why not chosen:** Without enforcement, compliance degrades over time. CI gates ensure consistent quality and catch regressions immediately.

### Separate Documentation Standards

Use different, simpler standards for paver's internal docs.

**Why not chosen:** Creates confusion about what "proper" documentation looks like. Users would have to distinguish between paver's internal docs and the example docs they're supposed to emulate.

## Verification

Prerequisites: Build paver with `cargo build --release`

Verify paver validates its own documentation:

```bash
# Run documentation validation
./target/release/paver check --strict
```

Expected: Command exits with status 0 (success) and reports all documents pass.

Note: `paver verify` is not included in the verification block to avoid recursive verification loops.

## Examples

### CI Configuration for Self-Validation

The `.github/workflows/docs.yml` workflow enforces self-compliance:

```yaml
name: Documentation

on:
  push:
    branches: [main]
  pull_request:

jobs:
  validate:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - name: Install Rust
        uses: dtolnay/rust-toolchain@stable

      - name: Build paver
        run: cargo build --release

      - name: Validate documentation
        run: ./target/release/paver check --strict
```

### Local Development Workflow

Before committing documentation changes:

```bash
# Check documentation follows PAVED rules
./target/release/paver check

# Verify executable code blocks work
./target/release/paver verify

# If checks pass, commit
git add docs/
git commit -m "Update documentation"
```
