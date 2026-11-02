---
title: Winlux installieren
description: Das Installationsprogramm führt Sie Schritt für Schritt von der Sprachwahl bis zum Neustart, allein auf der Festplatte oder neben Windows.
---

Starten Sie **„Winlux installieren“** vom Desktop der Live-Sitzung, oder wählen Sie die Installation im Menü des Sticks. Das Installationsprogramm passt sich an die Größe Ihres Bildschirms an.

![Der Schritt Netzwerk des Installationsprogramms](img:installateur-reseau.jpg "Das Winlux-Installationsprogramm, Schritt Netzwerk: WLAN und bei Bedarf Proxy.")

## Die Schritte

1. **Sprache und Tastatur**: Französisch, Englisch, Deutsch, Spanisch und weitere; Belegungen AZERTY, QWERTY, QWERTZ…
2. **Netzwerk**: Verbinden Sie bei Bedarf das WLAN; ein Firmen-Proxy kann angegeben werden.
3. **Installationsart und Festplatte**: siehe unten.
4. **Aus Windows importieren**: Übernehmen Sie Hintergrundbild, Favoriten und Dokumente von einer Windows-Partition oder ziehen Sie über das Netzwerk von einem anderen PC um.
5. **Nutzungsprofil**: ein kurzer Hardwaretest, dann Büro, Spiele, Entwicklung oder Server. Winlux stellt Prozessor, Arbeitsspeicher und Grafikkarte entsprechend ein.
6. **Konto**: ein **lokales** Konto mit Name, Passwort und Bild. Ein Online-Konto wird nicht verlangt.
7. **Zusammenfassung**: Lesen Sie alles noch einmal durch und klicken Sie dann auf „Installieren“.

## Wählen, wohin installiert wird

| Auswahl | Was passiert |
|---|---|
| Ganze Festplatte (Standard) | Winlux belegt die gesamte gewählte Festplatte. Ihr Inhalt wird gelöscht. |
| Neben Windows | Das Installationsprogramm verkleinert die Windows-Partition und lässt einen Puffer; beim Start bietet ein Menü beide Systeme an. |
| Erweiterter Modus | Manuelle Partitionierung: erstellen, löschen, Größe ändern, formatieren. |
| Verschlüsselung | Optional: Die Passphrase wird bei jedem Start abgefragt. |

![Die Wahl des Nutzungsprofils](img:installateur-profil.jpg "Das Nutzungsprofil stellt das System auf Ihre Nutzung ein; Sie können es später ändern.")

> [!warning]
> Wenn Windows auf dem PC installiert ist, fahren Sie es vollständig herunter, bevor Sie Winlux daneben installieren (halten Sie beim Klick auf Herunterfahren die Umschalttaste gedrückt): Sonst bleibt seine Partition durch den Schnellstart gesperrt.

Entfernen Sie am Ende den USB-Stick und starten Sie neu. Das Installationsprogramm bietet den Stick, von dem der PC gestartet wurde, nie als Ziel an.

Mehr dazu: [Installation im Wiki](wiki:Installation) · [unbeaufsichtigte Installation mehrerer Rechner](repo:docs/AUTOINSTALL.md).
