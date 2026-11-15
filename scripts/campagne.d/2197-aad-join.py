# #2197 : Teams charge netapi32!NetGetAadJoinInformation en différé.
c = (runtime_dlls / "lot_e_comptes.rs").read_text(encoding="utf-8")
ok('"NetGetAadJoinInformation" => fn NetGetAadJoinInformation' in c
   and '"NetFreeAadJoinInformation" => fn NetFreeAadJoinInformation' in c,
   "netapi32 exporte NetGetAadJoinInformation et NetFreeAadJoinInformation (#2197)")
ok("fn poste_non_joint_a_azure_ad_2197(" in c,
   "poste non joint à Azure AD : S_OK et ppJoinInfo nul, comme documenté (#2197)")
