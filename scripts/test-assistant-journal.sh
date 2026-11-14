#!/bin/sh
# Assistant d'installation (#2328) : la sortie du runtime part dans le
# journal au fil de l'eau. Un installateur qui écrit puis travaille longtemps
# laisse sa trace dans la sortie de l'assistant (last-launch.log pour
# winlux-open) avant la fin, et elle survit si l'assistant est tué.
set -eu
root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
wizard=$root/scripts/winlux-setup-wizard.py
py=
for c in python3 python3.12; do
    if command -v "$c" >/dev/null && "$c" -c 'import gi; gi.require_version("Gtk", "3.0"); from gi.repository import Gtk' 2>/dev/null; then py=$c; break; fi
done
test -n "$py" || { echo "python3-gi absent : test sauté"; exit 0; }
command -v xvfb-run >/dev/null || { echo "xvfb-run absent : test sauté"; exit 0; }
work=$(mktemp -d)
pid=
trap '[ -z "$pid" ] || kill -KILL "-$pid" 2>/dev/null; rm -rf "$work"' EXIT
fail() { echo "$1" >&2; cat "$work/journal.txt" >&2 2>/dev/null || true; exit 1; }
cd "$work"
: >long.exe
mkdir home
cat >runtime.sh <<'SH'
#!/bin/sh
case "$1 $2" in
    "--setup --info") printf 'nsis\tLong\t1.0\tDémo\tC:\\Program Files\\Long\n' ;;
    "--setup --license") exit 1 ;;
    *) echo "setup : décompression de l'archive en cours"; exec sleep 120 ;;
esac
SH
chmod +x runtime.sh
HOME="$work/home" WINLUX_LANG=fr WINLUX_SETUP_WIZARD_AUTO=1 WINLUX_BIN="$work/runtime.sh" \
    setsid xvfb-run -a "$py" "$wizard" long.exe >journal.txt 2>&1 &
pid=$!
i=0
until grep -q "décompression de l'archive en cours" journal.txt 2>/dev/null; do
    i=$((i + 1))
    [ "$i" -le 60 ] || fail "journal : sortie du runtime absente pendant l'installation (#2328)"
    sleep 1
done
# L'assistant est tué en cours de route : la ligne reste dans le journal.
kill -KILL "-$pid"
wait "$pid" 2>/dev/null || true
pid=
grep -q "décompression de l'archive en cours" journal.txt || fail "journal : trace perdue après l'arrêt de l'assistant (#2328)"
echo "assistant : sortie du runtime écrite au fil de l'eau dans le journal (#2328)"
