# Les membres protégés de codecvt<wchar_t> exportés sont aussi les cases 3 à 9
# de sa table virtuelle : chacun est déclaré #[inline(never)], sinon le build
# release en copie un par unité de compilation et l'export n'a plus l'adresse
# de la case (#3165).
import re as _re
_src = (ROOT / "runtime/src/dlls/msvcp140_flux.rs").read_text()
_table = _re.search(r"codecvt_w: \[(\s*a!.*?)\]", _src, _re.S).group(1)
_noms = _re.findall(r"a!\((\w+)\)", _table)[3:10]
_libres = [n for n in _noms
           if not _re.search(r"#\[inline\(never\)\]\s*unsafe extern \"win64\" fn " + n + r"\(", _src)]
print(f"#3165 : {_noms} ; membres de codecvt<wchar_t> sans #[inline(never)] : {_libres}")
ok(len(_noms) == 7 and not _libres,
   "codecvt<wchar_t> : chaque membre exporté a l'adresse de sa case de table virtuelle, release compris (#3165)")
