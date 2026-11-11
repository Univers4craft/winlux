# Steam.exe x86 : variable cachée « =C: » écrite par _wchdir, masques
# d'affinité DWORD_PTR, imports shell32 #680 et wsock32 #1142 (#1380).
ok("fn is_hidden_env_name" in (ROOT / "runtime/src/dlls/kernel32.rs").read_text(encoding="utf-8")
   and "fn set_process_affinity_mask(h: usize, mask: usize)" in (ROOT / "runtime/src/dlls/kernel32_cpu.rs").read_text(encoding="utf-8")
   and '"#680" => fn IsUserAnAdmin' in (ROOT / "runtime/src/dlls/security.rs").read_text(encoding="utf-8")
   and '(1142, "GetAcceptExSockaddrs")' in (ROOT / "runtime/src/dlls/ws2_32.rs").read_text(encoding="utf-8"),
   "Steam.exe x86 : chdir, affinité et imports par ordinal (#1380)")
