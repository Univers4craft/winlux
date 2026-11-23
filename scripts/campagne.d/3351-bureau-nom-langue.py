# #3351 : le premier bureau virtuel s'appelait « Bureau Winlux » dans le rc.xml
# d'openbox, en dur : sa bulle de changement de bureau (Win+Ctrl+←) l'affichait
# ainsi dans toutes les langues, quand la vue des tâches dit « Bureau 1 ».
# winlux-native-config, lancé avant openbox, donne aux noms livrés ou
# génériques le nom « Bureau N » de la langue de session ; un nom choisi reste.
import os as _os_3351
import subprocess as _sp_3351
import tempfile as _tmp_3351
import xml.etree.ElementTree as _et_3351
_ns_3351 = "{http://openbox.org/3.4/rc}"


def _noms_3351(home):
    root = _et_3351.parse(home / ".config/openbox/rc.xml").getroot()
    return [item.text for item in root.find(f"{_ns_3351}desktops/{_ns_3351}names")]


def _config_3351(home, lang):
    env = {**_os_3351.environ, "HOME": str(home), "WINLUX_NATIVE_SOURCE": str(NATIVE), "WINLUX_LANG": lang}
    _sp_3351.run([sys.executable, "-B", str(SCRIPTS / "winlux-native-config.py")], env=env, check=True,
                 stdout=_sp_3351.DEVNULL, stderr=_sp_3351.DEVNULL)


_vus_3351 = {}
with _tmp_3351.TemporaryDirectory() as _dossier_3351:
    _home_3351 = Path(_dossier_3351)
    for _lang_3351 in ("fr", "en", "es", "de", "fr"):
        _config_3351(_home_3351, _lang_3351)
        _vus_3351[_lang_3351] = _noms_3351(_home_3351)
print(f"  noms du premier bureau : {_vus_3351}")
ok(_vus_3351 == {"fr": ["Bureau 1"], "en": ["Desktop 1"], "es": ["Escritorio 1"], "de": ["Desktop 1"]},
   "openbox : le premier bureau s'appelle « Bureau 1 » dans la langue de session, plus « Bureau Winlux » (#3351)")
with _tmp_3351.TemporaryDirectory() as _dossier_3351:
    _home_3351 = Path(_dossier_3351)
    _config_3351(_home_3351, "fr")
    _rc_3351 = _home_3351 / ".config/openbox/rc.xml"
    _rc_3351.write_text(_rc_3351.read_text(encoding="utf-8").replace(">Bureau 1<", ">Travail<").replace(">Bureau Winlux<", ">Travail<"), encoding="utf-8")
    _config_3351(_home_3351, "en")
    _garde_3351 = _noms_3351(_home_3351)
ok(_garde_3351 == ["Travail"], "openbox : un nom de bureau choisi n'est pas renommé au changement de langue (#3351)")
# Seul le nom change : le rc.xml du compte garde ses commentaires et sa mise
# en forme (une réécriture par ElementTree les perdait).
with _tmp_3351.TemporaryDirectory() as _dossier_3351:
    _home_3351 = Path(_dossier_3351)
    _config_3351(_home_3351, "en")
    _livre_3351 = (NATIVE / "config/openbox/rc.xml").read_text(encoding="utf-8").splitlines()
    _compte_3351 = (_home_3351 / ".config/openbox/rc.xml").read_text(encoding="utf-8").splitlines()
_ecarts_3351 = [(a, b) for a, b in zip(_livre_3351, _compte_3351) if a != b]
ok(len(_livre_3351) == len(_compte_3351) and len(_ecarts_3351) == 1 and "Desktop 1" in _ecarts_3351[0][1],
   "openbox : seul le nom du bureau change dans rc.xml, commentaires et mise en forme gardés (#3351)")
