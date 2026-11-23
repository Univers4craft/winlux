# msftedit sert CreateTextServices et ITextServices pour de vrai (Learn,
# « About Windowless Rich Edit Controls ») : Acrobat la demande au démarrage
# (#3268).
ts_3268 = (ROOT / "runtime/src/dlls/textserv.rs").read_text(encoding="utf-8")
mod_3268 = (ROOT / "runtime/src/dlls/mod.rs").read_text(encoding="utf-8")
ok('"CreateTextServices" => fn create_text_services' in ts_3268
   and '"IID_ITextServices" => data IID_ITEXT_SERVICES' in ts_3268
   and "exports: textserv::exports()" in mod_3268
   and "fn text_services_sert_le_texte_3268" in ts_3268,
   "msftedit : CreateTextServices et ITextServices servies et testées (#3268)")
