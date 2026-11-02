# Application Jeux

Bibliothèque des jeux Windows (`.exe`) installés, issue #285 (première partie :
la bibliothèque ; le client Steam viendra après #232, #236 et #137).

Code : `integration/native/jeux/winlux-games.py` (installé dans
`/usr/share/winlux/native/jeux/`, lanceur `winlux-games.desktop`, catégorie
Jeux du menu Démarrer), `runtime/src/playtime.rs` (temps de jeu), page
Paramètres › Jeux (`page_games` de `scripts/winlux-settings-pages.py`, page 29).
Test : `scripts/test-games.py` (et `--gtk` sous Xvfb pour la fenêtre).

## Jeux détectés (sans jamais exécuter un fichier)

| Source | Comment |
|---|---|
| GOG | `goggame-*.info` laissé par l'installateur : nom et tâche principale |
| itch.io | dossier contenant `.itch/receipt.json.gz` : titre du jeu |
| Dossiers de jeux | chaque sous-dossier de `C:\Games`, `C:\Jeux`, `C:\Juegos`, `C:\Spiele`, `C:\GOG Games`, GOG Galaxy, `C:\Program Files`, `AppData\Local\Programs`, et des dossiers ajoutés dans Paramètres |
| Programmes installés | registre Uninstall, raccourcis `.lnk` et programmes déjà lancés (ceux de `winlux-applications`) |
| Ajout à la main | bouton « Ajouter un jeu » |

Un `.exe` compte comme jeu s'il importe Direct3D, DXGI, OpenGL, Vulkan ou
XInput : le même critère que le mode jeu du runtime (`cpu::is_heavy`). Les
outils livrés avec les jeux (désinstalleurs, redistribuables, rapports de
plantage, anti-triche…) sont écartés. Dans un dossier, le `.exe` retenu est
le moins profond puis le plus gros. Un jeu 32 bits est affiché mais ne peut
pas encore être lancé (#232).

## Lancement, performances et temps de jeu

« Jouer » passe par `winlux-open`, comme un double-clic : choix du GPU
(Paramètres › Applications › Programmes Windows), mode jeu et overlay de
performances (Paramètres › Système › Processeur et Graphismes) s'appliquent
sans rien de plus. Aucune seconde page de réglages graphiques.

Le temps de jeu est compté par le runtime pour tout lancement (Jeux, menu
Démarrer, double-clic) : une séance
`~/.local/share/winlux/jeux/sessions/<pid>-<début>.session` est rafraîchie
toutes les 30 s ; l'application additionne les séances finies dans
`totals.tsv`. Rien ne quitte le PC.

## Réglages

`~/.config/winlux/jeux.conf`, lignes `CLÉ=valeur` jamais exécutées :
`PLAYTIME=0|1`, `AUTOSCAN=0|1`, `FOLDER=/dossier`, `HIDDEN=/jeu.exe` (retiré de
la bibliothèque, « Remettre » dans Paramètres), `ADDED=/jeu.exe`.

## Raccourcis

Menu « … » ou clic droit sur un jeu : épingler au menu Démarrer (réutilise le
lanceur Winlux du programme s'il existe, sinon crée
`winlux-game-<id>.desktop`, catégorie Jeux), créer un raccourci sur le bureau
(marqué fiable pour xfdesktop), ouvrir l'emplacement, retirer de la
bibliothèque.

Ligne de commande : `winlux-games.py --scan | --list | --playtime | --add JEU.exe | --forget-playtime`.

Voir aussi [VRAIS-JEUX.md](VRAIS-JEUX.md) : vrais jeux Windows lancés sous Winlux et leurs manques.
