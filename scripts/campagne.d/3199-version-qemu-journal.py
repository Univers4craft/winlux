# #3199 : le test QEMU de l'ISO écrit la version de QEMU et le -cpu utilisés,
# pour comparer la CI (QEMU 10.2) et le conteneur (QEMU 8.2).
_t_3199 = (SCRIPTS / "test-live-iso-qemu.sh").read_text(encoding="utf-8")
_l_3199 = [l for l in _t_3199.splitlines() if l.startswith('echo "QEMU : ')]
print(f"      ligne de journal : {_l_3199}")
ok(len(_l_3199) == 1 and "qemu-system-x86_64 --version" in _l_3199[0] and "${WINLUX_QEMU_CPU:-max}" in _l_3199[0],
   "Le test QEMU de l'ISO journalise la version de QEMU et le -cpu (#3199)")
