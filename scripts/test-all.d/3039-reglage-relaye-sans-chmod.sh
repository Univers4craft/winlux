# #3039 : en session relayée, winlux-root écrit le réglage en root (644) ;
# winlux-control ne doit pas relancer un chmod en utilisateur, qui échoue
# et fait annoncer « Échec de l'opération » pour un réglage enregistré.
# Hors relais, le fichier reste mis en 644.
reglage_relaye_sans_chmod() {
    d=$(mktemp -d "${TMPDIR:-/tmp}/winlux-3039.XXXXXX") || return 1
    mkdir -p "$d/bin" "$d/etc"
    # Faux outils : id rend un compte ordinaire, chmod note et échoue comme
    # sur un fichier à root, winlux-root note sa demande et écrit.
    printf '#!/bin/sh\necho 1000\n' > "$d/bin/id"
    printf '#!/bin/sh\necho "$*" >> "%s/chmod.log"\nexit 1\n' "$d" > "$d/bin/chmod"
    printf '#!/bin/sh\necho "$*" >> "%s/root.log"\nprintf "%%s=%%s\\n" "$3" "$4" >> "%s/etc/$2"\n' "$d" "$d" > "$d/bin/winlux-root"
    /bin/chmod +x "$d/bin/id" "$d/bin/chmod" "$d/bin/winlux-root"
    python3 -c 'import socket,sys; socket.socket(socket.AF_UNIX).bind(sys.argv[1])' "$d/root.sock"
    # Les fonctions telles qu'écrites dans winlux-control.
    awk '/^relayed_session\(\)/{print; next} /^(save|save_public)\(\) \{/{f=1} f{print} f&&/^}/{f=0}' scripts/winlux-control.sh > "$d/fonctions.sh"
    rc=0
    cat > "$d/essai.sh" <<FIN
root=; config="$d/etc"; relay_socket="$d/root.sock"; WINLUX_ROOT_TOOL="$d/bin/winlux-root"
PATH="$d/bin:\$PATH"
. "$d/fonctions.sh"
save_public "\$config/lock.conf" LOCK_AFTER 0
FIN
    sh "$d/essai.sh" > "$d/sortie" 2>&1 || { echo "réglage relayé : code d'échec"; cat "$d/sortie"; rc=1; }
    printf 'demandes au relais :\n'; cat "$d/root.log" 2>/dev/null
    printf 'chmod lancés :\n'; cat "$d/chmod.log" 2>/dev/null
    grep -qx "conf lock.conf LOCK_AFTER 0" "$d/root.log" 2>/dev/null || { echo "le relais n'a pas reçu le réglage"; rc=1; }
    test ! -s "$d/chmod.log" || { echo "chmod lancé en session relayée"; rc=1; }
    # Hors relais (pas de socket) : écrit sur place puis mis en 644.
    : > "$d/chmod.log"
    sed -i "s|relay_socket=\"$d/root.sock\"|relay_socket=\"$d/absent.sock\"|; s|PATH=\"$d/bin:|PATH=\"$d/bin2:|" "$d/essai.sh"
    mkdir -p "$d/bin2"; cp "$d/bin/id" "$d/bin2/id"
    sh "$d/essai.sh" > "$d/sortie2" 2>&1 || { echo "réglage direct : code d'échec"; cat "$d/sortie2"; rc=1; }
    perm=$(stat -c %a "$d/etc/lock.conf" 2>/dev/null)
    printf 'droits hors relais : %s\n' "$perm"
    test "$perm" = 644 || { echo "hors relais, lock.conf n'est pas en 644"; rc=1; }
    grep -qx "LOCK_AFTER=0" "$d/etc/lock.conf" || { echo "hors relais, valeur absente"; rc=1; }
    case $d in "${TMPDIR:-/tmp}"/winlux-3039.*) rm -r -- "$d" ;; esac
    return $rc
}
step "winlux-control : un réglage relayé (verrouillage, confidentialité, superposition) n'annonce plus d'échec (#3039)" reglage_relaye_sans_chmod
