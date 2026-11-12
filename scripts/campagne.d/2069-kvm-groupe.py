# /init : /dev/kvm et /dev/net/tun au groupe kvm en 0660, jamais au monde ; le compte du bureau y entre une seule fois (#2069).
_b = (ROOT / "scripts/build-live-iso.sh").read_text(encoding="utf-8")
_k = _b[_b.index("# Virtualisation (#2069)"):]
_k = _k[:_k.index("\nfi\n") + 4]
ok("for node in /dev/kvm /dev/net/tun; do test -c \"$node\" && /bin/busybox chgrp kvm \"$node\" && /bin/busybox chmod 660 \"$node\"" in _k,
   "/dev/kvm et /dev/net/tun : groupe kvm, 0660 (#2069)")
ok("666" not in _k and "o+" not in _k, "/dev/kvm et /dev/net/tun jamais ouverts au monde (#2069)")
ok("/etc/winlux/kvm-compte-ajoute" in _k and 'test -n "$desktop_user"' in _k,
   "groupe kvm : seul le compte du bureau, une seule fois (#2069)")
