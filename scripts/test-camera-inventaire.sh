#!/bin/sh
# Inventaire des caméras au lancement (#3100) : testc/msvc/camlist_msvc.c,
# construit ici avec clang (x86_64-pc-windows-msvc), appelle
# MFEnumDeviceSources sans décision mémorisée pour la caméra. Lister n'est
# pas utiliser (#1733) : aucune fenêtre de demande ne doit s'ouvrir, et
# l'appel revient tout de suite. Le faux assistant winlux-permission note
# chaque question et met 10 s à répondre. Un refus mémorisé reste un refus :
# la liste est vide, sans question non plus. Sauté (code 0) sans runtime ni
# clang.
set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
runtime=${WINLUX_BIN:-$root/target/release/winlux}
out=${WINLUX_MINGW_DIR:-$root/target/mingw}
src="$root/testc/msvc/camlist_msvc.c"
exe="$out/camlist.exe"

test -x "$runtime" || { echo "runtime absent ($runtime) : test sauté"; exit 0; }
for d in clang lld-link llvm-dlltool; do
    command -v "$d" >/dev/null 2>&1 || { echo "$d absent : test sauté"; exit 0; }
done
if ! test -f "$exe" || test "$src" -nt "$exe" || test "$0" -nt "$exe"; then
    tmp="$out/camlist"
    mkdir -p "$tmp"
    printf 'LIBRARY kernel32.dll\nEXPORTS\nExitProcess\nGetTickCount\n' >"$tmp/kernel32.def"
    printf 'LIBRARY mfplat.dll\nEXPORTS\nMFCreateAttributes\n' >"$tmp/mfplat.def"
    printf 'LIBRARY mf.dll\nEXPORTS\nMFEnumDeviceSources\n' >"$tmp/mf.def"
    for d in kernel32 mfplat mf; do
        llvm-dlltool -m i386:x86-64 -d "$tmp/$d.def" -l "$tmp/$d.lib"
    done
    clang --target=x86_64-pc-windows-msvc -O1 -fno-builtin -fno-stack-protector -c "$src" -o "$tmp/camlist.obj"
    lld-link /nologo /entry:entry /subsystem:console /nodefaultlib "$tmp/camlist.obj" \
        "$tmp/kernel32.lib" "$tmp/mfplat.lib" "$tmp/mf.lib" /out:"$exe"
fi

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
mkdir -p "$work/home" "$work/video" "$work/prog"
# Programme posé hors des dossiers de Winlux : il n'est pas « de confiance ».
cp "$exe" "$work/prog/camlist.exe"
cat >"$work/winlux-permission" <<EOS
#!/bin/sh
echo "\$*" >>"$work/questions"
sleep 10
exit 0
EOS
chmod 755 "$work/winlux-permission"
run() {
    set +e
    debut=$(date +%s)
    env HOME="$work/home" WINLUX_HOME="$work/home" WINLUX_DRIVE_C="$work/home" \
        XDG_CONFIG_HOME="$work/home/.config" WINLUX_GUI=headless DISPLAY=:9999 \
        WINLUX_PERMISSIONS_FILE="$work/permissions.tsv" WINLUX_PERMISSION_HELPER="$work/winlux-permission" \
        WINLUX_PRIVACY_CONF="$work/privacy.conf" WINLUX_VIDEO_DIR="$work/video" \
        WINLUX_REGISTRY_PATH="$work/registre.tsv" TMPDIR="$work" \
        env -u WINLUX_PERMISSIONS timeout 60 "$runtime" "$work/prog/camlist.exe" >"$work/out.txt" 2>&1
    status=$?
    duree=$(( $(date +%s) - debut ))
    set -e
}
: >"$work/privacy.conf"
: >"$work/permissions.tsv"
run
questions=$(cat "$work/questions" 2>/dev/null || true)
echo "camlist sans décision : code $status, ${duree} s, questions : « $questions »"
test "$status" -eq 0 && test -z "$questions" || {
    tail -20 "$work/out.txt"
    echo "inventaire des caméras : question posée ou attente (code $status, ${duree} s) (#3100)" >&2
    exit 1
}
real=$(readlink -f "$work/prog/camlist.exe")
printf 'camera\tdeny\t%s\n' "$real" >"$work/permissions.tsv"
run
questions=$(cat "$work/questions" 2>/dev/null || true)
echo "camlist caméra refusée : code $status, ${duree} s, questions : « $questions »"
test "$status" -eq 0 && test -z "$questions" || {
    tail -20 "$work/out.txt"
    echo "inventaire des caméras refusées : code $status (#3100)" >&2
    exit 1
}
echo "camlist : MFEnumDeviceSources liste les caméras sans rien demander, refus respecté"
