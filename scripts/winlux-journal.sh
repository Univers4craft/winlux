#!/bin/sh
# winlux-journal : gestion du journal centralisé Winlux.
# Usage :
#   winlux-journal status       — nombre d'entrées et taille du journal
#   winlux-journal list [N]     — dernières N entrées (défaut 50)
#   winlux-journal errors [N]   — dernières N erreurs
#   winlux-journal crashes      — liste des crash dumps
#   winlux-journal clear        — vider le journal
#   winlux-journal export FILE  — exporter le journal complet
#   winlux-journal rotate       — rotation des fichiers de journal
#   winlux-journal events FILE  — journaux système et sécurité pour
#                                 l'Observateur d'événements (une ligne par
#                                 événement : journal, niveau, date ISO,
#                                 source et message séparés par des tabulations)
set -eu

JOURNAL_DIR=/run/winlux/journal
LOG_FILE="$JOURNAL_DIR/winlux.log"
CRASH_FILE="$JOURNAL_DIR/crash.log"
PERSISTENT_DIR=/var/lib/winlux/journal

case "${1:-status}" in
    status)
        if test -f "$LOG_FILE"; then
            lines=$(wc -l < "$LOG_FILE" 2>/dev/null || echo 0)
            size=$(du -h "$LOG_FILE" 2>/dev/null | cut -f1 || echo "0K")
            printf 'Journal: %s entrees, %s\n' "$lines" "$size"
        else
            printf 'Journal vide\n'
        fi
        if test -f "$CRASH_FILE"; then
            # grep -c affiche déjà 0 (et sort en erreur) sans correspondance.
            crashes=$(grep -c '^\[CRASH\]' "$CRASH_FILE" 2>/dev/null || true)
            crashes=${crashes:-0}
            printf 'Crash dumps: %s\n' "$crashes"
        fi
        ;;
    list)
        n=${2:-50}
        if test -f "$LOG_FILE"; then
            tail -n "$n" "$LOG_FILE"
        else
            printf 'Journal vide\n'
        fi
        ;;
    errors)
        n=${2:-50}
        if test -f "$LOG_FILE"; then
            grep '\[error\]' "$LOG_FILE" | tail -n "$n"
        else
            printf 'Aucune erreur\n'
        fi
        ;;
    crashes)
        if test -f "$CRASH_FILE"; then
            cat "$CRASH_FILE"
        else
            printf 'Aucun crash enregistre\n'
        fi
        ;;
    clear)
        : > "$LOG_FILE" 2>/dev/null || true
        : > "$CRASH_FILE" 2>/dev/null || true
        printf 'Journal efface\n'
        ;;
    export)
        dest=${2:-/tmp/winlux-journal-export.log}
        {
            printf '=== Journal Winlux ===\n'
            cat "$LOG_FILE" 2>/dev/null || printf '(vide)\n'
            printf '\n=== Crash Dumps ===\n'
            cat "$CRASH_FILE" 2>/dev/null || printf '(aucun)\n'
        } > "$dest"
        printf 'Journal exporte dans %s\n' "$dest"
        ;;
    rotate)
        mkdir -p "$JOURNAL_DIR"
        if test -f "$LOG_FILE" && test "$(wc -c < "$LOG_FILE" 2>/dev/null)" -gt 1048576; then
            mv "$LOG_FILE" "$LOG_FILE.1"
            : > "$LOG_FILE"
            printf 'Rotation effectuee\n'
        else
            printf 'Pas de rotation necessaire\n'
        fi
        ;;
    events)
        dest=${2:?fichier de destination}
        tmp="$dest.tmp.$$"
        : > "$tmp"
        # journalctl -o short-iso : « DATE HÔTE SOURCE[PID]: message ».
        to_tsv='{ src=$3; sub(/:$/, "", src); msg=$0; sub(/^[^ ]+ [^ ]+ [^ ]+ ?/, "", msg);
                  gsub(/\t/, " ", msg); printf "%s\t%s\t%s\t%s\t%s\n", cat, lvl, $1, src, msg }'
        if command -v journalctl >/dev/null 2>&1; then
            for spec in "error 0..3" "warn 4..4" "info 5..6"; do
                set -- $spec
                journalctl -q --no-pager -o short-iso -p "$2" -n 1500 2>/dev/null |
                    awk -v cat=system -v lvl="$1" "$to_tsv" >> "$tmp" || true
                journalctl -q --no-pager -o short-iso -p "$2" -n 500 SYSLOG_FACILITY=4 SYSLOG_FACILITY=10 2>/dev/null |
                    awk -v cat=security -v lvl="$1" "$to_tsv" >> "$tmp" || true
            done
        fi
        # Sans journald (ISO live) : messages du noyau.
        if ! test -s "$tmp" && command -v dmesg >/dev/null 2>&1; then
            dmesg -x --time-format iso 2>/dev/null | awk '{
                split($0, f, ":"); fac = f[1]; lvl = f[2]; gsub(/ /, "", fac); gsub(/ /, "", lvl)
                if (lvl ~ /^(emerg|alert|crit|err)$/) lvl = "error"
                else if (lvl != "warn") lvl = "info"
                rest = $0; sub(/^[^:]*:[^:]*: /, "", rest)
                ts = rest; sub(/ .*/, "", ts); sub(/,[0-9]+/, "", ts)
                msg = rest; sub(/^[^ ]+ ?/, "", msg); gsub(/\t/, " ", msg)
                cat = (fac == "auth" || fac == "authpriv") ? "security" : "system"
                printf "%s\t%s\t%s\tkernel\t%s\n", cat, lvl, ts, msg
            }' | tail -n 3000 >> "$tmp" || true
        fi
        mv "$tmp" "$dest"
        ;;
    *)
        printf 'Usage: winlux-journal {status|list|errors|crashes|clear|export|rotate|events}\n' >&2
        exit 1
        ;;
esac
