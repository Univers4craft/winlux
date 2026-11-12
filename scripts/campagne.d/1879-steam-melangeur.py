# #1879 winmm : mixerGetDevCapsA et mixerMessage servis (Steam).
s = (runtime_dlls / "lot_f_winmm.rs").read_text(encoding="utf-8")
ok('"mixerGetDevCapsA" => fn mixerGetDevCapsA' in s and '"mixerMessage" => fn mixerMessage' in s,
   "winmm sert mixerGetDevCapsA et mixerMessage (#1879)")
