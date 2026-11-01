#!/bin/sh
# Horloge de la barre des tâches (greffon genmon) : heure et date en clair
# sur la barre sombre, clic = calendrier et notifications façon Windows 11.
# Volontairement en sh : relancé toutes les deux secondes par la barre.
# La couleur est fixée ici (et vérifiée par test-shell-contrast.py) pour ne
# jamais dépendre du thème GTK : en Adwaita clair, le texte des boutons de la
# barre était gris foncé sur fond noir.
lang=${WINLUX_LANG:-}
if test -z "$lang" && test -r /etc/winlux/locale.conf; then
    lang=$(sed -n 's/^LANG=//p' /etc/winlux/locale.conf | head -n 1)
fi
lang=${lang:-${LANG:-fr}}
case "$lang" in en*) lang=en ;; es*) lang=es ;; de*) lang=de ;; *) lang=fr ;; esac

set -- $(date '+%H %M %d %m %Y %u %-d %-m %-I %p')
hour=$1 minute=$2 day=$3 month=$4 year=$5 weekday=$6 dayn=$7 monthn=$8 hour12=$9
shift 9
ampm=$1

case "$lang" in
    en) days="Monday Tuesday Wednesday Thursday Friday Saturday Sunday"
        months="January February March April May June July August September October November December" ;;
    es) days="lunes martes miércoles jueves viernes sábado domingo"
        months="enero febrero marzo abril mayo junio julio agosto septiembre octubre noviembre diciembre" ;;
    de) days="Montag Dienstag Mittwoch Donnerstag Freitag Samstag Sonntag"
        months="Januar Februar März April Mai Juni Juli August September Oktober November Dezember" ;;
    *)  days="lundi mardi mercredi jeudi vendredi samedi dimanche"
        months="janvier février mars avril mai juin juillet août septembre octobre novembre décembre" ;;
esac
dayname=$(echo $days | cut -d' ' -f"$weekday")
monthname=$(echo $months | cut -d' ' -f"$monthn")

case "$lang" in
    en) time="$hour12:$minute $ampm"; date="$monthn/$dayn/$year"
        tooltip="$dayname, $monthname $dayn, $year" ;;
    es) time="$hour:$minute"; date="$day/$month/$year"; tooltip="$dayname, $dayn de $monthname de $year" ;;
    de) time="$hour:$minute"; date="$day.$month.$year"; tooltip="$dayname, $dayn. $monthname $year" ;;
    *)  time="$hour:$minute"; date="$day/$month/$year"; tooltip="$dayname $dayn $monthname $year" ;;
esac

# #f3f0fa sur le fond de barre #1c1b25 : 16:1.
color=${WINLUX_CLOCK_COLOR:-#f3f0fa}
popup=/usr/bin/winlux-popup
test -x "$popup" || popup="python3 $(dirname "$0")/winlux-popup.py"
printf '<txt><span foreground="%s" size="small">%s\n%s</span></txt>' "$color" "$time" "$date"
printf '<txtclick>%s calendar</txtclick><tool>%s</tool>\n' "$popup" "$tooltip"
