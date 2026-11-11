# MSI : C:\Windows\Installer existe pour les scripts WiX, et la casse d'un préfixe existant est retrouvée (#1663, #1674).
inst = (ROOT / "runtime/src/msi/install.rs").read_text(encoding="utf-8")
paths = (ROOT / "runtime/src/paths.rs").read_text(encoding="utf-8")
k32 = (ROOT / "runtime/src/dlls/kernel32.rs").read_text(encoding="utf-8")
ok("fn ensure_installer_dir" in inst and inst.count("ensure_installer_dir();") >= 2,
   "MSI : C:\\Windows\\Installer créé à l'installation et à la désinstallation (#1663)")
ok("for rest in &comps[i + 1..]" in paths,
   "Chemins : préfixe existant retrouvé sans tenir compte de la casse, suite absente gardée (#1663)")
ok("(#1674)" in k32, "CreateFile : ERROR_PATH_NOT_FOUND quand le dossier parent manque (#1674)")
