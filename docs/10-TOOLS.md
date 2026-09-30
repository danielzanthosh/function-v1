# Tool System

Tools are controlled capabilities exposed to the agent.

## Tool Categories

### Computer

- mouse
- keyboard
- screen

### Browser

- open
- navigate
- click
- type
- read
- screenshot

### Files

- list
- read
- write
- move
- delete

### Applications

- launch
- focus
- close

### Terminal

- execute

### Search

- web search

## Tool Requirements

Every tool must define:

- Name
- Description
- Input schema
- Output schema
- Permission level
- Failure behavior

## Tool Permissions

Tools should have permission categories:

SAFE
CONFIRM
RESTRICTED

The agent cannot bypass the tool permission system.

## Tool Results

Tool results should be structured.

Avoid returning enormous raw outputs when a summary is sufficient.
