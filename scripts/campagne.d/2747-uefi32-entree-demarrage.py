# #2747 : en UEFI 32 bits, les menus GRUB passent no5lvl au noyau et plus
# efi=noruntime, qui coupait les variables UEFI : l'installateur crée de
# nouveau l'entrée de démarrage « Winlux » (tablette Acer : « No Bootable
# Device » après installation). Gardé par scripts/test-boot-paths.py.
chemins_2747 = (ROOT / "scripts" / "test-boot-paths.py").read_text(encoding="utf-8")
ok("Mode mixte (#2747)" in chemins_2747 and "set winlux_efi_args=no5lvl" in chemins_2747
   and all("set winlux_efi_args=no5lvl" in (ROOT / "scripts" / nom).read_text(encoding="utf-8")
           for nom in ("build-live-iso.sh", "install.sh", "winlux-installer.py", "winlux-reset.py")),
   "UEFI 32 bits : services UEFI gardés (no5lvl), entrée de démarrage créée (#2747)")
