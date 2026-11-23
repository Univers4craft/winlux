# Release : chaque démarrage de l'ISO (bios, uefi, uefi32) et le test
# d'installation sont des étapes distinctes qui tournent dès que l'ISO est
# construite, même si une autre a échoué : une panique uefi32 (#3199) ne masque
# plus le test d'installation (#3250).
_wf = (ROOT / ".github/workflows/release.yml").read_text()
_etapes = {}
for _bloc in _wf.split("\n      - name: ")[1:]:
    _nom, _, _corps = _bloc.partition("\n")
    _etapes[_nom] = _corps
def _seule(commande):
    noms = [n for n, corps in _etapes.items() if commande in corps]
    return noms[0] if len(noms) == 1 else None, noms
_cibles = {m: _seule(f'test-live-iso-qemu.sh "build/winlux-live-$RELEASE_NAME.iso" {m}\n') for m in ("bios", "uefi", "uefi32")}
_cibles["installation"] = _seule("scripts/test-install-vm-guest.sh")
_independantes = all(n and "!cancelled()" in _etapes[n] and "steps.iso.outcome == 'success'" in _etapes[n]
                     for n, _ in _cibles.values())
_distinctes = len({n for n, _ in _cibles.values()}) == 4
print(f"#3250 : étapes {{{', '.join(f'{k}: {v[0]!r}' for k, v in _cibles.items())}}} ; "
      f"distinctes={_distinctes} ; indépendantes={_independantes}")
ok(_distinctes and _independantes,
   "release : bios, uefi, uefi32 et l'installation validés dans des étapes indépendantes (#3250)")
