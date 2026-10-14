# Security

Security is a core requirement.

## Principles

- Least privilege
- Explicit permissions
- Secure secrets
- Controlled tools
- User confirmation
- No arbitrary hidden actions

## Sensitive Actions

Require confirmation before:

- Sending communications
- Deleting data
- Purchasing
- Changing security settings
- Running dangerous commands

## API Keys

Use OS credential storage.

Never commit secrets.

## Terminal

Terminal execution is powerful and must be treated as a privileged capability.

The application should clearly communicate when terminal commands are executed.

## Logs

Logs must redact:

- API keys
- Tokens
- Passwords
- Authentication cookies
- Sensitive user data

## Browser

Never intentionally expose credentials to the model.

The agent should interact with authenticated sessions without extracting secrets.
