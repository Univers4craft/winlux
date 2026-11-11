# #1779 : captures de la CI faciles à voir (index.html, 90 jours) et galerie
# « Suivi des travaux » du site limitée à une liste blanche de nos écrans.
ok("scripts/index-captures.py build/captures " in (ROOT / ".github/workflows/ci.yml").read_text(encoding="utf-8")
   and (ROOT / ".github/workflows/ci.yml").read_text(encoding="utf-8").count("retention-days: 90") >= 2
   and (SCRIPTS / "telecharger-captures.sh").is_file(),
   "Captures de la CI : index.html dans l'artefact, gardées 90 jours, script de téléchargement (#1779)")
ok(not ({l.split("\t")[0] for l in (ROOT / "site/suivi-captures.tsv").read_text(encoding="utf-8").splitlines()
         if l.strip() and not l.startswith("#")}
        & {"bienvenue", "outils-apps", "outils-tasks", "parametres-00", "parametres-01", "parametres-09",
           "parametres-10", "parametres-11", "parametres-19", "parametres-24", "parametres-34", "parametres-39"})
   and all(len(l.split("\t")) == 6 for l in (ROOT / "site/suivi-captures.tsv").read_text(encoding="utf-8").splitlines()
           if l.strip() and not l.startswith("#")),
   "Suivi des travaux : ni Bienvenue, ni À propos, ni pages décrivant le runner dans la liste blanche (#1779)")
ok("--exiger-ocr" in (ROOT / ".github/workflows/site.yml").read_text(encoding="utf-8")
   and "head_repository" in (SCRIPTS / "suivi-travaux.py").read_text(encoding="utf-8")
   and "footer_suivi" in (ROOT / "site/build.py").read_text(encoding="utf-8"),
   "Suivi des travaux : OCR obligatoire, PR de forks ignorées, lien en bas du site (#1779)")
ok("cancel-in-progress: ${{ github.event_name == 'pull_request' }}" in (ROOT / ".github/workflows/site.yml").read_text(encoding="utf-8"),
   "Site : une publication sur master n'est plus annulée par la CI suivante (#1801)")
ok('OMP_THREAD_LIMIT="1"' in (SCRIPTS / "suivi-travaux.py").read_text(encoding="utf-8")
   and "OCR impossible" in (SCRIPTS / "suivi-travaux.py").read_text(encoding="utf-8"),
   "Suivi des travaux : OCR borné, une capture illisible est refusée sans tout arrêter (#1829)")
_site = (ROOT / ".github/workflows/site.yml").read_text(encoding="utf-8")
_etape = _site.split("- name: Suivi des travaux (captures de la CI)", 1)[-1].split("- name:", 1)[0]
ok("continue-on-error" not in _etape and "galerie du suivi vide" in _etape and "for essai in 1 2" in _etape,
   "Site : sans galerie complète, rien n'est publié et la galerie en ligne reste (#1871)")
ok("suivi-galerie-precedente.py" in _etape
   and "PERMIS" in (SCRIPTS / "suivi-galerie-precedente.py").read_text(encoding="utf-8"),
   "Site : sans OCR, la dernière galerie publiée est reprise, rien de nouveau publié (#1871)")
