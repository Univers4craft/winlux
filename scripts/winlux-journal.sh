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
            crashes=$(grep -c '^\[CRASH\]' "$CRASH_FILE" 2>/dev/null || echo 0)
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
    *)
        printf 'Usage: winlux-journal {status|list|errors|crashes|clear|export|rotate}\n' >&2
        exit 1
        ;;
esac
