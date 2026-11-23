# Un programme lancé par CreateProcess depuis un service n'hérite plus de
# WINLUX_SERVICE_NAME : ses questions d'autorisation ne sont plus refusées et
# mémorisées comme celles d'un service, et service_process ne le prend plus
# pour le service (#3385). Le test unitaire nommé ici relance le binaire de
# test comme un service et vérifie l'enfant.
_k32 = (ROOT / "runtime" / "src" / "dlls" / "kernel32.rs").read_text(encoding="utf-8")
ok("fn enfant_d_un_service_sans_nom_de_service" in _k32
   and "fn service_qui_lance_un_enfant" in _k32,
   "enfant d'un service sans nom de service, vérifié par un service simulé (#3385)")
