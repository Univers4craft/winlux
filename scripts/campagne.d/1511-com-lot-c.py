# #1511 : lot C du relevé, COM (ole32 et oleaut32) pour pywin32, Qt 6, .NET, wxPython, ffmpeg.
mod_rs = (runtime_dlls / "mod.rs").read_text(encoding="utf-8")
lot = (runtime_dlls / "ole32_lot_c.rs").read_text(encoding="utf-8")
mk = (runtime_dlls / "ole32_moniker.rs").read_text(encoding="utf-8")
oa = (runtime_dlls / "oleaut32.rs").read_text(encoding="utf-8")
oa_c = (runtime_dlls / "oleaut32_lot_c.rs").read_text(encoding="utf-8")
rec = (runtime_dlls / "oleaut32_record.rs").read_text(encoding="utf-8")
pic = (runtime_dlls / "oleaut32_picture.rs").read_text(encoding="utf-8")
ok("ole32_lot_c::exports()" in mod_rs and "ole32_moniker::exports()" in mod_rs
   and "oleaut32_record::exports()" in mod_rs and "oleaut32_picture::exports()" in mod_rs
   and "oleaut32_lot_c::exports()" in mod_rs, "ole32 et oleaut32 : modules du lot C inscrits (#1511)")
ok('"CoMarshalInterface" => fn co_marshal_interface' in lot and '"CoUnmarshalInterface" => fn co_unmarshal_interface' in lot
   and "CO_E_OBJNOTCONNECTED" in lot, "ole32 : marshaling dans le processus, refus propre ailleurs (#1511)")
ok('"CoRegisterInitializeSpy"' in lot and '"RoRegisterForApartmentShutdown"' in lot and '"SetRestrictedErrorInfo"' in lot,
   "ole32 : espions d'initialisation et Windows Runtime (#1511)")
ok('"MkParseDisplayName" => fn mk_parse_display_name_api' in mk and '"GetRunningObjectTable"' in mk
   and "MK_E_SYNTAX" in mk, "ole32 : monikers et table des objets actifs (#1511)")
ok('"#21" => fn safe_array_lock' in oa and '"#42" => fn safe_array_create_ex' in oa,
   "oleaut32 : SafeArray du lot C par nom et par ordinal (#1511)")
ok('"#323" => fn get_record_info_from_type_info' in rec and '"#419" => fn ole_create_picture_indirect' in pic
   and '"#102" => fn var_cy_from_r8' in oa_c, "oleaut32 : IRecordInfo, images, polices, CY (#1511)")
