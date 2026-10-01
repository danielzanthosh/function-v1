# Platform

## Primary Platforms

Windows and macOS are first-class platforms.

## Windows

Target:

- Windows 10
- Windows 11

Global hotkey:

Alt + Space

## macOS

Target modern supported macOS versions.

Global hotkey:

Command + ;

## Abstraction

Platform functionality should expose common interfaces.

Examples:

GlobalHotkey
ScreenCapture
MouseControl
KeyboardControl
ApplicationControl
WindowControl
Clipboard

## Structure

platform/
├── common/
├── windows/
└── macos/

## Principle

Platform-specific code belongs inside platform implementations whenever possible.

The agent should not need to know whether it is running on Windows or macOS.

## Permissions

The application must clearly explain required permissions.

Examples:

- Accessibility
- Screen recording
- Microphone
- Automation
