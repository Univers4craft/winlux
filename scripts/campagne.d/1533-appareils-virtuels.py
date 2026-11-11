# #1533 : un pilote constructeur s'ouvre par son nom et répond à DeviceIoControl, sans .sys.
vdev = (runtime_dlls / "vdev.rs").read_text(encoding="utf-8")
k32 = (runtime_dlls / "kernel32.rs").read_text(encoding="utf-8")
node = (runtime_dlls / "kernel32_node.rs").read_text(encoding="utf-8")
ok("vdev::is_virtual" in k32 and "vdev::device_io_control" in node,
   "appareils virtuels : \\\\.\\Nom ouvert et DeviceIoControl transmis (#1533)")
ok("ERROR_INVALID_FUNCTION" in vdev and "WINLUX_VDEV_DUMP" in vdev,
   "appareils virtuels : code inconnu refusé et journalisé sans contenu par défaut (#1533)")
