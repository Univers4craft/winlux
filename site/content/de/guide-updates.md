---
title: Aktualisieren
description: Neue Winlux-Versionen werden aus den Einstellungen installiert, mit Wiederherstellungspunkt und automatischer Rückkehr zur vorherigen Version.
---

## Aus den Einstellungen

1. Öffnen Sie **Einstellungen › Updates › OS-Update**.
2. Klicken Sie auf **„Nach Updates suchen“**. Gibt es eine neuere Version, zeigt die Seite „Update verfügbar“, ihre Größe und ihre Neuerungen.
3. Klicken Sie auf **„Herunterladen und installieren“** und verfolgen Sie den Fortschritt.
4. Starten Sie neu, wann es Ihnen passt: Winlux erzwingt nie einen Neustart.

![Die Seite OS-Update](img:mises-a-jour.jpg "Einstellungen › Updates › OS-Update: Suche, Pause, automatische Installation, Verlauf und Rückkehr zur vorherigen Version.")

Auf derselben Seite installiert **„Neue Versionen automatisch installieren“** sie ohne Ihr Zutun, außerhalb Ihrer Nutzungszeit und nie während eines Spiels; **„Updates anhalten“** pausiert sie. Die Pakete des Basissystems werden über die Zeile **„Linux-Software und -Treiber“** derselben Seite aktualisiert.

## Was Ihr System schützt

- Jedes Update wird von den veröffentlichten Versionen des Projekts heruntergeladen; die Signatur seiner Beschreibung, Größe und SHA-256-Prüfsumme werden vor der Installation geprüft.
- Unmittelbar davor wird ein **Wiederherstellungspunkt** angelegt; die letzten drei bleiben erhalten.
- Startet der Desktop nach einem Update nicht mehr richtig, **kehrt Winlux von selbst** zur vorherigen Version zurück.
- Ihre Dokumente, installierten Programme und Einstellungen bleiben erhalten.

> [!note]
> **Sie kommen von einer alten Version?** Eine Installation mit v0.2.12 oder älter muss einmal mit einem aktuellen Abbild neu installiert werden: Ihr Update-Werkzeug kann sich nicht selbst reparieren. Ab v0.2.13 geht alles über die Einstellungen.

## Zurückkehren

Wenn nach einem Update etwas nicht stimmt, starten Sie neu und wählen Sie im Startmenü **„Winlux secours“** (Rettung, Taste `s`), dann „Vorherigen Wiederherstellungspunkt wiederherstellen“.

Mehr dazu: [Veröffentlichung und Updates](wiki:Publication-et-mises-a-jour).
