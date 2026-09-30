# Project Structure

Suggested structure:

project/
├── Cargo.toml
├── Cargo.lock
├── PRODUCT.md
├── README.md
│
├── crates/
│   ├── app/
│   ├── agent/
│   ├── memory/
│   ├── tools/
│   ├── providers/
│   ├── platform/
│   ├── config/
│   └── ui/
│
├── assets/
│
├── docs/
│
└── prompts/

## app

Application entry point and lifecycle.

## ui

GPUI views, components, layouts, themes, and visual systems.

## agent

Agent loop, task execution, tool orchestration, and state.

## memory

Persistent context and retrieval.

## tools

Computer capabilities exposed to the agent.

## providers

AI, speech, search, and text-to-speech integrations.

## platform

Windows and macOS implementations.

## config

Configuration and secure credential access.

## Principle

Keep the core agent independent from the UI.

The UI should display agent state rather than contain agent logic.
