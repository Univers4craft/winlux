# Compter les micros (waveInGetNumDevs, waveInGetDevCaps, WAVE_FORMAT_QUERY) ne pose pas la question « micro » : Steam ne la déclenche plus au lancement (#1733, #1882).
_p = (ROOT / "runtime/src/permissions.rs").read_text(encoding="utf-8")
_w = (ROOT / "runtime/src/dlls/winmm.rs").read_text(encoding="utf-8")
ok("pub fn not_refused(cap: Capability) -> bool" in _p and "fn enumerer_le_micro_ne_demande_rien" in _p,
   "capacité « pas refusée » lue sans rien demander, avec test (#1882)")
ok("crate::permissions::not_refused(crate::permissions::Capability::Microphone)\n        && alsa_capture().is_ok()" in _w
   and "WAVE_FORMAT_QUERY n'ouvre rien : pas de question (#1882)" in _w,
   "énumération et requête de format du micro sans question, ouverture réelle seule demandée (#1882)")
_a = (ROOT / "runtime/src/audio/mod.rs").read_text(encoding="utf-8")
ok('.name("winlux-mic-permission".into())' in _a and "fn attach_microphone(q: std::sync::Weak<CaptureQueue>)" in _a
   and "sur le fil de capture, sans bloquer l'application (#1882)" in _w,
   "la question « micro » ne bloque jamais le fil de l'application (WASAPI, DirectSound, waveIn) (#1882)")
