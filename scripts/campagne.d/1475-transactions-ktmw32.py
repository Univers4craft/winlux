# CreateTransaction réussit, comme sous Windows : l'installeur de Steam x86
# met alors ses fichiers en place au lieu de boucler (#1475).
ok("fn transaction_sans_isolation" in (ROOT / "runtime/src/dlls/sysdlls.rs").read_text(encoding="utf-8")
   and "(#1475)" in (ROOT / "runtime/src/dlls/sysdlls.rs").read_text(encoding="utf-8"),
   "transactions ktmw32 sans isolation pour la mise à jour de Steam (#1475)")
