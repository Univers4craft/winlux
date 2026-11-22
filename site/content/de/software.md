---
title: Software und Spiele
description: Die Windows-Programme und -Spiele, die sich in Winlux installieren und starten lassen, einzeln geprüft, und die noch in Arbeit sind.
---

Jedes Programm auf dieser Seite wurde von der offiziellen Website seines Herausgebers in der Windows-Version heruntergeladen, dann in Winlux auf unserem Prüfstand installiert und gestartet. Die Liste wird nach jeder Prüfung aktualisiert.

> [!note]
> Liste vom 22. November 2026, nach der veröffentlichten Version v0.2.21 und den seither durchgeführten Prüfungen. Einige Zeilen betreffen die nächste Version, v0.3, die noch nicht veröffentlicht ist.

## Software, die funktioniert

| Software | Kategorie | Geprüft |
|---|---|---|
| VLC | Multimedia | Installiert sich, spielt Videos und Musik ab |
| Audacity 3.6 | Multimedia | Saubere Installation und Deinstallation, öffnet sich und ist benutzbar |
| ScreenToGif | Multimedia | Öffnet sich und nimmt den Bildschirm auf |
| OBS Studio | Multimedia | Öffnet sein Fenster mit der Vorschau ([#2262](issue:2262), [#2313](issue:2313)) |
| Paint.NET 5.1 | Bild | Installiert sich (Winlux holt das vollständige Paket beim Herausgeber), öffnet sich und zeichnet; der Downloader des Web-Installationsprogramms kommt noch nicht ans Ende ([#2752](issue:2752)) |
| 7-Zip (64 und 32 Bit) | Dateien | Installiert sich, öffnet und erstellt Archive, im Fenster wie in der Konsole |
| Notepad++ 8.6 | Dokumente | Saubere Installation und Deinstallation, bearbeitet Dateien |
| SumatraPDF 3.6 | Dokumente | Öffnet und zeigt PDF-Dateien an |
| Flow Launcher | Werkzeuge | Installiert sich und sucht Programme und Dateien |
| PuTTY (64 und 32 Bit) | Netzwerk | Startet und öffnet SSH-Sitzungen |
| FileZilla | Netzwerk | Installiert sich und überträgt Dateien per FTP und SFTP |
| WinSCP (32 Bit) | Netzwerk | Startet und überträgt Dateien per SFTP |
| Claude Code | Entwicklung | Installiert sich und funktioniert im Terminal |
| Pythonwin | Entwicklung | Öffnet sein Hauptfenster ([#1785](issue:1785)) |

## Spiele, die funktionieren

| Spiel | Grafik | Geprüft |
|---|---|---|
| Warzone 2100 4.7 | Direct3D 11 oder OpenGL | Startet und ist spielbar |
| SuperTuxKart 1.5 | OpenGL | Startet und ist spielbar, etwa 116 Bilder pro Sekunde auf dem Prüfstand |
| Xonotic | OpenGL | Startet und ist spielbar, über 220 Bilder pro Sekunde auf dem Prüfstand |

## Sauber installiert, Nutzung noch nicht geprüft

| Software | Geprüft |
|---|---|
| WinMerge 2.16 | Saubere Installation und Deinstallation |
| ShareX 16.1 | Saubere Installation und Deinstallation |

## In Arbeit

| Software | Stand |
|---|---|
| Office | Das Bereitstellungstool lädt Office herunter und prüft seine Signaturen; die Installation kommt noch nicht ans Ende ([#2740](issue:2740)) |
| Adobe Acrobat Reader | Installiert sich; die Anwendung ist noch nicht benutzbar ([#236](issue:236)) |
| Teams | Installiert sich und startet, erreicht aber den Anmeldebildschirm noch nicht ([#2511](issue:2511), [#2726](issue:2726)) |
| Adobe Creative Cloud | Das Installationsprogramm startet und erreicht die Server des Herausgebers, sein Fenster erscheint noch nicht ([#2784](issue:2784)) |
| OneDrive | Das Installationsprogramm startet noch nicht ([#2851](issue:2851)) |
| Claude für Windows | Installiert sich und öffnet sich; die Fehler beim Maximieren und Verschieben sind behoben, das Ganze muss neu geprüft werden |
| Steam | Installiert sich und lädt seinen Client herunter, verbindet sich noch nicht ([#285](issue:285)) |
| IrfanView | Noch nicht vollständig ausprobiert ([#488](issue:488)) |
| Photoshop | Noch nicht vollständig ausprobiert |

## Ihre Software oder Ihr Spiel fehlt in der Liste?

Probieren Sie es aus! Wenn es sich nicht öffnet, bietet Winlux an, es mit dem Namen der fehlenden Funktion zu melden, und genau das hilft uns, es zum Laufen zu bringen. Siehe [Hilfe erhalten](page:guide-help#ein-problem-melden) und, für Einzelheiten nach Programmtyp, [Anwendungen](page:apps).
