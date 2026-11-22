# Inscriptions WER tenues dans le processus ; WerUnregisterFile et le reste
# de la famille Register/Unregister servis par kernel32 (OneDrive, #2898).
_w = (runtime_dlls / "wer_inscriptions.rs").read_text(encoding="utf-8")
ok(all(f'"{n}"' in _w for n in ("WerUnregisterFile", "WerUnregisterCustomMetadata", "WerUnregisterAdditionalProcess",
                                  "WerUnregisterRuntimeExceptionModule", "WerUnregisterExcludedMemoryBlock",
                                  "WerUnregisterAppLocalDump")),
   "kernel32 sert les désinscriptions WER (#2898)")
ok("fn fichier_inscrit_puis_desinscrit_2898" in _w and "fn autres_inscriptions_wer_2898" in _w,
   "tests des inscriptions et désinscriptions WER (#2898)")
