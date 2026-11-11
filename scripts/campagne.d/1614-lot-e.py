# #1614 : lot E de #1410 (comptes, réseau, pdh, impression, cache wininet, setupapi, ntdll).
lot = "".join((runtime_dlls / f).read_text(encoding="utf-8")
              for f in ("lot_e_comptes.rs", "lot_e_reseau.rs", "lot_e_divers.rs", "pdh.rs"))
ok(all(f'"{n}" => fn' in lot for n in ("NetWkstaGetInfo", "WTSEnumerateSessionsW", "LoadUserProfileW")),
   "netapi32, wtsapi32 et userenv servis par le lot E (#1614)")
ok(all(f'"{n}" => fn' in lot for n in ("GetIfTable2", "IcmpSendEcho", "DnsQueryEx", "WSAStringToAddressA")),
   "iphlpapi, ICMP, dnsapi et ws2_32 complétés par le lot E (#1614)")
ok(all(f'"{n}" => fn' in lot for n in ("PdhEnumObjectsW", "PdhLookupPerfNameByIndexW", "PdhGetFormattedCounterArrayW")),
   "pdh : chemins, objets et index de compteurs (#1614)")
ok(all(f'"{n}" => fn' in lot for n in ("CreateUrlCacheEntryW", "InternetGetCookieW", "EnumFormsW")),
   "cache wininet et formulaires winspool (#1614)")
ok('("winspool", 204, "SetDefaultPrinterW")' in (runtime_dlls / "common_apps.rs").read_text(encoding="utf-8"),
   "SetDefaultPrinterW servi par ordinal #204 du SDK (#1614)")
svc = (runtime_dlls / "advapi32_services.rs").read_text(encoding="utf-8")
ok(all(f'"{n}" => fn' in svc for n in ("ChangeServiceConfigA", "ChangeServiceConfig2A", "QueryServiceConfigA",
   "EnumDependentServicesA", "ControlServiceExW")),
   "variantes A des services et ControlServiceExW pour driverinstalltool_64 (#1614)")
