<#
.SYNOPSIS
    Packages Function as a standalone Windows x64 application.
.DESCRIPTION
    Builds the release binary with embedded icon and GUI subsystem,
    assembles the distribution folder, and creates a distributable ZIP package.
#>

$ErrorActionPreference = "Stop"

$Root = $PSScriptRoot
$DistDir = Join-Path $Root "dist"
$AppDir = Join-Path $DistDir "Function-Windows-x64"
$ZipPath = Join-Path $DistDir "Function-Windows-x64.zip"

Write-Host "==> Building Function in Release mode..." -ForegroundColor Cyan
cargo build --release --bin function

if ($LASTEXITCODE -ne 0) {
    Write-Error "Cargo build failed with exit code $LASTEXITCODE"
    exit 1
}

Write-Host "==> Creating distribution folder: $AppDir" -ForegroundColor Cyan
if (Test-Path $AppDir) {
    Remove-Item $AppDir -Recurse -Force
}
New-Item -ItemType Directory -Force -Path (Join-Path $AppDir "assets") | Out-Null

Write-Host "==> Copying binary and assets..." -ForegroundColor Cyan
Copy-Item (Join-Path $Root "target\release\function.exe") -Destination (Join-Path $AppDir "Function.exe") -Force
Copy-Item (Join-Path $Root "assets\icon.ico") -Destination (Join-Path $AppDir "assets\icon.ico") -Force
Copy-Item (Join-Path $Root "assets\icon.png") -Destination (Join-Path $AppDir "assets\icon.png") -Force
if (Test-Path (Join-Path $Root "assets\brand")) {
    Copy-Item (Join-Path $Root "assets\brand") -Destination (Join-Path $AppDir "assets\brand") -Recurse -Force
}

# Copy installer and readme
$ShortcutScript = @"
`$ScriptDir = Split-Path -Parent `$MyInvocation.MyCommand.Definition
`$ExePath = Join-Path `$ScriptDir "Function.exe"
`$IconPath = Join-Path `$ScriptDir "assets\icon.ico"
`$WshShell = New-Object -ComObject WScript.Shell

`$DesktopPath = [Environment]::GetFolderPath("Desktop")
`$DesktopShortcut = `$WshShell.CreateShortcut((Join-Path `$DesktopPath "Function.lnk"))
`$DesktopShortcut.TargetPath = `$ExePath
`$DesktopShortcut.WorkingDirectory = `$ScriptDir
if (Test-Path `$IconPath) {
    `$DesktopShortcut.IconLocation = "`$IconPath,0"
}
`$DesktopShortcut.Description = "Function AI Assistant"
`$DesktopShortcut.Save()

`$StartMenuPath = [Environment]::GetFolderPath("StartMenu")
`$ProgramsPath = Join-Path `$StartMenuPath "Programs"
`$StartShortcut = `$WshShell.CreateShortcut((Join-Path `$ProgramsPath "Function.lnk"))
`$StartShortcut.TargetPath = `$ExePath
`$StartShortcut.WorkingDirectory = `$ScriptDir
if (Test-Path `$IconPath) {
    `$StartShortcut.IconLocation = "`$IconPath,0"
}
`$StartShortcut.Description = "Function AI Assistant"
`$StartShortcut.Save()

Write-Host "Function shortcuts created on Desktop and Start Menu!" -ForegroundColor Green
"@
Set-Content -Path (Join-Path $AppDir "Install-Shortcut.ps1") -Value $ShortcutScript -Encoding UTF8

$Readme = @"
==================================================
  Function - Desktop AI Assistant (Windows x64)
==================================================

QUICK START:
1. Double-click "Function.exe" to start the application.
2. It runs silently in your Windows system tray.
3. Press [Ctrl + Space] anytime to summon or toggle Function.
4. Press [Escape] to dismiss or go back.

KEY FEATURES:
- Global Hotkey: Ctrl + Space (with mechanical switch sound)
- Settings & Customization: Ctrl + , or type "settings"
- Built-in Calculator, Shell Execution, and Web Search
- Natural Language Assistant with full chat history and conversation storage

SHORTCUT:
Run "Install-Shortcut.ps1" in PowerShell to add shortcuts to Desktop and Start Menu.
"@
Set-Content -Path (Join-Path $AppDir "README.txt") -Value $Readme -Encoding UTF8

Write-Host "==> Creating ZIP archive: $ZipPath" -ForegroundColor Cyan
if (Test-Path $ZipPath) {
    Remove-Item $ZipPath -Force
}
Compress-Archive -Path "$AppDir\*" -DestinationPath $ZipPath -Force

Write-Host "==> Packaging complete!" -ForegroundColor Green
Write-Host "Folder: $AppDir" -ForegroundColor White
Write-Host "ZIP:    $ZipPath" -ForegroundColor White
