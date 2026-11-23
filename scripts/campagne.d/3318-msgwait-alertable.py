# MsgWaitForMultipleObjectsEx ignorait MWMO_ALERTABLE et MWMO_WAITALL : un fil
# qui reçoit son travail par APC pendant cette attente ne le voyait jamais
# (Discord, fil « aecdump », #3318). Le test unitaire nommé ici vérifie la
# sémantique de Learn : APC exécutée et WAIT_IO_COMPLETION, attente de tous.
_u32 = (ROOT / "runtime" / "src" / "dlls" / "user32.rs").read_text(encoding="utf-8")
ok("fn msg_wait_ex_alertable_et_attendre_tout_3281" in _u32,
   "MsgWaitForMultipleObjectsEx suit MWMO_ALERTABLE et MWMO_WAITALL (#3318)")
