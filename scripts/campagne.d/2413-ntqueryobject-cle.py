# #2413 : NtQueryObject reconnaît une clé du registre ouverte (type « Key »,
# nom \REGISTRY\…). Le comportement est gardé par le test unitaire nommé.
ew = (ROOT / "runtime" / "src" / "dlls" / "electron_win32.rs").read_text(encoding="utf-8")
rg = (ROOT / "runtime" / "src" / "registry" / "mod.rs").read_text(encoding="utf-8")
ok("fn nom_nt_d_une_cle_du_registre" in ew, "test du nom NT d'une clé du registre (#2413)")
ok("pub fn key_nt_name" in rg, "le registre donne le nom NT d'une clé ouverte (#2413)")
