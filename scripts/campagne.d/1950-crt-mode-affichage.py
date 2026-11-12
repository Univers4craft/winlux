# msvcr120 exporte __crtGetShowWindowMode (gldriverquery64.exe de Steam, #1950).
_c = (ROOT / "runtime/src/dlls/crt_releve.rs").read_text(encoding="utf-8")
ok('"__crtGetShowWindowMode" => fn crt_get_show_window_mode' in _c
   and "fn mode_d_affichage_par_defaut" in _c,
   "__crtGetShowWindowMode rend SW_SHOWDEFAULT sans STARTF_USESHOWWINDOW (#1950)")
