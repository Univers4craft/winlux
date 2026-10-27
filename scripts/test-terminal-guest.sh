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
timeout 20 /usr/bin/winlux-terminal --cwd '/tmp/Terminal avec espaces' -e /bin/sh "$script" > /tmp/winlux-terminal.log 2>&1
grep -qx OK "$result"
echo 'WINLUX_LIVE_TERMINAL_EXIT=0'
