# Planificateur de tâches : trace WINLUX_TRACE des propriétés posées, des QueryInterface refusés et de l'enregistrement (AddTask d'Acrobat, #1825, #1824).
_src = (ROOT / "runtime/src/dlls/taskschd.rs").read_text(encoding="utf-8")
ok("fn trace(" in _src and "RegisterTaskDefinition(" in _src, "trace du planificateur de tâches sous WINLUX_TRACE (#1825)")
