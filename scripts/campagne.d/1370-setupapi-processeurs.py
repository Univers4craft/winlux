# #1370 : SetupDiGetClassDevsW(GUID_DEVCLASS_PROCESSOR) liste les processeurs.
ok("PROCESSOR_CLASS" in (runtime_dlls / "devnodes.rs").read_text(encoding="utf-8")
   and "fn parse_cpuinfo" in (runtime_dlls / "devnodes.rs").read_text(encoding="utf-8")
   and "classe_processor_depuis_cpuinfo" in (SCRIPTS / "test-all.d/1370-setupapi-processeurs.sh").read_text(encoding="utf-8"),
   "setupapi : classe Processor, nom du processeur pour le diagnostic de Paint.NET (#1370)")
