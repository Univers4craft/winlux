# #3195 : le chargeur suit les assemblies privées du manifeste de l'exe
# (Learn, « Assembly Searching Sequence ») ; Chromium installé trouve ainsi
# chrome_elf.dll dans Application\<version>\. Gardé par le test unitaire nommé.
t = (ROOT / "runtime/src/loader.rs").read_text(encoding="utf-8")
ok(
    "fn dll_d_une_assembly_privee_dans_un_sous_dossier_versionne_3195" in t,
    "une DLL d'assembly privée rangée dans un sous-dossier versionné se charge (#3195)",
)
