# Runbook: Update Pave Documentation

## When to Use
Use this runbook when updating pave's own documentation. This applies when:
- Modifying source code in `src/` that changes behavior
- Adding or changing CLI commands
- Modifying configuration options in `.pave.toml`
- Fixing bugs that affect documented behavior
- Adding new features that need documentation

## Preconditions
- Rust toolchain installed (via [rustup](https://rustup.rs/))
- Repository cloned locally
- Understanding of the [PAVED framework](../manifesto.md)
- Familiarity with the change that requires documentation updates

## Steps

### 1. Check which docs are impacted

After making code changes, run the `changed` command to see which documentation files may need updates:

```bash
cargo run -- changed
```

This analyzes source mappings to identify documentation that references modified code.

### 2. Review and update affected documentation

For each impacted document:
- Read the existing content
- Update sections that reference changed behavior
- Add new sections if the change introduces new functionality
- Remove outdated information

Common documents to check:
- `docs/components/` - Component documentation for modified subsystems
- `docs/runbooks/` - Runbooks referencing changed commands
- `site/docs/commands.md` - CLI command reference

### 3. Validate documentation structure

Run the check command to ensure all documentation follows PAVED rules:

```bash
cargo run -- check
```

Fix any validation errors before proceeding.

### 4. Run verification commands

Execute verification blocks embedded in documentation to ensure examples still work:

```bash
cargo run -- verify
```

Fix any failing verifications by updating the documentation or fixing the underlying code.

### 5. Update the index if needed

If you added new documents, regenerate the index:

```bash
cargo run -- index --update
```

### 6. Run the full test suite

Ensure all tests pass:

```bash
cargo test
```

## Rollback

If documentation changes are incorrect or break CI:

```bash
git checkout -- docs/
```

Or revert specific files:

```bash
git checkout -- docs/path/to/file.md
```

## Verification

Confirm documentation is correct:

```bash
./target/release/pave check
```

```bash
cargo test
```

## Escalation
For a small project like pave, escalation is typically not needed. If issues arise:
1. Review the [PAVED manifesto](../manifesto.md) for documentation guidelines
2. Check existing documents in `docs/` for patterns
3. Open an issue on the pave repository

## Examples

### Adding documentation for a new command

After adding a `stats` command to the codebase:

1. Check impacted docs:
   ```bash
   cargo run -- changed
   ```

2. Create component documentation:
   ```bash
   cargo run -- new component stats
   ```

3. Fill out `docs/components/stats.md` with purpose, interface, and examples

4. Update `site/docs/commands.md` with CLI usage

5. Validate:
   ```bash
   cargo run -- check
   ```

### Updating documentation after a bug fix

After fixing a bug in the `check` command:

1. Review `docs/components/validation-engine.md` for accuracy
2. Update any examples that showed the buggy behavior
3. Run `cargo run -- verify` to ensure examples work
4. Run `cargo run -- check` to validate structure

### Documenting a configuration change

After adding a new config option:

1. Update `docs/components/configuration.md` with the new option
2. Add the option to any relevant examples
3. Update `.pave.toml` if it serves as a reference
4. Validate with `cargo run -- check`
