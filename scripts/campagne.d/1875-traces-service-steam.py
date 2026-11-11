# Traces (WINLUX_TRACE=1) des paramètres de ShellExecuteEx, de la ligne de commande reçue, des clés de registre et des chemins lus : pourquoi SteamService.exe s'arrête en code 1 (#1875).
_s = (ROOT / "runtime/src/dlls/shell32.rs").read_text(encoding="utf-8")
_a = (ROOT / "runtime/src/dlls/advapi32.rs").read_text(encoding="utf-8")
_k = (ROOT / "runtime/src/dlls/kernel32.rs").read_text(encoding="utf-8")
_p = (ROOT / "runtime/src/process.rs").read_text(encoding="utf-8")
ok('"ShellExecuteEx(verbe {verb:?}, fichier {file:?}, paramètres {params:?}' in _s and '"ShellExecute → {} {args:?}"' in _s,
   "ShellExecuteEx trace ses paramètres et la commande lancée (#1875)")
ok('crate::trace!("ligne de commande : {cmd}");' in _p,
   "ligne de commande du programme tracée au démarrage (#1875)")
ok("unsafe fn traced_open(" in _a and "RegQueryValueExW({handle:#x}, {name:?}) = {r}" in _a
   and 'crate::trace!("GetFileAttributesW({path}) = {r:#x}");' in _k,
   "clés de registre et chemins lus tracés (#1875)")
