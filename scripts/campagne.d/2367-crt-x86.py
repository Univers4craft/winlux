# Exports de la CRT universelle servis en x64 et en x86 (#2367). Le
# comportement est vérifié par le test unitaire nommé ci-dessous ; ce
# contrôle vérifie seulement qu'il existe et que ses noms sont là.
import re as _re
_m = (ROOT / "runtime/src/wow32/mod.rs").read_text()
_c = (ROOT / "runtime/src/wow32/crt.rs").read_text()
ok(_re.search(r"fn crt_x64_toujours_servie_en_x86\(\)", _m) is not None,
   "test : tout export CRT x64 a une passerelle x86 (#2367)")
ok(all(f'"{n}"' in _c for n in ("__crtFlsAlloc", "_snscanf", "_swprintf_c")),
   "__crt*, _snscanf et _swprintf_c servis en x86 (#2367)")
