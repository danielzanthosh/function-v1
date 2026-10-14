# GPUI

GPUI is the primary desktop UI framework.

## Goals

The application should feel native and highly responsive.

## GPUI Responsibilities

- Windows
- Views
- Layout
- Rendering
- Keyboard actions
- Input
- Animation
- Themes

## UI Architecture

Keep UI state separate from agent state.

Views observe application state and trigger actions.

## Actions

Use GPUI actions for keyboard-first interaction.

Important actions may include:

- OpenFunction (alias OpenAssistant)
- CloseFunction (alias CloseAssistant)
- SubmitRequest
- CancelTask
- ToggleExpanded
- OpenSettings
- OpenCommandPalette

## Performance

Avoid unnecessary UI re-rendering.

Long-running AI operations must occur outside blocking UI execution.

## Custom Components

Prefer reusable GPUI components over duplicated view code.

## Compatibility

Because GPUI evolves quickly, isolate framework-specific code where practical.
