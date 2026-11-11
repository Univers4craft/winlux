# #1625 : les outils d'installation de pilotes (driverinstalltool_64 d'iCUE) trouvent setupapi et cfgmgr32 complets.
inf = (runtime_dlls / "setupinf.rs").read_text(encoding="utf-8")
dev = (runtime_dlls / "devnodes.rs").read_text(encoding="utf-8")
ok(all(f'"{n}"' in dev for n in ("CM_Locate_DevNode_ExW", "CM_Connect_MachineW", "CM_Get_First_Log_Conf_Ex",
                                  "CM_Get_Next_Res_Des_Ex", "CM_Free_Res_Des_Handle", "CM_Reenumerate_DevNode_Ex")),
   "pilotes : variantes cfgmgr32 _Ex et configurations logiques servies (#1625)")
ok(all(f'"{n}"' in inf for n in ("SetupOpenInfFileW", "SetupGetStringFieldW", "SetupScanFileQueueW",
                                  "SetupDiBuildDriverInfoList", "SetupDiGetDriverInfoDetailW", "DiUninstallDevice")),
   "pilotes : lecture des .inf, files de copie et listes de pilotes servies (#1625)")
ok("SetupQueueCopy" not in inf,
   "pilotes : aucune copie de fichier de pilote vers System32\\drivers (#1625)")
