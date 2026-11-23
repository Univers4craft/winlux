# Les journaux de validation du release (qemu-*.log, serial.log…) sont gardés
# à chaque run, vert compris : un démarrage réussi sert de référence face à
# un démarrage qui panique (#3203, bloquait #3199).
import re as _re
_wf = (ROOT / ".github/workflows/release.yml").read_text()
_etape = _re.search(r"- name: Conserver les journaux de validation\n(.*?)(?=\n      - name:)", _wf, _re.S)
_corps = _etape.group(1) if _etape else ""
_condition = _re.search(r"^\s*if: (.+)$", _corps, _re.M)
print(f"#3203 : étape trouvée={bool(_etape)} ; condition={_condition.group(1) if _condition else None!r} ; "
      f"qemu-*.log={'build/qemu-*.log' in _corps}")
ok(bool(_etape) and _condition is not None and _condition.group(1).strip() == "always()"
   and "build/qemu-*.log" in _corps and "${{ github.run_attempt }}" in _corps,
   "release : journaux de validation QEMU gardés aussi quand le run est vert (#3203)")
