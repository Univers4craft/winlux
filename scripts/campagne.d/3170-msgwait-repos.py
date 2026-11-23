# MsgWaitForMultipleObjects dort dans poll(2) jusqu'au signal d'un objet ou à l'arrivée
# d'un message, au lieu de se réveiller toutes les 10 ms (#3170).
# Le comportement est vérifié sous Xvfb par scripts/test-msgwait-repos.sh (réveils comptés).
_k = (ROOT / "runtime/src/dlls/kernel32.rs").read_text(encoding="utf-8")
ok("pub(crate) struct ObjectWait" in _k
   and (ROOT / "scripts/test-msgwait-repos.sh").is_file()
   and (ROOT / "testc/attente/msgwait_repos.c").is_file(),
   "MsgWaitForMultipleObjects : attente sans réveil périodique, testée sous Xvfb (#3170)")
