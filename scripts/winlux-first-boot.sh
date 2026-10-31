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

# ============================================================
#  Étape 1 : Langue
# ============================================================
log 'Étape 1/7 : Langue'
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
log 'Étape 2/7 : Clavier'
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
log 'Étape 3/7 : Fuseau horaire'
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
log 'Étape 4/7 : Nom de la machine'
input "Nom de la machine" "winlux-desktop"
HOSTNAME_CHOICE=${REPLY:-winlux-desktop}
# Nettoyage : uniquement alphanumérique et tirets
HOSTNAME_CHOICE=$(printf '%s' "$HOSTNAME_CHOICE" | tr -cd 'a-zA-Z0-9-' | head -c 63)
test -n "$HOSTNAME_CHOICE" || HOSTNAME_CHOICE=winlux-desktop

# ============================================================
#  Étape 5 : Compte utilisateur
# ============================================================
log 'Étape 5/7 : Compte utilisateur'
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

# ============================================================
#  Étape 6 : Mot de passe root
# ============================================================
log 'Étape 6/7 : Mot de passe root'
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
#  Étape 7 : Connexion automatique
# ============================================================
log 'Étape 7/7 : Connexion automatique'
AUTO_LOGIN=0
if yesno "Activer la connexion automatique pour $USERNAME ?" oui; then
    AUTO_LOGIN=1
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
    # Remplacer l'ancienne entrée localhost si présente
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

# --- Compte utilisateur ---
log "Création du compte : $USERNAME"
GROUPS="sudo,audio,video,plugdev,input"
# Ajouter les groupes s'ils existent
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

# Sudoers sans mot de passe pour le groupe sudo (optionnel mais pratique)
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
        apt-get update -qq 2>/dev/null || true
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
