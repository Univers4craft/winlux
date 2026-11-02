---
title: Aktualisieren
description: Neue Winlux-Versionen werden aus den Einstellungen installiert, mit Wiederherstellungspunkt und automatischer Rückkehr zur vorherigen Version.
---

## Aus den Einstellungen

1. Öffnen Sie **Einstellungen › Updates**.
2. Klicken Sie auf **„Suchen“**. Die Seite zeigt die installierte und die verfügbare Version.
3. Gibt es eine neuere Version, klicken Sie auf **„Installieren“** und verfolgen Sie den Fortschritt.
4. Starten Sie neu, wann es Ihnen passt: Winlux erzwingt nie einen Neustart.

![Die Seite Updates](img:mises-a-jour.jpg "Einstellungen › Updates: installierte Version, verfügbare Version, automatische Updates.")

Auf derselben Seite installiert **„Automatische Updates“** neue Versionen ohne Ihr Zutun. Die Pakete des Basissystems werden unter **Einstellungen › Systemupdates** aktualisiert.

## Was Ihr System schützt

- Jedes Update wird von den veröffentlichten Versionen des Projekts heruntergeladen; Größe und SHA-256-Prüfsumme werden vor der Installation geprüft.
- Unmittelbar davor wird ein **Wiederherstellungspunkt** angelegt; die letzten drei bleiben erhalten.
- Startet der Desktop nach einem Update nicht mehr richtig, **kehrt Winlux von selbst** zur vorherigen Version zurück.
- Ihre Dokumente, installierten Programme und Einstellungen bleiben erhalten.

> [!note]
> **Sie kommen von einer alten Version?** Eine Installation mit v0.2.12 oder älter muss einmal mit einem aktuellen Abbild neu installiert werden: Ihr Update-Werkzeug kann sich nicht selbst reparieren. Ab v0.2.13 geht alles über die Einstellungen.

## Zurückkehren

Wenn nach einem Update etwas nicht stimmt, starten Sie neu und wählen Sie im Startmenü **„Winlux secours“** (Rettung, Taste `s`), dann „Vorherigen Wiederherstellungspunkt wiederherstellen“.

Mehr dazu: [Veröffentlichung und Updates](wiki:Publication-et-mises-a-jour).
