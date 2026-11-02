---
title: Install Winlux
description: The installer guides you step by step, from choosing the language to restarting, alone on the disk or next to Windows.
---

Start **“Install Winlux”** from the Live session desktop, or choose the installation in the drive's menu. The installer adapts to the size of your screen.

![The Network step of the installer](img:installateur-reseau.jpg "The Winlux installer, Network step: Wi-Fi and proxy if needed.")

## The steps

1. **Language and keyboard**: French, English, German, Spanish and others; AZERTY, QWERTY, QWERTZ layouts…
2. **Network**: connect to Wi-Fi if needed; a corporate proxy can be entered.
3. **Installation type and disk**: see below.
4. **Import from Windows**: bring over the wallpaper, favourites and documents from a Windows partition, or migrate from another PC over the network.
5. **Usage profile**: a short hardware test, then office, gaming, development or server. Winlux tunes the processor, memory and graphics card accordingly.
6. **Account**: a **local** account, with name, password and picture. No online account is required.
7. **Summary**: read everything again, then click “Install”.

## Choose where to install

| Choice | What happens |
|---|---|
| Whole disk (default) | Winlux takes the whole chosen disk. Its contents are erased. |
| Next to Windows | The installer shrinks the Windows partition while keeping a margin; at startup, a menu offers both systems. |
| Advanced mode | Manual partitioning: create, delete, resize, format. |
| Encryption | Optional: the passphrase is asked at every startup. |

![Choosing the usage profile](img:installateur-profil.jpg "The usage profile tunes the system to how you use it; you can change it later.")

> [!warning]
> If Windows is installed on the PC, shut it down completely before installing Winlux next to it (hold Shift while clicking Shut down): otherwise its partition stays locked by fast startup.

At the end, remove the USB drive and restart. The installer never offers the drive the PC booted from as a destination.

Learn more: [Installation in the wiki](wiki:Installation) · [unattended installation of several machines](repo:docs/AUTOINSTALL.md).
