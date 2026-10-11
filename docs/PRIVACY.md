# Privacy Policy

_Last updated: October 11, 2026_

This policy explains how the Function desktop application handles information. Function is an early-stage open-source project, and behavior can change as the code changes. Read this alongside the settings and provider documentation for the version you're running.

## The short version

Function runs on your computer and can inspect or control your desktop when you ask it to. That doesn't mean every piece of information stays on your computer: if you configure a hosted AI, speech, or search provider, Function may send that provider the information needed to complete your request.

## Information Function may handle

Depending on which features you use, this can include:

- **Prompts and conversation context:** text you type or dictate and relevant recent conversation messages sent to the model.
- **Screenshots and tool results:** images of your screen, text extracted from tool results, and details needed for the model to decide what to do next.
- **Voice data:** audio used by speech features, which may be processed locally or sent to the speech provider you've configured.
- **Settings and credentials:** provider configuration and credentials needed to connect to services. Do not paste secrets into prompts or ask Function to save them as memory.
- **Persistent memory:** short notes you explicitly ask Function to save, such as preferences or recurring instructions.
- **Diagnostic logs:** technical events and error details used to understand failures. Logs may include operational metadata; avoid sharing logs publicly before checking them for sensitive information.

The exact information involved depends on your configuration, provider, platform, and the task.

## Where information goes

### Local storage

Function stores its persistent memory through the configured local memory file. The precise path is resolved by the app's configuration code and may vary by platform. You can inspect or remove that file using your operating system's file tools after closing Function.

If the persistent store fails to initialize, the current application can fall back to an in-memory store. Memory saved during that fallback may disappear when the app closes.

### Third-party providers

When you use a hosted model, transcription service, text-to-speech service, or search provider, the relevant prompt, screenshot, audio, or query may be sent to that service. That provider processes the data under its own terms and privacy policy. Check those documents before enabling it, especially if your screen or prompt contains private information.

Function's source code does not make every provider's retention, training, or processing policy the same. Those policies are controlled by each provider.

### No promise of zero data transfer

Function is not a guarantee that all processing is local. Whether data leaves your device depends on the features and providers you enable. Review provider settings and avoid sending confidential information unless you've checked the provider's terms and your organization's rules.

## Memory and deletion

Function's memory tool is designed for concise notes, not automatic archival of every conversation. You can ask Function to recall saved notes, forget a specific note, or clear a category. Memory is not encrypted by this policy and should not be used to store passwords, API keys, tokens, recovery phrases, or other secrets.

Deleting a memory item does not necessarily delete copies already sent to a third-party provider, included in application logs, or saved elsewhere by your operating system.

## Children's privacy

Function is general-purpose desktop software, not a service designed to collect personal information from children. If you're a minor, get help from a parent or guardian before connecting services or using Function with private or sensitive information.

## Security

The project aims to limit unnecessary context, protect credentials through the configured credential abstractions, and make tool execution explicit. No software can promise perfect security. Keep Function and its dependencies updated, use trusted providers, and review actions that affect your files or accounts.

If you believe you've found a security issue, please follow [SECURITY.md](../SECURITY.md).

## Changes to this policy

This policy may be updated as Function changes. The date at the top indicates the latest revision. Check the repository for the current version.

## Contact

For project questions or privacy concerns, open a GitHub issue without including private data. For suspected security vulnerabilities, use the private reporting options described in [SECURITY.md](../SECURITY.md).

---

This document is a starting point for an open-source project and is not legal advice. It should be reviewed by a qualified professional before being used as a binding privacy notice for a distributed product.
