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

## Régénérer les images

```sh
python3 scripts/render-brand.py
```

Le script (Python, Pillow, NumPy) produit le SVG, les PNG du logo, la
bannière, les fonds et le thème GRUB. Modifier `CY` ou les couleurs ici
impose la même modification dans `shell/brand.rs`.

## Licences

- Logo, fonds, sons, thème : créations Winlux, sous la licence du dépôt (GPL v3).
- Texte rendu dans les PNG : police Inter (SIL Open Font License 1.1), ou
  DejaVu Sans à défaut. Les images ne contiennent que des pixels, aucune police.
- Menu GRUB : `unicode.pf2` de GRUB (GNU Unifont, GPL v2+ avec exception
  pour les polices / OFL 1.1), copiée à la construction depuis le paquet `grub-common`.
