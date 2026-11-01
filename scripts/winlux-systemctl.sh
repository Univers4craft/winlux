#!/bin/sh
# Shim systemctl pour Winlux : traduit les commandes systemd en gestion de
# processus directe. Winlux utilise son propre init, pas systemd. Ce script
# permet aux paquets installés via APT de démarrer leurs services normalement.
set -eu

UNIT_DIRS="/etc/systemd/system /usr/lib/systemd/system /lib/systemd/system /run/systemd/system"
ENABLED_DIR="/etc/winlux/services-enabled"
RUN_DIR="/run/winlux/services"
LOG_DIR="/run/winlux"

mkdir -p "$ENABLED_DIR" "$RUN_DIR"

usage() {
    echo "Usage: systemctl {start|stop|restart|reload|status|enable|disable|is-active|is-enabled|daemon-reload|mask|unmask|list-unit-files} [unit]"
    exit 1
}

find_unit_file() {
    name=$1
    case "$name" in *.service|*.socket|*.timer|*.target|*.mount|*.path) ;; *) name="$name.service" ;; esac
    for dir in $UNIT_DIRS; do
        if test -f "$dir/$name"; then
            printf '%s\n' "$dir/$name"
            return 0
        fi
    done
    return 1
}

unit_name() {
    name=$1
    case "$name" in *.service|*.socket|*.timer|*.target|*.mount|*.path) ;; *) name="$name.service" ;; esac
    printf '%s\n' "$name"
}

parse_unit() {
    unit_file=$1
    _exec_start=; _exec_stop=; _exec_reload=; _type=simple
    _user=; _group=; _workdir=; _pidfile=; _remain_after=no
    _env_files=; _env_vars=
    section=
    while IFS= read -r line; do
        case "$line" in
            '#'*|';'*|'') continue ;;
            '['*']') section=$line; continue ;;
        esac
        test "$section" = "[Service]" || continue
        key=${line%%=*}; value=${line#*=}
        case "$key" in
            ExecStart) _exec_start=$value ;;
            ExecStop) _exec_stop=$value ;;
            ExecReload) _exec_reload=$value ;;
            Type) _type=$value ;;
            User) _user=$value ;;
            Group) _group=$value ;;
            WorkingDirectory) _workdir=$value ;;
            PIDFile) _pidfile=$value ;;
            RemainAfterExit) _remain_after=$value ;;
            EnvironmentFile) _env_files="$_env_files $value" ;;
            Environment) _env_vars="$_env_vars $value" ;;
        esac
    done < "$unit_file"
}

is_masked() {
    name=$(unit_name "$1")
    test -L "/etc/systemd/system/$name" && test "$(readlink "/etc/systemd/system/$name")" = /dev/null
}

read_pid() {
    name=$(unit_name "$1")
    pidfile="$RUN_DIR/${name%.service}.pid"
    if test -n "${_pidfile:-}" && test -f "$_pidfile"; then
        pidfile=$_pidfile
    fi
    test -f "$pidfile" && cat "$pidfile"
}

# Un pid vide, non numérique ou nul (oneshot RemainAfterExit) n'est jamais
# « vivant » : kill -0 0 interrogerait tout le groupe de processus.
pid_alive() {
    case "${1:-}" in ''|*[!0-9]*) return 1 ;; esac
    test "$1" -gt 0 && kill -0 "$1" 2>/dev/null
}

is_running() {
    pid=$(read_pid "$1")
    pid_alive "$pid"
}

build_env() {
    env_cmd=""
    for ef in $_env_files; do
        case "$ef" in -*) ef=${ef#-}; test -f "$ef" || continue ;; esac
        if test -f "$ef"; then
            while IFS= read -r eline; do
                case "$eline" in '#'*|'') continue ;; esac
                env_cmd="$env_cmd $eline"
            done < "$ef"
        fi
    done
    for ev in $_env_vars; do
        ev=$(printf '%s' "$ev" | sed 's/^"//;s/"$//')
        env_cmd="$env_cmd $ev"
    done
}

# Ligne Exec* d'une unité : préfixes systemd (-, @, :, +, !) retirés, commande
# relative résolue dans PATH. La ligne garde ses guillemets : elle est confiée
# telle quelle à sh -c par run_exec, jamais découpée par le shell courant.
exec_command() {
    cmd=$1
    while :; do
        case "$cmd" in -*|@*|:*|+*|!*) cmd=${cmd#?} ;; *) break ;; esac
    done
    case "$cmd" in
        /*|'"'*) ;;
        *)
            program=${cmd%% *}
            if resolved=$(command -v "$program" 2>/dev/null) && test -n "$resolved"; then
                case "$cmd" in *' '*) cmd="$resolved ${cmd#* }" ;; *) cmd=$resolved ;; esac
            fi ;;
    esac
    printf '%s\n' "$cmd"
}

# Exécute une ligne Exec* dans un shell enfant (guillemets respectés, pas
# d'eval dans ce script), sous l'utilisateur de l'unité le cas échéant.
run_exec() {
    if test -n "${run_as:-}"; then
        # shellcheck disable=SC2086 # env_prefix et run_as sont des listes de mots
        env $env_prefix $run_as "exec $1"
    else
        # shellcheck disable=SC2086
        env $env_prefix /bin/sh -c "exec $1"
    fi
}

do_start() {
    name=$(unit_name "$1")
    if is_masked "$1"; then
        echo "Unit $name is masked." >&2; return 1
    fi
    unit_file=$(find_unit_file "$1") || { echo "Unit $name not found." >&2; return 1; }
    if is_running "$1"; then return 0; fi
    parse_unit "$unit_file"
    test -n "$_exec_start" || { echo "Unit $name has no ExecStart." >&2; return 1; }
    cmd=$(exec_command "$_exec_start")

    build_env
    env_prefix=""
    for pair in $env_cmd; do
        env_prefix="$env_prefix $pair"
    done
    run_as=""
    if test -n "$_user"; then
        if command -v su >/dev/null 2>&1; then
            run_as="su -s /bin/sh $_user -c"
        fi
    fi
    if test -n "$_workdir" && test -d "$_workdir"; then
        cd "$_workdir"
    fi
    pidfile="$RUN_DIR/${name%.service}.pid"
    logfile="$LOG_DIR/service-${name%.service}.log"
    case "$_type" in
        oneshot)
            run_exec "$cmd" >>"$logfile" 2>&1
            if test "$_remain_after" = yes; then
                echo "0" > "$pidfile"
            fi
            ;;
        forking)
            run_exec "$cmd" >>"$logfile" 2>&1
            if test -n "$_pidfile" && test -f "$_pidfile"; then
                cp "$_pidfile" "$pidfile"
            fi
            ;;
        simple|notify|dbus|idle|exec|*)
            run_exec "$cmd" >>"$logfile" 2>&1 &
            echo "$!" > "$pidfile"
            ;;
    esac
}

do_stop() {
    name=$(unit_name "$1")
    unit_file=$(find_unit_file "$1" 2>/dev/null) || true
    if test -n "$unit_file"; then
        parse_unit "$unit_file"
        if test -n "$_exec_stop"; then
            run_as=""; env_prefix=""
            run_exec "$(exec_command "$_exec_stop")" >>"$LOG_DIR/service-${name%.service}.log" 2>&1 || true
        fi
    fi
    pid=$(read_pid "$1")
    if pid_alive "$pid"; then
        kill "$pid" 2>/dev/null || true
        attempt=0
        while test "$attempt" -lt 20 && kill -0 "$pid" 2>/dev/null; do
            sleep 0.25
            attempt=$((attempt + 1))
        done
        if kill -0 "$pid" 2>/dev/null; then
            kill -9 "$pid" 2>/dev/null || true
        fi
    fi
    rm -f "$RUN_DIR/${name%.service}.pid"
}

do_reload() {
    name=$(unit_name "$1")
    unit_file=$(find_unit_file "$1") || { echo "Unit $name not found." >&2; return 1; }
    parse_unit "$unit_file"
    if test -n "$_exec_reload"; then
        run_as=""; env_prefix=""
        run_exec "$(exec_command "$_exec_reload")" >>"$LOG_DIR/service-${name%.service}.log" 2>&1
    else
        pid=$(read_pid "$1")
        if pid_alive "$pid"; then
            kill -HUP "$pid"
        fi
    fi
}

do_status() {
    name=$(unit_name "$1")
    unit_file=$(find_unit_file "$1" 2>/dev/null) || { echo "$name: not-found"; return 4; }
    if is_masked "$1"; then echo "$name: masked"; return 1; fi
    if is_running "$1"; then
        pid=$(read_pid "$1")
        echo "$name: active (running, pid=$pid)"
        return 0
    fi
    pidfile="$RUN_DIR/${name%.service}.pid"
    if test -f "$pidfile" && test "$(cat "$pidfile")" = 0; then
        echo "$name: active (exited)"
        return 0
    fi
    echo "$name: inactive"
    return 3
}

do_enable() {
    name=$(unit_name "$1")
    find_unit_file "$1" >/dev/null || { echo "Unit $name not found." >&2; return 1; }
    touch "$ENABLED_DIR/$name"
}

do_disable() {
    name=$(unit_name "$1")
    rm -f "$ENABLED_DIR/$name"
}

do_is_active() {
    name=$(unit_name "$1")
    if is_running "$1"; then echo "active"; return 0; fi
    pidfile="$RUN_DIR/${name%.service}.pid"
    if test -f "$pidfile" && test "$(cat "$pidfile")" = 0; then echo "active"; return 0; fi
    echo "inactive"; return 1
}

do_is_enabled() {
    name=$(unit_name "$1")
    if is_masked "$1"; then echo "masked"; return 1; fi
    if test -f "$ENABLED_DIR/$name"; then echo "enabled"; return 0; fi
    echo "disabled"; return 1
}

do_mask() {
    name=$(unit_name "$1")
    mkdir -p /etc/systemd/system
    ln -sf /dev/null "/etc/systemd/system/$name"
}

do_unmask() {
    name=$(unit_name "$1")
    if test -L "/etc/systemd/system/$name" && test "$(readlink "/etc/systemd/system/$name")" = /dev/null; then
        rm -f "/etc/systemd/system/$name"
    fi
}

do_list_unit_files() {
    for dir in $UNIT_DIRS; do
        test -d "$dir" || continue
        for f in "$dir"/*.service; do
            test -f "$f" || continue
            name=${f##*/}
            if is_masked "$name"; then state=masked
            elif test -f "$ENABLED_DIR/$name"; then state=enabled
            else state=disabled; fi
            printf '%-48s %s\n' "$name" "$state"
        done
    done | sort -u
}

boot_start_enabled() {
    test -d "$ENABLED_DIR" || return 0
    for entry in "$ENABLED_DIR"/*; do
        test -f "$entry" || continue
        name=${entry##*/}
        do_start "$name" || echo "winlux-systemctl: failed to start $name" >&2
    done
}

test "$#" -ge 1 || usage
command=$1; shift

case "$command" in
    start)      test "$#" -ge 1 || usage; for u in "$@"; do do_start "$u"; done ;;
    stop)       test "$#" -ge 1 || usage; for u in "$@"; do do_stop "$u"; done ;;
    restart)    test "$#" -ge 1 || usage; for u in "$@"; do do_stop "$u"; do_start "$u"; done ;;
    reload)     test "$#" -ge 1 || usage; for u in "$@"; do do_reload "$u"; done ;;
    reload-or-restart) test "$#" -ge 1 || usage; for u in "$@"; do do_reload "$u" 2>/dev/null || { do_stop "$u"; do_start "$u"; }; done ;;
    try-restart) test "$#" -ge 1 || usage; for u in "$@"; do is_running "$u" && { do_stop "$u"; do_start "$u"; } || true; done ;;
    status)     test "$#" -ge 1 || usage; rc=0; for u in "$@"; do do_status "$u" || rc=$?; done; exit $rc ;;
    enable)     test "$#" -ge 1 || usage; for u in "$@"; do do_enable "$u"; done ;;
    disable)    test "$#" -ge 1 || usage; for u in "$@"; do do_disable "$u"; done ;;
    is-active)  test "$#" -ge 1 || usage; rc=0; for u in "$@"; do do_is_active "$u" || rc=$?; done; exit $rc ;;
    is-enabled) test "$#" -ge 1 || usage; rc=0; for u in "$@"; do do_is_enabled "$u" || rc=$?; done; exit $rc ;;
    mask)       test "$#" -ge 1 || usage; for u in "$@"; do do_mask "$u"; done ;;
    unmask)     test "$#" -ge 1 || usage; for u in "$@"; do do_unmask "$u"; done ;;
    list-unit-files) do_list_unit_files ;;
    daemon-reload|daemon-reexec|reset-failed|preset) exit 0 ;;
    show)       exit 0 ;;
    cat)        test "$#" -ge 1 || usage; find_unit_file "$1" | xargs cat ;;
    is-system-running) echo "running"; exit 0 ;;
    --version)  echo "systemctl shim (winlux)"; exit 0 ;;
    --boot-start) boot_start_enabled ;;
    *)          echo "systemctl: unsupported command: $command" >&2; exit 1 ;;
esac
