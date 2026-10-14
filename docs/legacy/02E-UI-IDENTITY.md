# Function UI Identity

## General

Function should feel like a native operating-system utility combined with a high-quality developer tool.

The interface should not resemble a conventional chatbot.

## Main Assistant

The default assistant is compact.

Conceptually:

┌─────────────────────────────────────┐
│  Function                           │
│                                     │
│  Ask Function...              ◉     │
└─────────────────────────────────────┘

The exact layout can evolve.

The important characteristic is compactness.

## Expanded Workspace

When expanded, the interface becomes a task workspace.

It may contain:

- Current request
- Agent status
- Tool activity
- Result
- Input
- Task history

## Surfaces

Use a small number of surfaces.

Avoid:

Card
inside card
inside card
inside another card.

The interface should feel like a unified workspace.

## Borders

Borders should be subtle.

Use borders to establish structure rather than decoration.

## Radius

Use restrained corner radii.

Do not make every element a pill.

Pills should be reserved for:

- Status
- Tags
- Compact controls
- Shortcuts

## Icons

Use simple functional icons.

Icons should explain actions.

Do not use icons as decoration.

## Buttons

Buttons should be visually quiet until interaction.

Primary actions may use stronger contrast.

## Inputs

The main input is a major interaction surface.

It should feel immediate and focused.

The user should be able to press the global shortcut and start typing without additional navigation.

## Keyboard

Function is keyboard-first.

Important actions should have shortcuts.

Examples:

Double Command (⌘ ⌘)
Ctrl + Space

Escape
Cancel

Command/Ctrl + Enter
Confirm or execute

Command/Ctrl + K
Command palette

The exact shortcuts remain configurable.

## Activity

Agent activity should be readable at a glance.

Good:

Opening Chrome
Reading YouTube Studio
Found latest video
Retrieving analytics

Avoid:

Thinking...
Thinking...
Thinking...

Do not expose private model reasoning.

## Empty State

The empty state should be minimal.

It should communicate what Function can do without becoming an onboarding presentation.

## Error State

Errors should explain:

What happened
What Function attempted
What the user can do next

Avoid raw stack traces in the primary interface.
