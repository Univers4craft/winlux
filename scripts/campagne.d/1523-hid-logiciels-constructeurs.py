# #1523 : hid.dll décrit l'appareil (HidP_*), une interface par collection de tête, avis de branchement.
src = (runtime_dlls / "hid_report.rs").read_text(encoding="utf-8")
hid = (runtime_dlls / "hid.rs").read_text(encoding="utf-8")
ok("HidP_GetValueCaps" in src and "HidP_GetUsages" in src and "HidP_SetUsageValueArray" in src,
   "hid.dll : fonctions HidP_* de description et de rapports (#1523)")
ok("&col" in hid and "&mi_" in hid and "attributes_only" in hid,
   "hid.dll : collections de tête et ouverture sans droit (#1523)")
ok((runtime_dlls / "devnotify.rs").exists(),
   "WM_DEVICECHANGE à l'arrivée et au retrait d'un appareil (#1523)")
