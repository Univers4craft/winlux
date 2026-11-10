---
title: Software und Spiele
description: Die Windows-Programme und -Spiele, die sich in Winlux installieren und starten lassen, einzeln geprüft, und die noch in Arbeit sind.
---

Jedes Programm auf dieser Seite wurde von seiner offiziellen Website in der Windows-Version installiert, dann in Winlux auf unserem Prüfstand gestartet und benutzt. Die Liste wird nach jeder Prüfung aktualisiert.

> [!note]
> Liste vom 10. November 2026. Einige Zeilen betreffen die nächste Testversion, die noch nicht veröffentlicht ist.

## Software, die funktioniert

| Software | Kategorie | Geprüft |
|---|---|---|
| VLC | Multimedia | Installiert sich, spielt Videos und Musik ab |
| Audacity 3.6 | Multimedia | Saubere Installation und Deinstallation, öffnet sich und ist benutzbar |
| ScreenToGif | Multimedia | Öffnet sich und nimmt den Bildschirm auf |
| 7-Zip (64 und 32 Bit) | Dateien | Installiert sich, öffnet und erstellt Archive; in der Konsole endet `7z.exe` noch mit Code 5 ([#1479](issue:1479)) |
| Notepad++ 8.6 | Dokumente | Saubere Installation und Deinstallation, bearbeitet Dateien |
| SumatraPDF 3.6 | Dokumente | Öffnet und zeigt PDF-Dateien an |
| Flow Launcher | Werkzeuge | Installiert sich und sucht Programme und Dateien |
| PuTTY (64 und 32 Bit) | Netzwerk | Startet und öffnet SSH-Sitzungen |
| FileZilla | Netzwerk | Installiert sich und überträgt Dateien per FTP und SFTP |
| WinSCP (32 Bit) | Netzwerk | Startet und überträgt Dateien per SFTP |
| Claude Code | Entwicklung | Installiert sich und funktioniert im Terminal |

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
| Claude für Windows | Installiert sich, öffnet sich, lässt sich minimieren und schließen; die Schaltfläche „Maximieren“ ([#1476](issue:1476)) und das Verschieben über die Titelleiste ([#1477](issue:1477)) funktionieren noch nicht |
| Paint.NET 5.1 | Installiert und deinstalliert sich; das Öffnen des Hauptfensters wird gerade geprüft ([#138](issue:138)) |
| Steam | Installiert sich und lädt seinen Client herunter, verbindet sich noch nicht ([#285](issue:285)) und stürzt beim Beenden manchmal ab ([#1440](issue:1440)) |
| IrfanView | Noch nicht vollständig ausprobiert ([#488](issue:488)) |
| Photoshop | Noch nicht vollständig ausprobiert |

## Ihre Software oder Ihr Spiel fehlt in der Liste?

Probieren Sie es aus! Wenn es sich nicht öffnet, bietet Winlux an, es mit dem Namen der fehlenden Funktion zu melden, und genau das hilft uns, es zum Laufen zu bringen. Siehe [Hilfe erhalten](page:guide-help#ein-problem-melden) und, für Einzelheiten nach Programmtyp, [Anwendungen](page:apps).
