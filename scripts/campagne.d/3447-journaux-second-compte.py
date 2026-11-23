# À l'échec du test du second compte, le diagnostic lit les journaux là où
# winlux-sessions les écrit : session et Xorg dans /run/winlux/session-logs
# (dossier à root, #633), pas dans /run/winlux même (#3447).
_sessions_3447 = (SCRIPTS / "winlux-sessions.sh").read_text(encoding="utf-8")
_guest_3447 = (SCRIPTS / "test-native-session-guest.py").read_text(encoding="utf-8")
ok('own_dir "$run/session-logs"' in _sessions_3447 and 'log="$logs/session-$user.log"' in _sessions_3447
   and 'xlog="$logs/Xorg.$n.log"' in _sessions_3447
   and 'Path("/run/winlux/session-logs")' in _guest_3447
   and 'Path("/run/winlux").glob("session-invite.log")' not in _guest_3447,
   "test du second compte : journaux de session lus dans /run/winlux/session-logs (#3447)")
