# #2868 : les fichiers publics du dépôt ne décrivent plus l'interface de Winlux
# par comparaison avec Windows 11 (AGENTS.md, « Marque et vocabulaire »).
# Une mention d'interopérabilité (client Windows 11 face au partage Samba,
# polices de Windows 11) reste permise : seules les tournures de comparaison
# sont refusées.
import re
racine = SCRIPTS.parent
publics = [racine / nom for nom in ("README.md", "Suivi.md", "TODO.md", "TRACKING.md", "INTERFACE.md",
                                    "COMPATIBILITY.md", "PROJECTS_IMPORT.md")]
publics += sorted((racine / "docs").glob("*.md")) + sorted((racine / "wiki").glob("*.md"))
publics += sorted((racine / "site/content").glob("*/*.md"))
comparaison = re.compile(r"(façon|style|à la|thème|inspirée?s? de|comme sous|comme|reprend l'\w+ de|habillée?s?) Windows 11", re.I)
juridiques = {"DEFENSE-DU-NOM.md", "RISQUES-JURIDIQUES.md", "NOM-ET-STRUCTURE-JURIDIQUE.md", "POLICES-WINDOWS.md", "PARTAGE.md"}
fautifs = [f"{chemin.relative_to(racine)}: {m.group(0)}" for chemin in publics if chemin.name not in juridiques
           for m in comparaison.finditer(chemin.read_text(encoding="utf-8"))]
ok(not fautifs, f"textes publics : aucune comparaison de l'interface avec Windows 11 (#2868) {fautifs[:5]}")
