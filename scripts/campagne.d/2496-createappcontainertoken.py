# #2496 : CreateAppContainerToken servi par KernelBase (jamais par kernel32).
# Le comportement est gardé par le test unitaire nommé.
st = (ROOT / "runtime" / "src" / "dlls" / "security" / "tests.rs").read_text(encoding="utf-8")
ok("fn jeton_de_conteneur_par_kernelbase_seulement" in st, "test de CreateAppContainerToken par KernelBase (#2496)")
