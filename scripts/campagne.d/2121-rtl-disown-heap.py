# #2121 : Teams (WIL) appelle ntdll!RtlDisownModuleHeapAllocation.
n = (runtime_dlls / "ntdll.rs").read_text(encoding="utf-8")
ok('"RtlDisownModuleHeapAllocation" => fn rtl_disown_module_heap_allocation' in n,
   "ntdll exporte RtlDisownModuleHeapAllocation (#2121)")
