---
title: Update
description: New versions of Winlux install from Settings, with a restore point and automatic rollback.
---

## From Settings

1. Open **Settings › Updates › OS update**.
2. Click **“Check for updates”**. If a newer version exists, the page shows “Update available”, its size and what's new.
3. Click **“Download and install”** and follow the progress.
4. Restart when it suits you: Winlux never forces a restart.

![The OS update page](img:mises-a-jour.jpg "Settings › Updates › OS update: check, pause, automatic installation, history and going back to the previous version.")

On the same page, **“Install new versions automatically”** installs them without any action from you, outside your active hours and never during a game; **“Pause updates”** pauses them. The base system packages are updated from the **“Linux software and drivers”** row on the same page.

## What protects your system

- Each update is downloaded from the project's published releases, and the signature of its description, its size and its SHA-256 checksum are checked before installation.
- A **restore point** is created just before; the last three are kept.
- If the desktop no longer starts properly after an update, Winlux **rolls back by itself** to the previous version.
- Your documents, installed programs and settings are kept.

> [!note]
> **Coming from an old version?** An installation of v0.2.12 or older must be reinstalled once with a recent image: its update tool cannot fix itself. From v0.2.13 on, everything is done from Settings.

## Roll back

If something goes wrong after an update, restart and choose **“Winlux secours”** (rescue) in the boot menu (key `s`), then “Restore the previous restore point”.

Learn more: [Releases and updates](wiki:Publication-et-mises-a-jour).
