# #1933 : GetSystemMetrics(SM_CONVERTIBLESLATEMODE) rend 0 seulement en mode
# tablette (Learn : « 0 for Slate Mode and non-zero otherwise »), d'après
# l'interrupteur SW_TABLET_MODE du noyau ; un PC sans interrupteur rend 1.
_u_1933 = (ROOT / "runtime" / "src" / "dlls" / "user32.rs").read_text(encoding="utf-8")
_e_1933 = (ROOT / "runtime" / "src" / "dlls" / "evdev.rs").read_text(encoding="utf-8")
print("0x2003 servi :", "0x2003 => crate::dlls::evdev::slate_mode_metric()" in _u_1933,
      "| test :", "fn interrupteur_mode_tablette_lu_dans_le_masque_sysfs_1933" in _e_1933)
ok("0x2003 => crate::dlls::evdev::slate_mode_metric()" in _u_1933
   and "tablet_mode() != Some(true)" in _e_1933
   and "fn interrupteur_mode_tablette_lu_dans_le_masque_sysfs_1933" in _e_1933,
   "GetSystemMetrics(SM_CONVERTIBLESLATEMODE) : non nul hors mode tablette (#1933)")
