---
title: Kompatible Software und Spiele
description: Die Liste der Windows-Programme und -Spiele, die sich unter Winlux installieren und starten lassen, und derjenigen, an denen noch gearbeitet wird.
---

Jedes Programm auf dieser Seite wurde unter Winlux installiert und anschließend gestartet, in seiner offiziellen Windows-Version, so wie man sie beim Hersteller herunterlädt. Keines wurde dafür verändert.

> [!note]
> Diese Tests laufen auf dem Prüfstand, in einer virtuellen Maschine, mit der Version in Entwicklung. Einige Korrekturen kommen erst mit der nächsten veröffentlichten Version.

## Software, die funktioniert

Sie lässt sich installieren (oder bei portablen Versionen entpacken), öffnet sich und lässt sich normal benutzen.

| Software | Wofür sie dient | Hinweis |
|---|---|---|
| VLC | Video- und Audioplayer | |
| Audacity | Audioaufnahme und -bearbeitung | Installationsprogramm Inno Setup |
| ScreenToGif | Bildschirmaufnahme als Animation | .NET-Anwendung |
| 7-Zip | Archive und Komprimierung | Der Dateimanager funktioniert; `7z.exe` in der Konsole listet Archive korrekt auf, meldet beim Beenden aber einen Fehler ([#1479](issue:1479)) |
| Notepad++ | Texteditor | Installationsprogramm NSIS, saubere Installation und Deinstallation |
| SumatraPDF | PDF-Reader | Portable Version |
| Flow Launcher | Programmstarter und Suche | .NET-Anwendung |
| PuTTY | SSH-Terminal | 64-Bit- und 32-Bit-Version |
| FileZilla | Dateiübertragung per FTP und SFTP | |
| WinSCP | Dateiübertragung per SFTP | 32-Bit-Version |
| Claude Code | Programmierassistent für die Kommandozeile | |

## Spiele, die funktionieren

| Spiel | Genre | Grafik |
|---|---|---|
| SuperTuxKart 1.5 | Rennspiel | OpenGL, im Fenster und im Vollbild |
| Warzone 2100 4.7 | Echtzeitstrategie | OpenGL, Vulkan und Direct3D 11 |
| Xonotic | Ego-Shooter | OpenGL, in beiden Versionen |

## Installation geprüft, Nutzung noch nicht

Diese Programme lassen sich sauber installieren und deinstallieren, ihre Nutzung wurde aber noch nicht im Detail geprüft.

| Software | Wofür sie dient |
|---|---|
| WinMerge | Vergleich von Dateien und Ordnern |
| ShareX | Bildschirmfotos und Teilen |

## In Arbeit

Diese Programme machen Fortschritte, lassen sich aber noch nicht von Anfang bis Ende benutzen. Jedes Hindernis hat sein eigenes Issue auf GitHub.

| Software | Stand |
|---|---|
| Paint.NET | Es wird installiert und startet seine .NET-Laufzeit; sein Hauptfenster ist noch nicht benutzbar ([#138](issue:138)) |
| Claude-Desktop-Client für Windows | Er startet, lässt sich minimieren und schließen; Maximieren und Verschieben des Fensters funktionieren noch nicht ([#1476](issue:1476), [#1477](issue:1477)) |
| Steam | Es wird installiert und aktualisiert seinen Client, ist aber noch nicht benutzbar ([#285](issue:285), [#1440](issue:1440)) |
| Photoshop | Noch nicht vollständig getestet |
| IrfanView | Noch nicht ([#488](issue:488)) |

Was noch nicht funktioniert, nach Art des Programms, steht auf der Seite [Getestete Anwendungen](page:apps).

## Ihre Software steht nicht auf der Liste?

Probieren Sie sie aus! Wenn sie sich nicht öffnet, bietet Winlux an, das zu melden, mit dem Namen der fehlenden Funktion. Siehe [Hilfe erhalten](page:guide-help#ein-problem-melden).
