# #3198 : réveil par _NET_CLIENT_LIST au lieu du sondage de 5 s, et pixels
# d'une icône déjà décodée servis sans glycin ni fork.
_r_3198 = subprocess.run([sys.executable, "-B", str(SCRIPTS / "test-icone-immediate.py")], capture_output=True, text=True, timeout=90)
print(_r_3198.stdout + _r_3198.stderr)
ok(_r_3198.returncode == 0, "catalogue : icône d'une fenêtre posée dès son apparition, cache décodé par application (#3198)")
