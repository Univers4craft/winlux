#!/bin/sh
# Case « Créer un raccourci sur le Bureau » de l'assistant (#2295) : elle
# décide du raccourci du Bureau pour un installateur NSIS, dans les deux
# sens. --desktop : posé même si le script ne le crée pas (copie du
# raccourci du menu Démarrer) ; /nodesktop : jamais, même si le script le
# crée. Sans l'une ni l'autre, le script décide seul.
set -eu
root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
runtime=${WINLUX_BIN:-$root/target/release/winlux}
test -x "$runtime" || { echo "runtime absent ($runtime) : test sauté"; exit 0; }
command -v makensis >/dev/null || { echo "makensis absent (paquet nsis) : test sauté"; exit 0; }
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
fail() { echo "ÉCHEC : $1" >&2; cat "$work/out.txt" >&2 2>/dev/null || true; exit 1; }
cd "$work"
echo bonjour >app.exe
for variante in sans avec; do
    {
        echo 'Unicode true'
        echo "Name \"Bureau $variante\""
        echo "OutFile \"$variante.exe\""
        echo "InstallDir \"\$PROGRAMFILES64\\Bureau $variante\""
        echo 'RequestExecutionLevel user'
        echo 'Section'
        echo '  SetOutPath $INSTDIR'
        echo '  File app.exe'
        echo '  CreateDirectory "$SMPROGRAMS\Bureau Demo"'
        echo '  CreateShortCut "$SMPROGRAMS\Bureau Demo\Bureau Demo.lnk" "$INSTDIR\app.exe"'
        echo '  CreateShortCut "$SMPROGRAMS\Bureau Demo\Désinstaller.lnk" "$INSTDIR\uninst.exe"'
        [ "$variante" = avec ] && echo '  CreateShortCut "$DESKTOP\Bureau Demo.lnk" "$INSTDIR\app.exe"'
        echo '  WriteUninstaller "$INSTDIR\uninst.exe"'
        echo 'SectionEnd'
        echo 'Section "Uninstall"'
        echo 'SectionEnd'
    } >"$variante.nsi"
    makensis -V1 "$variante.nsi" >/dev/null
done
run() {
    rm -rf "$work/c"
    mkdir -p "$work/c"
    HOME="$work/c" WINLUX_HOME="$work/c" WINLUX_DRIVE_C="$work/c" XDG_CONFIG_HOME="$work/c/.config" \
        timeout 120 "$runtime" --setup "$work/$1" "$2" >"$work/out.txt" 2>&1 || fail "$1 $2 : installation en échec"
}
bureau() { find "$work/c/Desktop" -name '*.lnk' 2>/dev/null | wc -l; }
run sans.exe --desktop
test "$(bureau)" = 1 || fail "--desktop : $(bureau) raccourci(s) sur le Bureau au lieu de 1"
test -f "$work/c/Desktop/Bureau Demo.lnk" || fail "--desktop : raccourci du programme absent (pas celui du désinstalleur)"
run avec.exe /nodesktop
test "$(bureau)" = 0 || fail "/nodesktop : raccourci posé malgré la case décochée"
test -n "$(find "$work/c" -path '*Start Menu*' -name 'Bureau Demo.lnk')" || fail "/nodesktop : menu Démarrer touché"
run avec.exe --desktop
test "$(bureau)" = 1 || fail "--desktop : raccourci du script doublé ($(bureau))"
run sans.exe /S
test "$(bureau)" = 0 || fail "sans choix : raccourci ajouté alors que le script n'en pose pas"
echo "raccourci du Bureau : la case de l'assistant est respectée pour NSIS, dans les deux sens (#2295)"
