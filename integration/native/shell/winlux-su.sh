# Winlux : « su » quand le compte root est verrouillé (réglage par défaut).
# Sous Winlux comme sous Ubuntu, root n'a pas de mot de passe : « su » seul
# répond « Échec de l'authentification ». Pour un administrateur, la même
# commande passe donc par sudo, qui demande son propre mot de passe.
# Lu par le terminal Winlux (bash --rcfile /usr/share/winlux/native/shell/bashrc).

_winlux_su_message() {
    case "${LANGUAGE:-${LC_ALL:-${LANG:-fr}}}" in
        en*) printf '%s\n' 'The root account is locked on Winlux: sudo takes over, type your own password.' ;;
        es*) printf '%s\n' 'La cuenta root está bloqueada en Winlux: sudo toma el relevo, escriba su propia contraseña.' ;;
        de*) printf '%s\n' 'Das root-Konto ist unter Winlux gesperrt: sudo übernimmt, geben Sie Ihr eigenes Passwort ein.' ;;
        *) printf '%s\n' 'Le compte root est verrouillé sous Winlux : sudo prend le relais, tapez votre propre mot de passe.' ;;
    esac
}

su() {
    local login=0 command= user=root have_command=0
    local root_enabled=${WINLUX_ROOT_ENABLED_FILE:-/etc/winlux/root-enabled}
    if test "$(id -u)" = 0 || test -e "$root_enabled" || ! command -v sudo >/dev/null 2>&1; then
        command su "$@"; return
    fi
    case " $(id -Gn 2>/dev/null) " in
        *' sudo '*|*' admin '*|*' wheel '*) ;;
        *) command su "$@"; return ;;
    esac
    local seen_user=0
    while test "$#" -gt 0; do
        case "$1" in
            -|-l|--login) login=1 ;;
            -c|--command) test "$#" -ge 2 || { command su "$@"; return; }; command=$2; have_command=1; shift ;;
            --command=*) command=${1#--command=}; have_command=1 ;;
            -c*) command=${1#-c}; have_command=1 ;;
            -*) command su "$@"; return ;;
            *) if test "$seen_user" = 1; then command su "$@"; return; fi
               user=$1; seen_user=1 ;;
        esac
        shift
    done
    if test "$user" != root; then
        # Autre compte : su habituel, avec le mot de passe de ce compte.
        set -- "$user"
        test "$login" = 1 && set -- - "$@"
        test "$have_command" = 1 && set -- "$@" -c "$command"
        command su "$@"
        return
    fi
    _winlux_su_message >&2
    if test "$have_command" = 1; then
        if test "$login" = 1; then sudo -i sh -c "$command"; else sudo sh -c "$command"; fi
    elif test "$login" = 1; then sudo -i
    else sudo -s
    fi
}
