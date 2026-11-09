# Bac à sable de Chromium (#767) : NtOpenThread, NtOpenProcess et les ouvertures de jeton exportées par ntdll.
ok(all(f'"{n}" => fn' in (runtime_dlls / "ntdll.rs").read_text()
       for n in ("NtOpenThread", "NtOpenProcess", "NtOpenProcessToken", "NtOpenProcessTokenEx",
                 "NtOpenThreadToken", "NtOpenThreadTokenEx")),
   "ntdll : ouvertures interceptées par le bac à sable de Chromium exportées (#767)")
