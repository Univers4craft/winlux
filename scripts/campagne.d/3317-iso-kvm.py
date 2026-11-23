# #3317 : le test QEMU de l'ISO live prend KVM quand le runner l'offre
# (bios et uefi) ; uefi32 reste en TCG pour #3199.
_t_3317 = (SCRIPTS / "test-live-iso-qemu.sh").read_text(encoding="utf-8")
ok('-accel "$accel"' in _t_3317 and "test -w /dev/kvm" in _t_3317 and 'test "$mode" != uefi32' in _t_3317,
   "Test QEMU de l'ISO : KVM si disponible, uefi32 en TCG (#3317)")
_g_3317 = (SCRIPTS / "test-terminal-guest.sh").read_text(encoding="utf-8")
ok("WINLUX_LIVE_TERMINAL_TIME=$label" in _g_3317,
   "Test live : durée de chaque terminal sur le port série (#3317)")
