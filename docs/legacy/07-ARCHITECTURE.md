# Architecture

The system consists of five major layers.

## 1. Interface

GPUI handles:

- Floating assistant
- Main workspace
- Settings
- Activity
- Input
- Status

## 2. Agent

The agent converts user intent into structured actions.

## 3. Tools

Tools provide controlled access to the computer.

Examples:

- Browser
- Mouse
- Keyboard
- Screen
- Files
- Terminal
- Applications

## 4. Platform

Platform code provides Windows and macOS implementations.

## 5. Providers

External services provide:

- LLM
- Speech recognition
- Search
- Text-to-speech

## Data Flow

User
↓
GPUI
↓
Agent
↓
LLM
↓
Tool request
↓
Tool runtime
↓
New computer state
↓
Agent
↓
Result
↓
GPUI

The LLM must never directly execute arbitrary system operations.

All operations go through controlled tools.
