# #2429 : une interface COM du runtime rendue au code x86 par une fonction
# non décrite est refusée et nommée, au lieu d'une faute muette (famille
# #2428).
w = (ROOT / "runtime/src/wow32/mod.rs").read_text(encoding="utf-8")
ok("fn runtime_com_object" in w
   and "fn refuse_runtime_object" in w
   and "fn interface_du_runtime_sans_mandataire_refusee" in w,
   "wow32 : interface du runtime sans mandataire 32 bits refusée et nommée (#2429)")
