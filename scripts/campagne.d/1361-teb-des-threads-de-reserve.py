# #1361 : Claude ne grossit plus de 65 Mo/s, chaque thread de réserve rend son TEB.
ok("crate::process::free_current_teb();" in (runtime_dlls / "kernel32_node.rs").read_text(encoding="utf-8")
   and "fn thread_de_reserve_libere_son_teb" in (runtime_dlls / "kernel32_node.rs").read_text(encoding="utf-8")
   and "thread_de_reserve_libere_son_teb" in (SCRIPTS / "test-all.d/1361-teb-des-threads-de-reserve.sh").read_text(encoding="utf-8"),
   "Threads de réserve : TEB libéré en fin de thread (#1361)")
