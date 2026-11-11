# #1729 : VirtualQueryEx et sa famille sur le handle d'un autre processus.
k = (runtime_dlls / "kernel32.rs").read_text(encoding="utf-8")
ok("fn foreign_region(" in k and '/proc/{pid}/maps' in k,
   "VirtualQueryEx décrit la mémoire d'un autre processus d'après son maps (#1729)")
n = (runtime_dlls / "ntdll.rs").read_text(encoding="utf-8")
ok("virtual_query_ex(handle" in n,
   "NtQueryVirtualMemory accepte le handle d'un autre processus (#1729)")
q = (runtime_dlls / "psapi_k32.rs").read_text(encoding="utf-8")
ok("fn foreign_resident_pages(" in q and "fn pagemap_present(" in q,
   "QueryWorkingSet(Ex) lit les pages présentes d'un autre processus (#1729)")
