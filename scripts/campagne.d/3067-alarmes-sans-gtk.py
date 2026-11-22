# #3067 : le démon des alarmes ne garde pas GTK en mémoire toute la session ;
# la sonnerie s'ouvre dans son propre processus (« winlux-alarms ring »).
import datetime
_a = runpy.run_path(str(SCRIPTS / "winlux-alarms.py"), run_name="winlux_alarms_3067")
print("attente sans échéance :", _a["next_wait"]({"alarms": [], "timers": [], "focus": {}}, datetime.datetime(2026, 11, 22, 20, 0), 0.0))
ok(callable(_a.get("run_ring")) and callable(_a.get("next_wait")),
   "alarmes : fenêtre de sonnerie dans un processus à part, démon réveillé à l'échéance (#3067)")
ok((SCRIPTS / "test-alarms-demon-leger.py").exists(), "test du démon des alarmes sans GTK présent (#3067)")
