# WebAuthenticationCoreManager servie sans fournisseur ni jeton (#2729).
c = (ROOT / "runtime" / "src" / "dlls" / "winrt_wam.rs").read_text(encoding="utf-8")
ok("fn gestionnaire_comptes_web_2729" in c, "WebAuthenticationCoreManager servie sans fournisseur ni jeton (#2729)")
