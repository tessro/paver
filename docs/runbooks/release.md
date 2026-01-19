# Runbook: Release Paver

## When to Use
When releasing a new version of paver. This includes new features, bug fixes, or breaking changes that should be published as a tagged release.

## Preconditions
- On main branch with clean working directory (`git status` shows no changes)
- All tests passing (`cargo test` succeeds)
- `paver check` passes on all documentation
- Version number determined following semantic versioning

## Steps

1. Verify working directory is clean and on main:
   ```bash
   git checkout main
   git pull origin main
   git status
   ```

2. Run tests to ensure everything passes:
   ```bash
   cargo test
   ```

3. Validate all documentation:
   ```bash
   paver check
   ```

4. Update version in Cargo.toml:
   ```bash
   # Open Cargo.toml and change the version field to the new version
   # Example: version = "0.2.0"
   ```

5. Build the release binary:
   ```bash
   cargo build --release
   ```

6. Commit the version bump:
   ```bash
   git add Cargo.toml Cargo.lock
   git commit -m "Bump version to v0.x.x"
   ```

7. Create an annotated tag:
   ```bash
   git tag -a v0.x.x -m "Release v0.x.x"
   ```

8. Push changes and tags:
   ```bash
   git push origin main
   git push origin --tags
   ```

## Rollback

If a release needs to be reverted:

1. Delete the remote tag:
   ```bash
   git push origin --delete v0.x.x
   ```

2. Delete the local tag:
   ```bash
   git tag -d v0.x.x
   ```

3. Revert the version commit if needed:
   ```bash
   git revert HEAD
   git push origin main
   ```

## Verification

Confirm the release was successful:

```bash
# Check the latest tag exists
git tag -l | tail -1

# Verify the tag points to the correct commit
git show v0.x.x --oneline --no-patch

# Confirm the release binary version
./target/release/paver --version
```

## Examples

**Patch release** (bug fixes):
```bash
# Version: 0.1.0 -> 0.1.1
git tag -a v0.1.1 -m "Release v0.1.1"
```

**Minor release** (new features):
```bash
# Version: 0.1.1 -> 0.2.0
git tag -a v0.2.0 -m "Release v0.2.0"
```

**Major release** (breaking changes):
```bash
# Version: 0.2.0 -> 1.0.0
git tag -a v1.0.0 -m "Release v1.0.0"
```

## Escalation
For a small project like paver, escalation is typically not needed. If issues arise, review the git log and tag history to diagnose problems.
