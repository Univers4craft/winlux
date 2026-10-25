# Interface et expérience du bureau Winlux

Ce document décrit l'état du prototype graphique et la cible d'interface à
construire. Le shell actuel vit dans `testexe/src/shell/` et sert notamment au
test graphique `gui_demo.exe` ; ce n'est pas encore un gestionnaire de bureau
autonome.

Audit du 25 octobre 2026 : voir [les preuves](docs/AUDIT-MARKDOWN.md).
Une sous-tâche cochée ne valide pas toute la cible dont elle dépend.

## Principes d'interface

- **Bureau vide à la première installation** : aucun raccourci ni icône système
  n'est affiché par défaut. En particulier, aucune icône Corbeille n'est
  imposée.
- **Applications accessibles depuis Démarrer et la barre des tâches** : leur
  installation ne crée pas automatiquement de raccourci sur le bureau.
- **Raccourci de bureau volontaire** : l'utilisateur peut choisir d'en créer
  un depuis un menu ou une action explicite. Un assistant d'installation peut
  proposer ce choix, sans le présélectionner.
- **Icônes système facultatives** : Ce PC, Documents, Corbeille et toute autre
  icône système éventuelle sont des options distinctes dans Paramètres,
  désactivées par défaut. Les activer n'ajoute que l'icône choisie.
- **Préférences persistantes** : visibilité, positions, thème, fond d'écran et
  alignement sont conservés dans le profil utilisateur ; une session Live
  indique clairement que ses changements peuvent être perdus au redémarrage.
- **Aucune fausse commande** : ne proposer que des applications, réglages et
  actions réellement disponibles ; expliquer lorsqu'une fonction dépend de
  l'hôte Linux ou n'est pas encore implémentée.

## État actuel observé

- Le prototype dessine un fond d'écran, une barre des tâches, un menu Démarrer,
  des notifications et des fenêtres intégrées Explorer/Paramètres.
- Le menu et la barre donnent accès à un petit ensemble d'applications
  intégrées ; le catalogue complet des applications installées et la gestion
  dynamique des épingles restent à réaliser.
- Le bureau démarre sans icônes par défaut. Les icônes système (Ce PC,
  Documents, Corbeille) sont réglables individuellement dans Paramètres >
  Apparence > Icônes du bureau ; elles sont désactivées par défaut. Le
  déplacement fonctionne (grille temporaire, positions non sauvegardées).
- Paramètres contient actuellement des pages système Linux, applications
  Windows, réseau, son/batterie, clavier/trackpad, apparence et mises à jour.
  Les pages Écran et définition et À propos (BUILD-INFO) sont aussi présentes.
  Les options d'apparence visibles incluent le thème clair/sombre,
  l'alignement de la barre des tâches et les icônes du bureau ; elles ne
  constituent pas encore un panneau de personnalisation complet.
- Le shell est un prototype de test graphique ; il ne remplace pas encore un
  environnement de bureau Linux et n'est pas livré comme gestionnaire de
  fenêtres autonome.

## Bureau et raccourcis

### Comportement cible

- [x] Démarrer sur un bureau sans icône par défaut, y compris après une
  installation neuve ou une réinitialisation des préférences.
- [x] Ajouter dans Paramètres une rubrique « Icônes du bureau » avec une
  option indépendante par icône système ; toutes sont désactivées par défaut.
- [ ] Permettre la création, le renommage et la suppression de raccourcis
  applicatifs depuis l'interface, sans supprimer l'application cible.
- [ ] Ne créer aucun raccourci de bureau de manière implicite lors de
  l'installation d'un logiciel ; présenter un choix explicite lorsque c'est
  pertinent.
- [ ] Proposer l'alignement sur une grille ou le placement libre, avec
  coordonnées enregistrées, limites de l'écran respectées et réorganisation
  après changement de résolution ou d'échelle.
  - [x] Déplacer les trois icônes prédéfinies sur une grille et limiter leur position à l'écran ; positions uniquement en mémoire.
- [ ] Ajouter un menu contextuel du bureau : créer un raccourci, actualiser,
  organiser, choisir l'affichage et ouvrir les paramètres pertinents.
- [ ] Gérer sélection, multi-sélection, déplacement, menu contextuel et
  navigation clavier avec un état de focus visible.
- [ ] Prévoir un bouton pour restaurer les préférences ou la disposition du
  bureau sans effacer les fichiers personnels.

### Règles des icônes système

Les options proposées pourront inclure « Ce PC », « Dossier personnel » et
« Corbeille ». Elles doivent être configurables séparément et rester masquées
par défaut. Les icônes d'applications ajoutées à la demande sont des raccourcis
utilisateur, pas des icônes système. La suppression d'un raccourci ne doit
jamais désinstaller son application ; la suppression d'un fichier depuis la
Corbeille doit, elle, demander confirmation lorsque nécessaire.

## Barre des tâches et menu Démarrer

- [ ] Répertorier les applications réellement installées depuis les entrées
  `.desktop`, les raccourcis Windows connus et les applications intégrées.
- [ ] Permettre de rechercher, lancer, épingler et désépingler une application
  depuis Démarrer ; afficher clairement les applications indisponibles ou
  incompatibles.
- [ ] Afficher les applications ouvertes, leur fenêtre active et leur état
  (ouverte, réduite, attention requise), avec bascule et aperçu si disponible.
  - [x] Afficher/basculer Explorer et Paramètres dans la barre du prototype ; pas de suivi général des fenêtres Linux externes.
- [ ] Rendre la zone de notification actionnable et accessible au clavier ;
  distinguer l'état réseau/audio/batterie de la disponibilité réelle des
  réglages correspondants.
- [ ] Conserver les préférences de position, d'alignement et d'épinglage après
  redémarrage.
  - [x] Conserver l'alignement de la barre et le thème sur racine persistante ; positions et épingles restent à sauvegarder.
- [ ] Ajouter une recherche cohérente couvrant applications, fichiers et
  paramètres, avec indication de la source des résultats.
- [ ] Clarifier les actions de session, arrêt et redémarrage et demander
  confirmation pour les opérations susceptibles de faire perdre du travail.

## Paramètres à compléter

Les sections suivantes sont une cible fonctionnelle ; chaque contrôle doit
afficher son état réel et une commande indisponible doit être désactivée ou
expliquée.

| Section | Réglages à prévoir |
| --- | --- |
| Système | Informations système, stockage, périphériques, session, veille, alimentation et actions d'arrêt |
| Applications | Applications Linux et Windows installées, association de fichiers, application par défaut, lancement et désinstallation contrôlée |
| Réseau | État et interfaces, connexion disponible, renouvellement, diagnostic et informations DNS ; ne pas promettre la gestion du Wi-Fi si elle appartient à l'hôte |
| Son et batterie | Périphérique de sortie, volume, muet, état de batterie et options uniquement si le matériel les expose |
| Clavier et pavé tactile | Disposition, répétition, raccourcis, options du pavé tactile lorsque le périphérique les expose |
| Affichage | Résolution, écrans, échelle DPI, orientation et fréquence si le backend les prend en charge |
| Personnalisation | Thème clair/sombre/système, fond d'écran, accent, alignement de la barre, icônes du bureau et grille/placement libre |
| Accessibilité | Taille du texte, contraste, réduction des animations, visibilité du focus et navigation clavier |
| Notifications | Autorisations, historique ou effacement, niveaux de priorité et options de silence |
| Confidentialité et sécurité | Permissions et accès aux fichiers, provenance des logiciels, état des mises à jour et confirmations des actions privilégiées |
| Mises à jour | Version installée, composants disponibles, progression, journal, restauration et demande explicite de redémarrage |

- [ ] Organiser les pages avec navigation, recherche dans les réglages,
  descriptions courtes, états de chargement, erreurs et retours de réussite.
  - [x] Navigation latérale, cartes, télémétrie et journaux d'action ; pas de recherche transversale dans les réglages.
- [x] Ajouter À propos avec version, commit et date lus dans BUILD-INFO, et signaler son absence sur les anciennes images.
- [ ] Enregistrer les modifications de façon prévisible et indiquer si elles
  s'appliquent immédiatement, à la prochaine session ou après redémarrage.
- [ ] Éviter de demander des privilèges administrateur au bureau entier ;
  demander une autorisation ciblée uniquement pour une action système qui la
  nécessite.
  L'exigence reste ouverte : le démarrage ISO exécute encore le shell en root.

## Fonctionnalités natives pratiques à long terme

Ces idées sont des objectifs de conception, pas des fonctions déjà disponibles.
Elles viennent après la fiabilisation du runtime, de l'interface et de la
gestion des applications ; chaque intégration doit rester utilisable sans
connaître les détails internes de Linux ou de Windows.

### Priorité haute

- [ ] **Centre d'applications unifié** : rechercher, installer, lancer, mettre
  à jour et désinstaller les applications Linux et Windows depuis un catalogue
  commun. Pour chaque application Windows, afficher les dépendances requises
  et un état de compatibilité fondé sur des tests réels. Demander explicitement
  si un raccourci doit être créé ; ne jamais en ajouter au bureau par défaut.
- [ ] **Profils par application Windows** : regrouper les préférences de
  résolution/DPI, variables d'environnement, accès aux dossiers, réseau,
  composants .NET et mode de compatibilité. Prévoir une réinitialisation du
  profil sans supprimer les documents de l'utilisateur. Distinguer les options
  effectivement appliquées de celles encore expérimentales.
- [ ] **Diagnostic compréhensible** : expliquer les erreurs de lancement en
  langage clair, avec les détails techniques consultables. Identifier, quand
  c'est possible, l'architecture, l'import Windows, la DLL ou le runtime
  manquant et proposer uniquement une solution vérifiée. Ne pas annoncer une
  compatibilité garantie sur la seule base d'une liste d'imports résolus.
- [ ] **Sauvegarde et récupération** : proposer une sauvegarde des préférences,
  profils d'applications et données choisies, avec aperçu du contenu, dates,
  vérification et restauration sélective. Offrir un retour arrière après une
  mise à jour ou une modification système et expliquer clairement ce qui sera
  restauré.
- [ ] **Permissions compréhensibles** : montrer les accès aux fichiers et
  ressources accordés à chaque application, permettre de les limiter ou de
  les révoquer et demander une autorisation ciblée pour une opération
  privilégiée. Ne pas exécuter le bureau complet en administrateur pour
  simplifier une action ponctuelle.

### Confort et productivité

- [ ] **Recherche globale** dans les applications, fichiers et réglages, avec
  filtres simples, résultats identifiés par catégorie et respect des
  permissions d'accès.
- [ ] **Gestionnaire de tâches accessible** : lister les applications en cours,
  signaler celles qui ne répondent plus et permettre de les fermer proprement
  avant de proposer un arrêt forcé.
- [ ] **Presse-papiers cohérent** entre applications Linux et Windows, avec
  formats texte et fichiers définis, et une option pour désactiver l'historique.
- [ ] **Glisser-déposer inter-applications** avec indication de l'action
  (copie, déplacement ou ouverture), notamment entre Explorer et les
  applications Linux.
- [ ] **Outils courants** : capture d'écran, raccourcis clavier configurables,
  espaces de travail ou bureaux virtuels, et accès rapide aux périphériques
  amovibles.
- [ ] **Accessibilité intégrée** : réglage de la taille du texte et du
  contraste, réduction des animations, navigation clavier, focus visible et
  annonces accessibles des états et notifications.
- [ ] **Assistant de première installation** : langue, disposition clavier,
  compte utilisateur, réseau, mises à jour et préférences visuelles. Garder un
  parcours court, permettre de passer les étapes et laisser le bureau vide
  conformément aux préférences par défaut de Winlux.

### Principes de mise en oeuvre

- Commencer par les fonctions les plus utiles au parcours quotidien :
  installer ou ajouter une application, la lancer, comprendre un échec, puis
  récupérer les données et préférences.
- Ne pas intégrer un catalogue qui installe des logiciels sans source,
  provenance, licence et mécanisme de mise à jour clairement affichés.
- Séparer nettement les applications Linux des applications Windows, tout en
  proposant une recherche et une bibliothèque communes.
- Pour les programmes Windows, exposer les limites connues et les résultats de
  tests par application ; ne pas confondre profil de compatibilité et
  implémentation d'API Windows manquantes.
- Conserver l'accès aux fichiers personnels lors des mises à jour,
  désinstallations, réinitialisations de profil et restaurations.
- Ajouter des confirmations pour les suppressions irréversibles et les actions
  privilégiées, mais éviter les confirmations répétitives pour les opérations
  simples et réversibles.

### Autres fonctions utiles

- [ ] **Emplacements des fichiers** : expliquer où sont stockés les fichiers
  Windows, les dossiers personnels, les données et les profils d'applications.
  Permettre d'ouvrir ces emplacements depuis Explorer ou le gestionnaire de
  fichiers Linux et de comprendre les correspondances entre `C:\` et les
  chemins Linux.
- [ ] **Fiche avant installation** : afficher l'architecture, les dépendances
  détectées, les problèmes connus et les résultats de tests de l'application.
  Séparer explicitement les états « non testé », « lancement vérifié » et
  « fonctions testées » ; ne pas promettre une compatibilité complète sur la
  seule base d'une détection automatique.
- [ ] **Désinstallation maîtrisée** : répertorier les fichiers connus de
  l'application, son profil, ses dépendances et ses raccourcis, puis laisser
  choisir les éléments à retirer. Préserver les documents personnels et
  demander confirmation avant toute suppression irréversible.
- [ ] **Mode de récupération** : proposer un démarrage de secours, une
  annulation de mise à jour ou une réparation du système qui ne commence pas
  par effacer les données utilisateur. Indiquer les sauvegardes disponibles
  et les conséquences avant de restaurer.
- [ ] **Vue du stockage** : répartir l'espace utilisé par le système, les
  applications, les profils Windows, les mises à jour, les fichiers temporaires
  et la Corbeille. Présenter un aperçu et demander confirmation avant tout
  nettoyage ; ne rien supprimer automatiquement sans règle explicite.
- [ ] **Applications par défaut** : choisir quelle application ouvre chaque
  type de fichier ou protocole, parmi les applications réellement installées,
  qu'elles soient Linux ou Windows. Afficher clairement le comportement de
  repli si l'application choisie ne peut pas démarrer.
- [ ] **Historique système** : consulter les installations, mises à jour,
  restaurations et actions privilégiées récentes, avec date, résultat et
  détails utiles. Ne jamais y exposer les jetons, mots de passe ou autres
  secrets.
- [ ] **Configuration initiale reprenable** : permettre de configurer la langue,
  le clavier, le réseau et quelques préférences, de passer une étape puis de
  reprendre l'assistant plus tard. Chaque choix doit rester modifiable dans
  Paramètres.
- [ ] **Vérification du matériel** : résumer au premier démarrage l'état de
  l'écran, du son, du réseau, du stockage et des périphériques détectés.
  Distinguer les capacités de l'hôte Linux de celles exposées par une machine
  virtuelle et indiquer les éléments qui n'ont pas pu être vérifiés.
- [ ] **Gestionnaire de périphériques** : lister le matériel détecté, son état
  et les réglages réellement disponibles. Utiliser en priorité les pilotes
  Linux de l'hôte ; ne pas laisser croire qu'un pilote Windows peut être
  installé comme un pilote natif Winlux.
- [ ] **Importation de données** : guider la copie de documents et autres
  fichiers personnels depuis un autre système ou un volume monté, avec aperçu
  de la destination, vérification des collisions et choix explicite avant toute
  copie. Ne jamais importer ou remplacer des fichiers sans consentement.
- [ ] **Export et transfert de profils** : sauvegarder puis restaurer ou
  transférer les réglages d'une application Windows, avec aperçu du contenu et
  vérification des versions et dépendances nécessaires. Garder les secrets hors
  de l'export par défaut.
- [ ] **Session invitée temporaire** : proposer un environnement de session
  distinct dont les données sont supprimées à la fermeture, en prévenant
  clairement l'utilisateur et en séparant cette suppression des données du
  compte principal.
- [ ] **Aide hors ligne** : fournir des guides accessibles sans réseau pour
  installer et lancer un programme, retrouver les fichiers, comprendre les
  diagnostics et utiliser les outils de récupération.
- [ ] **Prévisualisation des mises à jour** : afficher l'espace requis, les
  composants concernés, les changements importants et la nécessité éventuelle
  d'une relance ou d'un redémarrage avant confirmation de l'installation.
- [ ] **Mode économie d'énergie** : proposer des réglages adaptés aux
  appareils portables, tels que réduire les animations ou le travail en
  arrière-plan, uniquement lorsque l'état de batterie est réellement disponible.
  Expliquer les compromis et laisser l'utilisateur reprendre le contrôle.

## Qualité visuelle, ergonomie et intégration

- [ ] Tester les fenêtres, menus et pages à basse résolution, en HiDPI et lors
  de changements de résolution, sans texte ou boutons inaccessibles.
- [ ] Compléter le redimensionnement, l'empilement et le focus des fenêtres,
  ainsi que les états maximisé, réduit et restauré.
  - [x] Implémenter ces états pour les deux fenêtres internes Explorer/Paramètres (`wm.rs`) ; validation ergonomique complète et gestion des fenêtres externes distinctes.
- [ ] Assurer la navigation clavier, les raccourcis cohérents, les libellés
  accessibles et un contraste suffisant dans les thèmes clair et sombre.
- [ ] Respecter le clavier réellement configuré, les caractères Unicode,
  l'IME et les touches mortes.
- [ ] Prévoir les comportements de fenêtres multiples, bureaux virtuels et
  notifications sans les annoncer avant leur implémentation.
- [ ] Séparer clairement les commandes gérées par Winlux de celles qui relèvent
  du bureau ou de la distribution Linux hôte.
- [ ] Valider l'intégration avec les environnements Linux ciblés et garder une
  session de test automatisée pour les interactions graphiques importantes.

## Critères d'acceptation prioritaires

1. Après une première installation, le bureau ne montre aucune icône.
2. Activer une icône système dans Paramètres ne montre que cette icône et le
   choix survit au redémarrage sur un système persistant.
3. Installer une application ne crée pas de raccourci sur le bureau sans
   demande explicite ; l'application reste trouvable dans Démarrer.
4. Un raccourci créé par l'utilisateur peut être déplacé en mode grille ou
   libre, sa position est persistée, et supprimer le raccourci ne désinstalle
   pas l'application.
5. Les paramètres reflètent l'état réel du système et affichent les erreurs
   d'action au lieu de simuler une réussite.
