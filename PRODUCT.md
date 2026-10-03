# Product Context

## Product

A native desktop AI computer assistant for Windows and macOS.

The assistant is designed to operate a computer on the user's behalf using natural language and voice.

It should feel like a serious desktop productivity tool rather than a traditional chatbot.

## Core Idea

The user presses a global shortcut:

- macOS: Double Command (⌘ ⌘)
- Windows: Ctrl + Space

A compact floating assistant appears.

The user can speak or type a request.

The assistant interprets the request, plans the required actions, uses available computer tools, observes the results, and continues until the task is complete.

Example:

> "Tell me the analytics on my latest YouTube video."

The assistant may:

1. Open the browser.
2. Navigate to YouTube Studio.
3. Find the latest video.
4. Open analytics.
5. Read the relevant information.
6. Return the results to the user.

The goal is for the assistant to operate the computer similarly to how a human would.

## Platforms

Primary platforms:

- Windows
- macOS

Both platforms are first-class targets.

The application should not feel like a Windows application awkwardly ported to macOS or vice versa.

## Technology Direction

Core technology:

- Rust
- GPUI
- SQLite
- Playwright where appropriate
- Native OS APIs
- OpenAI-compatible AI APIs
- Whisper-based speech-to-text
- Optional web search
- OS or external text-to-speech

## Brand

Name:

Function

Function is a native AI computer assistant.

The visual identity is based on a monochrome geometric symbol featuring a bright central convergence point emerging from a dark field.

The brand language is:

- Precise
- Minimal
- Technical
- Native
- Quiet
- Fast
- Controlled

The primary visual system is monochrome.

Black and white define the brand.

Gray provides hierarchy.

Semantic colors are reserved for system states.

The icon is the primary visual identity and should not be altered casually.

Function should feel like serious computing software rather than a generic AI product.

## Design Direction

The interface should take inspiration from high-quality developer tools such as Zed.

It should be:

- Minimal
- Fast
- Dense when necessary
- Keyboard-first
- Native
- Professional
- Modern
- Quiet
- Highly responsive

Avoid:

- Generic AI chatbot layouts
- Giant chat windows
- Excessive rounded cards
- Glassmorphism everywhere
- Excessive gradients
- Cartoon robot imagery
- Fake futuristic interfaces
- Unnecessary animations

## Primary Interaction

The main interaction is a compact floating assistant.

States include:

- Idle
- Listening
- Processing
- Acting
- Waiting for confirmation
- Completed
- Error

The interface can expand into a larger workspace showing:

- User request
- Current agent state
- Tool actions
- Browser activity
- Memory/context
- Results
- Logs where useful

## Agent Model

The assistant operates through an observe-think-act loop.

Conceptually:

OBSERVE
↓
THINK
↓
ACT
↓
OBSERVE AGAIN
↓
THINK
↓
ACT
↓
DONE

The AI should request structured tools.

The runtime executes those tools.

The runtime then returns the new state to the agent.

## Memory

The assistant should maintain persistent context.

Memory may include:

- User preferences
- Frequently used applications
- Websites
- Previous tasks
- User-defined instructions
- Useful contextual information

Memory should be retrieved when relevant rather than blindly inserting the entire history into every request.

## Configuration

The user should be able to configure:

- AI base URL
- API key
- Model
- Speech-to-text provider
- Search provider
- Text-to-speech
- Hotkeys
- Permissions
- Memory behavior

Secrets must not be stored as plaintext configuration files.

## Product Philosophy

The assistant should feel like a tool that works with the computer, not another website opened inside the computer.

Speed, reliability, transparency, and user control are more important than visual gimmicks.
