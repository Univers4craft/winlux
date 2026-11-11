# Discord et Electron : CoCreateInstance(CLSID_ShellLink) sert IShellLinkW, IPersistFile et IPropertyStore.
ok("shell_link::create_builtin" in (runtime_dlls / "ole32.rs").read_text(encoding="utf-8")
   and "fn f_save" in (runtime_dlls / "shell_link.rs").read_text(encoding="utf-8")
   and "raccourci_ecrit_puis_relu" in (SCRIPTS / "test-all.d" / "1732-shelllink.sh").read_text(encoding="utf-8"),
   "Raccourcis .lnk par COM (ShellLink) : Discord se lance (#1732)")
