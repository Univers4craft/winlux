# MSI : les traces « winlux[wow32] » des processus d'action personnalisée sont recopiées au journal (#1666).
act = (ROOT / "runtime/src/msi/actions.rs").read_text(encoding="utf-8")
ok("fn is_trace_line" in act and "lines.extend(traces)" in act,
   "MSI : trace wow32 des actions personnalisées recopiée au journal de msiexec (#1666)")
