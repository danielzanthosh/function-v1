# Layout

## Compact Mode

The default interface is a small floating assistant.

Conceptually:

+--------------------------------+
|  ●  Ask anything...       🎙   |
+--------------------------------+

It should occupy minimal screen space.

## Expanded Mode

Expanded mode becomes a focused workspace.

Structure:

┌─────────────────────────────────────┐
│ Assistant                     •••   │
├─────────────────────────────────────┤
│                                     │
│ User request                        │
│                                     │
│ Agent activity                      │
│                                     │
│ ┌─────────────────────────────────┐ │
│ │ Opening Chrome                  │ │
│ │ Navigating to YouTube Studio   │ │
│ │ Reading analytics              │ │
│ └─────────────────────────────────┘ │
│                                     │
│ Result                              │
│                                     │
├─────────────────────────────────────┤
│ Type a message...             🎙    │
└─────────────────────────────────────┘

## Agent Activity

The user should be able to understand what the assistant is doing.

Activity should be concise.

Example:

Opening Chrome
→ Navigating to YouTube Studio
→ Finding latest video
→ Reading analytics

Do not expose internal chain-of-thought.

Show actions and statuses, not private reasoning.

## Responsive Behavior

The interface should adapt to:

- Small screens
- Large screens
- Different DPI settings
- Windows scaling
- macOS Retina displays

## Windows

Respect Windows window behavior and conventions.

## macOS

Respect macOS window behavior and conventions.
