---
title: Software and games
description: The Windows software and games that install and launch in Winlux, checked one by one, and the ones still in progress.
---

Every program on this page was downloaded from its publisher's official website, in its Windows version, then installed and launched in Winlux on our test bench. The list is updated after every check.

> [!note]
> List drawn up on 22 November 2026, from the published v0.2.21 release and the checks made since. Some rows apply to the next version, v0.3, not yet published.

## Software that works

| Software | Category | Checked |
|---|---|---|
| VLC | Multimedia | Installs, plays videos and music |
| Audacity 3.6 | Multimedia | Installs and uninstalls cleanly, opens and is usable |
| ScreenToGif | Multimedia | Opens and records the screen |
| OBS Studio | Multimedia | Opens its window with the preview ([#2262](issue:2262), [#2313](issue:2313)) |
| Paint.NET 5.1 | Image | Installs (Winlux fetches the full package from the publisher), opens and draws; the web installer's downloader does not finish yet ([#2752](issue:2752)) |
| 7-Zip (64 and 32 bit) | Files | Installs, opens and creates archives, in a window as well as in a console |
| Notepad++ 8.6 | Documents | Installs and uninstalls cleanly, edits files |
| SumatraPDF 3.6 | Documents | Opens and displays PDF files |
| Flow Launcher | Tools | Installs and searches for programs and files |
| PuTTY (64 and 32 bit) | Network | Launches and opens SSH sessions |
| FileZilla | Network | Installs and transfers files over FTP and SFTP |
| WinSCP (32 bit) | Network | Launches and transfers files over SFTP |
| Claude Code | Development | Installs and works in the terminal |
| Pythonwin | Development | Opens its main window ([#1785](issue:1785)) |

## Games that work

| Game | Graphics | Checked |
|---|---|---|
| Warzone 2100 4.7 | Direct3D 11 or OpenGL | Launches and plays |
| SuperTuxKart 1.5 | OpenGL | Launches and plays, about 116 frames per second on the bench |
| Xonotic | OpenGL | Launches and plays, over 220 frames per second on the bench |

## Installed cleanly, use not checked yet

| Software | Checked |
|---|---|
| WinMerge 2.16 | Clean install and uninstall |
| ShareX 16.1 | Clean install and uninstall |

## In progress

| Software | Status |
|---|---|
| Office | The deployment tool downloads Office and checks its signatures; the installation does not finish yet ([#2740](issue:2740)) |
| Adobe Acrobat Reader | Installs; the application is not usable yet ([#236](issue:236)) |
| Teams | Installs and starts, without reaching the sign-in screen ([#2511](issue:2511), [#2726](issue:2726)) |
| Adobe Creative Cloud | The installer starts and reaches the publisher's servers, its window does not show yet ([#2784](issue:2784)) |
| OneDrive | The installer does not start yet ([#2851](issue:2851)) |
| Claude for Windows | Installs and opens; the maximize and move defects are fixed, the whole app still needs a new check |
| Steam | Installs and downloads its client, does not connect yet ([#285](issue:285)) |
| IrfanView | Not fully tried yet ([#488](issue:488)) |
| Photoshop | Not fully tried yet |

## Your software or game is not on the list?

Try it! If it does not open, Winlux offers to report it with the name of the missing function, and that is what lets us make it work. See [Get help](page:guide-help#report-a-problem) and, for details by type of program, [Applications](page:apps).
