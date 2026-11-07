#!/bin/sh
# Remplace xscreensaver-systemd dans l'image Winlux (#634).
#
# XScreenSaver lance xscreensaver-systemd à chaque démarrage. Sans systemd ni
# logind, l'original s'arrête aussitôt (« inhibit sleep failed », « system bus
# connection failed ») et XScreenSaver écrit « exited unexpectedly » dans
# screensaver.log ; il pouvait aussi prendre un instant le nom
# org.freedesktop.ScreenSaver avant de mourir. Dans Winlux, l'inhibition
# (vidéo, jeux) est tenue par winlux-screensaver-inhibit (activé par D-Bus) et
# le verrouillage au retour de veille par winlux-lock : ce remplaçant ne fait
# qu'attendre la fin de XScreenSaver, sans erreur et sans prendre aucun nom.
parent=$PPID
while kill -0 "$parent" 2>/dev/null; do
    sleep "${WINLUX_XSS_SYSTEMD_INTERVAL:-30}"
done
exit 0
