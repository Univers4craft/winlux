---
title: Den USB-Stick erstellen
description: Bereiten Sie einen USB-Stick vor, der Winlux auf jedem PC startet, mit dem mitgelieferten Stick-Ersteller.
---

## Mit dem USB-Stick-Ersteller

Der USB-Stick-Ersteller von Winlux lädt das neueste Abbild herunter, prüft es und schreibt es auf den Stick. Es gibt ihn für einen Windows-PC (`.exe`) und für Linux oder Winlux (`.deb`). Sie finden ihn auf der Seite [Herunterladen](page:download).

![Der USB-Stick-Ersteller von Winlux](img:createur-cle-usb.jpg "Der USB-Stick-Ersteller: Abbild wählen, Stick wählen, erstellen.")

1. **Winlux-Abbild**: Lassen Sie „Neueste Version herunterladen“ ausgewählt, oder wählen Sie ein bereits heruntergeladenes ISO-Abbild.
2. **USB-Stick**: Wählen Sie den Stick aus der Liste. Angeboten werden nur USB-Sticks und SD-Karten, nie die Festplatte des PCs.
3. **USB-Stick erstellen**: Ein Fenster nennt noch einmal Namen und Größe des Sticks. Nichts wird gelöscht, bevor Sie auf „Löschen und Stick erstellen“ klicken.
4. Warten Sie, bis das Schreiben und danach die Prüfung abgeschlossen sind: Der Stick wird zurückgelesen und mit dem Abbild verglichen.

> [!warning]
> **Der gesamte Inhalt des Sticks wird gelöscht.** Kopieren Sie vorher alles, was Sie behalten möchten, an einen anderen Ort.

## Mit einem anderen Werkzeug

Das Winlux-Abbild ist „hybrid“: Jedes Werkzeug, das ein Datenträgerabbild auf einen USB-Stick kopiert, ist geeignet, und auch Ventoy startet es. Unter Linux auf der Kommandozeile, wobei `/dev/sdX` durch den Stick ersetzt wird (und keinesfalls durch eine Festplatte):

```sh
sudo dd if=winlux-live-v0.2.17.iso of=/dev/sdX bs=4M status=progress conv=fsync
```

## Den PC vom Stick starten

1. Stecken Sie den Stick ein und schalten Sie den PC ein (oder starten Sie ihn neu).
2. Drücken Sie sofort die Taste für das Startmenü: oft **F12**, **F11**, **F8** oder **Esc**, je nach Hersteller.
3. Wählen Sie den USB-Stick. Er startet im UEFI- wie im klassischen BIOS-Modus, und Secure Boot kann aktiviert bleiben. Auf einem Tablet mit 32-Bit-Firmware (Atom) schalten Sie Secure Boot aus: Der Stick startet auch in diesem Modus.
4. Das Winlux-Menü erscheint: Wählen Sie die Live-Sitzung zum Ausprobieren oder die Installation.

> [!tip]
> Die Live-Sitzung läuft vollständig im Arbeitsspeicher: Jetzt ist der richtige Moment, um zu prüfen, ob Bildschirm, WLAN, Ton und Ihre Programme funktionieren, ohne etwas an Ihrem PC zu ändern.

Mehr dazu: [Installation im Wiki](wiki:Installation).
