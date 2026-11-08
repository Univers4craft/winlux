# Installeur Creative Cloud 32 bits : famille CreateTimerQueue (#1226).
ok('"CreateTimerQueueTimer" => fn super::timer_queue::create_timer_queue_timer' in (runtime_dlls / "kernel32_node.rs").read_text(encoding="utf-8")
   and "file_de_minuteurs_unique_periodique_et_suppression" in (runtime_dlls / "timer_queue.rs").read_text(encoding="utf-8")
   and "CreateTimerQueueTimer(&tm" in (ROOT / "testc/wow32/thread32.c").read_text(encoding="utf-8"),
   "Files de minuteurs kernel32 (CreateTimerQueue…), test x86 dans thread32 (#1226)")
