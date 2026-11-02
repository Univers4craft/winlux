# Réinitialiser ce PC, seconde étape (#283) : lu par l'init de l'initramfs
# (scripts/winlux-live-init.sh) avant d'entrer dans le système installé.
#
# `winlux-reset prepare` a décompressé le système d'origine dans
# /.winlux-reset/new, sur la même partition, et listé ses dossiers de premier
# niveau dans new-names. Ici, rien n'est copié : chaque dossier est échangé
# par deux renommages (l'ancien vers old/, le nouveau à sa place).
#   - /boot reste (GRUB, noyau déjà posé) ; /home reste en « Conserver mes
#     fichiers » ; lost+found et le fichier d'échange aussi (keep-names).
#   - Une coupure de courant n'abîme rien : tant que new/NOM existe, /NOM est
#     l'ancien ; au démarrage suivant l'échange reprend où il s'était arrêté.
# Ensuite seulement old/ est supprimé (et, en « Tout supprimer », les
# partitions /home séparées sont vidées, le disque éventuellement nettoyé).
# Dépend de "$B" (BusyBox) ; WINLUX_RESET_TTY ajoute un écran à la console.

winlux_reset_say() {
    case "$winlux_reset_lang:$1" in
        en:start) text='Resetting this PC. Do not turn off the computer.' ;;
        es:start) text='Restableciendo este PC. No apague el equipo.' ;;
        de:start) text='Dieser PC wird zurückgesetzt. Schalten Sie den Computer nicht aus.' ;;
        *:start) text="Réinitialisation de ce PC en cours. N'éteignez pas l'ordinateur." ;;
        en:remove) text='Removing the old files...' ;;
        es:remove) text='Eliminando los archivos antiguos...' ;;
        de:remove) text='Alte Dateien werden entfernt...' ;;
        *:remove) text='Suppression des anciens fichiers...' ;;
        en:clean) text='Cleaning the drive (this can take a long time)...' ;;
        es:clean) text='Limpiando la unidad (puede tardar mucho)...' ;;
        de:clean) text='Laufwerk wird bereinigt (das kann lange dauern)...' ;;
        *:clean) text='Nettoyage du lecteur (cela peut être long)...' ;;
        en:done) text='Reset complete. Winlux is starting.' ;;
        es:done) text='Restablecimiento terminado. Winlux se está iniciando.' ;;
        de:done) text='Zurücksetzen abgeschlossen. Winlux wird gestartet.' ;;
        *:done) text='Réinitialisation terminée. Winlux démarre.' ;;
        en:failed) text='The reset was interrupted. Restart to resume it.' ;;
        es:failed) text='El restablecimiento se interrumpió. Reinicie para reanudarlo.' ;;
        de:failed) text='Das Zurücksetzen wurde unterbrochen. Starten Sie neu, um fortzufahren.' ;;
        *:failed) text='La réinitialisation a été interrompue. Redémarrez pour la reprendre.' ;;
    esac
    echo "Winlux : $text"
    if test -n "${WINLUX_RESET_TTY:-}" && test -c "$WINLUX_RESET_TTY"; then echo "Winlux : $text" >"$WINLUX_RESET_TTY" 2>/dev/null; fi
    return 0
}

winlux_reset_value() {
    "$B" sed -n "s/^$1=//p" "$winlux_reset_state/plan" | "$B" head -n 1
}

# Noms jamais déplacés : la partition de démarrage, l'état de l'échange…
winlux_reset_kept() {
    case "$1" in
        .winlux-reset|lost+found|boot) return 0 ;;
        home) test "$winlux_reset_mode" = keep && return 0 ;;
    esac
    "$B" grep -Fxq -- "$1" "$winlux_reset_state/keep-names" 2>/dev/null
}

# Met l'ancien NOM de côté dans old/ (un nom déjà pris reçoit un suffixe).
winlux_reset_park() {
    target="$winlux_reset_state/old/$2"
    count=0
    while test -e "$target" || test -L "$target"; do
        count=$((count + 1))
        target="$winlux_reset_state/old/$2.$count"
    done
    "$B" mv "$1" "$target"
}

winlux_reset_resolve() {
    case "$1" in UUID=*|LABEL=*|PARTUUID=*) "$B" findfs "$1" 2>/dev/null || true ;; *) echo "$1" ;; esac
}

# Remplit l'espace libre de zéros puis le rend (effacement sûr optionnel).
winlux_reset_zero() {
    "$B" dd if=/dev/zero of="$1/.winlux-zero" bs=1048576 2>/dev/null || true
    "$B" sync
    "$B" rm -f "$1/.winlux-zero"
    "$B" sync
}

winlux_reset_apply() {
    winlux_reset_root=$1
    winlux_reset_state="$winlux_reset_root/.winlux-reset"
    test -f "$winlux_reset_state/ready" && test -f "$winlux_reset_state/plan" || return 0
    winlux_reset_mode=$(winlux_reset_value mode)
    winlux_reset_lang=$(winlux_reset_value lang)
    case "$winlux_reset_mode" in keep|wipe) ;; *) return 0 ;; esac
    winlux_reset_say start
    phase=$("$B" cat "$winlux_reset_state/state" 2>/dev/null || true)
    if test "$phase" != cleanup; then
        echo swapping >"$winlux_reset_state/state"
        "$B" sync
        "$B" mkdir -p "$winlux_reset_state/old"
        # 1. Ce que la nouvelle version n'a plus : mis de côté.
        for path in "$winlux_reset_root"/* "$winlux_reset_root"/.[!.]* "$winlux_reset_root"/..?*; do
            test -e "$path" || test -L "$path" || continue
            name=${path##*/}
            winlux_reset_kept "$name" && continue
            "$B" grep -Fxq -- "$name" "$winlux_reset_state/new-names" && continue
            winlux_reset_park "$path" "$name" || { winlux_reset_say failed; return 1; }
        done
        # 2. Chaque dossier de la nouvelle version prend la place de l'ancien.
        while IFS= read -r name; do
            test -n "$name" || continue
            winlux_reset_kept "$name" && continue
            test -e "$winlux_reset_state/new/$name" || test -L "$winlux_reset_state/new/$name" || continue
            if test -e "$winlux_reset_root/$name" || test -L "$winlux_reset_root/$name"; then
                winlux_reset_park "$winlux_reset_root/$name" "$name" || { winlux_reset_say failed; return 1; }
            fi
            "$B" mv "$winlux_reset_state/new/$name" "$winlux_reset_root/$name" || { winlux_reset_say failed; return 1; }
        done <"$winlux_reset_state/new-names"
        "$B" sync
        echo cleanup >"$winlux_reset_state/state"
        "$B" sync
    fi
    winlux_reset_say remove
    # Liste des applications retirées, sur le bureau de chaque compte gardé.
    if test -s "$winlux_reset_state/removed-apps.txt" && test -s "$winlux_reset_state/removed-apps"; then
        while IFS=: read -r uid gid destination; do
            case "$destination" in /home/*) ;; *) continue ;; esac
            case "$destination" in *..*) continue ;; esac
            test -d "$winlux_reset_root${destination%/*}" || continue
            "$B" cp "$winlux_reset_state/removed-apps.txt" "$winlux_reset_root$destination" &&
                "$B" chown "$uid:$gid" "$winlux_reset_root$destination" 2>/dev/null
        done <"$winlux_reset_state/removed-apps"
    fi
    "$B" rm -rf "$winlux_reset_state/old" "$winlux_reset_state/new"
    "$B" rm -f "$winlux_reset_root/boot/vmlinuz.before-reset" "$winlux_reset_root/boot/winlux-initramfs.cpio.gz.before-reset"
    clean=$(winlux_reset_value clean)
    if test "$winlux_reset_mode" = wipe && test -s "$winlux_reset_state/wipe-mounts"; then
        "$B" mkdir -p /winlux-reset-home
        while IFS= read -r spec; do
            device=$(winlux_reset_resolve "$spec")
            test -b "$device" || continue
            "$B" mount -t ext4 -o rw "$device" /winlux-reset-home || continue
            for path in /winlux-reset-home/* /winlux-reset-home/.[!.]* /winlux-reset-home/..?*; do
                test -e "$path" || test -L "$path" || continue
                test "${path##*/}" = lost+found && continue
                "$B" rm -rf "$path"
            done
            # Le nouveau compte vit sur cette partition, montée sur /home.
            "$B" cp -a "$winlux_reset_root/home/." /winlux-reset-home/
            if test "$clean" = 1; then winlux_reset_say clean; winlux_reset_zero /winlux-reset-home; fi
            "$B" umount /winlux-reset-home
        done <"$winlux_reset_state/wipe-mounts"
    fi
    if test "$winlux_reset_mode" = wipe && test "$clean" = 1; then
        winlux_reset_say clean
        winlux_reset_zero "$winlux_reset_root"
    fi
    "$B" rm -rf "$winlux_reset_state"
    "$B" sync
    winlux_reset_say done
    return 0
}
