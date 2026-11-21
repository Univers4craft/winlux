# Code de sortie complet d'un enfant pour son parent Windows (#2267).
_k = (ROOT / "runtime/src/dlls/kernel32.rs").read_text(encoding="utf-8")
_o = (ROOT / "integration/winlux-open").read_text(encoding="utf-8")
_w = (ROOT / "scripts/winlux-setup-wizard.py").read_text(encoding="utf-8")
_p = (ROOT / "testexe/src/bin/p1_suite.rs").read_text(encoding="utf-8")
ok("fn child_exit_code(" in _k and "fn exit_code_channel(" in _k,
   "CreateProcess : GetExitCodeProcess lit le code complet écrit par l'enfant (#2267)")
ok("std::process::exit(" not in (ROOT / "runtime/src/main.rs").read_text(encoding="utf-8"),
   "runtime : toute sortie écrit son code complet pour le parent (#2267)")
ok("parent_code_file" in _o and "trap forward_code EXIT" in _o,
   "winlux-open : code complet transmis au parent de ShellExecuteEx (#2267)")
ok("def full_code(" in _w and "def record_code(" in _w,
   "assistant d'installation : 3010 et 1602 lus et rendus entiers (#2267)")
ok("exitcode.exe\\\" 3010" in _p,
   "p1_suite : un enfant .exe qui rend 3010 est lu 3010 par son parent (#2267)")
