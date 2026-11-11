# #1876 iphlpapi : GetBestInterface, GetBestInterfaceEx et GetBestRoute suivent les routes Linux (Steam).
s = (runtime_dlls / "iphlpapi.rs").read_text(encoding="utf-8")
ok('"GetBestInterface" => fn GetBestInterface' in s and "fn best_route(" in s and "/proc/net/ipv6_route" in s,
   "GetBestInterface servi et calculé depuis les tables de routage (#1876)")
