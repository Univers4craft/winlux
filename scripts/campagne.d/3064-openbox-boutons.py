# #3064 : Réduire et Agrandir suivent WS_MINIMIZEBOX et WS_MAXIMIZEBOX, grâce à
# l'openbox de Winlux. Le comportement est gardé par
# scripts/test-openbox-boutons.py (openbox réel sous Xvfb, CI « Tests runtime »).
# Ici : le correctif publié, et NOTICES qui déclare openbox modifié (GPL).
patches = sorted((ROOT / "integration" / "native" / "openbox").glob("*.patch"))
ok(any("Winlux" in p.read_text(encoding="utf-8") for p in patches),
   "Correctif openbox de Winlux publié dans integration/native/openbox (#3064)")
test_3064 = (ROOT / "scripts" / "test-openbox-boutons.py").read_text(encoding="utf-8")
ok("Iconic" in test_3064 and "SW_MINIMIZE" in test_3064,
   "Test sous openbox réel : boutons selon le style, et ShowWindow(SW_MINIMIZE) réduit encore "
   "une fenêtre sans bouton Réduire (#3064)")
notices = (ROOT / "NOTICES").read_text(encoding="utf-8")
ok("Openbox modifié" in notices and "integration/native/openbox/" in notices,
   "NOTICES déclare l'openbox modifié et la source de ses correctifs (#3064)")

# Repli de la session : l'openbox de Winlux s'il démarre, sinon celui
# d'Ubuntu. Joué avec deux faux programmes.
with tempfile.TemporaryDirectory() as temp_3064:
    t = Path(temp_3064)
    ubuntu = t / "ubuntu"
    ubuntu.write_text('#!/bin/sh\necho "ubuntu $*"\n'); ubuntu.chmod(0o755)
    sain = t / "sain"
    sain.write_text('#!/bin/sh\necho "winlux $*"\n'); sain.chmod(0o755)
    casse = t / "casse"
    casse.write_text("#!/bin/sh\nexit 127\n"); casse.chmod(0o755)
    def lance_3064(winlux):
        env = dict(os.environ, WINLUX_OPENBOX_WINLUX=str(winlux), WINLUX_OPENBOX_UBUNTU=str(ubuntu))
        return subprocess.run(["sh", str(SCRIPTS / "winlux-openbox.sh"), "--sm-disable"],
                              env=env, capture_output=True, text=True).stdout.strip()
    sorties_3064 = (lance_3064(sain), lance_3064(casse), lance_3064(t / "absent"))
    print("winlux-openbox :", sorties_3064)
ok(sorties_3064 == ("winlux --sm-disable", "ubuntu --sm-disable", "ubuntu --sm-disable"),
   "Session : openbox de Winlux, ou celui d'Ubuntu s'il manque ou ne démarre pas (#3064)")
