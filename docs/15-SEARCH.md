# Search

Search is an optional capability.

## Purpose

Search allows the assistant to retrieve current external information.

## Architecture

Search should be implemented behind an abstraction.

The agent should call:

search(query)

rather than directly depending on a specific search provider.

## Results

Each result should provide:

- Title
- URL
- Relevant text
- Optional metadata

## Context

Only relevant search results should be passed into the AI context.

Avoid sending unnecessary page content.

## Failure

If search fails, the agent should continue where possible or clearly report the limitation.
