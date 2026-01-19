# Runbook: Onboard Existing Project to Pave

## When to Use

Use this runbook when helping a user adopt pave in an existing codebase. This applies when:
- A user asks for help adopting pave in their project
- You're evaluating a project's documentation for PAVED compliance
- You're migrating legacy documentation to PAVED format

## Preconditions

- pave built (`cargo build --release` produces `./target/release/pave`)
- Access to the target project's repository
- Understanding of the project's documentation structure
- Read access to existing documentation files

## Steps

### 1. Run initial assessment

Scan the project to understand its documentation landscape:

```bash
./target/release/pave adopt
```

Review the output for:
- Detected documentation paths
- Document counts by type
- Recommended document type mappings
- Missing sections that PAVED requires

### 2. Generate suggested configuration

Get configuration recommendations:

```bash
./target/release/pave adopt --suggest-config
```

Review the suggested settings and note any adjustments needed for the project's specific needs.

### 3. Preview initialization

Dry-run the init to see what would be created:

```bash
./target/release/pave adopt --dry-run
```

Confirm the proposed changes make sense before proceeding.

### 4. Initialize pave

Create the configuration file:

```bash
./target/release/pave init
```

### 5. Configure gradual mode

Edit `.pave.toml` to enable gradual adoption:

```toml
[rules]
gradual = true
gradual_until = "YYYY-MM-DD"  # Set to 2-3 months from now
```

This prevents CI failures while documentation is being migrated.

### 6. Identify high-priority documents

Determine which documents to convert first. Prioritize by:
1. Frequency of use (most-read docs first)
2. Criticality (onboarding, core services)
3. Freshness (recently updated docs are more accurate)

List candidates:
```bash
ls docs/ | head -10
```

### 7. Convert priority documents

For each high-priority document:

1. Determine the appropriate PAVED type:
   - Service/module documentation → Component
   - Operational procedures → Runbook
   - Design decisions → ADR

2. Create a new PAVED document:
   ```bash
   ./target/release/pave new component <name>
   # or
   ./target/release/pave new runbook <name>
   # or
   ./target/release/pave new adr <name>
   ```

3. Migrate content from the legacy document into PAVED sections

4. Add Verification commands that prove accuracy

5. Add Examples with copy-paste code

### 8. Validate converted documents

Check that converted docs pass validation:

```bash
./target/release/pave check docs/components/<name>.md
```

In gradual mode, errors appear as warnings. Address them before disabling gradual mode.

### 9. Set up CI integration

Help the user add pave to their CI pipeline. Provide the appropriate configuration for their CI system (GitHub Actions, GitLab CI, etc.).

### 10. Install git hooks

Set up pre-commit hooks:

```bash
./target/release/pave hooks install
```

For verification in hooks:

```bash
./target/release/pave hooks install --verify
```

### 11. Document progress tracking

Show the user how to monitor adoption progress:

```bash
./target/release/pave check
./target/release/pave coverage
```

### 12. Plan gradual mode exit

Establish criteria for disabling gradual mode:
- All priority documents converted
- CI passing consistently
- Team comfortable with workflow

Set a target date and add it to `gradual_until`.

## Rollback

If adoption causes problems:

1. Remove the git hooks:
   ```bash
   ./target/release/pave hooks uninstall
   ```

2. Remove CI integration (revert workflow file changes)

3. Optionally remove `.pave.toml`:
   ```bash
   rm .pave.toml
   ```

The original documentation remains intact throughout the process.

## Verification

Confirm pave is correctly configured:

```bash
./target/release/pave config list
```

Confirm validation runs:

```bash
./target/release/pave check
```

## Escalation

If issues arise during onboarding:

1. Check the user-facing guide at `site/docs/onboarding-existing-projects.md` for common patterns
2. Review project-specific documentation needs
3. Open an issue on the pave repository with:
   - Project structure summary
   - Error messages or unexpected behavior
   - Steps attempted

## Examples

### Onboarding a Python project

```bash
# Assess existing docs
./target/release/pave adopt

# Initialize with defaults
./target/release/pave init

# Enable gradual mode
# Edit .pave.toml: gradual = true

# Convert README to component
./target/release/pave new component my-python-lib

# Validate
./target/release/pave check
```

### Onboarding a monorepo

For monorepos, configure the docs root appropriately:

```bash
./target/release/pave init --docs-root packages/my-service/docs
```

Or set up multiple pave configurations:

```bash
# In each package directory
cd packages/service-a && ./target/release/pave init
cd packages/service-b && ./target/release/pave init
```

### Handling mixed documentation styles

When a project has multiple doc formats (Markdown, RST, AsciiDoc):

1. Focus pave on Markdown files initially
2. Exclude other formats in `.pave.toml`:
   ```toml
   [mapping]
   exclude = ["**/*.rst", "**/*.adoc"]
   ```
3. Convert other formats to Markdown over time

## Paths

- `src/commands/adopt.rs` - Adopt command implementation
- `src/commands/init.rs` - Init command implementation
- `site/docs/onboarding-existing-projects.md` - User-facing guide
