# winlux-shell-app lance le runtime de WINLUX_RUNTIME quand il est posé,
# comme winlux-open, pour qu'une passe « par chemin » essaie vraiment le
# binaire voulu (#2987).
_sa = (SCRIPTS / "winlux-shell-app.sh").read_text(encoding="utf-8")
ok("runtime=${WINLUX_RUNTIME:-/usr/bin/winlux}" in _sa
   and "exec /usr/bin/winlux /opt" not in _sa
   and _sa.count('exec "$runtime" /opt/winlux/tests/gui_demo.exe') == 3,
   "winlux-shell-app respecte WINLUX_RUNTIME pour l'Explorateur, la Boutique et le Bloc-notes (#2987)")
