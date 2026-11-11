# Appels Nt du registre utilisés par chrome_elf.dll (steamwebhelper) : NtSetValueKey et ses voisins (#1635).
_nt = (ROOT / "runtime/src/dlls/ntdll.rs").read_text(encoding="utf-8")
ok(all(f'"{n}" => fn' in _nt for n in ("NtSetValueKey", "NtDeleteValueKey", "NtEnumerateKey", "NtEnumerateValueKey", "NtQueryKey", "NtFlushKey")),
   "ntdll sert NtSetValueKey, NtQueryKey et les énumérations de registre (#1635)")
