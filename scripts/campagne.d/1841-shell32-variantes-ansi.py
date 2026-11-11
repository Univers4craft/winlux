# shell32 sert SHFileOperationA (import différé de l'ODT) et la disposition x86 compactée de SHFILEOPSTRUCT (#1841).
_src = (ROOT / "runtime/src/dlls/shell32_ansi.rs").read_text(encoding="utf-8")
_ops = (ROOT / "runtime/src/dlls/apps_win32.rs").read_text(encoding="utf-8")
ok('"SHFileOperationA"' in _src, "shell32 exporte SHFileOperationA (#1841)")
ok("[4, 8, 12, 16, 18]" in _ops, "SHFILEOPSTRUCT x86 lu avec la disposition pshpack1 (#1841)")
