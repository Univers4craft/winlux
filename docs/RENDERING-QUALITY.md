# Diagnostic du rendu et de Bonjour.exe — 27 octobre 2026

## Rendu : ce qui a été vérifié

La configuration de vm-winlux annonce une sortie virtio 1920×1080 et une
connexion SPICE sans compression d'image. Les captures antérieures du bureau
ont cette définition native. Le facteur de zoom du client hôte n'a pas été
mesuré : il peut ajouter une seconde transformation à celle du système invité.
À la demande de l'utilisateur, la comparaison visuelle repart des captures
existantes ; la capture directe au début de l'audit était noire.

Les défauts identifiés dans Winlux étaient réels :

- StretchBlt redimensionnait les bitmaps au plus proche voisin avant le rendu
  X11. Le filtre bilinéaire ultérieur ne pouvait pas effacer ce crénelage.
- Les diagonales des pictogrammes étaient constituées de petits carrés ; les
  ellipses X11 de base n'avaient pas d'anticrénelage.
- Le shell demandait Segoe UI, absente des polices distribuées, alors que
  DejaVu Sans est copiée dans l'ISO. La sélection explicite rend les mesures
  et les captures cohérentes, sans garantir une police identique à Windows.
- La limite des bitmaps de 4 Mégapixels était inférieure à un écran 4K à 100 %.
- Le fond source mesure 1672×941 : l'agrandir ne restitue pas de détails absents.

Le correctif utilise le filtrage bilinéaire dès le redimensionnement GDI,
un cache pour éviter de le recalculer sur une image inchangée et Cairo pour
les contours. Les rectangles alignés restent nets sur les limites de pixels.
La bibliothèque reste chargée jusqu'à la fermeture Xlib pour préserver les
callbacks enregistrés par Cairo ; les surfaces sont libérées avant l'affichage.

## Référence Linux Mint / Cinnamon

Le [code officiel des libellés Cinnamon](https://github.com/linuxmint/cinnamon/blob/master/src/st/st-button.c)
utilise des objets ClutterText, avec mise en page Pango ; ses
[réglages d'interface](https://github.com/linuxmint/cinnamon/blob/master/src/st/st-settings.c)
suivent la police et le thème d'icônes de Cinnamon. Cela sert de référence
technique pour des polices mises en forme à leur taille native.
Winlux conserve sa propre pile Win32/Xft/Cairo et ses dessins actuels ; ce
correctif ne constitue pas une égalité visuelle ou fonctionnelle avec Mint.
Aucun benchmark avec une VM Mint identique n'est revendiqué.

## Validation

- Tests X11 : des pixels intermédiaires sont présents sur les bords des
  ellipses et diagonales, et pas seulement des pixels noirs/blancs.
- Test du redimensionnement : interpolation intermédiaire et bitmap 3840×2160.
- Captures du vrai shell via Xvfb en 1080p à 100 % et 4K à 200 %, sans
  retouche ; elles sont à examiner à leur taille native.
- Tests PE et lanceur, y compris le cas d'une console héritée du bureau.

## Bonjour.exe de la VM

La VM était arrêtée lors de l'examen de son disque. Une copie privée en lecture
seule a permis de lire `/root/Téléchargements/Bonjour.exe`, sans modifier le
disque d'origine. Cette copie complète temporaire a ensuite été supprimée.

- Version installée constatée : v0.2.0-preview.11.
- SHA-256 de Bonjour.exe :
  `452ae6173df34975578eb7d180849277075e727b00df7798c8b69925bfe8be50`.
- PE32+ console x86-64, imports WriteFile, GetStdHandle, ExitProcess et printf.
- Exécution réelle avec Winlux : message Bonjour et ligne printf, code de fin 0.
- Aucun terminal xterm/gnome-terminal installé dans la VM ; xmessage est présent.
- Le lanceur pouvait interpréter stdout attachée à /dev/console comme un terminal
  utilisateur, puis afficher la sortie derrière le bureau. Le marqueur
  WINLUX_DESKTOP_LAUNCH transmis par ShellExecute distingue maintenant ce cas.
- La reproduction avec un pseudo-terminal vérifie que la sortie est conservée
  malgré isatty(stdout), sans forcer l'utilisateur à créer un profil .exe.
