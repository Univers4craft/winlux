# cgroup v2 monté au démarrage, sinon le pare-feu perd ses règles sortantes par application (#2142).
_init = (ROOT / "scripts/build-live-iso.sh").read_text(encoding="utf-8")
_fw = (ROOT / "runtime/src/firewall.rs").read_text(encoding="utf-8")
ok("mount -t cgroup2 cgroup2 /sys/fs/cgroup" in _init and "' /sys/fs/cgroup cgroup2 '" in _init, "/init monte cgroup2 une seule fois (#2142)")
ok(_init.index("mount -t cgroup2") > _init.index("mount -t sysfs sysfs /sys"), "cgroup2 monté après sysfs (#2142)")
ok("fn mount_cgroup2()" in _fw and "mount_cgroup2();" in _fw, "le pare-feu monte cgroup2 s'il manque (#2142)")
