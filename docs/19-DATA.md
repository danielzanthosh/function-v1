# Data

## Local Database

SQLite.

## Core Tables

Potential tables:

users
preferences
memories
applications
websites
tasks
task_events
settings

## Task Events

A task may contain:

- Timestamp
- Action
- Tool
- Result
- Status

## Migration

Database schema changes must use migrations.

## Data Lifecycle

The application should make it clear what information is stored locally.

Users should be able to remove stored data.
