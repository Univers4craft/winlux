#!/bin/sh
# Voix du Narrateur Winlux et sons système du bureau.
#   winlux-speak [--lang fr|en|es|de] [--rate 1-10] "texte"   lit le texte (interrompt la lecture en cours)
#   winlux-speak --stop                  fait taire la voix
#   winlux-speak --sound NOM             joue un son : notification, warning, toggle
# Moteurs essayés : piper (voix neuronale, modèle dans /usr/share/winlux/piper),
# puis espeak-ng, puis espeak. Aucun réseau n'est utilisé.
set -u
run=${WINLUX_SPEAK_RUN:-${XDG_RUNTIME_DIR:-/tmp}/winlux-speak-$(id -u)}
mkdir -p "$run" 2>/dev/null || exit 1
pidfile="$run/voice.pid"
have() { command -v "$1" >/dev/null 2>&1; }

stop_voice() {
    test -r "$pidfile" || return 0
    pid=$(cat "$pidfile" 2>/dev/null)
    rm -f "$pidfile"
    case "$pid" in ''|*[!0-9]*) return 0 ;; esac
    # La lecture tourne dans son propre groupe : synthèse et lecteur audio.
    kill -TERM -- "-$pid" 2>/dev/null || kill -TERM "$pid" 2>/dev/null || true
}

# Lance une commande détachée dans un nouveau groupe de processus.
detach() {
    if have setsid; then
        setsid "$@" </dev/null >/dev/null 2>&1 &
    else
        "$@" </dev/null >/dev/null 2>&1 &
    fi
    printf '%s\n' "$!" > "$pidfile"
}

language() {
    case "${voice_lang:-${WINLUX_SPEAK_LANG:-${LANG:-fr}}}" in
        en*) printf 'en' ;;
        es*) printf 'es' ;;
        de*) printf 'de' ;;
        *) printf 'fr' ;;
    esac
}

piper_model() {
    if test -n "${WINLUX_PIPER_MODEL:-}" && test -r "$WINLUX_PIPER_MODEL"; then
        printf '%s' "$WINLUX_PIPER_MODEL"; return 0
    fi
    for model in "${WINLUX_PIPER_DIR:-/usr/share/winlux/piper}"/"$(language)"*.onnx; do
        if test -r "$model" && test -r "$model.json"; then printf '%s' "$model"; return 0; fi
    done
    return 1
}

speak() {
    rate=$1 text=$2
    case "$rate" in ''|*[!0-9]*) rate=5 ;; esac
    test "$rate" -ge 1 || rate=1
    test "$rate" -le 10 || rate=10
    stop_voice
    test -n "$text" || return 0
    if have piper && model=$(piper_model) && have aplay; then
        # length_scale : plus grand = plus lent ; 5 est le débit normal.
        scale=$(awk -v r="$rate" 'BEGIN { printf "%.2f", 5 / r }')
        sample=$(sed -n 's/.*"sample_rate"[^0-9]*\([0-9][0-9]*\).*/\1/p' "$model.json" | head -n 1)
        detach sh -c 'printf "%s\n" "$1" | piper --model "$2" --length_scale "$3" --output_raw 2>/dev/null | aplay -q -r "$4" -f S16_LE -t raw -' \
            winlux-speak "$text" "$model" "$scale" "${sample:-22050}"
        return 0
    fi
    # espeak-ng : 110 à 290 mots par minute.
    words=$((90 + rate * 20))
    for engine in espeak-ng espeak; do
        if have "$engine"; then
            detach "$engine" -v "$(language)" -s "$words" -- "$text"
            return 0
        fi
    done
    echo "winlux-speak : aucun moteur de synthese vocale (espeak-ng ou piper)" >&2
    return 1
}

# Son court généré localement (WAV 8 bits, 8 kHz) : aucun fichier requis.
tone_wav() {
    LC_ALL=C awk -v kind="$1" 'function le(v, n,  i) { for (i = 0; i < n; i++) { printf "%c", v % 256; v = int(v / 256) } }
    BEGIN {
        rate = 8000
        if (kind == "warning") { f1 = 440; f2 = 330; len = 0.30 }
        else if (kind == "toggle") { f1 = 880; f2 = 1175; len = 0.16 }
        else { f1 = 660; f2 = 990; len = 0.24 }
        n = int(rate * len)
        printf "RIFF"; le(36 + n, 4); printf "WAVEfmt "; le(16, 4); le(1, 2); le(1, 2)
        le(rate, 4); le(rate, 4); le(1, 2); le(8, 2); printf "data"; le(n, 4)
        for (i = 0; i < n; i++) {
            f = (i < n / 2) ? f1 : f2
            env = 1 - i / n
            v = int(128 + 60 * env * sin(2 * 3.14159265 * f * i / rate))
            if (v < 1) v = 1
            printf "%c", v
        }
    }'
}

play_sound() {
    kind=$1
    case "$kind" in notification|warning|toggle) ;; *) echo "Son inconnu : $kind" >&2; return 1 ;; esac
    case "$kind" in
        notification) name=message ;;
        warning) name=dialog-warning ;;
        *) name=audio-volume-change ;;
    esac
    sound="/usr/share/sounds/freedesktop/stereo/$name.oga"
    for player in pw-play paplay; do
        if have "$player" && test -r "$sound"; then
            "$player" "$sound" </dev/null >/dev/null 2>&1 &
            return 0
        fi
    done
    have aplay || { echo "winlux-speak : aucun lecteur audio" >&2; return 1; }
    wav="$run/$kind.wav"
    test -s "$wav" || tone_wav "$kind" > "$wav" || return 1
    aplay -q "$wav" </dev/null >/dev/null 2>&1 &
}

rate=5
voice_lang=
while test $# -gt 0; do
    case "$1" in
        --stop) stop_voice; exit 0 ;;
        --sound) shift; play_sound "${1:-notification}"; exit $? ;;
        --rate) shift; rate=${1:-5}; shift ;;
        --lang) shift; voice_lang=${1:-}; shift ;;
        --) shift; break ;;
        *) break ;;
    esac
done
speak "$rate" "$*"
