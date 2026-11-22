#!/bin/sh
# Horloge de la barre des tâches (greffon genmon) : heure et date en clair
# sur la barre sombre, clic = calendrier et notifications façon Windows 11.
# Volontairement en sh : relancé toutes les deux secondes par la barre.
# Un seul programme lancé par tour, date (#3130) : sed, head, tr, cut et
# leurs sous-shells en faisaient environ quinze, 2 % d'un cœur au repos.
# La couleur est fixée ici (et vérifiée par test-shell-contrast.py) pour ne
# jamais dépendre du thème GTK : en Adwaita clair, le texte des boutons de la
# barre était gris foncé sur fond noir.
lang=${WINLUX_LANG:-}
if test -z "$lang" && test -r /etc/winlux/locale.conf; then
    # Première ligne LANG=, comme l'ancien « sed -n 's/^LANG=//p' | head -n 1 ».
    while IFS= read -r line || test -n "$line"; do
        case $line in LANG=*) lang=${line#LANG=}; break ;; esac
    done </etc/winlux/locale.conf
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
# N-ième mot d'une liste, sans echo | cut.
pick() { n=$1; shift; eval "word=\${$n}"; }
# shellcheck disable=SC2086
pick "$weekday" $days; dayname=$word
# shellcheck disable=SC2086
pick "$monthn" $months; monthname=$word

case "$lang" in
    en) time="$hour12:$minute $ampm"; date="$monthn/$dayn/$year"
        tooltip="$dayname, $monthname $dayn, $year" ;;
    es) time="$hour:$minute"; date="$day/$month/$year"; tooltip="$dayname, $dayn de $monthname de $year" ;;
    de) time="$hour:$minute"; date="$day.$month.$year"; tooltip="$dayname, $dayn. $monthname $year" ;;
    *)  time="$hour:$minute"; date="$day/$month/$year"; tooltip="$dayname $dayn $monthname $year" ;;
esac

# La barre suit le thème du compte (#703) : texte clair sur barre sombre
# (#f3f0fa sur #1c1a26 : 16:1), texte foncé sur barre claire (#1d1a24 sur
# #f3f0f9 : 15:1). Même règle que panel_dark() de winlux-tray.
theme=${WINLUX_PANEL_THEME:-}
if test -z "$theme"; then
    settings=${XDG_CONFIG_HOME:-$HOME/.config}/gtk-3.0/settings.ini
    # « clé *= *valeur », première occurrence de chaque clé, lue sans sed.
    name= prefer= got_name= got_prefer=
    if test -r "$settings"; then
        while IFS= read -r line || test -n "$line"; do
            case $line in
                gtk-theme-name*) key=name; rest=${line#gtk-theme-name} ;;
                gtk-application-prefer-dark-theme*) key=prefer; rest=${line#gtk-application-prefer-dark-theme} ;;
                *) continue ;;
            esac
            while case $rest in ' '*) true ;; *) false ;; esac; do rest=${rest# }; done
            case $rest in =*) rest=${rest#=} ;; *) continue ;; esac
            while case $rest in ' '*) true ;; *) false ;; esac; do rest=${rest# }; done
            if test "$key" = name && test -z "$got_name"; then name=$rest got_name=1
            elif test "$key" = prefer && test -z "$got_prefer"; then prefer=$rest got_prefer=1; fi
        done <"$settings"
    fi
    # Casse ignorée (l'ancien « tr 'A-Z' 'a-z' ») par les motifs eux-mêmes.
    case "$name" in
        [Ww][Ii][Nn][Ll][Uu][Xx]-*-[Dd][Aa][Rr][Kk]*|[Ww][Ii][Nn][Ll][Uu][Xx]-[Dd][Aa][Rr][Kk]*) theme=dark ;;
        [Ww][Ii][Nn][Ll][Uu][Xx]-*) theme=light ;;
        *[Dd][Aa][Rr][Kk]*|*[Ii][Nn][Vv][Ee][Rr][Ss][Ee]*) theme=dark ;;
        '') theme=dark ;;
        *) case "$prefer" in 1|true) theme=dark ;; *) theme=light ;; esac ;;
    esac
fi
if test "$theme" = light; then default=#1d1a24; else default=#f3f0fa; fi
color=${WINLUX_CLOCK_COLOR:-$default}
popup=/usr/bin/winlux-popup
case $0 in */*) here=${0%/*} ;; *) here=. ;; esac
test -x "$popup" || popup="python3 $here/winlux-popup.py"
printf '<txt><span foreground="%s" size="small">%s\n%s</span></txt>' "$color" "$time" "$date"
printf '<txtclick>%s calendar</txtclick><tool>%s</tool>\n' "$popup" "$tooltip"
