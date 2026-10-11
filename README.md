# Function

Function is a little AI assistant that lives on your computer. Give it a task in plain English, and it can use your apps, browser, keyboard, mouse, and screen to help get it done.

It is still a work in progress. Some parts are solid, some are experimental, and bugs occasionally insist on being part of the team.

## What it can do

- **Control your desktop:** open apps, click, type, press keys, scroll, and take screenshots.
- **Work through multi-step tasks:** observe the screen, act, check what happened, and try to recover when something fails.
- **Talk or type:** voice transcription and speech features depend on your configured providers and platform support.
- **Use different AI providers:** configure a supported provider instead of being locked into one.
- **Remember useful things:** ask Function to save a preference or fact, then have it recall or remove that memory later.
- **Keep context under control:** context compaction and image processing help avoid sending unnecessarily huge requests.

Function is built with Rust and GPUI, with separate crates for the interface, agent, tools, memory, providers, platform integration, and configuration.

## Status

This is an early-stage project, not a polished, production-ready assistant. Features and settings may change, and computer-control actions can have real consequences. Check what Function is about to do, keep backups of important files, and don't hand it a task you can't afford to have go wrong.

## Getting started

### Requirements

- Rust and Cargo (use the current stable toolchain unless the project CI specifies otherwise).
- The platform-specific build dependencies required by GPUI and the native audio/desktop integrations.
- An API key for a supported AI provider if you want to use a hosted model. Provider availability and image/voice support vary.

### Run it

From the repository root:

```sh
cargo run --bin function
```

For an optimized build:

```sh
cargo build --release --bin function
```

Run the workspace tests with:

```sh
cargo test --workspace
```

If a native dependency fails to build, check the relevant crate's error first. Desktop frameworks are very good at turning one missing system package into a surprisingly long afternoon.

## Using memory

Function's persistent memory is meant for short, useful notes, not a transcript of everything you say.

- Ask Function explicitly to remember a stable preference or fact.
- Ask what it remembers to retrieve saved notes.
- Ask it to forget a specific note or clear a category when you no longer want it stored.
- Don't save passwords, API keys, access tokens, or other secrets.

Memory is stored locally through Function's configured memory store. If that store cannot be opened, the app currently falls back to in-memory storage for that run, which means those fallback notes won't survive restarting the app. See [Privacy Policy](docs/PRIVACY.md) for how prompts and screenshots may be handled by your selected AI provider.

## How the code is organized

| Crate | What it does |
| --- | --- |
| `crates/app` | Application entry point and wiring |
| `crates/ui` | GPUI interface and views |
| `crates/agent` | Agent loop, tool dispatch, context management |
| `crates/tools` | Desktop, browser, filesystem, terminal, and memory tools |
| `crates/memory` | Persistent and in-memory context stores |
| `crates/providers` | LLM, image, speech, and provider integrations |
| `crates/platform` | OS integration, audio, hotkeys, and window behavior |
| `crates/config` | Settings, configuration, and credentials |

For product direction and more detailed technical notes, start with [PRODUCT.md](PRODUCT.md) and the files in [docs/](docs/).

## Contributing

Bug reports, small fixes, and thoughtful improvements are welcome. Please read [CONTRIBUTING.md](CONTRIBUTING.md) before opening a pull request. If you find a security issue, please follow [SECURITY.md](SECURITY.md) instead of posting exploit details publicly.

## Project policies

- [Privacy Policy](docs/PRIVACY.md)
- [Memory Guide](docs/MEMORY.md)
- [Terms of Use](docs/TERMS.md)
- [Security Policy](SECURITY.md)
- [Code of Conduct](CODE_OF_CONDUCT.md)
- [Contributing Guide](CONTRIBUTING.md)
- [License](LICENSE)

These policies are practical project documents, not legal advice. If Function is distributed to the public or used commercially, have the policies reviewed for the actual product, jurisdictions, and data flows before treating them as final.

## License

Function is offered under either the [MIT License](LICENSE-MIT) or the [Apache License, Version 2.0](LICENSE-APACHE), at your option. See [LICENSE](LICENSE) for details.
