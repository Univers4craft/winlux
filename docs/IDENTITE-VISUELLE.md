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

- « Winlux Nuit » (défaut) et « Winlux Aube » : calculés au pixel dans
  `shell/personalize.rs`, et rendus en PNG pour la session XFCE.
- Son de démarrage « Prisme » (défaut), synthétisé comme les autres sons.
- L'ancien fond `Fond ecran.png` reste proposé sous le nom « Winlux classique ».

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

Thème d'icônes freedesktop « Winlux » (`integration/native/icons/Winlux`,
installé dans `/usr/share/icons/Winlux`), choisi par défaut dans GTK et
xsettings sans écraser un choix existant. 34 icônes : tuiles d'applications
(dégradé + pictogramme blanc), dossiers ambrés avec emblème, disques,
corbeille, logo Prisme pour `start-here`, et une fenêtre marquée du logo pour
les `.exe` / `.msi`. Tout le reste vient d'Adwaita (`Inherits`).

Régénération : `python3 scripts/render-icons.py`.

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
