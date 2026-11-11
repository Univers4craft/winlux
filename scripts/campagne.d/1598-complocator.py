# MSI : AppSearch/CompLocator retrouve un composant posé par un autre paquet (python.exe pour pip.msi) (#1598).
ok("fn component_key_path" in (ROOT / "runtime/src/msi/install.rs").read_text(encoding="utf-8")
   and "fn component_search" in (ROOT / "runtime/src/msi/extra.rs").read_text(encoding="utf-8"),
   "MSI : CompLocator renseigne PYTHON_EXE, pip s'installe avec Python (#1598)")
