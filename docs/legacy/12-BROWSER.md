# Browser Automation

The browser is one of the primary tools.

## Goals

The assistant should be capable of completing common browser tasks.

Examples:

- Open websites
- Search
- Navigate dashboards
- Read information
- Fill forms
- Click controls
- Extract structured information

## Supported Browsers

Initial focus:

- Chrome
- Microsoft Edge

## Strategy

Prefer semantic browser automation when reliable.

Use visual/screen interaction when necessary.

## Authentication

The assistant should use existing user browser sessions where technically appropriate.

Never extract or expose passwords.

## Failure

If a page changes unexpectedly:

1. Re-observe.
2. Re-evaluate the page.
3. Attempt recovery.

## Confirmation

Require confirmation for consequential actions such as:

- Sending messages
- Purchases
- Account deletion
- Publishing content
