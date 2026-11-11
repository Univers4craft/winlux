# #1823 discord_utils.node : contrats sddl-ansi et service-*, MSI, shell32, shlwapi, gdi32, crypt32.
m = (runtime_dlls / "mod.rs").read_text(encoding="utf-8")
d = (runtime_dlls / "discord_win32.rs").read_text(encoding="utf-8")
ok('base.starts_with("api-ms-win-security-sddl-ansi-l1-")' in m and 'base.starts_with("api-ms-win-service-")' in m,
   "api-ms-win-security-sddl-ansi et api-ms-win-service-* servis par advapi32 (#1823)")
ok('"#88" => fn MsiInstallProductW' in d and '"MsiSetInternalUI"' in d and "discord_win32::msi_exports()" in m,
   "msi.dll sert MsiInstallProductW (#88) et MsiSetInternalUI (#141) (#1823)")
ok(all(f'"{n}"' in d for n in ("SHQueryUserNotificationState", "wnsprintfW", "D3DKMTSetProcessSchedulingPriorityClass", "CryptMsgOpenToDecode", "CryptMsgUpdate")),
   "shell32, shlwapi, gdi32 et crypt32 servent les imports de discord_utils.node (#1823)")
