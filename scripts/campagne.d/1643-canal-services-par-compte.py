# Services Windows : canal de contrôle dans un dossier par compte, jamais un dossier commun ouvert à tous (#1643).
import re
svc = (ROOT / "runtime/src/dlls/advapi32_services.rs").read_text(encoding="utf-8")
ctl = (ROOT / "scripts/winlux-systemctl.sh").read_text(encoding="utf-8")
ok('format!("{prefix}-{euid}")' in svc and "crate::journal::ensure(&dir, euid)" in svc,
   "Services : canal de contrôle dans services-<uid>, dossier 0700 vérifié au compte (#1643)")
ok(not re.search(r"chmod\s+[0-7]*7[0-7]*\s+\S*RUN_DIR|chmod\s+[0-7]*7\s+/run/winlux/services", ctl),
   "Services : /run/winlux/services n'est pas ouvert en écriture aux autres comptes (#1643)")
