# #1616 : entrée brute aux tailles x86 (pygame 32 bits, manettes de SDL 2).
raw = (runtime_dlls / "rawinput.rs").read_text(encoding="utf-8")
ok("size as usize != device_size()" in raw and "size as usize != list_size()" in raw
   and "fn for_caller(" in raw, "RAWINPUTDEVICE 12, RAWINPUTDEVICELIST 8 et en-tête de 16 octets acceptés en x86 (#1616)")
