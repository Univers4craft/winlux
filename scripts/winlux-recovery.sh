#!/bin/sh
# Winlux Recovery : menu de secours, exécuté par l'initramfs (winlux.recovery=1)
# dans un chroot sur la racine installée (/dev, /proc, /sys déjà montés).
# Textes en français, anglais, allemand et espagnol (langue du système).
PATH=/usr/sbin:/usr/bin:/sbin:/bin
export PATH

msg()  { printf '\033[1;32m[*]\033[0m %s\n' "$*"; }
warn() { printf '\033[1;31m[!]\033[0m %s\n' "$*"; }

lang=$(sed -n 's/^LANG=//p' /etc/winlux/locale.conf 2>/dev/null | head -n 1 | cut -c1-2)
case "$lang" in en|es|de) ;; *) lang=fr ;; esac

# t CLÉ : texte dans la langue du système.
t() {
    case "$lang:$1" in
        en:title) echo 'Winlux - Recovery' ;; es:title) echo 'Winlux - Recuperación' ;; de:title) echo 'Winlux - Wiederherstellung' ;; *:title) echo 'Winlux — Mode récupération' ;;
        en:m1) echo 'Repair startup (GRUB)' ;; es:m1) echo 'Reparar el inicio (GRUB)' ;; de:m1) echo 'Starthilfe (GRUB reparieren)' ;; *:m1) echo 'Réparer le démarrage (GRUB)' ;;
        en:m2) echo 'Restore the previous version' ;; es:m2) echo 'Restaurar la versión anterior' ;; de:m2) echo 'Vorherige Version wiederherstellen' ;; *:m2) echo 'Restaurer le point de restauration précédent' ;;
        en:m3) echo 'Recovery terminal' ;; es:m3) echo 'Terminal de recuperación' ;; de:m3) echo 'Wiederherstellungs-Terminal' ;; *:m3) echo 'Terminal de secours' ;;
        en:m4) echo 'Check the file system (fsck)' ;; es:m4) echo 'Comprobar el sistema de archivos (fsck)' ;; de:m4) echo 'Dateisystem prüfen (fsck)' ;; *:m4) echo 'Vérifier le système de fichiers (fsck)' ;;
        en:m5) echo 'Restart' ;; es:m5) echo 'Reiniciar' ;; de:m5) echo 'Neu starten' ;; *:m5) echo 'Redémarrer' ;;
        en:m6) echo 'Shut down' ;; es:m6) echo 'Apagar' ;; de:m6) echo 'Herunterfahren' ;; *:m6) echo 'Éteindre' ;;
        en:m7) echo 'Reset this PC' ;; es:m7) echo 'Restablecer este PC' ;; de:m7) echo 'Diesen PC zurücksetzen' ;; *:m7) echo 'Réinitialiser ce PC' ;;
        en:invalid) echo 'Invalid choice.' ;; es:invalid) echo 'Opción no válida.' ;; de:invalid) echo 'Ungültige Auswahl.' ;; *:invalid) echo 'Choix invalide.' ;;
        en:no_root) echo 'System partition not found.' ;; es:no_root) echo 'No se encuentra la partición del sistema.' ;; de:no_root) echo 'Systempartition nicht gefunden.' ;; *:no_root) echo 'Partition racine introuvable.' ;;
        en:no_update) echo 'winlux-update is missing.' ;; es:no_update) echo 'Falta winlux-update.' ;; de:no_update) echo 'winlux-update fehlt.' ;; *:no_update) echo 'winlux-update absent.' ;;
        en:restoring) echo 'Restoring the previous version...' ;; es:restoring) echo 'Restaurando la versión anterior...' ;; de:restoring) echo 'Vorherige Version wird wiederhergestellt...' ;; *:restoring) echo 'Restauration du point de restauration précédent...' ;;
        en:restored) echo 'Restore complete. Restart.' ;; es:restored) echo 'Restauración terminada. Reinicie.' ;; de:restored) echo 'Wiederherstellung abgeschlossen. Starten Sie neu.' ;; *:restored) echo 'Restauration terminée. Redémarrez.' ;;
        en:no_point) echo 'No valid restore point.' ;; es:no_point) echo 'No hay ningún punto de restauración válido.' ;; de:no_point) echo 'Kein gültiger Wiederherstellungspunkt.' ;; *:no_point) echo 'Aucun point de restauration valide.' ;;
        en:terminal) echo 'Recovery terminal on the installed system. Type "exit" to return to the menu.' ;; es:terminal) echo 'Terminal de recuperación en el sistema instalado. Escriba "exit" para volver al menú.' ;; de:terminal) echo 'Wiederherstellungs-Terminal im installierten System. Mit „exit“ zurück zum Menü.' ;; *:terminal) echo "Terminal de secours sur le système installé. Tapez « exit » pour revenir au menu." ;;
        en:no_fsck) echo 'e2fsck is missing: check from the Winlux recovery drive.' ;; es:no_fsck) echo 'Falta e2fsck: compruebe desde la unidad de recuperación de Winlux.' ;; de:no_fsck) echo 'e2fsck fehlt: Prüfen Sie vom Winlux-Wiederherstellungslaufwerk.' ;; *:no_fsck) echo 'e2fsck absent : vérifiez depuis la clé de secours Winlux.' ;;
        en:no_ro) echo 'Cannot make the root read-only; fsck cancelled.' ;; es:no_ro) echo 'No se puede pasar la raíz a solo lectura; fsck cancelado.' ;; de:no_ro) echo 'Root kann nicht schreibgeschützt werden; fsck abgebrochen.' ;; *:no_ro) echo 'Impossible de passer la racine en lecture seule ; fsck annulé.' ;;
        en:checking) echo 'Checking' ;; es:checking) echo 'Comprobando' ;; de:checking) echo 'Prüfe' ;; *:checking) echo 'Vérification de' ;;
        en:no_rw) echo 'Cannot remount read-write: restart.' ;; es:no_rw) echo 'No se puede volver a montar en escritura: reinicie.' ;; de:no_rw) echo 'Erneutes Einhängen mit Schreibzugriff unmöglich: Neu starten.' ;; *:no_rw) echo 'Remontage en écriture impossible : redémarrez.' ;;
        en:fsck_ok) echo 'No errors.' ;; es:fsck_ok) echo 'Sin errores.' ;; de:fsck_ok) echo 'Keine Fehler.' ;; *:fsck_ok) echo 'Aucune erreur.' ;;
        en:fsck_fixed) echo 'Errors fixed. Restart to apply.' ;; es:fsck_fixed) echo 'Errores corregidos. Reinicie para aplicarlo.' ;; de:fsck_fixed) echo 'Fehler behoben. Neu starten zum Übernehmen.' ;; *:fsck_fixed) echo 'Erreurs corrigées. Redémarrez pour appliquer.' ;;
        en:fsck_bad) echo 'e2fsck could not fix every error, code' ;; es:fsck_bad) echo 'e2fsck no pudo corregir todos los errores, código' ;; de:fsck_bad) echo 'e2fsck konnte nicht alle Fehler beheben, Code' ;; *:fsck_bad) echo 'e2fsck a laissé des erreurs non corrigées, code' ;;
        en:keep) echo 'Keep my files (accounts and personal folders kept, system and apps restored)' ;; es:keep) echo 'Conservar mis archivos (cuentas y carpetas personales conservadas, sistema y aplicaciones restaurados)' ;; de:keep) echo 'Eigene Dateien behalten (Konten und persönliche Ordner bleiben, System und Apps zurückgesetzt)' ;; *:keep) echo 'Conserver mes fichiers (comptes et dossiers personnels gardés, système et applis remis d origine)' ;;
        en:wipe) echo 'Remove everything (accounts, files, apps and settings)' ;; es:wipe) echo 'Quitar todo (cuentas, archivos, aplicaciones y configuración)' ;; de:wipe) echo 'Alles entfernen (Konten, Dateien, Apps und Einstellungen)' ;; *:wipe) echo 'Tout supprimer (comptes, fichiers, applis et réglages)' ;;
        en:back) echo 'Back' ;; es:back) echo 'Volver' ;; de:back) echo 'Zurück' ;; *:back) echo 'Retour' ;;
        en:no_source) echo 'Plug in the Winlux recovery drive (or installation drive), then press Enter. Or type the path of a Winlux ISO.' ;; es:no_source) echo 'Conecte la unidad de recuperación de Winlux (o de instalación) y pulse Intro. O escriba la ruta de una ISO de Winlux.' ;; de:no_source) echo 'Schließen Sie das Winlux-Wiederherstellungs- oder Installationslaufwerk an und drücken Sie Eingabe. Oder geben Sie den Pfad einer Winlux-ISO ein.' ;; *:no_source) echo "Branchez la clé de secours (ou la clé d'installation) Winlux puis appuyez sur Entrée. Ou tapez le chemin d'une ISO Winlux." ;;
        en:source) echo 'Winlux will be reinstalled from:' ;; es:source) echo 'Winlux se reinstalará desde:' ;; de:source) echo 'Winlux wird neu installiert von:' ;; *:source) echo 'Winlux sera réinstallé depuis :' ;;
        en:clean) echo 'Also clean the drive (secure erase, much longer)? [y/N]' ;; es:clean) echo '¿Limpiar también la unidad (borrado seguro, mucho más lento)? [s/N]' ;; de:clean) echo 'Laufwerk auch bereinigen (sicheres Löschen, viel länger)? [j/N]' ;; *:clean) echo 'Nettoyer aussi le lecteur (effacement sûr, beaucoup plus long) ? [o/N]' ;;
        en:user) echo 'Name of the new account (lowercase):' ;; es:user) echo 'Nombre de la nueva cuenta (minúsculas):' ;; de:user) echo 'Name des neuen Kontos (Kleinbuchstaben):' ;; *:user) echo 'Nom du nouveau compte (minuscules) :' ;;
        en:password) echo 'Password of the new account:' ;; es:password) echo 'Contraseña de la nueva cuenta:' ;; de:password) echo 'Kennwort des neuen Kontos:' ;; *:password) echo 'Mot de passe du nouveau compte :' ;;
        en:password2) echo 'Password again:' ;; es:password2) echo 'Repita la contraseña:' ;; de:password2) echo 'Kennwort wiederholen:' ;; *:password2) echo 'Mot de passe, encore une fois :' ;;
        en:mismatch) echo 'The passwords differ.' ;; es:mismatch) echo 'Las contraseñas no coinciden.' ;; de:mismatch) echo 'Die Kennwörter stimmen nicht überein.' ;; *:mismatch) echo 'Les mots de passe diffèrent.' ;;
        en:warn_keep) echo 'Apps you installed and every setting will be removed. Your accounts and the files in /home stay.' ;; es:warn_keep) echo 'Se quitarán las aplicaciones instaladas y toda la configuración. Sus cuentas y los archivos de /home se conservan.' ;; de:warn_keep) echo 'Installierte Apps und alle Einstellungen werden entfernt. Ihre Konten und die Dateien in /home bleiben.' ;; *:warn_keep) echo 'Les applications installées et tous les réglages seront supprimés. Vos comptes et les fichiers de /home restent.' ;;
        en:warn_wipe) echo 'EVERYTHING will be erased: all accounts, all personal files, apps and settings.' ;; es:warn_wipe) echo 'Se borrará TODO: todas las cuentas, todos los archivos personales, aplicaciones y configuración.' ;; de:warn_wipe) echo 'ALLES wird gelöscht: alle Konten, alle persönlichen Dateien, Apps und Einstellungen.' ;; *:warn_wipe) echo 'TOUT sera effacé : tous les comptes, tous les fichiers personnels, les applications et les réglages.' ;;
        en:confirm) echo 'Type YES to reset this PC:' ;; es:confirm) echo 'Escriba SÍ para restablecer este PC:' ;; de:confirm) echo 'Geben Sie JA ein, um diesen PC zurückzusetzen:' ;; *:confirm) echo 'Tapez OUI pour réinitialiser ce PC :' ;;
        en:cancelled) echo 'Nothing was changed.' ;; es:cancelled) echo 'No se ha modificado nada.' ;; de:cancelled) echo 'Es wurde nichts geändert.' ;; *:cancelled) echo "Rien n'a été modifié." ;;
        en:restart_now) echo 'Remove the recovery drive, then choose Restart to finish the reset.' ;; es:restart_now) echo 'Retire la unidad de recuperación y elija Reiniciar para terminar.' ;; de:restart_now) echo 'Entfernen Sie das Wiederherstellungslaufwerk und wählen Sie Neu starten, um abzuschließen.' ;; *:restart_now) echo 'Retirez la clé de secours puis choisissez Redémarrer pour terminer la réinitialisation.' ;;
        en:no_reset) echo 'This system cannot reset itself: reinstall Winlux from the recovery drive.' ;; es:no_reset) echo 'Este sistema no puede restablecerse: reinstale Winlux desde la unidad de recuperación.' ;; de:no_reset) echo 'Dieses System kann sich nicht zurücksetzen: Installieren Sie Winlux vom Wiederherstellungslaufwerk neu.' ;; *:no_reset) echo 'Ce système ne sait pas se réinitialiser : réinstallez Winlux depuis la clé de secours.' ;;
    esac
}

cmdline_value() {
    for argument in $(cat /proc/cmdline); do
        case "$argument" in "$1"=*) printf '%s\n' "${argument#*=}"; return 0 ;; esac
    done
    return 1
}

root_partition() {
    device=$(cmdline_value root) || return 1
    case "$device" in UUID=*|LABEL=*|PARTUUID=*) device=$(findfs "$device" 2>/dev/null) || return 1 ;; esac
    test -b "$device" && printf '%s\n' "$device"
}

repair_bootloader() {
    if test -x /usr/bin/winlux-reset; then
        /usr/bin/winlux-reset repair-boot | while read -r kind text; do
            case "$kind" in PROGRESS) msg "${text#* }" ;; DONE) msg "$text" ;; ERROR) warn "$text" ;; esac
        done
        return 0
    fi
    warn "$(t no_reset)"
    return 1
}

restore_snapshot() {
    test -x /usr/bin/winlux-update || { warn "$(t no_update)"; return 1; }
    msg "$(t restoring)"
    if /usr/bin/winlux-update --rollback; then msg "$(t restored)"; else warn "$(t no_point)"; return 1; fi
}

rescue_terminal() {
    msg "$(t terminal)"
    PS1='(winlux-rescue) # ' /bin/sh -i
}

check_filesystem() {
    part=$(root_partition) || { warn "$(t no_root)"; return 1; }
    command -v e2fsck >/dev/null 2>&1 || command -v fsck.ext4 >/dev/null 2>&1 || {
        warn "$(t no_fsck)"
        return 1
    }
    fsck_tool=$(command -v e2fsck || command -v fsck.ext4)
    sync
    mount -o remount,ro / || { warn "$(t no_ro)"; return 1; }
    msg "$(t checking) $part..."
    "$fsck_tool" -f -y "$part"
    status=$?
    mount -o remount,rw / || warn "$(t no_rw)"
    case $status in
        0) msg "$(t fsck_ok)" ;;
        1|2) msg "$(t fsck_fixed)" ;;
        *) warn "$(t fsck_bad) $status" ;;
    esac
}

# Réinitialiser ce PC depuis le menu de secours : même moteur que Paramètres,
# source = clé de secours, clé d'installation ou ISO Winlux.
reset_pc() {
    test -x /usr/bin/winlux-reset || { warn "$(t no_reset)"; return 1; }
    echo
    echo "  1) $(t keep)"
    echo "  2) $(t wipe)"
    echo "  0) $(t back)"
    printf '> '
    read -r answer || return 0
    case "$answer" in 1) mode=keep ;; 2) mode=wipe ;; *) return 0 ;; esac
    source=
    while test -z "$source"; do
        source=$(/usr/bin/winlux-reset sources --scan | sed -n 's/^\(media:[^|]*\)|.*/\1/p' | head -n 1)
        test -n "$source" && break
        warn "$(t no_source)"
        printf '> '
        read -r answer || return 0
        case "$answer" in /*.iso|/*.ISO) test -f "$answer" && source="iso:$answer" ;; 0) return 0 ;; esac
    done
    msg "$(t source) ${source#*:}"
    set -- --mode "$mode" --source "$source"
    password=
    if test "$mode" = wipe; then
        printf '%s ' "$(t clean)"
        read -r answer || return 0
        case "$answer" in [oOyYsSjJ]*) set -- "$@" --clean ;; esac
        printf '%s ' "$(t user)"
        read -r user || return 0
        while :; do
            printf '%s ' "$(t password)"
            stty -echo 2>/dev/null; read -r password; stty echo 2>/dev/null; echo
            printf '%s ' "$(t password2)"
            stty -echo 2>/dev/null; read -r again; stty echo 2>/dev/null; echo
            test "$password" = "$again" && test -n "$password" && break
            warn "$(t mismatch)"
        done
        set -- "$@" --user "$user" --fullname "$user"
        warn "$(t warn_wipe)"
    else
        warn "$(t warn_keep)"
    fi
    printf '%s ' "$(t confirm)"
    read -r answer || return 0
    case "$answer" in OUI|oui|YES|yes|JA|ja|SÍ|sí|SI|si) ;; *) msg "$(t cancelled)"; return 0 ;; esac
    if printf '%s\n' "$password" | /usr/bin/winlux-reset prepare "$@" | while read -r kind text; do
        case "$kind" in PROGRESS) printf '\r%s %%  %s\033[K' "${text%% *}" "${text#* }" ;; DONE) echo; msg "$text"; touch /run/winlux-reset-ok ;; ERROR) echo; warn "$text" ;; esac
    done && test -f /run/winlux-reset-ok; then
        rm -f /run/winlux-reset-ok
        msg "$(t restart_now)"
    fi
}

# Arrivé ici par « Redémarrer sur le menu de secours » : une seule fois.
rm -f /boot/winlux-next-recovery 2>/dev/null

echo
echo '=========================================='
echo "        $(t title)"
echo '=========================================='
while :; do
    echo
    echo "  1) $(t m1)"
    echo "  2) $(t m2)"
    echo "  3) $(t m3)"
    echo "  4) $(t m4)"
    if test -x /usr/bin/winlux-reset; then echo "  7) $(t m7)"; fi
    echo "  5) $(t m5)"
    echo "  6) $(t m6)"
    printf '> '
    read -r choice || choice=5
    case "$choice" in
        1) repair_bootloader ;;
        2) restore_snapshot ;;
        3) rescue_terminal ;;
        4) check_filesystem ;;
        7) reset_pc ;;
        5) sync; reboot -f ;;
        6) sync; poweroff -f ;;
        *) warn "$(t invalid)" ;;
    esac
done
