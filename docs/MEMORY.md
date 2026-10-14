# Function memory

Function has two distinct kinds of context:

- **Current conversation history** is passed to the model during the active task.
- **Persistent memory** is a small set of notes stored by the configured memory backend and retrieved at the start of a run.

Persistent memory is not a full transcript archive, and it is not a self-training system. It does not update the model's weights.

## Using it

Ask Function directly to remember a stable preference or useful fact. The agent can use the `memory` tool to:

- `remember`: save a short note in a category.
- `recall`: search saved notes.
- `forget`: remove a note by its ID.
- `clear_category`: remove notes in a category.

Function should only save notes when asked, and it should not store credentials or secrets. The memory tool checks for common secret-like strings, but that check is only a safeguard, not a guarantee. Don't use memory as a password manager.

## Categories

- `user_preference`
- `frequent_application`
- `frequent_website`
- `task_summary`
- `custom_instruction`

Notes are stored in `~/.function/swift/memory.json` through atomic writes. New memory state is published only after persistence succeeds. If the file cannot be opened, Function reports the error. The memory browser lets you review, forget, or clear notes with confirmation. Disabling persistent memory excludes its tool and retrieved context from new runs.

## Current limitations

Function's memory system is intentionally smaller than a full conversation archive. It does not yet provide SQLite full-text search across past sessions, automatic end-of-session learning, or reusable learned skills. Recall is lightweight text matching, not semantic/vector search. Treat returned memories as helpful context, not as an authoritative record.

See the [Privacy Policy](PRIVACY.md) for information about local storage and third-party providers.
