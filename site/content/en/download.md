---
title: Download Winlux
description: Get the installation image of the latest version and the USB drive creator, and check that your PC is suitable.
---

{{download}}

The files come straight from the project's releases page on GitHub. The image is named like `winlux-live-v0.2.15.iso`; it weighs about 2 GB.

## What you need

| | Recommended minimum |
|---|---|
| Processor | 64-bit PC (x86-64) |
| Memory | 4 GB |
| Disk | 24 GB free (20 GB at the very least) |
| Boot | UEFI or legacy BIOS, Secure Boot on or off |
| Screen | from 1024 × 600 up to 4K |
| USB drive | 4 GB or more, to install on a real PC |

## Three ways to try it

::: cards
### In a virtual machine
The safest way to discover Winlux: nothing touches your PC. See the [virtual machine](#try-it-in-a-virtual-machine) section below.

### From a USB drive, without installing
The “Live” session runs Winlux in memory: you test the screen, network and sound, and nothing is written to the disk. [Create the drive](page:guide-usb)

### Installed on the PC
Alone on the disk, or next to an existing Windows with a menu to choose at startup. [Install Winlux](page:guide-install)
:::

## Try it in a virtual machine

| Software | Recommended settings |
|---|---|
| virt-manager, GNOME Boxes (QEMU/KVM) | 4 GB of memory, 2 to 4 processors, 24 GB disk, “Standard VGA” or QXL display |
| VirtualBox | Type “Linux / Ubuntu (64-bit)”, 4 GB, 24 GB, VMSVGA graphics controller |
| VMware | “Ubuntu 64-bit”, 4 GB, 24 GB |

Boot the virtual machine from the ISO file, then follow [the installation](page:guide-install) as on a real PC. In Settings › Display, the “Follow the VM size” option fits the desktop to the window.

## Check the downloaded file

The USB drive creator checks the image by itself. If you use it another way, compare its SHA-256 checksum with the `.sha256` file published next to it. On Linux:

```sh
sha256sum -c winlux-live-v0.2.15.iso.sha256
```

The two checksums must match; otherwise, download the image again.

> [!note]
> Already have Winlux installed? There is nothing to download here: new versions install from [Settings › Updates](page:guide-updates).
