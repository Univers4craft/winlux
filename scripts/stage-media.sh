#!/bin/sh
# Lecteur multimédia et moteur de décodage dans une racine d'ISO Winlux.
#
# - copie mediaplayer.exe/audioplayer.exe, le lanceur winlux-mediaplayer et
#   l'association MIME (le lecteur devient l'application par défaut) ;
# - copie ffmpeg/ffprobe et toutes leurs bibliothèques dans /usr/lib/winlux/media
#   (le runtime les cherche là en premier), ainsi que les pilotes VA-API
#   présents sur la machine de construction (chargés par dlopen, absents de ldd).
#
# Sans ffmpeg sur la machine de construction, le lecteur est quand même
# installé : il affiche « moteur multimédia absent » au lieu de planter.
set -eu
stage=${1:?répertoire racine de destination requis}
root=${2:?racine du dépôt requise}
case "$stage" in /*) ;; *) echo "destination absolue requise" >&2; exit 1 ;; esac
test "$stage" != / && test -d "$stage" || { echo "destination invalide" >&2; exit 1; }

release="$root/target/x86_64-pc-windows-msvc/release"
mkdir -p "$stage/opt/winlux/apps" "$stage/usr/bin" "$stage/usr/share/applications"
for name in mediaplayer audioplayer; do
    cp "$release/$name.exe" "$stage/opt/winlux/apps/$name.exe"
done
cp "$root/integration/winlux-mediaplayer" "$stage/usr/bin/winlux-mediaplayer"
chmod 755 "$stage/usr/bin/winlux-mediaplayer"
cp "$root/integration/winlux-mediaplayer.desktop" "$stage/usr/share/applications/winlux-mediaplayer.desktop"

# Application par défaut pour l'audio et la vidéo (xdg-open, ShellExecute).
mimeapps="$stage/etc/xdg/mimeapps.list"
mkdir -p "$(dirname -- "$mimeapps")"
test -f "$mimeapps" || printf '[Default Applications]\n' >"$mimeapps"
grep -q '^\[Default Applications\]' "$mimeapps" || printf '[Default Applications]\n' >>"$mimeapps"
while IFS= read -r type; do
    test -n "$type" || continue
    grep -q "^$type=" "$mimeapps" && continue
    sed -i "/^\[Default Applications\]/a $type=winlux-mediaplayer.desktop;" "$mimeapps"
done <"$root/integration/mime-media-types.txt"

copy_with_deps() {
    file=$1
    dependencies=$(ldd "$file" 2>/dev/null || true)
    case "$dependencies" in *'not found'*)
        echo "moteur multimédia : dépendance absente pour $file" >&2
        return 1 ;;
    esac
    {
        printf '%s\n' "$file"
        printf '%s\n' "$dependencies" | awk '$2 == "=>" && $3 ~ /^\// { print $3 } $1 ~ /^\// { print $1 }'
    } | sort -u | while IFS= read -r path; do
        test -e "$stage$path" && continue
        mkdir -p "$stage$(dirname -- "$path")"
        cp -L "$path" "$stage$path"
    done
}

# WINLUX_MEDIA_ENGINE=0 : ISO plus légère, sans moteur (≈ 240 Mo non compressés).
if test "${WINLUX_MEDIA_ENGINE:-1}" = 0; then
    echo "moteur multimédia non embarqué (WINLUX_MEDIA_ENGINE=0)" >&2
    exit 0
fi
ffmpeg=$(command -v ffmpeg 2>/dev/null || true)
ffprobe=$(command -v ffprobe 2>/dev/null || true)
if test -z "$ffmpeg" || test -z "$ffprobe"; then
    echo "attention : ffmpeg/ffprobe absents, l'ISO n'aura pas de décodage audio/vidéo" >&2
    exit 0
fi
mkdir -p "$stage/usr/lib/winlux/media"
for tool in "$ffmpeg" "$ffprobe"; do
    copy_with_deps "$tool"
    cp -L "$tool" "$stage/usr/lib/winlux/media/$(basename -- "$tool")"
    chmod 755 "$stage/usr/lib/winlux/media/$(basename -- "$tool")"
done
# Pilotes de décodage matériel chargés à la demande (Intel, AMD, NVIDIA/nouveau).
for driver in /usr/lib/x86_64-linux-gnu/dri/*_drv_video.so; do
    test -f "$driver" || continue
    copy_with_deps "$driver" || true
done
# Notices des paquets distribués (FFmpeg est LGPL/GPL selon la compilation).
for notice in /usr/share/doc/ffmpeg/copyright /usr/share/doc/libavcodec*/copyright \
    /usr/share/doc/libavformat*/copyright /usr/share/doc/libavutil*/copyright \
    /usr/share/doc/libva*/copyright /usr/share/doc/libass*/copyright; do
    test -f "$notice" || continue
    mkdir -p "$stage$(dirname -- "$notice")"
    cp -L "$notice" "$stage$notice"
done
