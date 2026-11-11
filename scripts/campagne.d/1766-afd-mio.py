# #1766 : \Device\Afd et IOCTL_AFD_POLL pour mio/tokio (Discord Updater).
a = (runtime_dlls / "afd.rs").read_text(encoding="utf-8")
n = (runtime_dlls / "ntdll.rs").read_text(encoding="utf-8")
f = (runtime_dlls / "ntdll_file.rs").read_text(encoding="utf-8")
ok("afd::is_afd_path(&win_path)" in n and "pub const IOCTL_AFD_POLL: u32 = 0x0001_2024;" in a,
   "NtCreateFile ouvre \\Device\\Afd et sert IOCTL_AFD_POLL (#1766)")
ok('"NtCancelIoFileEx" => fn NtCancelIoFileEx' in f and "afd::device_io_control" in f,
   "NtDeviceIoControlFile et NtCancelIoFileEx passent par le pilote AFD (#1766)")
ok("post_completion(h, apc_ctx" in a,
   "le sondage AFD se termine par un paquet au port, ApcContext en guise d'OVERLAPPED (#1766)")
