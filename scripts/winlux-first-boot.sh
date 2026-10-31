#!/bin/sh
# Configuration initiale au premier démarrage (OOBE - Out Of Box Experience).
# Lancé automatiquement après l'installation de Winlux sur disque.
set -eu
PATH=/usr/sbin:/usr/bin:/sbin:/bin
export PATH

MARKER=/etc/winlux/first-boot-done
if test -f "$MARKER"; then exit 0; fi

root=$(CDPATH= cd -- "$(dirname -- "$0")/.." 2>/dev/null && pwd) || root=
log() { printf '[winlux-oobe] %s\n' "$*"; }
die() { log "ERREUR : $*" >&2; exit 1; }

test "$(id -u)" -eq 0 || die 'droits root requis'

TOTAL_STEPS=10

# --- Détection de l'environnement d'affichage ---
HAS_DISPLAY=0
if test -n "${DISPLAY:-}" || test -n "${WAYLAND_DISPLAY:-}"; then HAS_DISPLAY=1; fi

# Outil TUI : dialog > whiptail > aucun (fallback readline)
TUI=none
if command -v dialog >/dev/null 2>&1; then TUI=dialog
elif command -v whiptail >/dev/null 2>&1; then TUI=whiptail
fi

# --- Fonctions d'entrée utilisateur ---
# Sélection dans une liste (retourne le tag choisi dans $REPLY)
menu() {
    title=$1; shift
    # $@ = tag1 label1 tag2 label2 ...
    count=$(( $# / 2 ))
    if test "$TUI" != none; then
        REPLY=$("$TUI" --title "$title" --menu "" 20 60 "$count" "$@" 3>&1 1>&2 2>&3) || REPLY=
    else
        echo
        echo "=== $title ==="
        i=1
        while test $# -ge 2; do
            printf '  %d) %s (%s)\n' "$i" "$2" "$1"
            i=$((i + 1))
            shift 2
        done
        # reparse original args
        set -- $MENU_ARGS
        printf 'Choix [1] : '
        read -r choice || choice=
        choice=${choice:-1}
        case "$choice" in *[!0-9]*) choice=1 ;; esac
        i=1
        while test $# -ge 2; do
            if test "$i" = "$choice"; then REPLY=$1; return; fi
            i=$((i + 1))
            shift 2
        done
        REPLY=$1  # défaut au premier
    fi
}

# Question oui/non (retourne 0 pour oui, 1 pour non)
yesno() {
    title=$1; default=${2:-oui}
    if test "$TUI" != none; then
        if test "$default" = non; then
            "$TUI" --title "$title" --defaultno --yesno "$title" 8 60 && return 0 || return 1
        else
            "$TUI" --title "$title" --yesno "$title" 8 60 && return 0 || return 1
        fi
    else
        if test "$default" = oui; then hint="[O/n]"; else hint="[o/N]"; fi
        printf '%s %s : ' "$title" "$hint"
        read -r ans || ans=
        case "$ans" in
            [oOyY]*) return 0 ;;
            [nN]*) return 1 ;;
            *) test "$default" = oui && return 0 || return 1 ;;
        esac
    fi
}

# Saisie de texte libre
input() {
    title=$1; default=${2:-}
    if test "$TUI" != none; then
        REPLY=$("$TUI" --title "$title" --inputbox "" 10 60 "$default" 3>&1 1>&2 2>&3) || REPLY=$default
    else
        printf '%s [%s] : ' "$title" "$default"
        read -r REPLY || REPLY=
        REPLY=${REPLY:-$default}
    fi
}

# Saisie de mot de passe (masqué)
password() {
    title=$1
    if test "$TUI" != none; then
        REPLY=$("$TUI" --title "$title" --passwordbox "" 10 60 3>&1 1>&2 2>&3) || REPLY=
    else
        printf '%s : ' "$title"
        stty -echo 2>/dev/null || true
        read -r REPLY || REPLY=
        stty echo 2>/dev/null || true
        echo
    fi
}

# Sélection multiple (checklist) — retourne les tags cochés séparés par espaces dans $REPLY
checklist() {
    title=$1; shift
    # $@ = tag1 label1 status1 tag2 label2 status2 ...
    count=$(( $# / 3 ))
    if test "$TUI" != none; then
        REPLY=$("$TUI" --title "$title" --checklist "" 20 70 "$count" "$@" 3>&1 1>&2 2>&3) || REPLY=
    else
        echo
        echo "=== $title ==="
        echo "(Entrez les numéros séparés par des espaces, ou Entrée pour les choix par défaut)"
        i=1
        defaults=
        while test $# -ge 3; do
            mark=" "
            if test "$3" = on; then mark="*"; defaults="$defaults $1"; fi
            printf '  %d) [%s] %s (%s)\n' "$i" "$mark" "$2" "$1"
            i=$((i + 1))
            shift 3
        done
        printf 'Sélection : '
        read -r sel || sel=
        if test -z "$sel"; then
            REPLY=$defaults
            return
        fi
        # reparse args
        set -- $CHECKLIST_ARGS
        result=
        for num in $sel; do
            case "$num" in *[!0-9]*) continue ;; esac
            j=1
            idx=1
            while test $j -le $# ; do
                eval "tag=\${$j}"
                if test "$idx" = "$num"; then
                    result="$result $tag"
                    break
                fi
                j=$((j + 3))
                idx=$((idx + 1))
            done
        done
        REPLY=$result
    fi
}

# ============================================================
#  Étape 1 : Langue
# ============================================================
log "Étape 1/$TOTAL_STEPS : Langue"
MENU_ARGS='fr_FR Français en_US English de_DE Deutsch es_ES Español it_IT Italiano pt_BR Português'
menu "Choisissez votre langue" $MENU_ARGS
LANG_CHOICE=${REPLY:-fr_FR}

case "$LANG_CHOICE" in
    fr_FR) LOCALE=fr_FR.UTF-8 ;;
    en_US) LOCALE=en_US.UTF-8 ;;
    de_DE) LOCALE=de_DE.UTF-8 ;;
    es_ES) LOCALE=es_ES.UTF-8 ;;
    it_IT) LOCALE=it_IT.UTF-8 ;;
    pt_BR) LOCALE=pt_BR.UTF-8 ;;
    *)     LOCALE=fr_FR.UTF-8 ;;
esac

# ============================================================
#  Étape 2 : Clavier
# ============================================================
log "Étape 2/$TOTAL_STEPS : Clavier"
case "$LANG_CHOICE" in
    fr_FR) kb_default=fr ;;
    en_US) kb_default=us ;;
    de_DE) kb_default=de ;;
    es_ES) kb_default=es ;;
    it_IT) kb_default=it ;;
    pt_BR) kb_default=br ;;
    *)     kb_default=fr ;;
esac

MENU_ARGS="fr AZERTY us QWERTY de QWERTZ es Espagnol it Italiano br Português-BR gb UK-QWERTY ch Suisse be Belge"
menu "Disposition du clavier" $MENU_ARGS
KB_LAYOUT=${REPLY:-$kb_default}

# ============================================================
#  Étape 3 : Fuseau horaire
# ============================================================
log "Étape 3/$TOTAL_STEPS : Fuseau horaire"
# Auto-détection basée sur la langue
case "$LANG_CHOICE" in
    fr_FR) tz_default=Europe/Paris ;;
    en_US) tz_default=America/New_York ;;
    de_DE) tz_default=Europe/Berlin ;;
    es_ES) tz_default=Europe/Madrid ;;
    it_IT) tz_default=Europe/Rome ;;
    pt_BR) tz_default=America/Sao_Paulo ;;
    *)     tz_default=Europe/Paris ;;
esac

input "Fuseau horaire" "$tz_default"
TIMEZONE=${REPLY:-$tz_default}
# Validation minimale
if ! test -f "/usr/share/zoneinfo/$TIMEZONE" 2>/dev/null; then
    log "Fuseau $TIMEZONE introuvable, utilisation de $tz_default"
    TIMEZONE=$tz_default
fi

# ============================================================
#  Étape 4 : Nom de la machine
# ============================================================
log "Étape 4/$TOTAL_STEPS : Nom de la machine"
input "Nom de la machine" "winlux-desktop"
HOSTNAME_CHOICE=${REPLY:-winlux-desktop}
# Nettoyage : uniquement alphanumérique et tirets
HOSTNAME_CHOICE=$(printf '%s' "$HOSTNAME_CHOICE" | tr -cd 'a-zA-Z0-9-' | head -c 63)
test -n "$HOSTNAME_CHOICE" || HOSTNAME_CHOICE=winlux-desktop

# ============================================================
#  Étape 5 : Thème (clair / sombre)
# ============================================================
log "Étape 5/$TOTAL_STEPS : Thème"
MENU_ARGS='dark Sombre_(recommandé) light Clair'
menu "Choisissez votre thème" $MENU_ARGS
THEME_CHOICE=${REPLY:-dark}

if test "$TUI" != none; then
    if test "$THEME_CHOICE" = dark; then
        preview="Thème sombre sélectionné.\n\n  ┌─────────────────────────────┐\n  │  ████████████████████████   │\n  │  █  Fond sombre #181b24  █  │\n  │  █  Texte clair #ecedf5  █  │\n  │  █  Accent    #8178b7    █  │\n  │  ████████████████████████   │\n  └─────────────────────────────┘"
    else
        preview="Thème clair sélectionné.\n\n  ┌─────────────────────────────┐\n  │  ████████████████████████   │\n  │  █  Fond clair  #f3f5fa  █  │\n  │  █  Texte foncé #202430  █  │\n  │  █  Accent    #8178b7    █  │\n  │  ████████████████████████   │\n  └─────────────────────────────┘"
    fi
    "$TUI" --title "Aperçu du thème" --msgbox "$preview" 14 40 2>/dev/null || true
else
    if test "$THEME_CHOICE" = dark; then
        echo "  -> Thème sombre : fond #181b24, texte #ecedf5, accent #8178b7"
    else
        echo "  -> Thème clair : fond #f3f5fa, texte #202430, accent #8178b7"
    fi
fi

# ============================================================
#  Étape 6 : Sélection des applications
# ============================================================
log "Étape 6/$TOTAL_STEPS : Applications"

CHECKLIST_ARGS='firefox Navigateur_Firefox on libreoffice Suite_bureautique_LibreOffice on vlc Lecteur_multimédia_VLC on gimp Éditeur_images_GIMP off git Outils_développeur_(git,build-essential) off geany Éditeur_de_code_Geany off audacity Éditeur_audio_Audacity off obs-studio Capture_vidéo_OBS_Studio off'
checklist "Applications à pré-installer" $CHECKLIST_ARGS
APPS_SELECTED=${REPLY:-firefox libreoffice vlc}

# ============================================================
#  Étape 7 : Compte utilisateur local
# ============================================================
log "Étape 7/$TOTAL_STEPS : Compte utilisateur local"
input "Nom d'utilisateur (login)" "utilisateur"
USERNAME=${REPLY:-utilisateur}
USERNAME=$(printf '%s' "$USERNAME" | tr -cd 'a-z0-9_-' | head -c 32)
test -n "$USERNAME" || USERNAME=utilisateur

input "Nom complet" "$USERNAME"
FULLNAME=${REPLY:-$USERNAME}

while :; do
    password "Mot de passe pour $USERNAME"
    PASS1=$REPLY
    password "Confirmez le mot de passe"
    PASS2=$REPLY
    if test "$PASS1" = "$PASS2" && test -n "$PASS1"; then
        break
    fi
    log 'Les mots de passe ne correspondent pas ou sont vides, réessayez.'
done
USER_PASSWORD=$PASS1

# --- Choix de l'avatar ---
AVATAR_DIR=
for d in \
    "${root:+$root/assets/avatars}" \
    /opt/winlux/assets/avatars \
    /usr/share/winlux/avatars; do
    test -n "$d" && test -d "$d" && { AVATAR_DIR=$d; break; }
done

USER_AVATAR=
if test -n "$AVATAR_DIR"; then
    avatar_count=$(ls "$AVATAR_DIR"/avatar-*.svg 2>/dev/null | wc -l)
    if test "$avatar_count" -gt 0; then
        MENU_ARGS=
        for f in "$AVATAR_DIR"/avatar-*.svg; do
            base=$(basename "$f" .svg)
            num=${base#avatar-}
            case "$num" in
                01) label="Personne_(défaut)" ;;
                02) label="Chat" ;;
                03) label="Chien" ;;
                04) label="Robot" ;;
                05) label="Astronaute" ;;
                06) label="Nature" ;;
                07) label="Musique" ;;
                08) label="Jeu_vidéo" ;;
                *)  label="Avatar_$num" ;;
            esac
            MENU_ARGS="$MENU_ARGS $base $label"
        done
        MENU_ARGS="$MENU_ARGS aucun Sans_avatar"
        menu "Choisissez un avatar (optionnel)" $MENU_ARGS
        case "$REPLY" in
            aucun|"") USER_AVATAR= ;;
            *) USER_AVATAR="$AVATAR_DIR/$REPLY.svg" ;;
        esac
    fi
fi

# ============================================================
#  Étape 8 : Mot de passe root
# ============================================================
log "Étape 8/$TOTAL_STEPS : Mot de passe root"
DISABLE_ROOT=1
if yesno "Désactiver le compte root ? (recommandé, sudo sera utilisé)" oui; then
    DISABLE_ROOT=1
else
    DISABLE_ROOT=0
    while :; do
        password "Mot de passe root"
        RPASS1=$REPLY
        password "Confirmez le mot de passe root"
        RPASS2=$REPLY
        if test "$RPASS1" = "$RPASS2" && test -n "$RPASS1"; then break; fi
        log 'Les mots de passe ne correspondent pas ou sont vides, réessayez.'
    done
    ROOT_PASSWORD=$RPASS1
fi

# ============================================================
#  Étape 9 : Connexion automatique
# ============================================================
log "Étape 9/$TOTAL_STEPS : Connexion automatique"
AUTO_LOGIN=0
if yesno "Activer la connexion automatique pour $USERNAME ?" oui; then
    AUTO_LOGIN=1
fi

# ============================================================
#  Étape 10 : Récapitulatif
# ============================================================
log "Étape 10/$TOTAL_STEPS : Récapitulatif"
summary="Langue : $LANG_CHOICE\nClavier : $KB_LAYOUT\nFuseau : $TIMEZONE\nMachine : $HOSTNAME_CHOICE\nThème : $THEME_CHOICE\nUtilisateur : $USERNAME ($FULLNAME)\nConnexion auto : $(test $AUTO_LOGIN = 1 && echo Oui || echo Non)\nApps : $APPS_SELECTED"
if test -n "$USER_AVATAR"; then
    summary="$summary\nAvatar : $(basename "$USER_AVATAR")"
fi

if test "$TUI" != none; then
    "$TUI" --title "Récapitulatif de la configuration" --msgbox "$summary" 18 60 2>/dev/null || true
else
    echo
    echo "=== Récapitulatif ==="
    printf '%b\n' "$summary"
fi

if ! yesno "Appliquer cette configuration ?" oui; then
    log "Configuration annulée par l'utilisateur."
    exit 1
fi

# ============================================================
#  Application des paramètres
# ============================================================
log 'Application des paramètres...'

# --- Locale ---
log "Locale : $LOCALE"
if command -v localectl >/dev/null 2>&1; then
    localectl set-locale "LANG=$LOCALE" 2>/dev/null || true
fi
if command -v locale-gen >/dev/null 2>&1; then
    if test -f /etc/locale.gen; then
        sed -i "s/^# *${LOCALE}/${LOCALE}/" /etc/locale.gen 2>/dev/null || true
    fi
    locale-gen 2>/dev/null || true
fi
mkdir -p /etc/default
cat >/etc/default/locale <<EOF
LANG=$LOCALE
LANGUAGE=${LANG_CHOICE}
EOF

# --- Clavier ---
log "Clavier : $KB_LAYOUT"
if command -v localectl >/dev/null 2>&1; then
    localectl set-keymap "$KB_LAYOUT" 2>/dev/null || true
    localectl set-x11-keymap "$KB_LAYOUT" 2>/dev/null || true
fi
if command -v loadkeys >/dev/null 2>&1; then
    loadkeys "$KB_LAYOUT" 2>/dev/null || true
fi
mkdir -p /etc/default
cat >/etc/default/keyboard <<EOF
XKBMODEL="pc105"
XKBLAYOUT="$KB_LAYOUT"
XKBVARIANT=""
XKBOPTIONS=""
BACKSPACE="guess"
EOF
if command -v setupcon >/dev/null 2>&1; then
    setupcon --save 2>/dev/null || true
fi

# --- Fuseau horaire ---
log "Fuseau horaire : $TIMEZONE"
if command -v timedatectl >/dev/null 2>&1; then
    timedatectl set-timezone "$TIMEZONE" 2>/dev/null || true
else
    ln -sf "/usr/share/zoneinfo/$TIMEZONE" /etc/localtime 2>/dev/null || true
fi
printf '%s\n' "$TIMEZONE" >/etc/timezone 2>/dev/null || true

# --- Nom de la machine ---
log "Hostname : $HOSTNAME_CHOICE"
printf '%s\n' "$HOSTNAME_CHOICE" >/etc/hostname
if test -f /etc/hosts; then
    if grep -q '127.0.1.1' /etc/hosts; then
        sed -i "s/^127\.0\.1\.1.*/127.0.1.1\t$HOSTNAME_CHOICE/" /etc/hosts
    else
        printf '127.0.1.1\t%s\n' "$HOSTNAME_CHOICE" >>/etc/hosts
    fi
else
    cat >/etc/hosts <<EOF
127.0.0.1	localhost
127.0.1.1	$HOSTNAME_CHOICE
::1		localhost ip6-localhost ip6-loopback
EOF
fi
hostname "$HOSTNAME_CHOICE" 2>/dev/null || true

# --- Thème ---
log "Thème : $THEME_CHOICE"
mkdir -p /etc/winlux
printf '%s\n' "$THEME_CHOICE" >/etc/winlux/theme

# --- Compte utilisateur ---
log "Création du compte : $USERNAME"
GROUPS="sudo,audio,video,plugdev,input"
for grp in netdev bluetooth lpadmin scanner; do
    if getent group "$grp" >/dev/null 2>&1; then
        GROUPS="$GROUPS,$grp"
    fi
done

if ! id "$USERNAME" >/dev/null 2>&1; then
    useradd -m -s /bin/bash -c "$FULLNAME" -G "$GROUPS" "$USERNAME"
else
    usermod -c "$FULLNAME" -G "$GROUPS" -s /bin/bash "$USERNAME" 2>/dev/null || true
fi
printf '%s:%s\n' "$USERNAME" "$USER_PASSWORD" | chpasswd

# Répertoires XDG standard
for dir in Bureau Documents Images Musique Téléchargements Vidéos; do
    su -c "mkdir -p \"\$HOME/$dir\"" "$USERNAME" 2>/dev/null || true
done

# Thème : lu par winlux-personalize à l'ouverture de session
user_home=$(getent passwd "$USERNAME" | cut -d: -f6)
if test -n "$user_home"; then
    mkdir -p "$user_home/.config/winlux"
    printf '{"theme":"%s","accent":"#8178b7"}\n' "$THEME_CHOICE" >"$user_home/.config/winlux/appearance.json"
    chown -R "$USERNAME:" "$user_home/.config" 2>/dev/null || true
fi

# Avatar
if test -n "$USER_AVATAR" && test -f "$USER_AVATAR"; then
    user_home=$(getent passwd "$USERNAME" | cut -d: -f6)
    if test -n "$user_home"; then
        avatar_dest="$user_home/.face"
        cp "$USER_AVATAR" "$avatar_dest" 2>/dev/null || true
        chown "$USERNAME:" "$avatar_dest" 2>/dev/null || true

        mkdir -p "$user_home/.config/winlux"
        printf '%s\n' "$USER_AVATAR" >"$user_home/.config/winlux/avatar"
        chown -R "$USERNAME:" "$user_home/.config/winlux" 2>/dev/null || true
    fi
fi

# Sudoers
if test -d /etc/sudoers.d; then
    printf '%s ALL=(ALL:ALL) ALL\n' "$USERNAME" >/etc/sudoers.d/winlux-user
    chmod 440 /etc/sudoers.d/winlux-user
fi

# --- Mot de passe root ---
if test "$DISABLE_ROOT" = 1; then
    log 'Compte root désactivé (sudo recommandé)'
    passwd -l root 2>/dev/null || true
else
    log 'Mot de passe root défini'
    printf 'root:%s\n' "$ROOT_PASSWORD" | chpasswd
fi

# --- Connexion automatique ---
if test "$AUTO_LOGIN" = 1; then
    log "Connexion automatique activée pour $USERNAME"
    # LightDM
    if test -d /etc/lightdm; then
        mkdir -p /etc/lightdm/lightdm.conf.d
        cat >/etc/lightdm/lightdm.conf.d/50-winlux-autologin.conf <<EOF
[Seat:*]
autologin-user=$USERNAME
autologin-user-timeout=0
EOF
    fi
    # GDM
    if test -f /etc/gdm3/custom.conf; then
        sed -i "s/^#\?AutomaticLoginEnable.*/AutomaticLoginEnable=true/" /etc/gdm3/custom.conf 2>/dev/null || true
        sed -i "s/^#\?AutomaticLogin .*/AutomaticLogin=$USERNAME/" /etc/gdm3/custom.conf 2>/dev/null || true
    fi
    # Getty (console tty1)
    if test -d /etc/systemd/system; then
        mkdir -p /etc/systemd/system/getty@tty1.service.d
        cat >/etc/systemd/system/getty@tty1.service.d/autologin.conf <<EOF
[Service]
ExecStart=
ExecStart=-/sbin/agetty --autologin $USERNAME --noclear %I \$TERM
EOF
    fi
    # Winlux init (session Xorg directe)
    mkdir -p /etc/winlux
    printf '%s\n' "$USERNAME" >/etc/winlux/autologin-user
fi

# --- Installation des applications sélectionnées ---
log "Installation des applications : $APPS_SELECTED"
if command -v apt-get >/dev/null 2>&1 && test -n "$APPS_SELECTED"; then
    apt-get update -qq 2>/dev/null || true
    for app in $APPS_SELECTED; do
        case "$app" in
            firefox)
                log "Installation de Firefox..."
                apt-get install -y --no-install-recommends firefox 2>/dev/null || \
                apt-get install -y --no-install-recommends firefox-esr 2>/dev/null || \
                log "Échec de l'installation de Firefox"
                ;;
            libreoffice)
                log "Installation de LibreOffice..."
                apt-get install -y --no-install-recommends \
                    libreoffice-writer libreoffice-calc libreoffice-impress \
                    libreoffice-l10n-fr 2>/dev/null || \
                log "Échec de l'installation de LibreOffice"
                ;;
            vlc)
                log "Installation de VLC..."
                apt-get install -y --no-install-recommends vlc 2>/dev/null || \
                log "Échec de l'installation de VLC"
                ;;
            gimp)
                log "Installation de GIMP..."
                apt-get install -y --no-install-recommends gimp 2>/dev/null || \
                log "Échec de l'installation de GIMP"
                ;;
            git)
                log "Installation des outils développeur..."
                apt-get install -y --no-install-recommends \
                    git build-essential python3 2>/dev/null || \
                log "Échec de l'installation des outils développeur"
                ;;
            geany)
                log "Installation de Geany..."
                apt-get install -y --no-install-recommends geany 2>/dev/null || \
                log "Échec de l'installation de Geany"
                ;;
            audacity)
                log "Installation d'Audacity..."
                apt-get install -y --no-install-recommends audacity 2>/dev/null || \
                log "Échec de l'installation d'Audacity"
                ;;
            obs-studio)
                log "Installation d'OBS Studio..."
                apt-get install -y --no-install-recommends obs-studio 2>/dev/null || \
                log "Échec de l'installation d'OBS Studio"
                ;;
            *)
                log "Application inconnue : $app, ignorée"
                ;;
        esac
    done
fi

# Sauvegarder la liste des apps sélectionnées
mkdir -p /etc/winlux
printf '%s\n' $APPS_SELECTED >/etc/winlux/selected-apps

# --- Détection et configuration GPU ---
log 'Détection du matériel graphique...'
hw_detect=
if test -n "$root" && test -x "$root/scripts/winlux-hw-detect.sh"; then
    hw_detect="$root/scripts/winlux-hw-detect.sh"
elif test -x /opt/winlux/scripts/winlux-hw-detect.sh; then
    hw_detect=/opt/winlux/scripts/winlux-hw-detect.sh
elif test -x /usr/share/winlux/winlux-hw-detect.sh; then
    hw_detect=/usr/share/winlux/winlux-hw-detect.sh
fi

if test -n "$hw_detect"; then
    mkdir -p /etc/winlux
    "$hw_detect" --report >/etc/winlux/hardware-report.txt 2>/dev/null || true
    log "Rapport matériel enregistré dans /etc/winlux/hardware-report.txt"
else
    log 'Script de détection matériel non trouvé, sauté'
fi

# Installer les pilotes GPU si APT est disponible
if command -v apt-get >/dev/null 2>&1; then
    gpu_vendors=$(lspci 2>/dev/null | grep -iE 'VGA|3D|Display' || true)

    if printf '%s' "$gpu_vendors" | grep -qi nvidia; then
        log 'GPU NVIDIA détecté — installation des pilotes...'
        apt-get install -y --no-install-recommends \
            nvidia-driver nvidia-utils 2>/dev/null || \
        apt-get install -y --no-install-recommends \
            nvidia-driver-535 2>/dev/null || \
        log 'Installation des pilotes NVIDIA échouée (pas de dépôt configuré ?)'
    fi

    if printf '%s' "$gpu_vendors" | grep -qi 'AMD\|ATI\|Radeon'; then
        log 'GPU AMD détecté — firmware Mesa/AMDGPU'
        apt-get install -y --no-install-recommends \
            mesa-vulkan-drivers firmware-amd-graphics 2>/dev/null || \
        apt-get install -y --no-install-recommends \
            mesa-vulkan-drivers linux-firmware 2>/dev/null || \
        log 'Installation des pilotes AMD échouée'
    fi

    if printf '%s' "$gpu_vendors" | grep -qi intel; then
        log 'GPU Intel détecté — pilotes Mesa i965/iris'
        apt-get install -y --no-install-recommends \
            mesa-vulkan-drivers intel-media-va-driver 2>/dev/null || \
        log 'Installation des pilotes Intel échouée'
    fi
fi

# ============================================================
#  Marquer le premier démarrage comme terminé
# ============================================================
mkdir -p /etc/winlux
touch "$MARKER"
log 'Configuration initiale terminée.'

# --- Proposition de redémarrage ---
if yesno "Redémarrer maintenant pour appliquer tous les paramètres ?" oui; then
    log 'Redémarrage...'
    sync
    if command -v systemctl >/dev/null 2>&1; then
        systemctl reboot
    else
        reboot -f 2>/dev/null || reboot
    fi
fi

log 'Terminé. Un redémarrage est recommandé pour appliquer tous les paramètres.'
