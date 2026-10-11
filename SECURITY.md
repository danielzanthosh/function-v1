# Security Policy

## Supported versions

Function is under active development. Unless a release explicitly says otherwise, assume only the latest version on the default branch is receiving security fixes.

## Reporting a vulnerability

Please report suspected vulnerabilities privately through GitHub's **Report a vulnerability** feature on this repository, if available. If private reporting is unavailable, contact the repository maintainer through their GitHub profile and ask for a private reporting channel.

Please include:

- A short description of the issue and its potential impact.
- The affected version, commit, platform, and configuration.
- Minimal reproduction steps or a proof of concept that does not access real user data.
- Any suggested mitigation.

Please do not publish exploit details, credentials, personal screenshots, or data belonging to other people in a public issue.

## What deserves a report?

Examples include credential exposure, unsafe command execution, bypasses of tool restrictions, unintended data disclosure, and vulnerabilities that let untrusted input control the local machine beyond the intended tool permissions.

## Response expectations

This is a small open-source project, so response times are not guaranteed. We'll do our best to acknowledge reports, understand their impact, and coordinate a fix and disclosure when practical. Please give maintainers reasonable time to investigate before making technical details public.

## Safe use

Function can interact with files, applications, and external AI providers. Use trusted providers, avoid putting secrets in prompts or memory, inspect consequential actions, and keep backups.
