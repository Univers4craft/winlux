# Revue des ajouts graphiques et multimédia — PR #1

Retenus : transfert et redimensionnement DIB non compressés (1/4/8/16/24/32 bits),
exports Media Foundation signalant explicitement les opérations non implémentées,
alias `mfreadwrite` et messages d'erreur DirectShow (`quartz`).

Corrections avant intégration : palette lue dans BITMAPINFO et non dans les pixels,
dimensions et compression contrôlées, débordements de coordonnées refusés,
retournement des destinations négatives et refus des opérations raster inconnues.
Le transfert par bandes partielles et DIB_PAL_COLORS restent non pris en charge.

Reportés après revue, sans effacer les commits d'origine :

- Décodage AAC : structure `NeAACDecFrameInfo` incomplète (notamment le tableau
  `channel_position[64]`), susceptible de déborder la mémoire passée à libfaad.
  La configuration FFI doit aussi être revue sur les architectures ciblées.
- Décodage MP3 : cycle global init/exit non synchronisé et aucune borne sur le PCM
  décodé ; reste à tester les changements de format et les fichiers malformés.
- Faux FilterGraph : une seule vtable renvoyée pour IGraphBuilder, IMediaControl
  et IMediaEvent, alors que leurs dispositions binaires diffèrent ; IID ignoré
  pendant la création. Aucune lecture vidéo n'est apportée par cette PR.
- Énumération Unicode des polices : callback W appelé avec des structures ANSI,
  et pointeur sur une structure locale déjà sortie de portée.
- Décorations X11 : barre peinte dans la zone cliente, sans décalage des événements
  ni gestion du bouton fermer ; risque de masquer le contenu des applications.

Ces limites ne permettent pas d'annoncer une compatibilité multimédia complète.

## PR #3

Les quatre glyphes utilisent le moteur de dessin existant, sans nouvelle dépendance.
Les changements sont limités au dessin des icônes ; aucune application n'est ajoutée.
La validation visuelle complète en VM reste une étape distincte.
