---
title: Create the USB drive
description: Prepare a USB drive that boots Winlux on any PC, with the provided drive creator.
---

## With the USB drive creator

The Winlux USB drive creator downloads the latest image, checks it and writes it to the drive. It exists for a Windows PC (`.exe`) and for Linux or Winlux (`.deb`). Get it from the [Download](page:download) page.

![The Winlux USB drive creator](img:createur-cle-usb.jpg "The USB drive creator: choose the image, choose the drive, create.")

1. **Winlux image**: keep “Download the latest version”, or choose an ISO image you have already downloaded.
2. **USB drive**: pick the drive from the list. Only USB drives and SD cards are offered, never the PC's own disk.
3. **Create the USB drive**: a window reminds you of the drive's name and size. Nothing is erased before you click “Erase and create the drive”.
4. Wait for the writing and then the verification to finish: the drive is read back and compared with the image.

> [!warning]
> **Everything on the drive is erased.** Copy what you want to keep somewhere else before you start.

## With another tool

The Winlux image is “hybrid”: any tool that copies a disk image to a USB drive will do, and Ventoy boots it too. On Linux, from the command line, replacing `/dev/sdX` with the drive (and certainly not with a disk):

```sh
sudo dd if=winlux-live-v0.2.14.iso of=/dev/sdX bs=4M status=progress conv=fsync
```

## Boot the PC from the drive

1. Plug in the drive and switch on (or restart) the PC.
2. Immediately press the boot menu key: often **F12**, **F11**, **F8** or **Esc**, depending on the manufacturer.
3. Choose the USB drive. It boots in UEFI as well as legacy BIOS mode, and Secure Boot can stay enabled.
4. The Winlux menu appears: choose the Live session to try it, or the installation.

> [!tip]
> The Live session runs entirely in memory: it is the right time to check that the screen, Wi-Fi, sound and your programs work, without changing anything on your PC.

Learn more: [Installation in the wiki](wiki:Installation).
