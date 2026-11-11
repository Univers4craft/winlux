# msiexec : correctif .msp et transformation .mst refusés en clair, jamais ignorés en silence (#1715).
m = (ROOT / "runtime/src/msi/mod.rs").read_text(encoding="utf-8")
ok("correctif .msp non pris en charge" in m and '"p" | "update"' in m,
   "msiexec /p et PATCH= signalés dans le journal (#1715)")
ok("transformation .mst non prise en charge" in m,
   "msiexec TRANSFORMS= signalé dans le journal (#1715)")
