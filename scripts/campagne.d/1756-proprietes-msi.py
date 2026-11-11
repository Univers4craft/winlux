# Propriétés standard de Windows Installer 5 et valeurs IE 11 du registre (#1756).
_inst = (ROOT / "runtime/src/msi/install.rs").read_text(encoding="utf-8")
for _p in ("VersionMsi", "MsiNTProductType", "MsiAMD64", "PhysicalMemory"):
    ok(f'("{_p}",' in _inst, f"propriété MSI {_p} définie (#1756)")
_reg = (ROOT / "runtime/src/registry/mod.rs").read_text(encoding="utf-8")
ok("svcVersion" in _reg and "add_later_defaults" in _reg,
   "valeurs Internet Explorer ajoutées aussi aux registres existants (#1756)")
