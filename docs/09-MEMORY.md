# Memory

## Purpose

Memory allows the assistant to retain useful information between sessions.

## Memory Types

### Preferences

Examples:

- Preferred browser
- Preferred applications
- UI preferences

### Applications

Examples:

- YouTube Studio
- Chrome
- VS Code
- Zed

### Websites

Remember useful websites and their purposes.

### User Instructions

Persistent instructions explicitly provided by the user.

### Task Context

Useful information from previous tasks.

## Storage

SQLite should be the primary persistent store.

## Retrieval

Memory should be retrieved based on relevance.

Do not insert the entire memory database into every AI request.

## Privacy

Memory should be inspectable and controllable by the user.

Sensitive information should not be stored unnecessarily.

## Deletion

Users should be able to delete stored memories.

## Embeddings

Semantic retrieval may use embeddings.

The embedding implementation should remain replaceable.
