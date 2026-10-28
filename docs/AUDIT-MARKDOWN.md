# Audit des listes et affirmations Markdown — 25 octobre 2026

## Périmètre et méthode

Audit des **16 documents Markdown suivis par Git** présents avant ce rapport,
sur la base du code `e368189` et des validations déjà disponibles. Les copies
générées de `build/`, les dépendances de `target/` et le fichier local d'instructions
`AGENTS.md` ne sont pas des listes fonctionnelles à cocher.

Une case cochée décrit un sous-ensemble effectivement réalisé, avec son contexte.
Une tâche contenant plusieurs exigences reste ouverte si une seule manque ;
des sous-tâches cochées rendent alors visible la partie terminée. Le matériel
réel, un script de test disponible, une simulation et un résultat obtenu ne sont
pas équivalents. Cet audit documentaire ne relance pas toute la matrice de tests
et ne certifie pas tous les usages ou logiciels.

## Documents examinés

| Documents | Décision |
| --- | --- |
| [TODO](../TODO.md), [ROADMAP-SSD](../ROADMAP-SSD.md), [INTERFACE](../INTERFACE.md) | Cases et sous-tâches réconciliées avec les preuves ; objectifs complets laissés ouverts. |
| [README](../README.md), [COMPATIBILITY](../COMPATIBILITY.md), [thème](../theme/README.md) | Descriptions actuelles corrigées, sans présenter le prototype comme un bureau complet. |
| [BUILD](BUILD.md), [INSTALLER-SAFETY](INSTALLER-SAFETY.md) | Prérequis non vérifiés et distinction des variantes de test clarifiés. |
| [AB_UPDATES](AB_UPDATES.md) | Fonctions transactionnelles existantes cochées ; signatures et véritable A/B laissés ouverts. |
| [AUDIO-CODECS](AUDIO-CODECS.md), [fixtures audio](../runtime/tests/audio/README.md) | Décodage, limites et tests réels cohérents avec le code ; aucune compatibilité vidéo ou audio physique ajoutée par cet audit. |
| [Revue runtime](../REVIEW-INTEGRATION.md), [revue graphique](../REVIEW-GRAPHICS.md) | Historique des décisions conservé ; codecs repris ensuite documentés séparément. |
| Notes [preview.2](releases/v0.2.0-preview.2.md), [preview.3](releases/v0.2.0-preview.3.md), [preview.4](releases/v0.2.0-preview.4.md) | Historique conservé : ne pas attribuer aux anciennes releases les ajouts récents de master. |

## Principales preuves

| Réalisé | Code et validation |
| --- | --- |
| Installation complète hors ligne sur disque virtuel, GPT/BIOS/ESP, confirmation et refus des cibles dangereuses | `scripts/install.sh`, `install-safety.sh`, `test-install-safety.sh` et `test-install-vm-guest.sh`. |
| Disque installé démarrant sans ISO en BIOS **et** UEFI | Journaux locaux `build/install-guest.UY7CCe/install.log`, `bios.log`, `uefi.log` : installation et marqueurs PE séquentiels/parallèles, X11 et résultat global réussis. Ce ne sont pas des essais SSD physiques. |
| ISO complète et paquet système | Construction locale et `build-update-bundle.sh --system` réussis ; BUILD-INFO indique `e368189`, sources suivies propres. Aucun nouveau livrable publié pendant ces tests. |
| MP3/AAC | `winmm_codecs.rs`, tests de vrais fichiers mono/stéréo, bornes PCM, concurrence et PlaySoundA/W vers ALSA null ; étapes obligatoires de CI. |
| Fonctions CRT et COM | `crt_stdio.rs` : fgetpos/fsetpos testés ; `ole32.rs` : registre de fabriques, cookies stables et révocation réentrante testés. Pas d'automatisation COM complète. |
| Graphismes | `gdi32_ext.rs` : tests DIB ; `gui_x11.rs` : commandes conservées et double tampon pixmap. Pas de cache indépendant des fenêtres internes du shell ni de benchmark de fluidité. |
| Bureau actuel | `wm.rs` : deux fenêtres internes ; `desktop.rs` : trois icônes statiques, positions en mémoire recalées sur grille, épingles fixes et recherche Démarrer non fonctionnelle. |
| Paramètres et traçabilité | `settings.rs`, `winlux-control.sh`, `build-info.sh` : navigation, commandes réelles, thème/alignement persistants et À propos. |
| Qualité | [CI de référence verte](https://github.com/Univers4craft/winlux/actions/runs/40220508989), avec hygiène, tests unitaires/audio, PE séquentiels/parallèles, intégration et MinGW. |
| Livrables déjà publiés | La release preview.4 contient `winlux-live-test.iso`, `SHA256SUMS`, le paquet système, le manifeste et les notes ; elle reste antérieure aux derniers ajouts. |

## Cases retirées ou laissées ouvertes

- **Bureau non privilégié et autorisations ciblées** : le script Xsession existe,
  mais l'init ISO lance encore `/usr/bin/winlux` en root. Aucun mécanisme complet
  d'autorisation ciblée n'a été établi par cet audit.
- **Tests sur l'hôte et GNOME/KDE/XFCE** : les scripts sont présents, mais la
  présence d'un script ou d'un test avec commandes simulées ne prouve pas une
  installation système et des clics réels dans ces trois bureaux.
- **Matrice visuelle courante 1080p/1440p/4K et accessibilité complète** : ne pas
  déduire sa réussite de la compilation, d'une échelle DPI ou d'un test X11 général.
- **Applications et personnalisation** : pas de catalogue dynamique, vraie
  recherche Démarrer, épinglage utilisateur, positions persistantes, bureau vide
  configurable ou gestionnaire unifié APT/Flatpak/EXE/MSI. Terminal/navigateur
  restent dépendants des programmes effectivement embarqués/installés.
- **Matériel réel** : sélection des pilotes de l'ISO encore orientée VM ;
  AHCI/NVMe, GPU physiques, Wi-Fi et trackpads I²C ne sont pas validés ou inclus
  comme une prise en charge générale. Aucun SSD réel ni Secure Boot validé.
- **Compatibilité et sécurité** : C++/SEH, déchargement sûr des DLL, CLR Windows,
  WPF/WinForms, Photoshop, vidéo, signatures et A/B demeurent incomplets.
- **Notices et publications futures** : un fichier NOTICES et un workflow ne
  prouvent ni l'inclusion exhaustive des mentions ni une publication future réussie.

Les tâches non cochées ne signifient pas nécessairement « aucun code » : elles
peuvent être partielles, conditionnelles, sans preuve de validation ou rester une
cible plus large. Aucun code fonctionnel ni numéro de version n'est modifié ici.
