---
title: Update
description: New versions of Winlux install from Settings, with a restore point and automatic rollback.
---

## From Settings

1. Open **Settings › Updates**.
2. Click **“Check”**. The page shows the installed version and the available version.
3. If a newer version exists, click **“Install”** and follow the progress.
4. Restart when it suits you: Winlux never forces a restart.

![The Updates page](img:mises-a-jour.jpg "Settings › Updates: installed version, available version, automatic updates.")

On the same page, **“Automatic updates”** installs new versions without any action from you. The base system packages are updated in **Settings › System updates**.

## What protects your system

- Each update is downloaded from the project's published releases, and its size and SHA-256 checksum are checked before installation.
- A **restore point** is created just before; the last three are kept.
- If the desktop no longer starts properly after an update, Winlux **rolls back by itself** to the previous version.
- Your documents, installed programs and settings are kept.

> [!note]
> **Coming from an old version?** An installation of v0.2.12 or older must be reinstalled once with a recent image: its update tool cannot fix itself. From v0.2.13 on, everything is done from Settings.

## Roll back

If something goes wrong after an update, restart and choose **“Winlux secours”** (rescue) in the boot menu (key `s`), then “Restore the previous restore point”.

Learn more: [Releases and updates](wiki:Publication-et-mises-a-jour).
