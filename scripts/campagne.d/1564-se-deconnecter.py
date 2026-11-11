# #1564 : « Se déconnecter » ferme aussi le bureau principal (lancé par
# l'init, sans PID suivi) et rouvre l'écran de connexion.
sessions_sh = (SCRIPTS / "winlux-sessions.sh").read_text(encoding="utf-8")
live = (SCRIPTS / "build-live-iso.sh").read_text(encoding="utf-8")
ok("printf 'logoff-session\\n' > \"$run/session-action\"" in sessions_sh
   and 'test "$request" = logoff-session' in live
   and "lock_at_start=1\n                    logged_off=1" in live
   and 'if test "$logged_off" = 1; then' in live
   and "#1564" in (SCRIPTS / "test-sessions.sh").read_text(encoding="utf-8"),
   "« Se déconnecter » ferme le bureau principal et rouvre l'écran de connexion (#1564)")
