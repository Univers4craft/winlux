---
title: Getestete Anwendungen
description: Was in Winlux schon funktioniert, was noch nicht funktioniert und wie Sie uns helfen können, die Liste zu erweitern.
---

Winlux öffnet Windows-Programme dank einer von Grund auf geschriebenen Kompatibilitätsschicht. Viele Programme funktionieren bereits, aber nicht alle: Diese Seite sagt es offen.

## Nach Art des Programms

| Art des Programms | Stand heute |
|---|---|
| Einfaches 64-Bit-Programm (Werkzeuge, Hilfsprogramme, Kommandozeilenprogramme) | Funktioniert in der Regel |
| Installationsprogramm `.msi`, NSIS oder Inno Setup | Funktioniert |
| .NET-Anwendung, die ihre eigene Laufzeit mitbringt | Startet, aber ihre Oberfläche öffnet sich nicht immer |
| Oberflächen mit WinForms und WPF | Teilweise: ScreenToGif und Flow Launcher (WPF) funktionieren, viele andere noch nicht |
| Spiele mit Direct3D 9, 11 und 12 | Teilweise, Spiel für Spiel zu prüfen |
| 32-Bit-Programm | Teilweise: PuTTY, 7-Zip und WinSCP in 32 Bit funktionieren, viele andere öffnen sich noch nicht |
| Treiber, Virenschutz, Anti-Cheat auf Systemebene | Nicht unterstützt |
| Anwendungen als MSIX- oder AppX-Paket | Installation, Verknüpfungen und Deinstallation |

## Ausprobierte Software

Die Liste der einzeln geprüften Programme und Spiele, mit dem, was funktioniert und was noch in Arbeit ist, steht auf der Seite [Software und Spiele](page:software).

Die Einzelheiten Funktion für Funktion und die Reihenfolge der nächsten Arbeiten stehen im Wiki: [Kompatibilität der Anwendungen](wiki:Compatibilite-des-applications).

## Was als Nächstes kommt

Nach Wirkung auf die Zahl der freigeschalteten Programme: die meistgefragten Windows-Funktionen, OpenGL und Vulkan für Anwendungen, die Visual-C++-Laufzeitbibliotheken, sichere HTTPS-Verbindungen, Direct3D-Shader, dann 32-Bit-Programme. Die [Roadmap](wiki:Feuille-de-route) verfolgt den Fortschritt.

## Ihre Software steht nicht auf der Liste?

Probieren Sie sie aus! Wenn sie sich nicht öffnet, bietet Winlux an, das zu melden, mit dem Namen der fehlenden Funktion: Genau das ermöglicht es uns, sie zum Laufen zu bringen. Siehe [Hilfe erhalten](page:guide-help#ein-problem-melden).
