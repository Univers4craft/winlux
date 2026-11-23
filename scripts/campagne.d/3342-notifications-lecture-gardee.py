# #3342 : le centre met sa liste à jour en place. Une notification qui
# arrive s'ajoute en tête sans refaire les autres cartes ni décharger celles
# déjà lues, et le seul marqueur « nouvelle » (MarkSeen) ne refait rien.
# Parcours GTK réel sous Xvfb, défilement compris :
# scripts/test-notification-lecture.py (test-all.d/3342).
import json as _json_3342
import os as _os_3342
import tempfile as _tempfile_3342
_etat_3342 = Path(_tempfile_3342.mkdtemp(prefix="campagne-3342-"))
(_etat_3342 / "winlux").mkdir()
_fichier_3342 = _etat_3342 / "winlux/notifications.json"
_hist_3342 = [dict(id=n, app="a", title=f"n{n}", body="", actions=[], time=0, nouvelle=True) for n in range(1, 201)]
_fichier_3342.write_text(_json_3342.dumps(_hist_3342))
_env_3342 = dict(_os_3342.environ)
try:
    _os_3342.environ.update(XDG_STATE_HOME=str(_etat_3342), WINLUX_LANG="fr")
    _popup_3342 = runpy.run_path(str(SCRIPTS / "winlux-popup.py"), run_name="campagne_3342")
finally:
    _os_3342.environ.clear()
    _os_3342.environ.update(_env_3342)


class _Boite3342:
    def __init__(self): self.enfants = []
    def get_children(self): return list(self.enfants)
    def remove(self, enfant): self.enfants.remove(enfant)
    def pack_start(self, enfant, *_): self.enfants.append(enfant)
    def reorder_child(self, enfant, rang): self.enfants.remove(enfant); self.enfants.insert(rang, enfant)
    def get_vadjustment(self): return self
    def get_value(self): return 0
    def __getattr__(self, _nom): return lambda *a, **k: None


class _Carte3342(list):
    pass


_centre_3342 = _popup_3342["Popup"].__new__(_popup_3342["Popup"])
_centre_3342.__dict__.update(cards=_Boite3342(), dnd=_Boite3342(), clear=_Boite3342(), window=None, snapshot=None,
                             updating=False, pending=[], shown={}, reading=None, empty=_Boite3342(), scroll=_Boite3342())
_centre_3342.notification_card = lambda item: _Carte3342([item["title"]])
_lecture_3342 = None
try:
    _centre_3342.refresh_notifications()
    while _centre_3342.more_cards(): pass
    _avant_3342 = list(_centre_3342.cards.enfants)
    for _item_3342 in _hist_3342: _item_3342.pop("nouvelle")
    _fichier_3342.write_text(_json_3342.dumps(_hist_3342))
    _centre_3342.refresh_notifications()
    _vu_3342 = all(a is b for a, b in zip(_avant_3342, _centre_3342.cards.enfants)) and len(_centre_3342.cards.enfants) == 200
    _hist_3342.append(dict(id=201, app="a", title="n201", body="", actions=[], time=0, nouvelle=True))
    _fichier_3342.write_text(_json_3342.dumps(_hist_3342))
    _centre_3342.refresh_notifications()
    _apres_3342 = _centre_3342.cards.enfants
    _lecture_3342 = (_vu_3342, len(_apres_3342), _apres_3342[0][0],
                     all(a is b for a, b in zip(_avant_3342, _apres_3342[1:])))
except Exception as _erreur_3342:  # centre d'avant #3342 : contrôle en échec, pas campagne arrêtée
    print(f"#3342 : {_erreur_3342!r}")
print(f"#3342 : MarkSeen sans carte refaite, cartes, tête, anciennes gardées = {_lecture_3342}")
ok(_lecture_3342 == (True, 201, "n201", True) and (SCRIPTS / "test-notification-lecture.py").exists(),
   "Centre de notifications : une notification qui arrive s'ajoute en tête sans refaire la liste (#3342)")
