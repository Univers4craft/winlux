#!/bin/sh
# Vérifie la voix du Narrateur et les sons système sans carte son :
# les moteurs sont remplacés par des enregistreurs de commande.
set -eu
project=$(CDPATH= cd -- "$(dirname "$0")/.." && pwd)
work=$(mktemp -d "${TMPDIR:-/tmp}/winlux-speak-test.XXXXXX")
trap 'rm -rf "$work"' EXIT HUP INT TERM
mkdir -p "$work/bin" "$work/run"
export WINLUX_SPEAK_RUN="$work/run" WINLUX_PIPER_DIR="$work/piper" LANG=fr_FR.UTF-8
export LOG="$work/log"
cat > "$work/bin/espeak-ng" <<'MOCK'
#!/bin/sh
printf 'espeak-ng %s\n' "$*" >> "$LOG"
exec sleep 30
MOCK
cat > "$work/bin/aplay" <<'MOCK'
#!/bin/sh
printf 'aplay %s\n' "$*" >> "$LOG"
MOCK
chmod 755 "$work/bin/"*
speak() { PATH="$work/bin:/usr/bin:/bin" sh "$project/scripts/winlux-speak.sh" "$@"; }
wait_for() { i=0; while ! grep -q "$1" "$LOG" 2>/dev/null; do i=$((i + 1)); test "$i" -lt 50 || { echo "Attendu : $1" >&2; cat "$LOG" >&2 || true; exit 1; }; sleep 0.1; done; }

speak --rate 7 "Paramètres, fenêtre"
wait_for 'espeak-ng -v fr -s 230 -- Paramètres, fenêtre'
pid=$(cat "$work/run/voice.pid")
kill -0 "$pid"
# Une nouvelle phrase interrompt la précédente.
speak "Bonjour"
wait_for 'espeak-ng -v fr -s 190 -- Bonjour'
i=0; while kill -0 "$pid" 2>/dev/null; do i=$((i + 1)); test "$i" -lt 30 || { echo "Voix precedente non interrompue" >&2; exit 1; }; sleep 0.1; done
pid=$(cat "$work/run/voice.pid")
speak --stop
test ! -e "$work/run/voice.pid"
i=0; while kill -0 "$pid" 2>/dev/null; do i=$((i + 1)); test "$i" -lt 30 || { echo "--stop ne coupe pas la voix" >&2; exit 1; }; sleep 0.1; done
# Débit borné.
speak --rate 99 "Vite"
wait_for 'espeak-ng -v fr -s 290 -- Vite'
speak --lang de --rate 5 "Hallo"
wait_for 'espeak-ng -v de -s 190 -- Hallo'
speak --stop
# Son système généré localement : WAV valide.
speak --sound notification
wait_for 'aplay -q'
head -c 4 "$work/run/notification.wav" | grep -q RIFF
size=$(wc -c < "$work/run/notification.wav")
test "$size" -eq $((44 + 1920)) || { echo "Taille WAV inattendue : $size" >&2; exit 1; }
if speak --sound inconnu 2>/dev/null; then echo "Son inconnu accepte" >&2; exit 1; fi
echo "Narrateur et sons systeme valides."
