# #3212 : avec MUI_MERGE_SYSTEM_FALLBACK, Get*PreferredUILanguages rend aussi
# la langue neutre (« fr » après fr-FR) ; gardé par le test unitaire nommé.
t = (ROOT / "runtime/src/intl/api.rs").read_text(encoding="utf-8")
ok("fn langue_neutre_dans_la_liste_avec_repli_3212" in t, "langue neutre dans la liste des langues d'interface avec repli (#3212)")
