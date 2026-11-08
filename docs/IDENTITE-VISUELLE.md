# Identité visuelle Winlux

Winlux garde l'ergonomie de Windows 11 mais n'utilise **aucune marque
Microsoft** : ni le logo à quatre carreaux, ni ses couleurs, ni ses fonds.
Tout ce qui suit est créé dans le dépôt et versionné (issue #170).

## Logo « Prisme »

Un losange taillé en quatre facettes : violet à gauche (Linux, la nuit),
cyan à droite (la lumière, *lux*). Le centre optique est remonté à 42 % de
la hauteur pour donner un relief de gemme.

| Facette | Haut | Bas |
|---|---|---|
| Haut-gauche | `#B69CFF` | `#7C4DFF` |
| Bas-gauche | `#7C4DFF` | `#4B23C8` |
| Haut-droite | `#7DE8FF` | `#22B8F0` |
| Bas-droite | `#22B8F0` | `#125CD6` |

Accent de marque : `#7C4DFF`. Fond de nuit : `#070A1F`.

## Où il apparaît

- Menu GRUB (ISO live et système installé) : `assets/brand/grub/`.
- Écran de démarrage du bureau, bouton Démarrer, assistant de bienvenue :
  `testexe/src/shell/brand.rs` (même géométrie, dessin natif).
- Paramètres > À propos : `testexe/src/shell/about.rs`.
- Session XFCE : `integration/native/start.svg`.
- Installateur : `/usr/share/winlux/brand/winlux-logo-*.png`.

## Fonds et son

- « Winlux Aurore » (défaut, #596) : rideaux d'aurore violet → cyan
  au-dessus de collines, le logo au-dessus, étoiles la nuit ; version claire
  « Winlux Aurore claire ». Calculé par `render-brand.py` en 1920x1080,
  2560x1440 et 3840x2160 (`winlux-prisme-aurore[-clair][-LxH].png`) ; la
  session et Paramètres prennent la définition la plus proche de l'écran.
- « Winlux Nuit » et « Winlux Aube » : calculés au pixel dans
  `shell/personalize.rs`, et rendus en PNG pour la session XFCE.
- Son de démarrage « Prisme » (défaut), synthétisé comme les autres sons.
- L'ancien fond `Fond ecran.png` est retiré (#590) : son motif central, une
  fenêtre à quatre carreaux, évoquait le logo Windows. `wallpaper.bmp`
  (écran de verrouillage, shell PE) est désormais tiré de « Winlux Aurore », et
  `test-no-microsoft-branding.py` refuse tout fond qui ne sort pas de
  `scripts/render-brand.py`.

## Écran de démarrage du noyau

`winlux --boot-splash` (`runtime/src/bootsplash.rs`) dessine le même écran
que le bureau (halo violet, logo, cercle de points) directement dans le
framebuffer `/dev/fb0`, dès que l'init a monté `/dev`. Il gère les écrans
16, 24 et 32 bits, se redessine si la résolution change (pilote graphique
chargé en cours de route) et s'arrête au signal `SIGTERM`, à la création de
`/run/winlux/splash.stop` ou après 90 s. Xorg démarre ensuite avec
`-background none`, si bien que l'image reste affichée jusqu'au bureau.

Image de test sans écran : `winlux --boot-splash --ppm splash.ppm 1920 1080 500`.

## Sons système

Dix sons maison dans `assets/sounds/`, installés dans
`/usr/share/winlux/sounds/` (`WINLUX_SOUNDS_DIR` pour les tests) :
`notification`, `information`, `question`, `avertissement`, `erreur`,
`defaut`, `appareil-branche`, `appareil-retire`, `connexion`, `deconnexion`.

- Programmes Windows : `MessageBeep` et `PlaySound(..., SND_ALIAS)` jouent le
  son Winlux correspondant (`SystemAsterisk` → information, `SystemHand` →
  erreur, `DeviceConnect` → appareil-branche, etc. ; table dans
  `runtime/src/dlls/winmm.rs`).
- Notifications du bureau : `winlux-notifications` joue `notification`, ou le
  `sound-name` demandé s'il existe, sauf indice `suppress-sound`.
- Clé USB ou disque branché / retiré : `appareil-branche` / `appareil-retire`.

Régénération : `python3 scripts/render-sounds.py` (NumPy, synthèse cloche +
nappe, 22 050 Hz mono 16 bits).

## Icônes

Jeu d'icônes freedesktop « Prisme », en deux thèmes installés par le paquet
winlux-desktop : `Winlux` (clair, `integration/native/icons/Winlux`) et
`Winlux-Sombre` (sombre, hérite de `Winlux`), dans `/usr/share/icons`.
Accent `#6A3DF0` en clair, `#B69CFF` en sombre ; aucun logo ni nom tiers : le
disque système porte le Prisme, le disque Linux (Z:) une invite `>_`.

* Emplacements, périphériques et types de fichiers (texte, PDF, archives,
  audio, vidéo, images, bureautique, polices, paquets, ISO, `.exe` / `.msi`…) :
  deux dessins par icône, un petit sur grille 16 (16 à 24 px, traits alignés
  sur le pixel) et un grand (32 px et au-delà, 4K compris), plus des PNG 16,
  20, 24, 32, 48, 64, 96, 128 et 256 px et des alias en liens relatifs.
* Applications (`scalable/apps`, tuiles dégradées avec pictogramme blanc et
  logo `winlux.svg`) : dans le thème clair seulement, dont le sombre hérite.

Le thème sombre est le défaut ; la session (`winlux-user-session`,
`winlux-native-session`) et Personnalisation (`winlux-personalize`) passent de
l'un à l'autre avec le mode clair ou sombre, sans remplacer un autre thème
choisi par l'utilisateur. Le postinst reconstruit le cache GTK
(`gtk-update-icon-cache`). Le reste vient d'Adwaita (`Inherits`).

Régénération (Python 3 + GdkPixbuf/librsvg) :
`python3 scripts/generer-icones.py` (thèmes, et PNG de l'Explorateur et
planches de contrôle dans `target/icones-prisme`), puis
`python3 scripts/render-icons.py` pour les applications. Vérification :
`sh scripts/test-icones.sh integration/native/icons` (sans argument :
`/usr/share/icons` d'une machine installée).

## Régénérer les images

```sh
python3 scripts/render-brand.py
```

Le script (Python, Pillow, NumPy) produit le SVG, les PNG du logo, la
bannière, les fonds et le thème GRUB. Modifier `CY` ou les couleurs ici
impose la même modification dans `shell/brand.rs`.

## Licences

- Logo, fonds, sons, icônes, thème : créations Winlux, sous la licence du dépôt (GPL v3).
- Texte rendu dans les PNG : police Inter (SIL Open Font License 1.1), ou
  DejaVu Sans à défaut. Les images ne contiennent que des pixels, aucune police.
- Menu GRUB : `unicode.pf2` de GRUB (GNU Unifont, GPL v2+ avec exception
  pour les polices / OFL 1.1), copiée à la construction depuis le paquet `grub-common`.
