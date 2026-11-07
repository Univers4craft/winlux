#!/bin/sh
# Appli WinForms .NET « Bonjour » autonome (#664, #665, #666) : la fenêtre
# s'ouvre en 320×200, le bouton « Fermer » reçoit le clic et l'appli se
# termine avec le code 0.
#
# Il faut le runtime .NET 9 pour Windows (Microsoft.NETCore.App et
# Microsoft.WindowsDesktop.App, win-x64). On reprend celui que Paint.NET
# portable embarque : WINLUX_PAINTNET_DIR=<dossier décompressé>, sinon le
# zip est téléchargé une fois dans ~/.cache/winlux/paintnet (SHA-256 vérifié).
# L'appli est compilée par mcs (Mono, compilateur de test uniquement), puis
# posée à la place de paintdotnet.dll dans une copie du dossier : le lanceur
# paintdotnet.exe démarre alors notre programme sur ce runtime.
#
# Usage : sh scripts/test-winforms-bonjour.sh [binaire winlux]
# Dépendances : mcs, xvfb-run, xdotool, unzip, curl (si téléchargement).
set -eu

WINLUX=${1:-target/release/winlux}
test -x "$WINLUX" || WINLUX=target/debug/winlux
test -x "$WINLUX" || { echo "winlux non compilé" >&2; exit 1; }
WINLUX=$(cd "$(dirname "$WINLUX")" && pwd)/$(basename "$WINLUX")
for tool in mcs xvfb-run xdotool unzip; do
    command -v "$tool" >/dev/null 2>&1 || { echo "SAUTÉ : $tool absent" >&2; exit 0; }
done

ZIP_URL=https://github.com/paintdotnet/release/releases/download/v5.1.12/paint.net.5.1.12.portable.x64.zip
ZIP_SHA=d5ae7043f2fb9d365b48dfe243a2aca1c74924de99b04b6445916c95354aefa3
runtime=${WINLUX_PAINTNET_DIR:-}
if test -z "$runtime"; then
    cache=${XDG_CACHE_HOME:-$HOME/.cache}/winlux/paintnet
    runtime=$cache/5.1.12
    if ! test -f "$runtime/hostfxr.dll"; then
        mkdir -p "$cache"
        test -f "$cache/pdn.zip" || curl -fsSL -o "$cache/pdn.zip" "$ZIP_URL"
        echo "$ZIP_SHA  $cache/pdn.zip" | sha256sum -c - >/dev/null
        rm -rf "$runtime" && mkdir -p "$runtime"
        unzip -q "$cache/pdn.zip" -d "$runtime" 2>/dev/null || true
    fi
fi
test -f "$runtime/hostfxr.dll" -a -f "$runtime/System.Windows.Forms.dll" ||
    { echo "runtime .NET introuvable dans $runtime" >&2; exit 1; }

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
cp -r "$runtime" "$work/app"
note=$work/notes.txt

cat >"$work/bonjour.cs" <<EOF
using System;
using System.IO;
using System.Windows.Forms;
public class Bonjour : Form {
  // Écriture octet par octet : mcs ne sait pas choisir entre les surcharges
  // ReadOnlySpan de .NET 9.
  static void Note(string s) {
    var b = new byte[s.Length + 1];
    for (int i = 0; i < s.Length; i++) b[i] = (byte)(s[i] < 128 ? s[i] : '?');
    b[s.Length] = 10;
    var w = new FileStream("$note", FileMode.Append); w.Write(b, 0, b.Length); w.Close();
  }
  public Bonjour() {
    Text = "Bonjour"; Width = 320; Height = 200;
    var b = new Button(); b.Text = "Fermer"; b.Left = 100; b.Top = 60;
    b.Click += Fermer; Controls.Add(b); Load += Charge;
  }
  void Charge(object s, EventArgs e) { Note("charge"); Note(Width.ToString()); Note(Height.ToString()); }
  void Fermer(object s, EventArgs e) { Note("clic"); Close(); }
  [STAThread] public static void Main() {
    try { Application.EnableVisualStyles(); Application.Run(new Bonjour()); Note("fin"); }
    catch (Exception e) { Note(e.ToString()); }
  }
}
EOF
refs=
for r in System.Private.CoreLib System.Runtime System.Windows.Forms System.Windows.Forms.Primitives \
    System.Private.Windows.Core System.ComponentModel.Primitives System.ComponentModel.TypeConverter \
    System.Drawing.Primitives System.Drawing.Common System.ComponentModel; do
    refs="$refs -r:$work/app/$r.dll"
done
# shellcheck disable=SC2086
mcs -nostdlib -noconfig $refs -target:winexe -out:"$work/app/paintdotnet.dll" "$work/bonjour.cs" >/dev/null

cat >"$work/run.sh" <<'EOF'
#!/bin/sh
# $1 winlux, $2 dossier de l'appli, $3 fichier d'état
cd "$2"
"$1" paintdotnet.exe >"$3.log" 2>&1 &
pid=$!
i=0
while test $i -lt 90 && ! grep -q charge "$3.notes" 2>/dev/null; do
    sleep 1; i=$((i + 1))
    kill -0 $pid 2>/dev/null || break
done
sleep 2
# Fenêtre posée en (100, 100) sans gestionnaire de fenêtres ; bouton en
# (100, 60) de la zone client.
xdotool mousemove 237 171 click 1
i=0
while test $i -lt 20 && kill -0 $pid 2>/dev/null; do sleep 1; i=$((i + 1)); done
if kill -0 $pid 2>/dev/null; then kill $pid; echo "toujours ouvert" >>"$3"; fi
wait $pid; echo "code $?" >>"$3"
EOF
ln -s "$note" "$work/state.notes"
xvfb-run -a -s "-screen 0 800x600x24" sh "$work/run.sh" "$WINLUX" "$work/app" "$work/state" || true

expected=$(printf 'charge\n320\n200\nclic\nfin')
got=$(cat "$note" 2>/dev/null || true)
if test "$got" = "$expected" && grep -qx "code 0" "$work/state"; then
    echo "OK : WinForms « Bonjour » ouvert en 320×200, clic reçu, fin en code 0"
    exit 0
fi
echo "ÉCHEC : WinForms « Bonjour »" >&2
echo "--- notes de l'appli" >&2; printf '%s\n' "$got" >&2
echo "--- état" >&2; cat "$work/state" >&2 2>/dev/null || true
echo "--- fin du journal winlux" >&2; tail -20 "$work/state.log" >&2 2>/dev/null || true
exit 1
