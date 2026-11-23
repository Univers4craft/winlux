# user32 exporte InSendMessageEx : ISMEX_SEND pendant un message envoyé par
# un autre fil, ISMEX_NOSEND sinon (Learn, #3104).
u32_3104 = (ROOT / "runtime/src/dlls/user32.rs").read_text(encoding="utf-8")
ok('"InSendMessageEx"=>fn in_send_message_ex' in u32_3104
   and "fn in_send_message_ex_distingue_l_envoi_d_un_autre_fil_3104" in u32_3104,
   "user32 : InSendMessageEx exportée et testée (#3104)")
