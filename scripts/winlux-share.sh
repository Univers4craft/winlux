#!/bin/sh
# winlux-share : partager des dossiers de Winlux avec les autres ordinateurs
# du réseau (Windows, Mac, Linux), comme « Partager ce dossier » sous Windows.
#
# Le serveur est Samba (smbd), l'outil standard de Linux pour le protocole
# SMB que Windows et macOS parlent nativement. Winlux ne fait que le piloter :
# registre des partages, droits par compte, mots de passe repris des comptes
# Winlux, pare-feu et annonce sur le réseau. Si Samba n'est pas installé,
# l'état publie available=0 et l'interface cache le partage.
#
# Usage (via winlux-control share-<action> ...) :
#   winlux-share status                 écrit /run/winlux/shares.txt
#   winlux-share add [CHEMIN]           partage le dossier (sinon le dossier courant)
#   winlux-share remove ID
#   winlux-share set ID COMPTE none|read|write
#   winlux-share guest ID none|read|write   (accès sans mot de passe)
#   winlux-share enable | disable       interrupteur général du partage
#   winlux-share ensure                 au démarrage de session
#   winlux-share apply                  régénère la configuration et recharge smbd
set -eu
umask 022
root=${WINLUX_CONTROL_ROOT:-}
etc="$root/etc/winlux"
registry="$etc/shares.tsv"
conf="$etc/sharing.conf"
smbdir="$etc/samba"
smbconf="$smbdir/smb.conf"
users_conf="$etc/users.conf"
state="$root/var/lib/winlux/samba"
run="$root/run/winlux"
out="$run/shares.txt"
pidfile="$run/samba/smbd.pid"
wsdd_pid="$run/samba/wsdd.pid"
firewall="$etc/firewall.conf"
avahi_dir="$root/etc/avahi/services"
mkdir -p "$etc" "$run"
# Règles réservées dans le pare-feu Winlux (SMB, découverte Windows et Mac).
FW_TCP=9445
FW_UDP=9446

# L'erreur est aussi publiée dans l'état (error=) pour l'interface.
fail() {
    printf 'winlux-share : %s\n' "$*" >&2
    printf '%s\n' "$*" > "$run/share-error" 2>/dev/null || true
    status 2>/dev/null || true
    exit 1
}
have() { command -v "$1" >/dev/null 2>&1 || test -x "/usr/sbin/$1"; }
tool() { if command -v "$1" >/dev/null 2>&1; then command -v "$1"; else printf '/usr/sbin/%s\n' "$1"; fi; }
smbd_bin=${WINLUX_SMBD:-smbd}
smbpasswd_bin=${WINLUX_SMBPASSWD:-smbpasswd}
available() { test "${WINLUX_SHARE_AVAILABLE:-}" = 1 || have "$smbd_bin"; }
get() { test -r "$conf" && sed -n "s/^$1=//p" "$conf" | tail -n 1 || true; }
put() {
    tmp="$conf.$$"
    if test -r "$conf"; then grep -v "^$1=" "$conf" > "$tmp" || true; else : > "$tmp"; fi
    printf '%s=%s\n' "$1" "$2" >> "$tmp"
    mv "$tmp" "$conf"
}
enabled() { test "$(get ENABLED)" != 0; }

# ---------------------------------------------------------------- comptes

accounts() { test -r "$users_conf" && cut -d: -f1 "$users_conf" | sed '/^$/d' || true; }
check_user() {
    case "$1" in ''|*[!A-Za-z0-9._-]*) fail "nom de compte invalide : $1" ;; esac
    accounts | grep -qxF "$1" || fail "compte inconnu : $1"
}
check_level() { case "$1" in none|read|write) ;; *) fail "droit inconnu : $1 (none, read ou write)" ;; esac; }

# ---------------------------------------------------------------- registre
# Une ligne par partage : ID, nom, chemin Linux, accès invité, droits
# (compte:read|write séparés par des virgules, « - » si aucun).

rows() { test -r "$registry" && grep -v '^#' "$registry" | sed '/^$/d' || true; }
row() { rows | awk -F '\t' -v id="$1" '$1 == id' | head -n 1; }
col() { printf '%s\n' "$1" | cut -f "$2"; }
check_id() {
    case "$1" in ''|*[!0-9]*) fail "partage invalide : $1" ;; esac
    test -n "$(row "$1")" || fail "partage inconnu : $1"
}
replace_row() {
    id=$1 new=$2
    tmp="$registry.$$"
    rows | awk -F '\t' -v id="$id" '$1 != id' > "$tmp"
    test -z "$new" || printf '%s\n' "$new" >> "$tmp"
    sort -n "$tmp" > "$tmp.s" && rm -f "$tmp"
    mv "$tmp.s" "$registry"
    chmod 644 "$registry"
}

# Dossiers système jamais partagés : le partage sert aux données.
check_path() {
    p=$1
    case "$p" in *'	'*|*'
'*) fail "nom de dossier non pris en charge" ;; esac
    test -d "$p" || fail "dossier introuvable : $p"
    rel=${p#"$root"}
    case "$rel" in
        ''|/|/bin|/bin/*|/boot|/boot/*|/dev|/dev/*|/etc|/etc/*|/lib|/lib/*|/lib64|/lib64/*|/proc|/proc/*|/sbin|/sbin/*|/sys|/sys/*|/usr|/usr/*|/var|/var/*|/opt|/opt/*|/snap|/snap/*)
            fail "les dossiers du système ne peuvent pas être partagés : $rel" ;;
        /run/media/*|/run/winlux/drives/*) ;;
        /run|/run/*|/tmp|/tmp/*) fail "dossier temporaire : partage impossible" ;;
    esac
}

# Nom visible sur le réseau : caractères interdits par Windows remplacés,
# 80 octets au plus, unique (« Photos 2 » si « Photos » existe déjà).
share_name() {
    base=$(printf '%s' "$1" | tr '\\/:*?"<>|[]+=;,%' '_______________' | cut -c 1-72 | sed 's/^ *//; s/ *$//')
    case "$(printf '%s' "$base" | tr 'A-Z' 'a-z')" in ''|global|homes|printers|'print$'|'ipc$') base="Partage" ;; esac
    name=$base n=2
    while rows | cut -f 2 | tr 'A-Z' 'a-z' | grep -qxF "$(printf '%s' "$name" | tr 'A-Z' 'a-z')"; do
        name="$base $n"; n=$((n + 1))
    done
    printf '%s\n' "$name"
}

# Droit d'un compte dans une liste « a:read,b:write ».
level_of() { printf '%s\n' "$1" | tr ',' '\n' | sed -n "s/^$2://p" | head -n 1; }
set_level() {
    list=$(printf '%s\n' "$1" | tr ',' '\n' | grep -v "^$2:" | grep -v '^-$' | sed '/^$/d' || true)
    test "$3" = none || list=$(printf '%s\n%s:%s\n' "$list" "$2" "$3" | sed '/^$/d')
    list=$(printf '%s\n' "$list" | tr '\n' ',' | sed 's/,$//')
    printf '%s\n' "${list:--}"
}

# ---------------------------------------------------------------- Samba

host_name() {
    h=$(cat "$root/etc/hostname" 2>/dev/null || hostname 2>/dev/null || printf 'winlux')
    h=$(printf '%s' "$h" | tr -cd 'A-Za-z0-9-' | cut -c 1-15 | tr 'a-z' 'A-Z')
    printf '%s\n' "${h:-WINLUX}"
}
address() {
    if command -v ip >/dev/null 2>&1; then
        ip -4 -o addr show scope global 2>/dev/null | awk 'NR == 1 { split($4, a, "/"); print a[1] }'
    fi
}
owner_of() {
    o=$(stat -c %U "$1" 2>/dev/null || printf 'root')
    case "$o" in UNKNOWN|'') o=root ;; esac
    printf '%s\n' "$o"
}
vfs_dir() { "$(tool "$smbd_bin")" -b 2>/dev/null | sed -n 's/^ *MODULESDIR: *//p' | head -n 1; }

write_config() {
    mkdir -p "$smbdir" "$state/private" "$state/lock" "$state/cache" "$run/samba"
    chmod 700 "$state/private"
    guests=0
    rows | awk -F '\t' '$4 != "none" { f=1 } END { exit !f }' && guests=1
    mac=
    modules=$(vfs_dir || true)
    if test -n "$modules" && test -f "$modules/vfs/fruit.so" && test -f "$modules/vfs/streams_xattr.so"; then mac=1; fi
    tmp="$smbconf.$$"
    {
        printf '# Généré par winlux-share : modifiez les partages depuis Winlux.\n'
        printf '[global]\n'
        printf '   workgroup = WORKGROUP\n'
        printf '   netbios name = %s\n' "$(host_name)"
        printf '   server string = Winlux\n'
        printf '   server role = standalone server\n'
        printf '   security = user\n'
        printf '   server min protocol = SMB2_10\n'
        printf '   ntlm auth = ntlmv2-only\n'
        printf '   smb encrypt = if_required\n'
        printf '   disable netbios = yes\n'
        printf '   smb ports = 445\n'
        printf '   passdb backend = tdbsam:%s/private/passdb.tdb\n' "$state"
        printf '   private dir = %s/private\n' "$state"
        printf '   lock directory = %s/lock\n' "$state"
        printf '   state directory = %s/lock\n' "$state"
        printf '   cache directory = %s/cache\n' "$state"
        printf '   pid directory = %s/samba\n' "$run"
        printf '   ncalrpc dir = %s/samba/ncalrpc\n' "$run"
        printf '   log file = %s/samba/log.%%m\n' "$run"
        printf '   max log size = 200\n'
        printf '   load printers = no\n   printing = bsd\n   printcap name = /dev/null\n   disable spoolss = yes\n'
        if test "$guests" = 1; then printf '   map to guest = bad user\n   guest account = nobody\n'; fi
        if test -n "$mac"; then
            # Compatibilité Finder (métadonnées et icônes macOS).
            printf '   vfs objects = fruit streams_xattr\n   fruit:metadata = stream\n   fruit:model = MacSamba\n'
        fi
        if enabled; then
            rows | while IFS='	' read -r id name path guest acl; do
                test -d "$path" || continue
                readers= writers=
                for entry in $(printf '%s\n' "$acl" | tr ',' ' '); do
                    case "$entry" in
                        *:write) writers="$writers ${entry%:write}" ;;
                        *:read) readers="$readers ${entry%:read}" ;;
                    esac
                done
                printf '\n[%s]\n' "$name"
                printf '   comment = Winlux %s\n' "$id"
                printf '   path = %s\n' "$path"
                printf '   browseable = yes\n'
                # Les fichiers créés à distance appartiennent au propriétaire du dossier.
                printf '   force user = %s\n' "$(owner_of "$path")"
                printf '   create mask = 0664\n   directory mask = 0775\n'
                case "$guest" in
                    write) printf '   guest ok = yes\n   read only = no\n' ;;
                    read)
                        printf '   guest ok = yes\n   read only = yes\n'
                        test -z "$writers" || printf '   write list =%s\n' "$writers" ;;
                    *)
                        printf '   guest ok = no\n   read only = yes\n'
                        allowed="$readers$writers"
                        printf '   valid users =%s\n' "${allowed:- nobody-allowed}"
                        test -z "$writers" || printf '   write list =%s\n' "$writers" ;;
                esac
            done
        fi
    } > "$tmp"
    mv "$tmp" "$smbconf"
    chmod 644 "$smbconf"
    # Le smbd du paquet (démarré par apt via le shim systemctl) lit la même
    # configuration : l'originale est gardée à côté.
    stock="$root/etc/samba/smb.conf"
    if test -d "$root/etc/samba" && ! grep -q '^# winlux-share' "$stock" 2>/dev/null; then
        test ! -f "$stock" || cp -p "$stock" "$stock.winlux-orig"
        printf '# winlux-share : configuration gérée par Winlux\ninclude = %s\n' "/etc/winlux/samba/smb.conf" > "$stock"
    fi
}

# Les comptes Winlux se connectent depuis le réseau avec leur propre mot de
# passe ; un compte sans mot de passe n'a accès qu'aux partages invités.
sync_passwords() {
    test -r "$users_conf" || return 0
    have "$smbpasswd_bin" || return 0
    known=$("$(tool pdbedit)" -s "$smbconf" -L 2>/dev/null | cut -d: -f1 || true)
    while IFS=: read -r name _role pass; do
        test -n "$name" || continue
        if test "$pass" = nopass || ! id -u "$name" >/dev/null 2>&1; then
            if printf '%s\n' "$known" | grep -qxF "$name"; then
                "$(tool "$smbpasswd_bin")" -c "$smbconf" -x "$name" >/dev/null 2>&1 || true
            fi
            continue
        fi
        printf '%s\n%s\n' "$pass" "$pass" | "$(tool "$smbpasswd_bin")" -c "$smbconf" -s -a "$name" >/dev/null 2>&1 || true
    done < "$users_conf"
}

running() {
    test -r "$pidfile" || return 1
    pid=$(cat "$pidfile")
    case "$pid" in ''|*[!0-9]*) return 1 ;; esac
    kill -0 "$pid" 2>/dev/null
}

# Pare-feu Winlux : ports SMB et découverte ouverts sur les réseaux privés
# et de domaine seulement (comme le partage de fichiers de Windows).
firewall_rules() {
    tmp="$firewall.$$"
    if test -r "$firewall"; then
        grep -v "^rule=$FW_TCP	" "$firewall" | grep -v "^rule=$FW_UDP	" > "$tmp" || true
    else : > "$tmp"; fi
    if test "$1" = on; then
        printf 'rule=%s\tPartage de fichiers Winlux (SMB)\tin\tallow\ttcp\t445,5357\t\tprivate,domain\t1\n' "$FW_TCP" >> "$tmp"
        printf 'rule=%s\tDecouverte du reseau Winlux\tin\tallow\tudp\t3702,5353\t\tprivate,domain\t1\n' "$FW_UDP" >> "$tmp"
    fi
    if test -s "$tmp" || test -r "$firewall"; then mv "$tmp" "$firewall"; chmod 644 "$firewall"; else rm -f "$tmp"; fi
}

# Annonce : Bonjour (Avahi) pour le Finder, WS-Discovery (wsdd) pour le
# panneau Réseau de l'Explorateur Windows. Les deux sont facultatifs.
announce() {
    if test -d "$avahi_dir"; then
        if test "$1" = on; then
            cat > "$avahi_dir/winlux-smb.service" <<'XML'
<?xml version="1.0" standalone='no'?>
<!DOCTYPE service-group SYSTEM "avahi-service.dtd">
<service-group>
  <name replace-wildcards="yes">%h</name>
  <service><type>_smb._tcp</type><port>445</port></service>
  <service><type>_device-info._tcp</type><port>0</port><txt-record>model=MacSamba</txt-record></service>
</service-group>
XML
        else rm -f "$avahi_dir/winlux-smb.service"; fi
    fi
    if test -r "$wsdd_pid" && kill -0 "$(cat "$wsdd_pid")" 2>/dev/null; then
        test "$1" = on || { kill "$(cat "$wsdd_pid")" 2>/dev/null || true; rm -f "$wsdd_pid"; }
    elif test "$1" = on && test -z "${WINLUX_SHARE_NO_SERVER:-}" && command -v wsdd >/dev/null 2>&1; then
        wsdd --shortlog --hostname "$(host_name)" --workgroup WORKGROUP >> "$run/samba/wsdd.log" 2>&1 &
        printf '%s\n' "$!" > "$wsdd_pid"
    fi
}

start() {
    test -z "${WINLUX_SHARE_NO_SERVER:-}" || return 0
    if running; then kill -HUP "$(cat "$pidfile")" 2>/dev/null || true; return 0; fi
    "$(tool "$smbd_bin")" -D -s "$smbconf" || fail "smbd n'a pas démarré (voir $run/samba/log.smbd)"
    tries=0
    while test "$tries" -lt 50 && ! running; do sleep 0.1; tries=$((tries + 1)); done
    running || fail "smbd s'est arrêté au démarrage (voir $run/samba/log.smbd)"
}
stop() {
    if running; then kill "$(cat "$pidfile")" 2>/dev/null || true; fi
    rm -f "$pidfile"
}

apply() {
    available || return 0
    write_config
    if enabled && test -n "$(rows)"; then
        sync_passwords
        firewall_rules on
        announce on
        start
    else
        firewall_rules off
        announce off
        stop
    fi
}

# ---------------------------------------------------------------- état

# Chemin vu par l'Explorateur : C:\ = dossier personnel, Z:\ = racine Linux.
windows_path() {
    p=${1#"$root"}
    home=${HOME:-/root}; home=${home#"$root"}
    case "$p" in
        "$home") printf 'C:\\\n' ;;
        "$home"/*) printf 'C:\\%s\n' "${p#"$home"/}" | tr '/' '\\' ;;
        *) printf 'Z:\\%s\n' "${p#/}" | tr '/' '\\' ;;
    esac
}

status() {
    serial=$(( $(cat "$run/shares.serial" 2>/dev/null || echo 0) + 1 ))
    printf '%s\n' "$serial" > "$run/shares.serial"
    {
        if available; then printf 'available=1\n'; else printf 'available=0\n'; fi
        if enabled; then printf 'enabled=1\n'; else printf 'enabled=0\n'; fi
        if running; then printf 'running=1\n'; else printf 'running=0\n'; fi
        printf 'host=%s\n' "$(host_name)"
        printf 'address=%s\n' "$(address)"
        profile=$(test -r "$firewall" && sed -n 's/^profile=//p' "$firewall" | tail -n 1 || true)
        printf 'firewall_profile=%s\n' "${profile:-private}"
        discovery=
        test ! -d "$avahi_dir" || discovery=mac
        if command -v wsdd >/dev/null 2>&1; then discovery="${discovery:+$discovery,}windows"; fi
        printf 'discovery=%s\n' "$discovery"
        if test -r "$users_conf"; then
            awk -F: 'NF >= 3 && $1 != "" { print "account=" $1 "|" ($3 == "nopass" ? 0 : 1) }' "$users_conf"
        fi
        rows | while IFS='	' read -r id name path guest acl; do
            ok=1; test -d "$path" || ok=0
            printf 'share=%s|%s|%s|%s|%s|%s\n' "$id" "$name" "$(windows_path "$path")" "$guest" "$acl" "$ok"
        done
        if test -r "$run/share-error"; then printf 'error=%s\n' "$(head -n 1 "$run/share-error")"; fi
        printf 'serial=%s\n' "$serial"
    } > "$out.tmp.$$"
    chmod 644 "$out.tmp.$$"
    mv "$out.tmp.$$" "$out"
    rm -f "$run/share-error"
}

action=${1:-status}
case "$action" in
    status) ;;
    add)
        available || fail "Samba (smbd) n'est pas installé : le partage est indisponible"
        path=${2:-$(pwd -P)}
        case "$path" in /*) ;; *) path="$(pwd -P)/$path" ;; esac
        path=$(cd -- "$path" 2>/dev/null && pwd -P) || fail "dossier introuvable : ${2:-.}"
        check_path "$path"
        existing=$(rows | awk -F '\t' -v p="$path" '$3 == p { print $1 }' | head -n 1)
        if test -z "$existing"; then
            id=$(( $(rows | cut -f 1 | sort -n | tail -n 1 | sed 's/^$/0/') + 1 ))
            name=$(share_name "$(basename -- "$path")")
            # Par défaut : le propriétaire du dossier modifie, les autres
            # comptes Winlux lisent ; aucun accès invité.
            owner=$(owner_of "$path")
            acl=-
            for user in $(accounts); do
                if test "$user" = "$owner"; then acl=$(set_level "$acl" "$user" write)
                else acl=$(set_level "$acl" "$user" read); fi
            done
            replace_row "$id" "$(printf '%s\t%s\t%s\tnone\t%s' "$id" "$name" "$path" "$acl")"
            put ENABLED 1
        fi
        apply ;;
    remove)
        check_id "${2:-}"
        replace_row "$2" ""
        apply ;;
    set)
        check_id "${2:-}"; check_user "${3:-}"; check_level "${4:-}"
        r=$(row "$2")
        acl=$(set_level "$(col "$r" 5)" "$3" "$4")
        replace_row "$2" "$(printf '%s\t%s\t%s\t%s\t%s' "$2" "$(col "$r" 2)" "$(col "$r" 3)" "$(col "$r" 4)" "$acl")"
        apply ;;
    guest)
        check_id "${2:-}"; check_level "${3:-}"
        r=$(row "$2")
        replace_row "$2" "$(printf '%s\t%s\t%s\t%s\t%s' "$2" "$(col "$r" 2)" "$(col "$r" 3)" "$3" "$(col "$r" 5)")"
        apply ;;
    enable) put ENABLED 1; apply ;;
    disable) put ENABLED 0; apply ;;
    ensure|apply) apply ;;
    *) sed -n '2,/^set -eu/p' "$0" | sed '$d' | sed 's/^# \{0,1\}//' >&2; exit 2 ;;
esac
status
