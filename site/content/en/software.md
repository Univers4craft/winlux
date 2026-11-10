---
title: Compatible software and games
description: The list of Windows software and games that install and run on Winlux, and of those still in progress.
---

Every program on this page was installed and then started on Winlux, in its official Windows version, as downloaded from its publisher. None of them was modified for the occasion.

> [!note]
> These tests are run on the test bench, in a virtual machine, on the version under development. Some fixes will arrive with the next published version.

## Software that works

They install (or unpack, for portable versions), open and work normally.

| Software | What it is for | Note |
|---|---|---|
| VLC | Video and audio player | |
| Audacity | Audio recording and editing | Inno Setup installer |
| ScreenToGif | Screen recording to animations | .NET application |
| 7-Zip | Archives and compression | The file manager works; `7z.exe` in a console lists archives correctly but reports an error on exit ([#1479](issue:1479)) |
| Notepad++ | Text editor | NSIS installer, clean install and uninstall |
| SumatraPDF | PDF reader | Portable version |
| Flow Launcher | Program launcher and search | .NET application |
| PuTTY | SSH terminal | 64-bit and 32-bit versions |
| FileZilla | FTP and SFTP file transfer | |
| WinSCP | SFTP file transfer | 32-bit version |
| Claude Code | Command-line programming assistant | |

## Games that work

| Game | Genre | Graphics |
|---|---|---|
| SuperTuxKart 1.5 | Racing | OpenGL, windowed and full screen |
| Warzone 2100 4.7 | Real-time strategy | OpenGL, Vulkan and Direct3D 11 |
| Xonotic | First-person shooter | OpenGL, in both of its versions |

## Installation checked, use not yet

These programs install and uninstall cleanly, but using them has not been checked in detail yet.

| Software | What it is for |
|---|---|
| WinMerge | Comparing files and folders |
| ShareX | Screen capture and sharing |

## In progress

These programs are moving forward, but cannot be used from start to finish yet. Each blocker has its own issue on GitHub.

| Software | Where it stands |
|---|---|
| Paint.NET | It installs and starts its .NET runtime; its main window is not usable yet ([#138](issue:138)) |
| Claude desktop client for Windows | It starts, minimizes and closes; maximizing it or moving its window does not work yet ([#1476](issue:1476), [#1477](issue:1477)) |
| Steam | It installs and updates its client, but is not usable yet ([#285](issue:285), [#1440](issue:1440)) |
| Photoshop | Not tested all the way through yet |
| IrfanView | Not yet ([#488](issue:488)) |

What does not work yet, type of program by type of program, is explained on the [Tested applications](page:apps) page.

## Your software is not on the list?

Try it! If it does not open, Winlux offers to report it, with the name of the missing function. See [Get help](page:guide-help#report-a-problem).
