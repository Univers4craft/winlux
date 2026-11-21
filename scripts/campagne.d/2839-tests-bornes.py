# #2839 (voir #2225) : une suite de tests figée échoue à la borne avec le
# code 124 et les piles de ses processus, au lieu de bloquer la CI.
import subprocess as _sp_2839
import time as _t_2839
_d_2839 = _t_2839.monotonic()
_r_2839 = _sp_2839.run(["bash", str(SCRIPTS / "tests-bornes.sh"), "bash", "-c", "sleep 60 & sleep 60"],
                       env={"PATH": "/usr/bin:/bin", "BORNE_TESTS": "2"},
                       capture_output=True, text=True, timeout=50)
_ci_2839 = (ROOT / ".github" / "workflows" / "ci.yml").read_text(encoding="utf-8")
ok(_r_2839.returncode == 124 and _t_2839.monotonic() - _d_2839 < 40
   and "Suite figée" in _r_2839.stderr
   and "scripts/tests-bornes.sh cargo test -p winlux" in _ci_2839,
   "Tests unitaires bornés : un gel échoue en 124 avec les piles (#2839)")
