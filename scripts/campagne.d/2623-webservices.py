# #2623 : webservices.dll (WWSAPI) intégrée, avec les quinze fonctions que
# l'outil de déploiement d'Office importe en différé (relevées par #2627).
mod_rs = (ROOT / "runtime/src/dlls/mod.rs").read_text(encoding="utf-8")
ws = (ROOT / "runtime/src/dlls/webservices.rs").read_text(encoding="utf-8")
c = (ROOT / "testc/wow32/crt32.c").read_text(encoding="utf-8")
noms = ["WsCreateHeap", "WsCreateError", "WsReadType", "WsReadStartElement",
        "WsReadToStartElement", "WsFillReader", "WsFreeReader", "WsGetErrorString",
        "WsCreateWriter", "WsGetErrorProperty", "WsFreeError", "WsFreeWriter",
        "WsCreateReader", "WsSetInput", "WsFreeHeap"]
ok('name: "webservices"' in mod_rs and all(f'"{n}" =>' in ws for n in noms),
   "webservices.dll : les quinze imports d'ODT servis (#2623)")
ok("fn structure_attribut_element_et_repetition" in ws and "#2623" in c,
   "webservices.dll : WsReadType d'une structure en x64 et en x86 (#2623)")
