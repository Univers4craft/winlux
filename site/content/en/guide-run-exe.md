---
title: Run a Windows program
description: Double-click an .exe, install software with its installer, and set its permissions.
---

## Just double-click

Double-click an `.exe` file in Explorer or on the desktop: it opens straight away. There is nothing to install or configure first.

![The Winlux tour: running a Windows program](img:lancer-un-exe.jpg "The first-start tour shows how to run a Windows program.")

## Install software

1. Download the installer from the publisher's website (`setup.exe`, `installer.exe` or `.msi`).
2. Double-click it and follow the steps of its installer.
3. The software appears in the Start menu. To remove it, open **Installed apps**.

The most common installers (MSI, NSIS, Inno Setup) are supported. The Winlux **Store** also offers Windows and Linux software to install in one click.

## Permissions

The first time a program wants to use the **camera**, the **microphone**, **screen capture** or your **personal folders**, Winlux asks for your consent. You can change your mind at any time in Settings › Privacy and security, where network access can also be blocked program by program.

When an installer asks for administrator rights, Winlux opens a confirmation window.

## If a program does not open

| Message | What it means |
|---|---|
| “32-bit Windows program” | Support for 32-bit programs is still partial and this kind is not accepted yet. Look for the 64-bit version of the software. |
| “Missing function” | The program uses a Windows function that Winlux does not provide yet. Report it: that is what tells us what to write next. |
| The program closes without a word | Open Event Viewer from the Start menu: the cause is recorded there. |

After a crash, Winlux offers to **report the problem**: see [Get help](page:guide-help). The list of software already tried is on the [Tested applications](page:apps) page.

Learn more: [Application compatibility](wiki:Compatibilite-des-applications).
