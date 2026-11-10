---
title: Tested applications
description: What already works in Winlux, what does not work yet, and how to help us extend the list.
---

Winlux opens Windows programs thanks to a compatibility layer written from scratch. Many programs already work, but not all of them: this page tells it straight.

## By type of program

| Type of program | Status today |
|---|---|
| Simple 64-bit program (tools, utilities, command-line programs) | Usually works |
| `.msi`, NSIS or Inno Setup installer | Works |
| .NET application that ships its own runtime | Starts, but its interface does not always open |
| WinForms and WPF interfaces | Not yet |
| Direct3D 9, 11 and 12 games | Partial, to be checked game by game |
| 32-bit program | Partial: the first 32-bit programs start, many do not open yet |
| Drivers, antivirus, system-level anti-cheat | Not supported |
| Applications packaged as MSIX or AppX | Install, shortcuts and uninstall |

## Software tried

The list of software and games checked one by one, with what works and what is still in progress, is on the [Software and games](page:software) page.

This list grows with every version. The function-by-function details and the order of the next work items are in the wiki: [Application compatibility](wiki:Compatibilite-des-applications).

## What comes next

In order of impact on the number of programs unlocked: the most requested Windows functions, OpenGL and Vulkan for applications, the Visual C++ redistributable libraries, secure HTTPS connections, Direct3D shaders, then 32-bit programs. The [roadmap](wiki:Feuille-de-route) tracks progress.

## Your software is not on the list?

Try it! If it does not open, Winlux offers to report it, with the name of the missing function: that is exactly what lets us make it work. See [Get help](page:guide-help#report-a-problem).
