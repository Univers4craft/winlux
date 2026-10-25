# Feuille de route — Winlux sur SSD et usage quotidien

Document de suivi créé le 25 octobre 2026 à partir des fonctionnalités
demandées. Les cases ci-dessous représentent des travaux à réaliser ou à
valider, pas des fonctionnalités promises comme déjà opérationnelles.

L'ISO actuelle est une image Live de test. L'installateur autonome sur SSD,
le dual boot et la compatibilité générale du matériel restent à construire.
Ne pas utiliser le disque principal comme premier terrain d'essai.

## Ordre de réalisation proposé

1. Bloc-notes et intégration avec la VM.
2. Installation sur un disque virtuel vierge, puis démarrage sans ISO.
3. Récupération et mises à jour renforcées.
4. Essais sur un SSD externe dédié et validation du matériel.
5. Installation sur SSD interne dédié.
6. Dual boot et manipulation des partitions existantes, après validation.

## 1. Installation sur SSD — priorité essentielle

- [ ] Assistant graphique d'installation en français.
- [ ] Choix entre essayer Winlux sans installer et lancer l'installation.
- [ ] Détection des SSD SATA, NVMe, disques USB et disques virtuels.
- [ ] Affichage du modèle, de la capacité et des partitions de chaque disque.
- [ ] Identification et protection du support contenant l'installateur.
- [ ] Installation sur un disque entier dédié.
- [ ] Installation dans un espace libre, après validation du partitionnement.
- [ ] Partitionnement manuel pour les utilisateurs avancés.
- [ ] Aperçu de toutes les modifications avant validation.
- [ ] Confirmation explicite avant tout effacement.
- [ ] Vérification de l'espace disponible et de l'état du disque.
- [ ] Partitionnement GPT et partition de démarrage EFI.
- [ ] Chargeur de démarrage autonome, indépendant de l'ISO.
- [ ] Prise en charge UEFI et compatibilité BIOS lorsque nécessaire.
- [ ] Choix du système de fichiers.
- [ ] Activation du TRIM pour les SSD et configurations compatibles.
- [ ] Chiffrement facultatif du système et des données.
- [ ] Création du compte utilisateur et du mot de passe.
- [ ] Choix de la langue, du clavier et du fuseau horaire.
- [ ] Installation hors ligne.
- [ ] Mises à jour facultatives pendant l'installation si Internet est disponible.
- [ ] Progression détaillée et journal d'installation.
- [ ] Vérification du système installé avant de proposer le redémarrage.
- [ ] Demande de retirer la clé USB ou l'ISO après installation.

## 2. Coexistence avec Windows et Linux

À commencer après la validation de l'installation sur disque dédié.

- [ ] Détection des systèmes déjà installés.
- [ ] Installation à côté de Windows ou d'une distribution Linux.
- [ ] Menu de démarrage permettant de choisir le système.
- [ ] Conservation des partitions de données existantes.
- [ ] Avertissements concernant BitLocker, le démarrage rapide et les
  partitions Windows hibernées.
- [ ] Gestion prudente des partitions EFI existantes.
- [ ] Procédure de réparation du démarrage.
- [ ] Procédure de désinstallation de Winlux sans supprimer les autres systèmes.
- [ ] Redimensionnement des partitions uniquement après sauvegarde et
  validation des systèmes de fichiers concernés.

## 3. Sécurité et récupération

- [ ] Bureau exécuté avec un compte non administrateur.
- [ ] Autorisation ciblée pour les opérations sensibles.
- [ ] Verrouillage de session et changement d'utilisateur.
- [ ] Pare-feu configurable.
- [ ] Signature cryptographique des mises à jour.
- [ ] Système A/B conservant un système démarrable si la nouvelle version échoue.
- [ ] Mode de récupération accessible au démarrage.
- [ ] Réparation du chargeur de démarrage.
- [ ] Vérification et réparation des systèmes de fichiers.
- [ ] Sauvegarde et restauration des documents et réglages.
- [ ] Rapports d'erreur exportables sans mots de passe ni tokens.
- [ ] Étude et validation d'une chaîne de démarrage compatible Secure Boot.

Les protections d'arrêt de l'OS invité ne peuvent pas empêcher une destruction
forcée de la VM ou une coupure de courant de l'hôte. La récupération actuelle
ne doit pas être assimilée à une garantie A/B contre toute interruption.

## 4. Applications de base

- [ ] Bloc-notes avec ouverture des `.txt`, modification et enregistrement.
- [ ] Terminal utilisable depuis le bureau.
- [ ] Navigateur réellement installé et accessible.
- [ ] Calculatrice.
- [ ] Visionneuse d'images.
- [ ] Lecteur audio et vidéo.
- [ ] Capture d'écran.
- [ ] Gestionnaire d'archives.
- [ ] Moniteur système : processus, mémoire, processeur et stockage.
- [ ] Recherche d'applications et de fichiers.
- [ ] Associations de fichiers modifiables.

## 5. Installation et désinstallation des applications

- [ ] Catalogue des applications Linux disponibles.
- [ ] Inventaire des applications effectivement installées.
- [ ] Gestion des dépendances et des mises à jour applicatives.
- [ ] Désinstallation avec choix de conserver ou supprimer les réglages.
- [ ] Suivi des installateurs et désinstallateurs Windows connus.
- [ ] Fiche de compatibilité par logiciel et par version.
- [ ] Diagnostic des composants Windows manquants.
- [ ] Environnements séparés lorsque nécessaire.
- [ ] Historique des installations et des erreurs.
- [ ] Vérification qu'aucune icône ne représente une application inexistante.

## 6. Compatibilité Windows

- [ ] Tests avec de vrais logiciels en complément des exécutables de validation.
- [ ] Renforcement des API de fichiers, processus, fenêtres et du registre.
- [ ] Chemins Unicode complets.
- [ ] Prise en charge des installateurs Windows ciblés.
- [ ] Amélioration du texte, des polices et de l'impression.
- [ ] Compatibilité C++ et gestion des exceptions.
- [ ] Travail dédié à .NET, WinForms et WPF.
- [ ] Travail dédié aux API graphiques et à l'accélération GPU.
- [ ] Rapports précisant ce qui fonctionne et ce qui bloque.
- [ ] Choix d'une version précise de Photoshop pour une étude de compatibilité.

La présence de composants ou de points d'entrée ne garantit pas le
fonctionnement d'une application. .NET graphique et Photoshop restent des
objectifs à mesurer, pas des compatibilités générales acquises.

## 7. Matériel et ordinateur portable

- [ ] Validation de la détection du stockage, des périphériques USB et des écrans.
- [ ] Pilotes et firmwares redistribuables nécessaires au matériel ciblé.
- [ ] Gestion Ethernet, Wi-Fi et Bluetooth sur matériel réel.
- [ ] Son, microphone et sélection des sorties.
- [ ] Batterie réelle, chargeur et autonomie.
- [ ] Réglage de la luminosité.
- [ ] Gestion du capot et des touches multimédias.
- [ ] Trackpad : toucher pour cliquer, défilement et gestes compatibles.
- [ ] Veille et reprise.
- [ ] Hibernation lorsque le matériel et le stockage la permettent.
- [ ] Profils d'alimentation et suivi thermique.
- [ ] Webcam.
- [ ] Imprimantes et scanners.
- [ ] Manettes.
- [ ] Gestion de plusieurs écrans.

## 8. Bureau et paramètres

Les premiers réglages d'écran et les cartes de paramètres ont été ajoutés en
`v0.2.0-preview.4`. Les points ci-dessous sont les améliorations et validations
à poursuivre, notamment sur matériel réel et à basse résolution.

- [ ] Poursuivre l'harmonisation et la structuration des pages de paramètres.
- [ ] Liste déroulante des résolutions en remplacement de la sélection précédente/suivante.
- [ ] Confirmation des changements d'affichage avec retour automatique après 15 secondes.
- [ ] Fréquence, orientation et échelle par écran.
- [ ] Fonds d'écran fixes ou animés avec réglage des performances.
- [ ] Bureau vide par défaut et raccourcis facultatifs.
- [ ] Conservation des positions des icônes.
- [ ] Épinglage et désépinglage des applications.
- [ ] Recherche dans Démarrer.
- [ ] Notifications interactives et réglables.
- [ ] Accessibilité : taille du texte, contraste et navigation clavier.
- [ ] Réglages de date, heure et synchronisation.
- [ ] Gestion des disques et périphériques depuis les paramètres.
- [ ] Validation des modes fixe/adaptatif et du suivi d'installation sur les
  hyperviseurs et configurations d'affichage ciblés.

## 9. Confort dans la VM

- [ ] Copier-coller entre l'hôte et Winlux.
- [ ] Partage de dossiers.
- [ ] Intégration SPICE adaptée.
- [ ] Validation du redimensionnement et du plein écran.
- [ ] Synchronisation de l'heure.
- [ ] Procédure simple de sauvegarde et restauration d'une VM de test.
- [ ] Tests automatisés clavier, souris et trackpad.

## 10. Validation avant installation sur un SSD réel

- [ ] Installation automatisée sur un disque virtuel vierge.
- [ ] Démarrage sans ISO après installation.
- [ ] Conservation des fichiers après plusieurs redémarrages.
- [ ] Tests de mise à jour, restauration et échec de démarrage sur le système installé.
- [ ] Tests d'installation interrompue sur des disques jetables.
- [ ] Vérification que les disques non sélectionnés ne sont jamais modifiés.
- [ ] Tests UEFI et, si retenu, BIOS.
- [ ] Tests sur SSD externe dédié ou ordinateur de test.
- [ ] Validation du stockage, réseau, audio, affichage et périphériques du portable.
- [ ] Sauvegarde externe vérifiée avant toute installation sur le disque principal.

## 11. Fluidité des fenêtres — priorité ergonomique

Diagnostiquer et mesurer avant de choisir les optimisations. Les pistes
ci-dessous ne constituent pas encore une cause confirmée des ralentissements.
Prioriser le déplacement et le redimensionnement avant les animations décoratives.

- [ ] Mesurer le temps de rendu, la latence des entrées et les saccades pendant
  le déplacement et le redimensionnement des fenêtres.
- [ ] Regrouper les événements de mouvement de souris pour utiliser la position
  la plus récente sans accumuler de retard ni perdre les clics.
- [ ] Éviter de redessiner tout le bureau à chaque mouvement ; limiter le travail
  aux zones réellement modifiées lorsque le backend le permet.
- [ ] Mettre en cache le contenu des fenêtres et ne recalculer que ce qui change.
- [ ] Séparer le déplacement d'une fenêtre du rendu de son contenu : déplacer
  sa surface plutôt que reconstruire chaque élément.
- [ ] Éviter les lectures de fichiers et commandes système dans le chemin de dessin.
- [ ] Viser une cadence régulière adaptée à l'écran, avec limitation du travail
  inutile lorsque l'interface est immobile.
- [ ] Tester déplacement et redimensionnement en 1080p, 1440p et 4K.
- [ ] Comparer les mesures avant/après sur la même configuration et vérifier
  l'absence de scintillement, de traces de fenêtres et de régressions d'entrée.

## 12. Compléments pour l'usage quotidien

Ces points précisent ou complètent les rubriques précédentes.

- [ ] Gestionnaire des tâches permettant de voir et arrêter une application bloquée.
- [ ] Sauvegarde automatique des documents compatibles et récupération après plantage.
- [ ] Recherche globale dans les applications, fichiers et paramètres.
- [ ] Montage et éjection des clés USB avec notifications et gestion des erreurs.
- [ ] Détection du manque d'espace disque, notamment avant une installation
  ou une mise à jour, avec indication de l'espace requis.
- [ ] Raccourcis cohérents : Alt+Tab, Alt+F4, capture d'écran et verrouillage.
- [ ] Accessibilité : navigation clavier, taille du pointeur et contraste.
- [ ] Diagnostic intégré exportable, avec suppression des tokens, mots de
  passe et autres informations sensibles des rapports.

## 13. Fonctionnalités distinctives de Winlux

- [ ] Assistant de compatibilité `.exe` : analyser un logiciel avant son
  lancement, expliquer les blocages observés et proposer uniquement les
  composants réellement pris en charge. Distinguer « fonctionne »,
  « partiellement compatible », « bloqué » et « non testé », avec version du
  logiciel, configuration et résultats vérifiables.
- [ ] Mode d'essai isolé pour les applications, avec limites d'accès aux
  fichiers. Vérifier réellement l'isolation avant de la présenter comme
  une protection de sécurité.
- [ ] Sauvegarde avant installation permettant d'annuler les changements
  suivis d'un logiciel sans perdre les documents personnels ; documenter
  les modifications qui ne peuvent pas être restaurées.
- [ ] Historique des fichiers pour retrouver une version précédente d'un
  document modifié ou supprimé, avec réglage de l'espace utilisé.
- [ ] Assistant de migration depuis Windows ou Linux : documents, favoris
  et réglages compatibles, avec aperçu avant copie et sans modifier la source.
- [ ] Profils Portable, Bureau et Jeu regroupant énergie, affichage et
  notifications, sans promesse de gains de performances non mesurés.
- [ ] Panneau de confidentialité pour les accès au microphone, à la caméra
  et aux dossiers, avec permissions effectivement appliquées et indication
  claire des limites de contrôle des applications natives et Windows.
- [ ] Mode dépannage guidé pour les applications et périphériques, avec
  explications et rapport partageable nettoyé des informations sensibles.
- [ ] Première configuration guidée : clavier, son, réseau, affichage et
  mises à jour vérifiés au premier démarrage.
- [ ] Mode concentration : suspendre les notifications non essentielles et
  différer les installations automatiques, sans interrompre une transaction
  de mise à jour déjà engagée.

Priorité distinctive proposée : l'assistant de compatibilité `.exe`, fondé
sur des résultats mesurés plutôt que sur la seule présence d'API ou de composants.

## Suivi

Cocher une fonctionnalité uniquement après validation réelle. Documenter le
matériel, l'hyperviseur ou le logiciel testé, les limites restantes et la
version de Winlux concernée. Compléter cette liste avec les décisions prises
pendant l'implémentation plutôt que de présenter des objectifs comme acquis.

Voir également [TODO.md](TODO.md), [INTERFACE.md](INTERFACE.md) et
[COMPATIBILITY.md](COMPATIBILITY.md).
