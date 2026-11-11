# #1837 : LoadRegTypeLib fournit la bibliothèque « Accessibility » 1.1
# (IAccessible), dont WinForms tire l'IDispatch de ses objets accessibles
# (Paint.NET plantait à la première infobulle clavier).
# #1838 : FormatMessage donne un texte aux HRESULT de COM.
typelib_src = (runtime_dlls / "oleaut32_typelib.rs").read_text(encoding="utf-8")
ok("pub const LIBID_ACCESSIBILITY" in typelib_src and "fn accessibility()" in typelib_src
   and "fn accessibilite_iaccessible_par_loadregtypelib" in typelib_src,
   "oleaut32 : bibliothèque de types Accessibility et IAccessible (#1837)")
ok("0x8002_801D => \"Bibliothèque de types non inscrite.\"" in (runtime_dlls / "kernel32_misc.rs").read_text(encoding="utf-8"),
   "kernel32 : FormatMessage donne un texte aux HRESULT de COM (#1838)")
