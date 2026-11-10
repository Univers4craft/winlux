# #1393 : wevtapi (différée, win32evtlog), activeds et aclui (par ordinal, adsi et authorization).
mod_rs = (runtime_dlls / "mod.rs").read_text(encoding="utf-8")
wev = (runtime_dlls / "wevtapi.rs").read_text(encoding="utf-8")
ok('name: "wevtapi"' in mod_rs, "wevtapi.dll intégrée (#1393)")
ok('"EvtSubscribe" => fn evt_subscribe' in wev and '"EvtNextChannelPath" => fn evt_next_channel_path' in wev,
   "wevtapi : fonctions importées par win32evtlog (#1393)")
ok("ERROR_EVT_CHANNEL_NOT_FOUND: u32 = 15007" in wev, "wevtapi : canal introuvable, sans prétendre avoir agi (#1393)")
acl = (runtime_dlls / "activeds_aclui.rs").read_text(encoding="utf-8")
ok('name: "activeds"' in mod_rs and 'name: "aclui"' in mod_rs, "activeds.dll et aclui.dll intégrées (#1393)")
ok('(3, "ADsGetObject")' in acl and '(2, "EditSecurity")' in acl,
   "activeds et aclui : ordinaux importés par adsi.pyd et authorization.pyd (#1393)")
