# Agent

## Core Loop

The agent follows:

OBSERVE
THINK
ACT
OBSERVE
THINK
ACT
DONE

## State

The agent should maintain:

- Current task
- Current step
- Available tools
- Relevant memory
- Recent observations
- Tool results
- Errors
- Completion state

## Tool Calls

Tools should have strict schemas.

Example:

mouse.click(x, y)

keyboard.type(text)

browser.navigate(url)

browser.click(selector)

screen.capture()

terminal.execute(command)

## Safety

Dangerous or irreversible actions should require confirmation.

Examples:

- Deleting files
- Sending messages
- Purchasing something
- Changing system settings
- Running potentially destructive commands

## Completion

The agent should explicitly determine when the task is complete.

It should not continue acting after completion.

## Failure

If an action fails:

1. Capture the failure.
2. Re-observe the environment.
3. Attempt a safe recovery if possible.
4. Otherwise report the failure.

## Transparency

Show the user what actions are being performed.

Do not display private model reasoning.
