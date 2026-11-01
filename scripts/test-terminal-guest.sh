#!/bin/sh
# Run inside the ISO diagnostic, after Xvfb has started.
set -eu
result=/tmp/winlux-terminal-result
script=/tmp/winlux-terminal-command
rm -f "$result"
cat > "$script" <<'COMMAND'
#!/bin/sh
set -eu
test -t 0 && test -t 1 && test -t 2
test "$TERM" = xterm-256color
test -r /usr/share/terminfo/x/xterm-256color
test "$(pwd)" = '/tmp/Terminal avec espaces'
stty size
printf 'Terminal UTF-8 : éàç — Winlux\n'
/usr/bin/winlux /opt/winlux/tests/hello.exe
printf 'OK\n' > /tmp/winlux-terminal-result
COMMAND
mkdir -p '/tmp/Terminal avec espaces'
chmod 755 '/tmp/Terminal avec espaces'
chmod 644 "$script"
# A slow first xterm start (fonts, firmware timer under UEFI) must not be
# mistaken for a broken terminal: allow 60 s and say which run failed.
check() {
    label=$1
    shift
    status=0
    timeout 60 "$@" >> /tmp/winlux-terminal.log 2>&1 || status=$?
    if ! grep -qx OK "$result" 2>/dev/null; then
        echo "WINLUX_LIVE_TERMINAL_FAILED=$label (code $status)"
        tail -n 20 /tmp/winlux-terminal.log
        exit 1
    fi
    rm -f "$result"
}
: > /tmp/winlux-terminal.log
check root /usr/bin/winlux-terminal --cwd '/tmp/Terminal avec espaces' -e /bin/sh "$script"
check winlux-web /usr/bin/setpriv --reuid=winlux-web --regid=winlux-web --init-groups --reset-env \
    env HOME=/home/winlux-web DISPLAY="$DISPLAY" \
    /usr/bin/winlux-terminal --cwd '/tmp/Terminal avec espaces' -e /bin/sh "$script"
echo 'WINLUX_LIVE_TERMINAL_EXIT=0'
