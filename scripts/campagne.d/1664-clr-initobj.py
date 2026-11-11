# #1664 : initobj (FE 15) porte un jeton de type ; sans lui le décodage IL
# se décalait (Corsair.Service.exe : « jeton 0x02020000 »).
clr_dir = runtime_dlls.parent / "clr"
clr_opcodes = (clr_dir / "opcodes.rs").read_text(encoding="utf-8")
clr_il = (clr_dir / "il.rs").read_text(encoding="utf-8")
ok("| OpCode::Initobj => 4," in clr_opcodes and "fn initobj_porte_un_jeton" in clr_opcodes,
   "CLR : initobj lit son jeton, l'instruction suivante est décodée au bon endroit (#1664)")
ok("qui n'est pas une méthode, dans" in clr_il and "fn location(" in clr_il,
   "CLR : un appel vers un jeton qui n'est pas une méthode nomme la méthode et l'offset IL (#1664)")
ok("fn chaque_opcode_a_la_taille_d_operande_d_ecma" in clr_opcodes and "0xFE19 => OpCode::No" in clr_opcodes,
   "CLR : chaque opcode d'ECMA-335 a sa taille d'opérande, préfixe no. compris (#1664)")
