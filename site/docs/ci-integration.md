---
layout: doc
title: CI/CD Integration
---

# CI/CD Integration

Running pave in your CI/CD pipeline ensures documentation stays valid and verification commands continue to pass.

## Overview

### Why Run Pave in CI?

- **Catch drift early** - Verification commands fail when docs diverge from reality
- **Enforce quality** - Validate structure, required sections, and line limits
- **Block bad merges** - Prevent documentation debt from accumulating
- **Inline feedback** - GitHub Actions annotations show issues directly in PR diffs

### Check vs Verify

| Command | Purpose | Speed | When to Run |
|---------|---------|-------|-------------|
| `pave check` | Validates structure and rules | Fast (seconds) | Every PR |
| `pave verify` | Runs verification commands | Slower (varies) | PRs touching code or docs |

Run `check` on every PR. Run `verify` when code or docs change.

## GitHub Actions

### Basic Check

Validate documentation structure on every pull request:

```yaml
# .github/workflows/docs.yml
name: Documentation

on: [pull_request]

jobs:
  pave:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - name: Install Rust
        uses: dtolnay/rust-toolchain@stable

      - name: Cache cargo
        uses: Swatinem/rust-cache@v2

      - name: Build pave
        run: cargo build --release

      - name: Check documentation
        run: ./target/release/pave check --format github
```

The `--format github` flag produces annotations that appear inline on PR diffs.

### Check + Verify

Extended validation including verification commands:

```yaml
name: Documentation

on:
  push:
    branches: [main]
    paths:
      - 'docs/**'
      - 'src/**'
      - '.pave.toml'
  pull_request:
    paths:
      - 'docs/**'
      - 'src/**'
      - '.pave.toml'

jobs:
  validate:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
        with:
          fetch-depth: 0  # Required for --changed flag

      - name: Install Rust
        uses: dtolnay/rust-toolchain@stable

      - name: Cache cargo
        uses: Swatinem/rust-cache@v2

      - name: Build pave
        run: cargo build --release

      - name: Check documentation
        run: ./target/release/pave check --strict --format github

      - name: Run verification commands
        run: ./target/release/pave verify --keep-going --format github
```

**Flags explained:**

- `--strict` - Overrides gradual mode, treating all warnings as errors
- `--keep-going` - Continues running after first failure (reports all issues)
- `--format github` - Outputs GitHub Actions annotations

### Only Changed Docs

Check only documentation affected by the current PR:

```yaml
- name: Check changed docs only
  run: ./target/release/pave check --changed --base origin/main --format github
```

The `--changed` flag compares against a base ref (default: `origin/main`) and only validates modified documents. This speeds up large documentation sets.

**Note:** Requires `fetch-depth: 0` in checkout to access git history.

### Gradual Mode Workflow

For teams adopting pave incrementally, run both gradual and strict checks:

```yaml
jobs:
  validate:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
        with:
          fetch-depth: 0

      - name: Install Rust
        uses: dtolnay/rust-toolchain@stable

      - name: Cache cargo
        uses: Swatinem/rust-cache@v2

      - name: Build pave
        run: cargo build --release

      # Always runs - reports warnings but doesn't fail
      - name: Check documentation (gradual)
        run: ./target/release/pave check --gradual --format github

      # Fails if new docs don't meet standards
      - name: Check new documentation (strict)
        run: ./target/release/pave check --changed --strict --format github
        if: github.event_name == 'pull_request'
```

This approach:
1. Reports all issues as warnings (visible but non-blocking)
2. Requires new/modified docs to meet standards
3. Allows legacy docs to be fixed incrementally

## GitLab CI

### Basic Example

```yaml
# .gitlab-ci.yml
stages:
  - validate

documentation:
  stage: validate
  image: rust:latest
  script:
    - cargo build --release
    - ./target/release/pave check
    - ./target/release/pave verify --keep-going
  rules:
    - changes:
        - docs/**/*
        - src/**/*
        - .pave.toml
```

### With Artifacts

Save verification reports as artifacts for debugging:

```yaml
documentation:
  stage: validate
  image: rust:latest
  script:
    - cargo build --release
    - ./target/release/pave check --format json > check-report.json
    - ./target/release/pave verify --keep-going --report verify-report.json
  artifacts:
    when: always
    paths:
      - check-report.json
      - verify-report.json
    expire_in: 1 week
  rules:
    - changes:
        - docs/**/*
        - src/**/*
```

## Generic CI

### Environment Variables

Pave respects these standard environment variables:

| Variable | Description |
|----------|-------------|
| `NO_COLOR` | Disable colored output when set |
| `CI` | Auto-detected; affects default output formatting |

### Exit Codes

| Code | Meaning |
|------|---------|
| `0` | All checks passed |
| `1` | Validation errors (or warnings in strict mode) |
| `2` | Configuration or runtime error |

Use exit codes for conditional steps:

```bash
pave check || echo "Documentation issues found"
```

### JSON Output for Custom Processing

Generate machine-readable output for custom integrations:

```bash
# Check output
pave check --format json > check-results.json

# Verify output with report
pave verify --format json --report verify-results.json
```

**Check JSON structure:**

```json
{
  "files_checked": 15,
  "errors": [
    {
      "file": "docs/components/auth.md",
      "line": 1,
      "message": "Missing required section: Verification"
    }
  ],
  "warnings": []
}
```

**Verify JSON structure:**

```json
{
  "documents_verified": 8,
  "commands_executed": 12,
  "commands_passed": 11,
  "commands_warned": 0,
  "commands_failed": 1,
  "documents": [
    {
      "file": "docs/components/auth.md",
      "commands": [
        {
          "command": "curl -s http://localhost:8080/health",
          "status": "pass",
          "exit_code": 0,
          "stdout": "ok",
          "duration_ms": 150
        }
      ],
      "status": "pass"
    }
  ]
}
```

## Best Practices

### When to Check vs Verify

| Scenario | `pave check` | `pave verify` |
|----------|---------------|----------------|
| Quick PR feedback | Yes | No |
| Docs-only changes | Yes | Yes |
| Code changes | Yes | Yes |
| Pre-merge gate | Yes | Recommended |
| Post-deploy smoke test | No | Yes |

### Handling Failures Gracefully

Use `--keep-going` with verify to collect all failures:

```bash
# Fails fast (default)
pave verify

# Reports all failures
pave verify --keep-going
```

For gradual adoption, use `--gradual` to report issues without failing:

```bash
pave check --gradual  # Exits 0 even with issues
```

### Caching the Pave Binary

Speed up CI by caching the compiled binary:

**GitHub Actions:**

```yaml
- name: Cache cargo
  uses: Swatinem/rust-cache@v2
```

**Generic:**

```bash
# Cache key based on Cargo.lock
CACHE_KEY="pave-$(sha256sum Cargo.lock | cut -d' ' -f1)"
```

### Running on Documentation-Only Changes

Filter CI runs to relevant changes:

**GitHub Actions:**

```yaml
on:
  pull_request:
    paths:
      - 'docs/**'
      - '.pave.toml'
      - 'templates/**'
```

**GitLab CI:**

```yaml
rules:
  - changes:
      - docs/**/*
      - .pave.toml
```

## Troubleshooting

### Check Passes Locally but Fails in CI

**Common causes:**

1. **Different working directory** - Pave looks for `.pave.toml` from the current directory
   ```bash
   # Ensure you're in the repo root
   cd $GITHUB_WORKSPACE
   pave check
   ```

2. **Missing git history** - The `--changed` flag needs commit history
   ```yaml
   - uses: actions/checkout@v4
     with:
       fetch-depth: 0  # Fetch all history
   ```

3. **Gradual mode differences** - Local config may have `gradual = true`
   ```bash
   # Force strict mode in CI
   pave check --strict
   ```

### Verify Commands Fail in CI

**Common causes:**

1. **Missing dependencies** - Install required tools before verify
   ```yaml
   - name: Install dependencies
     run: apt-get install -y curl jq

   - name: Run verify
     run: pave verify
   ```

2. **Services not running** - Start required services first
   ```yaml
   services:
     postgres:
       image: postgres:15
       env:
         POSTGRES_PASSWORD: test
   ```

3. **Timeout issues** - Increase timeout for slow commands
   ```bash
   pave verify --timeout 60  # 60 seconds per command
   ```

4. **Environment differences** - Set required environment variables
   ```yaml
   - name: Run verify
     run: pave verify
     env:
       DATABASE_URL: postgres://localhost/test
   ```

### Performance Optimization

For large documentation sets:

1. **Use `--changed` flag** - Only check modified docs
   ```bash
   pave check --changed --base origin/main
   ```

2. **Parallelize jobs** - Split check and verify
   ```yaml
   jobs:
     check:
       runs-on: ubuntu-latest
       steps:
         - run: pave check

     verify:
       runs-on: ubuntu-latest
       steps:
         - run: pave verify
   ```

3. **Skip verify on docs-only changes** - Use path filters to skip verify when only docs change
   ```yaml
   on:
     pull_request:
       paths:
         - 'src/**'  # Only run verify when source code changes
   ```

## Verification

Confirm CI integration works:

```bash
# Simulate GitHub Actions output
pave check --format github

# Test strict mode behavior
pave check --strict

# Verify JSON output is valid
pave check --format json | jq .

# Test verify with timeout
pave verify --timeout 10 --keep-going
```

## Next Steps

- [Commands Reference](/docs/commands/) - Full CLI documentation
- [Onboarding Existing Projects](/docs/onboarding-existing-projects/) - Gradual adoption guidance
- [Getting Started](/docs/getting-started/) - New project setup
