# #3058 : le centre relit tout l'historique gardé (200), pas les 50 dernières :
# 50 cartes à l'ouverture, puis une tranche à chaque bas de défilement, des
# plus récentes à la plus ancienne. Parcours GTK réel sous Xvfb :
# scripts/test-notification-historique.py (test-all.d/3058).
import json as _json_3058
import os as _os_3058
import tempfile as _tempfile_3058
_etat_3058 = Path(_tempfile_3058.mkdtemp(prefix="campagne-3058-"))
(_etat_3058 / "winlux").mkdir()
(_etat_3058 / "winlux/notifications.json").write_text(_json_3058.dumps(
    [dict(id=n, app="a", title=f"n{n}", body="", actions=[], time=0) for n in range(1, 201)]))
_env_3058 = dict(_os_3058.environ)
try:
    _os_3058.environ.update(XDG_STATE_HOME=str(_etat_3058), WINLUX_LANG="fr")
    _popup_3058 = runpy.run_path(str(SCRIPTS / "winlux-popup.py"), run_name="campagne_3058")
finally:
    _os_3058.environ.clear()
    _os_3058.environ.update(_env_3058)
class _Boite3058:
    def __init__(self): self.enfants = []
    def get_children(self): return list(self.enfants)
    def remove(self, enfant): self.enfants.remove(enfant)
    def pack_start(self, enfant, *_): self.enfants.append(enfant)
    def __getattr__(self, _nom): return lambda *a, **k: None
_centre_3058 = _popup_3058["Popup"].__new__(_popup_3058["Popup"])
_centre_3058.__dict__.update(cards=_Boite3058(), dnd=_Boite3058(), clear=_Boite3058(), window=None,
                             snapshot=None, updating=False, pending=[])
_centre_3058.notification_card = lambda item: item["title"]
_centre_3058.label = lambda *a, **k: "vide"
_centre_3058.refresh_notifications()
_ouverture_3058 = len(_centre_3058.cards.enfants)
while getattr(_centre_3058, "more_cards", lambda: False)(): pass
_tout_3058 = _centre_3058.cards.enfants
print(f"#3058 : ouverture={_ouverture_3058} après défilement={len(_tout_3058)} "
      f"premier={_tout_3058[0]} dernier={_tout_3058[-1]}")
ok(_ouverture_3058 == 50 and _tout_3058 == [f"n{n}" for n in range(200, 0, -1)]
   and (SCRIPTS / "test-notification-historique.py").exists(),
   "Centre de notifications : les 200 gardées se relisent, 50 à l'ouverture puis au défilement (#3058)")
