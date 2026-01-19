# Code Mapping

## Purpose

Code mapping links documentation to source code files via the `## Paths` section. When code changes, `pave changed` identifies which docs may need updates. This keeps documentation synchronized with the codebase.

**Non-goals:**
- Not automatic doc generation (you still write the docs)
- Not a file watcher (runs on-demand via CLI)
- Not a linter for code-doc consistency (just identifies impacted docs)

## Interface

### Paths Section Format

Add a `## Paths` section to any PAVED document listing the code files it covers:

````markdown
## Paths

- `src/auth.rs`
- `src/auth/*.rs`
- `src/middleware/session.rs`
````

Patterns support:
- Exact paths: `src/auth.rs`
- Single-level wildcards: `src/commands/*.rs`
- Recursive wildcards: `src/**/*.rs`
- Directory prefixes: `src/auth/`

### CLI Usage

```bash
pave changed [options]
```

| Option | Description |
|--------|-------------|
| `--base <ref>` | Git ref to compare against (default: `origin/main`) |
| `--format <format>` | Output format: `text` or `json` |
| `--strict` | Exit non-zero if impacted docs weren't updated |

### Output Format

**Text (default):**
```
Comparing against: origin/main (5 files changed)

Impacted documentation (2 docs):

  ✓ Auth Service (docs/components/auth-service.md)
      ← src/auth.rs
      ← src/auth/session.rs
  ✗ CLI Reference (docs/components/pave-cli.md)
      ← src/cli.rs

1 doc needs review:
  - docs/components/pave-cli.md
```

Legend:
- `✓` Doc was updated in the same changeset
- `✗` Doc was not updated (may need review)

**JSON:**
```json
{
  "base_ref": "origin/main",
  "changed_files_count": 5,
  "impacted_docs": [
    {
      "doc_path": "docs/components/auth-service.md",
      "title": "Auth Service",
      "matched_files": ["src/auth.rs", "src/auth/session.rs"],
      "was_updated": true
    }
  ],
  "missing_updates": ["docs/components/pave-cli.md"]
}
```

## Configuration

Code mapping uses the standard `.pave.toml` configuration to locate the docs root. No additional configuration is required.

The `## Paths` section is parsed from any markdown document in the docs directory. Documents without a Paths section are not tracked for code changes.

## Verification

Check that code mapping detects impacted docs:

```bash
./target/release/pave changed --base HEAD~1
```

Verify JSON output format works:

```bash
./target/release/pave changed --base HEAD~1 --format json | head -1
```

## Examples

### Basic Path Mapping

Map a component doc to its implementation files:

````markdown
# Auth Service

## Purpose
Handles user authentication and session management.

## Paths

- `src/auth.rs`
- `src/auth/*.rs`
- `src/middleware/session.rs`

...rest of doc...
````

### Using Glob Patterns

Match multiple files with wildcards:

````markdown
## Paths

- `src/commands/*.rs`      # All command implementations
- `src/**/*_test.rs`       # All test files
- `config/*.toml`          # Configuration files
````

### CI Integration

Fail the build if impacted docs weren't updated:

```yaml
# .github/workflows/docs.yml
- name: Check doc coverage
  run: pave changed --strict --base origin/main
```

This ensures documentation stays current with code changes.

### Checking Against Different Refs

Compare against a feature branch:
```bash
pave changed --base feature/new-auth
```

Compare against a specific commit:
```bash
pave changed --base abc1234
```

Compare against the previous commit:
```bash
pave changed --base HEAD~1
```

### JSON Output for Scripting

Process impacted docs programmatically:

```bash
pave changed --format json | jq '.missing_updates[]'
```

## Gotchas

- **Paths are relative to project root**: All patterns are matched from the directory containing `.pave.toml`.
- **Glob patterns use standard glob syntax**: `*` matches within a directory, `**` matches across directories.
- **Documents without Paths sections are skipped**: Only docs with explicit path mappings are tracked.
- **index.md is always skipped**: The index document is not included in change detection.
- **Base ref defaults vary**: Tries `origin/main`, then `origin/master`, then `HEAD~1`.

## Decisions

**Why a Paths section instead of frontmatter?** The Paths section is visible in the rendered doc, making the code-to-doc relationship explicit for readers. Frontmatter is often stripped by renderers.

**Why glob patterns?** Globs are widely understood and allow matching file groups without listing every file. This reduces maintenance burden as files are added or renamed.

**Why track update status?** Knowing whether a doc was updated in the same changeset helps distinguish "needs review" from "already handled." This reduces false positives in CI.

**Why not automatic path inference?** Explicit mappings are more reliable than heuristics. A doc about "authentication" might cover files in `src/auth/`, `src/middleware/`, and `tests/auth_test.rs`. Inference would miss edge cases.

## Paths

- `src/commands/changed.rs`
