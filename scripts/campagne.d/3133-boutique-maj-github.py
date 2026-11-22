# Le Store se met à jour depuis GitHub sans mot de passe (#3133). Le paquet
# est signé avec une clé propre au Store, jamais celle de l'OS. Le lanceur
# ignore la copie du compte en root, et la publication porte exactement les
# fichiers que le lanceur accepte. Cas détaillés : scripts/test-boutique-maj.py.
def _module_3133(chemin, nom):
    import importlib.machinery, importlib.util
    chargeur = importlib.machinery.SourceFileLoader(nom, str(chemin))
    spec = importlib.util.spec_from_loader(nom, chargeur)
    module = importlib.util.module_from_spec(spec)
    chargeur.exec_module(module)
    return module
_lanceur_3133 = _module_3133(SCRIPTS / "winlux-store-lanceur.py", "lanceur_3133")
_paquet_3133 = _module_3133(SCRIPTS / "build-store-bundle.py", "paquet_3133")
_noms_3133 = tuple(nom for nom, _source in _paquet_3133.FICHIERS)
print(f"      publiés : {_noms_3133} ; acceptés : {_lanceur_3133.NOMS}")
ok(set(_noms_3133) == set(_lanceur_3133.NOMS) and _paquet_3133.LANCEUR == _lanceur_3133.LANCEUR,
   "Store publié : les fichiers et la version du lanceur correspondent (#3133)")
_vrai_3133 = _lanceur_3133.os.geteuid
_lanceur_3133.os.geteuid = lambda: 0
_root_3133 = _lanceur_3133.copie_du_compte()
_lanceur_3133.os.geteuid = _vrai_3133
ok(_root_3133 is None, "Store : en root, la copie du compte n'est jamais exécutée (#3133)")
_site_3133 = (ROOT / ".github/workflows/site.yml").read_text(encoding="utf-8")
_job_3133 = _site_3133.split("  boutique:", 1)[1].split("\n  tester:", 1)[0] if "  boutique:" in _site_3133 else ""
print(f"      job de signature : {len(_job_3133)} caractères")
ok("WINLUX_STORE_SIGNING_KEY" in _job_3133 and "environment: boutique" in _job_3133
   and "WINLUX_UPDATE_SIGNING_KEY" not in _site_3133,
   "Store publié : signé par sa propre clé, dans l'environnement boutique, jamais par celle de l'OS (#3133)")
