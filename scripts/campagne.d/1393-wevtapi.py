# #1393 : wevtapi (importée en différé par win32evtlog) répond sans canal ni fournisseur.
mod_rs = (runtime_dlls / "mod.rs").read_text(encoding="utf-8")
wev = (runtime_dlls / "wevtapi.rs").read_text(encoding="utf-8")
ok('name: "wevtapi"' in mod_rs, "wevtapi.dll intégrée (#1393)")
ok('"EvtSubscribe" => fn evt_subscribe' in wev and '"EvtNextChannelPath" => fn evt_next_channel_path' in wev,
   "wevtapi : fonctions importées par win32evtlog (#1393)")
ok("ERROR_EVT_CHANNEL_NOT_FOUND: u32 = 15007" in wev, "wevtapi : canal introuvable, sans prétendre avoir agi (#1393)")
