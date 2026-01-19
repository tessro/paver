# Verification

## Purpose

The verification system extracts and executes commands from the `## Verification` section of PAVED documents. This enables documentation to be self-testing: commands documented as verification steps are actually run and validated.

**Non-goals:**
- Not a test framework replacement (use pytest, Jest, cargo test for unit tests)
- Not a CI pipeline (use GitHub Actions, Jenkins for orchestration)
- Doesn't manage test data or fixtures

## Verification

```bash
cargo test verification 2>&1 | grep "test result: ok"
```

```bash
cargo build --release
```

## Interface

### Command Format

Commands in the Verification section are extracted from fenced code blocks with shell language hints (`bash`, `sh`, `shell`, `console`):

```markdown
## Verification

Run the tests:
```bash
cargo test
```

Check the build:
```sh
cargo build --release
```
```

### Shell Prompts

Commands can optionally include shell prompts (`$` or `>`), which are stripped before execution:

```markdown
## Verification
```bash
$ paver --version
# Expected: paver 0.1.0

$ paver check
# Expected: exits 0 if all docs pass
```
```

Multiple commands on separate lines are joined with `&&`:
```markdown
```bash
$ cargo build
$ cargo test
```
```
Executes as: `cargo build && cargo test`

### Exit Codes

By default, commands are expected to exit with code 0. Commands that exit non-zero are marked as failures unless a different exit code is expected.

### CLI Usage

```bash
paver verify [paths...] [options]
```

| Option | Description |
|--------|-------------|
| `paths` | Files or directories to verify (default: docs root) |
| `--format <format>` | Output format: `text`, `json`, `github` |
| `--timeout <seconds>` | Timeout per command (default: 30) |
| `--keep-going` | Continue after first failure |
| `--report <path>` | Write JSON report to file |

### Output Formats

**Text (default):**
```
docs/components/auth-service.md:45
  [PASS] (0.12s) cargo test --lib
  [PASS] (0.05s) cargo build

Verified 1 document: 2 commands passed
```

**JSON:**
```json
{
  "documents_verified": 1,
  "commands_executed": 2,
  "commands_passed": 2,
  "commands_failed": 0,
  "documents": [...]
}
```

**GitHub:** Annotations for GitHub Actions.

## Configuration

Verification uses the standard `.paver.toml` configuration to locate the docs root. No additional configuration is required.

Verification is enabled when:
1. A document has a `## Verification` section
2. That section contains at least one fenced code block with a shell language hint

## Examples

### Basic Verification Section

A component document with verification:

```markdown
# Auth Service

## Purpose
Handles user authentication.

## Verification
```bash
# Check the service is running
curl -s localhost:8080/health | grep -q "ok"

# Run unit tests
cargo test auth
```

## Examples
...
```

### Multiple Code Blocks

Each code block runs independently:

```markdown
## Verification

Test the API:
```bash
cargo test api
```

Test the database layer:
```bash
cargo test db
```
```

### Using Comments for Documentation

Comments (lines starting with `#`) are skipped:

```markdown
## Verification
```bash
# Build the project first
cargo build

# Then run tests
cargo test
```
```

This executes: `cargo build && cargo test`

### CI Integration

Run verification in CI with JSON output:

```bash
paver verify --format github --keep-going
```

The `--keep-going` flag ensures all documents are verified even if some fail, giving a complete picture of verification status.

### Writing Effective Verification Commands

**Good:** Quick, focused checks
```markdown
## Verification
```bash
# Check version
paver --version

# Validate docs pass
paver check docs/
```
```

**Bad:** Long-running or flaky commands
```markdown
## Verification
```bash
# Don't do this - takes too long
cargo build --all-targets
npm run test:e2e
```
```

## Gotchas

- **Commands run from project root**: All commands execute from the directory containing `.paver.toml`, not from the doc's directory.
- **Shell required**: Commands run via `sh -c`, so shell features like pipes and redirects work.
- **Output not validated**: Currently only exit codes are checked. Output matching is not yet supported.
- **Timeout applies per-command**: The `--timeout` flag sets the limit for each individual command, not the total run time.
- **Non-shell code blocks ignored**: Only `bash`, `sh`, `shell`, and `console` code blocks are treated as executable.

## Decisions

**Why extract from existing docs?** Verification sections already existed in PAVED documents for human readers. Running them automatically ensures they stay accurate and provides value beyond documentation.

**Why `sh -c` for execution?** This provides a consistent execution environment across platforms and enables shell features like pipes and environment variables.

**Why exit-code-only validation?** Exit codes are the universal success/failure indicator. Output matching would require maintaining expected outputs, which become stale quickly.

**Why per-command timeout?** Long-running verifications should be split into focused checks. A global timeout would hide which specific command is slow.

## Paths

- `src/verification.rs`
- `src/commands/verify.rs`
