# AI Desktop Assistant

A native AI computer assistant for Windows and macOS.

## Concept

Press:

macOS: Option + Space

Windows: Alt + Space

Speak or type a task.

The assistant understands the request and can interact with the computer through controlled tools.

## Features

- Native GPUI interface
- AI agent
- Voice interaction
- Browser automation
- Mouse and keyboard control
- Screen observation
- Persistent memory
- Search
- Configurable AI providers
- Windows support
- macOS support

## Architecture

Rust
├── GPUI
├── Agent
├── Tools
├── Memory
├── Providers
└── Platform

## Development

Read PRODUCT.md first.

Then follow the documentation in docs/.

Implementation prompts are located in prompts/.

Build the project incrementally.

## Philosophy

The assistant should feel like a native computer tool rather than a chatbot.

It should be fast, transparent, controllable, and useful.
