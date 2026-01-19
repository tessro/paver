# Runbook: Adding a New CLI Command to Pave

## When to Use
Use this runbook when extending pave with a new CLI command. This applies when you need to add functionality that should be accessible via `pave <command>`.

## Preconditions
- Rust toolchain installed (via [rustup](https://rustup.rs/))
- Repository cloned locally
- Understanding of the new command's purpose and expected behavior
- Familiarity with the [clap](https://docs.rs/clap) CLI framework

## Steps

### 1. Create the command module

Create a new file at `src/commands/<newcmd>.rs`:

```rust
//! Implementation of the `pave <newcmd>` command.

use anyhow::Result;

/// Arguments for the `pave <newcmd>` command.
pub struct NewcmdArgs {
    // Add your command arguments here
}

/// Execute the `pave <newcmd>` command.
pub fn execute(args: NewcmdArgs) -> Result<()> {
    // Implement command logic here
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_functionality() {
        // Add unit tests here
    }
}
```

### 2. Export the module

Edit `src/commands/mod.rs` to add the new module:

```rust
pub mod newcmd;
```

### 3. Define CLI arguments

Edit `src/cli.rs` to add the subcommand to the `Command` enum:

```rust
#[derive(Subcommand)]
pub enum Command {
    // ... existing commands ...

    /// Description of what the new command does
    Newcmd {
        /// Argument description
        #[arg()]
        some_arg: String,

        /// Optional flag description
        #[arg(long)]
        some_flag: bool,
    },
}
```

### 4. Wire up in main.rs

Edit `src/main.rs` to add a match arm for the new command:

```rust
use pave::commands::newcmd;

// Inside the match cli.command block:
Command::Newcmd { some_arg, some_flag } => {
    newcmd::execute(newcmd::NewcmdArgs {
        some_arg,
        some_flag,
    })?;
}
```

### 5. Run tests

```bash
cargo test
```

### 6. Test the command manually

```bash
cargo build
./target/debug/pave <newcmd> --help
./target/debug/pave <newcmd> <args>
```

### 7. Create component documentation

```bash
pave new component <newcmd>
```

Fill out the generated `docs/components/<newcmd>.md` with:
- Purpose of the command
- Interface (arguments, options, output)
- Configuration options (if any)
- Usage examples
- Common gotchas

### 8. Update the commands reference

Edit `site/docs/commands.md` to add documentation for the new command, following the existing format with examples and option tables.

## Rollback

If the command doesn't work correctly:

1. Revert changes to `src/main.rs`
2. Revert changes to `src/cli.rs`
3. Revert changes to `src/commands/mod.rs`
4. Delete `src/commands/<newcmd>.rs`

Or use git to revert:

```bash
git checkout -- src/main.rs src/cli.rs src/commands/
```

## Verification

After completing all steps, verify success with:

```bash
cargo test
```

Validate PAVED documentation:

```bash
./target/release/pave check
```

Verify help text appears correctly:

```bash
./target/release/pave --help
```

## Examples

### Adding a `stats` command

To add a command that displays documentation statistics:

1. Create `src/commands/stats.rs` with:
   ```rust
   pub struct StatsArgs {
       pub verbose: bool,
   }
   pub fn execute(args: StatsArgs) -> Result<()> { /* ... */ }
   ```

2. Add to `src/commands/mod.rs`:
   ```rust
   pub mod stats;
   ```

3. Add to `src/cli.rs` in the Command enum:
   ```rust
   Stats {
       #[arg(long)]
       verbose: bool,
   },
   ```

4. Add to `src/main.rs`:
   ```rust
   Command::Stats { verbose } => {
       stats::execute(stats::StatsArgs { verbose })?;
   }
   ```

5. Test: `cargo test && ./target/debug/pave stats --help`

## Escalation

If you encounter issues:

1. Check the [clap documentation](https://docs.rs/clap) for CLI argument syntax
2. Review existing commands in `src/commands/` for patterns
3. Open an issue on the pave repository with:
   - What you were trying to add
   - Error messages or unexpected behavior
   - Steps you followed
