# #3176 : l'outil de déploiement d'Office (processus 32 bits) importe msi.dll
# en chargement différé par les ordinaux 90, 8, 141, 190, 113, 39, 173, 203
# et 45 ; chacun doit nommer une fonction servie par nom (et donc par
# ordinal, `with_msi_ordinals`), sinon 0xC06D007F au premier appel.
import re as _re
_table = dict(l.split("\t") for l in (runtime_dlls / "msi_ordinals.tsv").read_text(encoding="utf-8").splitlines()
              if l and not l.startswith("#"))
_servis = set()
for _f in [runtime_dlls / "legacy_dlls.rs", runtime_dlls / "discord_win32.rs",
           *sorted((ROOT / "runtime/src/msi").glob("*.rs"))]:
    _servis |= set(_re.findall(r'"(Msi\w+)"\s*=>\s*fn', _f.read_text(encoding="utf-8")))
_manquants = [f"#{n} {_table.get(str(n))}" for n in (90, 8, 141, 190, 113, 39, 173, 203, 45)
              if _table.get(str(n)) not in _servis]
print(f"#3176 : imports différés de msi.dll non servis : {_manquants}")
ok(not _manquants,
   "msi : les neuf imports différés de l'outil de déploiement d'Office sont servis (#3176)")
