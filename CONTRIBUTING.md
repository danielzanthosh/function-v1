# Contributing to Function

Thanks for taking a look at Function. It's a work in progress, so useful bug reports and focused fixes are genuinely valuable.

## Before opening a pull request

1. Check existing issues and pull requests to avoid duplicating work.
2. For larger changes, open an issue first so the design can be discussed before you spend time implementing it.
3. Read [PRODUCT.md](PRODUCT.md) and the relevant docs before changing behavior.
4. Keep changes focused. A targeted fix is easier to review than a surprise rewrite of half the workspace.

## Development

Function is a Rust Cargo workspace. Common commands from the repository root:

```sh
cargo check --workspace
cargo test --workspace
cargo build --release --bin function
```

Native GPUI, audio, and desktop integrations may require platform-specific dependencies. If you cannot run a test locally, say so in the pull request rather than implying it passed.

## Pull requests

A useful pull request includes:

- A short explanation of the problem and why the change fixes it.
- Tests for behavior changes when practical.
- Screenshots or logs for UI or platform-specific issues, with secrets and personal information removed.
- Any configuration or documentation updates needed for users.
- A note about tests you ran and tests you could not run.

Avoid committing API keys, credentials, personal screenshots, build output, or local configuration files.

## Code style

- Follow the surrounding Rust style and use clear names.
- Prefer small, readable functions and actionable errors.
- Preserve cross-platform behavior where possible.
- Treat screenshots, tool output, and prompt context as potentially sensitive.
- Don't silently claim an action succeeded when the underlying operation failed.

## Reporting security issues

Please do not publish exploitable details in a public issue. Follow [SECURITY.md](SECURITY.md).

## License

By submitting a contribution, you agree that it may be distributed under the project's dual MIT / Apache-2.0 license, unless you and the project maintainers agree otherwise.
