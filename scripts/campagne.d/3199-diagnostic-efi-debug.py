# #3199 : l'entrée de diagnostic, celle que la CI démarre, porte efi=debug et
# loglevel=7 ; aucune autre entrée du grub.cfg livré ne les porte.
_iso_3199 = (SCRIPTS / "build-live-iso.sh").read_text(encoding="utf-8")
_lignes_3199 = [l for l in _iso_3199.splitlines() if l.strip().startswith("linux ") and "efi=debug" in l]
print(f"      lignes linux avec efi=debug : {len(_lignes_3199)}")
ok(len(_lignes_3199) == 1 and "rdinit=/init panic=1 loglevel=7 efi=debug" in _lignes_3199[0],
   "Seule l'entrée de diagnostic démarre avec efi=debug et loglevel=7 (#3199)")
