# Steam, utility storage : GetFirmwareType (#1999) et
# WSAEnumNameSpaceProviders* (#2000) exportées.
k = (ROOT / "runtime/src/dlls/kernel32_misc.rs").read_text()
w = (ROOT / "runtime/src/dlls/electron_win32.rs").read_text()
ok('"GetFirmwareType" => fn get_firmware_type' in k and "/sys/firmware/efi" in k,
   "kernel32 : GetFirmwareType répond BIOS ou UEFI (#1999)")
ok(all(f'"WSAEnumNameSpaceProviders{n}"' in w for n in ("A", "W", "ExA", "ExW")),
   "ws2_32 : WSAEnumNameSpaceProviders A/W/ExA/ExW exportées (#2000)")
