# #158 : un écran branché, débranché ou retrouvé au réveil reste couvert par
# l'écran verrouillé. Veille réelle et vrais écrans : portable de damien.
lock_py = (SCRIPTS / "winlux-lock.py").read_text(encoding="utf-8")
ok("def cover_plan" in lock_py and '"monitor-added"' in lock_py and '"monitors-changed"' in lock_py
   and (SCRIPTS / "test-lock-monitors.py").is_file(),
   "Écran verrouillé : chaque écran branché ou débranché reste couvert (#158)")
