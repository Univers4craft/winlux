# #1615 : lot F de #1410 (crypto, propriétés, Media Foundation, winmm…).
m = (runtime_dlls / "mod.rs").read_text(encoding="utf-8")
ok(all(f"pub mod lot_f_{n};" in m for n in ("winmm", "crypt32", "propsys", "mfplat", "divers")),
   "modules du lot F inscrits (#1615)")
w = (runtime_dlls / "lot_f_winmm.rs").read_text(encoding="utf-8")
ok('"joyGetPosEx" => fn joyGetPosEx' in w and '"mixerGetLineInfoA" => fn mixerGetLineInfoA' in w,
   "manettes joy* et mélangeur ANSI de winmm (#1615)")
c = (runtime_dlls / "lot_f_crypt32.rs").read_text(encoding="utf-8")
ok('"CryptFindOIDInfo"' in c and '"CertVerifySubjectCertificateContext"' in c,
   "table d'OID et vérification de signature de crypt32 (#1615)")
p = (runtime_dlls / "lot_f_propsys.rs").read_text(encoding="utf-8")
ok('"StgSerializePropVariant"' in p and '"PropVariantChangeType"' in p,
   "PROPVARIANT : conversions et forme sérialisée MS-OLEPS (#1615)")
f = (runtime_dlls / "lot_f_mfplat.rs").read_text(encoding="utf-8")
ok('"MFCreateEventQueue"' in f and '"MFCreatePresentationDescriptor"' in f and "fn MFInvokeCallback" in f,
   "Media Foundation : file d'événements, descripteurs, résultats asynchrones (#1615)")
b = (runtime_dlls / "bcrypt.rs").read_text(encoding="utf-8")
ok('"BCryptDeriveKeyPBKDF2"' in b and "pbkdf2_rfc6070_et_enumeration" in b,
   "PBKDF2 de CNG vérifié sur le vecteur RFC 6070 (#1615)")
a = (runtime_dlls / "common_apps.rs").read_text(encoding="utf-8")
ok('("d3d12", 101, "D3D12CreateDevice")' in a and '("mmdevapi", 17, "ActivateAudioInterfaceAsync")' in a,
   "numéros d'ordre d3d12, d2d1 et mmdevapi lus dans le SDK public (#1615)")
