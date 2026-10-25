#!/bin/sh
set -eu
root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
task_dir=$(mktemp -d "${TMPDIR:-/tmp}/winlux-network-test.XXXXXX")
trap 'rm -rf "$task_dir"' EXIT HUP INT TERM
mkdir -p "$task_dir/bin" "$task_dir/etc" "$task_dir/sys/class/net/eth0" "$task_dir/sys/class/net/lo"
cat >"$task_dir/bin/ip" <<'MOCK'
#!/bin/sh
printf '%s\n' "$*" >>"$WINLUX_NETWORK_ROOT/ip.log"
MOCK
cat >"$task_dir/bin/dhcp" <<'MOCK'
#!/bin/sh
printf '%s\n' "$*" >"$WINLUX_NETWORK_ROOT/dhcp.log"
MOCK
chmod +x "$task_dir/bin/ip" "$task_dir/bin/dhcp"
export WINLUX_NETWORK_ROOT="$task_dir" WINLUX_IP="$task_dir/bin/ip" WINLUX_UDHCPC="$task_dir/bin/dhcp"
export WINLUX_DHCP_HOOK="$root/scripts/winlux-dhcp-hook.sh"
sh "$root/scripts/winlux-network.sh" start
for attempt in 1 2 3 4 5; do test -f "$task_dir/dhcp.log" && break; sleep 0.1; done
grep -q -- '-i eth0' "$task_dir/dhcp.log"
interface=eth0 ip=192.0.2.20 subnet=255.255.255.0 router=192.0.2.1 dns='192.0.2.53 999.1.2.3' \
    sh "$root/scripts/winlux-dhcp-hook.sh" bound
grep -q 'addr add 192.0.2.20/24 dev eth0' "$task_dir/ip.log"
grep -q 'route replace default via 192.0.2.1 dev eth0' "$task_dir/ip.log"
grep -q '^nameserver 192.0.2.53$' "$task_dir/etc/resolv.conf"
test "$(grep -c '^nameserver' "$task_dir/etc/resolv.conf")" -eq 1
if interface=eth0 ip='bad;command' sh "$root/scripts/winlux-dhcp-hook.sh" bound; then
    echo 'Malformed lease accepted' >&2; exit 1
fi
echo 'DHCP addressing, route, DNS and malformed lease validation passed.'
