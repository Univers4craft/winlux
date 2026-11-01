#!/bin/sh
# Avertit l'utilisateur que les logiciels choisis à l'installation sont en
# cours d'installation (premier démarrage), puis quand c'est fini.
#   winlux-first-boot-notice start PAQUET… | done | failed
# Notification du bureau (service org.freedesktop.Notifications de Winlux) ;
# sans session graphique, le message reste dans le journal.
set -u
event=${1:-}
test "$#" -gt 0 && shift
export DBUS_SESSION_BUS_ADDRESS=${DBUS_SESSION_BUS_ADDRESS:-unix:path=/run/winlux/session-bus}
lang=$(sed -n 's/^LANG=//p' /etc/winlux/locale.conf 2>/dev/null | head -n 1)
test -n "$lang" || lang=${LANG:-fr}
count=$#
case "$lang" in
    en*) case "$event" in
            start) title='Installing your apps'; body="Winlux is installing the $count apps chosen during setup in the background. You can use your PC meanwhile." ;;
            done) title='Apps installed'; body='The apps chosen during setup are ready in the Start menu.' ;;
            *) title='App installation postponed'; body='The apps chosen during setup could not be installed (network?). Winlux will try again at next startup.' ;;
         esac ;;
    de*) case "$event" in
            start) title='Apps werden installiert'; body="Winlux installiert im Hintergrund die $count bei der Einrichtung gewählten Apps. Sie können den PC währenddessen nutzen." ;;
            done) title='Apps installiert'; body='Die bei der Einrichtung gewählten Apps sind im Startmenü bereit.' ;;
            *) title='Installation verschoben'; body='Die gewählten Apps konnten nicht installiert werden (Netzwerk?). Winlux versucht es beim nächsten Start erneut.' ;;
         esac ;;
    es*) case "$event" in
            start) title='Instalando tus aplicaciones'; body="Winlux instala en segundo plano las $count aplicaciones elegidas durante la instalación. Puedes usar el PC mientras tanto." ;;
            done) title='Aplicaciones instaladas'; body='Las aplicaciones elegidas están listas en el menú Inicio.' ;;
            *) title='Instalación aplazada'; body='No se pudieron instalar las aplicaciones elegidas (¿red?). Winlux lo intentará de nuevo en el próximo inicio.' ;;
         esac ;;
    *) case "$event" in
            start) title='Installation de vos applications'; body="Winlux installe en arrière-plan les $count applications choisies pendant l'installation. Vous pouvez utiliser votre PC en attendant." ;;
            done) title='Applications installées'; body="Les applications choisies pendant l'installation sont prêtes dans le menu Démarrer." ;;
            *) title='Installation reportée'; body="Les applications choisies n'ont pas pu être installées (réseau ?). Winlux réessaiera au prochain démarrage." ;;
         esac ;;
esac
printf '%s : %s\n' "$title" "$body"
gdbus call --session --dest org.freedesktop.Notifications --object-path /org/freedesktop/Notifications \
    --method org.freedesktop.Notifications.Notify Winlux 0 system-software-install "$title" "$body" '[]' '{}' 10000 >/dev/null 2>&1 || true
