# Contrats API-set rattachés un par un après vérification du .def mingw-w64
# et des pages sdk-api (Refs #2902) : la liste est figée, tout ajout ou
# retrait doit être délibéré.
import re as _re
_m = (runtime_dlls / "mod.rs").read_text(encoding="utf-8")
_bloc = _m.split("pub const CONTRATS_VERIFIES", 1)[1].split("];", 1)[0]
ok(set(_re.findall(r'\("(api-ms-win-[a-z0-9-]+)", "([a-z0-9]+)"\)', _bloc)) == {
    ("api-ms-win-downlevel-normaliz-l1-1-0", "normaliz"),
    ("api-ms-win-eventing-consumer-l1-1-0", "advapi32"),
    ("api-ms-win-eventlog-legacy-l1-1-0", "advapi32"),
    ("api-ms-win-security-appcontainer-l1-1-0", "kernel32"),
    ("api-ms-win-security-base-ansi-l1-1-0", "advapi32"),
    ("api-ms-win-security-provider-ansi-l1-1-0", "advapi32"),
    ("api-ms-win-eventing-classicprovider-l1-1-0", "advapi32"),
    ("api-ms-win-mm-time-l1-1-0", "winmm"),
    ("api-ms-win-security-lsalookup-ansi-l2-1-0", "advapi32"),
    ("api-ms-win-security-lsalookup-l2-1-0", "advapi32"),
    ("api-ms-win-security-lsapolicy-l1-1-0", "advapi32"),
}, "les onze contrats API-set vérifiés sont rattachés à leur hôte documenté, et eux seuls (#2902)")
ok("fn contrats_verifies_servent_tout_leur_def" in _m,
   "test : chaque fonction du .def de chaque contrat vérifié se résout par le contrat (#2902)")
