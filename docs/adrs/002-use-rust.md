# ADR: Use Rust for Implementation

## Status
Accepted

## Context
Pave is a CLI tool for validating and managing PAVED documentation. The implementation language choice affects:

- **Distribution**: How easily the tool can be installed and run on different systems
- **Performance**: Startup time and execution speed matter for developer workflows
- **Reliability**: Type safety and compile-time guarantees reduce runtime errors
- **Dependencies**: Fewer runtime dependencies simplify installation and reduce failures

The tool needs to be fast, reliable, and easy to distribute as a single binary.

## Decision
Implement pave in Rust using:

- **clap**: For CLI argument parsing with derive macros
- **serde**: For TOML/JSON configuration handling
- **Rust 2024 edition**: For latest language features

## Consequences
**Positive:**
- Single binary distribution with no runtime dependencies
- Fast startup time (<50ms) suitable for CI/CD pipelines
- Strong compile-time type checking catches errors early
- Memory safety without garbage collection pauses
- Cross-compilation support for Linux, macOS, and Windows

**Negative:**
- Steeper learning curve for contributors unfamiliar with Rust
- Requires Rust toolchain (rustup, cargo) for development
- Longer compile times compared to interpreted languages
- Smaller contributor pool than Python or JavaScript

## Alternatives Considered
**Go**: Simpler language with fast compilation, but less expressive type system. Error handling is more verbose, and the dependency story is less mature than Cargo.

**Python**: Excellent for rapid prototyping, but slow startup time (~200ms+) makes it unsuitable for CLI tools that run frequently. Requires Python runtime and dependency management (pip/pipenv/poetry).

**Node.js**: Large runtime overhead (~100MB), slow startup time. npm ecosystem has security concerns for CLI tools.

**Shell scripts**: Maximum portability but limited functionality. Hard to maintain complex validation logic, poor error handling, inconsistent behavior across shells.

## Verification

Prerequisites: Install the Rust toolchain via [rustup](https://rustup.rs/).

Verify the Rust implementation is working correctly:

```bash
# Build pave (optional, cargo run will build automatically)
cargo build

# Run the test suite
cargo test

# Check the binary works
cargo run -- --version
```

Expected: All tests pass, binary outputs version information.

## Examples

### Building Pave

```bash
# Debug build for development
cargo build

# Release build for distribution
cargo build --release
```

### Running Tests

```bash
# Run all tests
cargo test

# Run tests with output
cargo test -- --nocapture
```

### Cross-Compilation

```bash
# Add target for Linux
rustup target add x86_64-unknown-linux-musl

# Build for Linux
cargo build --release --target x86_64-unknown-linux-musl
```
