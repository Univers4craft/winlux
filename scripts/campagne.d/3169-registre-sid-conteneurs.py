# #3169 : kernelbase sert AppContainerRegisterSid, UnregisterSid, LookupMoniker
# et FreeMemory, que le bac à sable de Chromium exige (CHECK). Le comportement
# est gardé par le test unitaire nommé.
t = (runtime_dlls / "electron_win32.rs").read_text(encoding="utf-8")
ok("fn registre_des_sid_de_conteneurs_3169" in t, "kernelbase tient le registre des SID de conteneurs du bac à sable de Chromium (#3169)")
