---
title: Winlux herunterladen
description: Holen Sie sich das Installationsabbild der neuesten Version und den USB-Stick-Ersteller, und prüfen Sie, ob Ihr PC geeignet ist.
---

{{download}}

Die Dateien kommen direkt von der Seite der veröffentlichten Versionen des Projekts auf GitHub. Das Abbild heißt etwa `winlux-live-v0.2.17.iso`; es ist rund 1,2 GB groß.

## Was Sie brauchen

| | Empfohlenes Minimum |
|---|---|
| Prozessor | 64-Bit-PC (x86-64) |
| Arbeitsspeicher | 4 GB |
| Festplatte | 24 GB frei (mindestens 20 GB) |
| Start | UEFI oder klassisches BIOS, Secure Boot an oder aus |
| Bildschirm | von 1024 × 600 bis 4K |
| USB-Stick | 4 GB oder mehr, für die Installation auf einem echten PC |

## Drei Wege zum Ausprobieren

::: cards
### In einer virtuellen Maschine
Der sicherste Weg, Winlux kennenzulernen: Ihr PC bleibt unberührt. Siehe den Abschnitt [virtuelle Maschine](#in-einer-virtuellen-maschine-ausprobieren) weiter unten.

### Vom USB-Stick, ohne Installation
Die „Live“-Sitzung startet Winlux im Arbeitsspeicher: Sie testen Bildschirm, Netzwerk und Ton, und nichts wird auf die Festplatte geschrieben. [Stick erstellen](page:guide-usb)

### Auf dem PC installiert
Allein auf der Festplatte oder neben einem vorhandenen Windows, mit einem Menü zur Auswahl beim Start. [Winlux installieren](page:guide-install)
:::

## In einer virtuellen Maschine ausprobieren

| Software | Empfohlene Einstellungen |
|---|---|
| virt-manager, GNOME Boxen (QEMU/KVM) | 4 GB Arbeitsspeicher, 2 bis 4 Prozessoren, 24-GB-Festplatte, Anzeige „Standard-VGA“ oder QXL |
| VirtualBox | Typ „Linux / Ubuntu (64-bit)“, 4 GB, 24 GB, Grafikcontroller VMSVGA |
| VMware | „Ubuntu 64-Bit“, 4 GB, 24 GB |

Starten Sie die virtuelle Maschine von der ISO-Datei und folgen Sie dann [der Installation](page:guide-install) wie auf einem echten PC. Unter Einstellungen › Bildschirm passt die Option „Größe der VM folgen“ den Desktop an das Fenster an.

## Die heruntergeladene Datei prüfen

Der USB-Stick-Ersteller prüft das Abbild selbst. Wenn Sie es anders verwenden, vergleichen Sie seine SHA-256-Prüfsumme mit der daneben veröffentlichten `.sha256`-Datei. Unter Linux:

```sh
sha256sum -c winlux-live-v0.2.17.iso.sha256
```

Die beiden Prüfsummen müssen übereinstimmen; andernfalls laden Sie das Abbild erneut herunter.

> [!note]
> Winlux ist bei Ihnen schon installiert? Dann müssen Sie hier nichts herunterladen: Neue Versionen werden über [Einstellungen › Updates](page:guide-updates) installiert.
