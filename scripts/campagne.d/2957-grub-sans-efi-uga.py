# #2957 : aucun grub.cfg généré ne charge efi_uga, absent du paquet
# grub-efi-ia32-bin d'Ubuntu (2.14-2ubuntu2.1) : sinon « error: file
# /boot/grub/i386-efi/efi_uga.mod not found » à chaque démarrage UEFI 32 bits.
_src_2957 = {n: (SCRIPTS / n).read_text(encoding="utf-8")
             for n in ("build-live-iso.sh", "install.sh", "winlux-installer.py", "winlux-reset.py")}
_uga_2957 = [n for n, t in _src_2957.items() if "insmod efi_uga" in t]
_gop_2957 = [n for n, t in _src_2957.items() if "insmod efi_gop" not in t]
print(f"      insmod efi_uga : {_uga_2957} ; sans efi_gop : {_gop_2957}")
ok(not _uga_2957 and not _gop_2957,
   "UEFI 32 bits : GRUB charge efi_gop et plus efi_uga, absent du paquet (#2957)")
