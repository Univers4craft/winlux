# Règles de travail pour les agents

## SUIVI DES TÂCHES (OBLIGATOIRE)

**Source unique de vérité : [Suivi.md](Suivi.md)**

- **Avant d'agir** : Consulte `Suivi.md`. Prends une tâche non cochée dans "À FAIRE" et commence à la réaliser. **Coche-la `[x]` IMMÉDIATEMENT** en la déplaçant vers "DÉJÀ FAIT" ou en la marquant faite.
- **Pendant l'action** : Travaille uniquement sur cette tâche. Si tu découvres une sous-tâche nécessaire, ajoute-la à `Suivi.md` dans la bonne section.
- **Après avoir terminé** : Teste (`cargo test -p winlux`, `scripts/build-tests.sh`, `scripts/test-runtime.sh` si pertinent ; `sh scripts/test-all.sh` lance toute la suite). **Met à jour `Suivi.md`** pour marquer la tâche comme `[x]` avec contexte si besoin. Commit et push.
- **À jour en temps réel** : À CHAQUE action concrète (ajout, correction, refactor), `Suivi.md` DOIT être mis à jour dans le même commit ou juste après. Pas "plus tard".
- **Miroirs** : `TODO.md` et `TRACKING.md` sont des miroirs. `Suivi.md` est la source à consulter et à mettre à jour en priorité.

## Efficacité

- Comprendre la demande et son périmètre avant d'agir. Pour une question ou une
  demande de plan, répondre sans modifier le dépôt ; pour une demande de
  changement, implémenter le changement demandé sans élargir le périmètre.
- Explorer de façon ciblée : repérer les fichiers utiles, lire le contexte
  nécessaire une fois, puis agir. Ne pas relancer des recherches ou relire les
  mêmes fichiers sans nouvel élément qui le justifie.
- Grouper les recherches et lectures indépendantes dans un même appel. Éviter
  les inspections globales, les sous-agents et les commandes coûteuses quand
  une vérification ciblée suffit.
- Choisir la plus petite validation qui couvre le changement. Ne pas lancer
  systématiquement toute la suite, une compilation complète ou des vérifications
  répétées ; élargir seulement si les résultats ou le risque le justifient.
- Ne pas multiplier les messages de progression : communiquer seulement lors
  d'un changement important de phase ou en cas de blocage. Garder la réponse
  finale concise et signaler les vérifications réellement effectuées.

## Exactitude et modifications

- Vérifier le code et la documentation pertinents avant d'affirmer qu'une
  fonctionnalité existe ou manque ; distinguer clairement l'état actuel des
  objectifs proposés.
- Effectuer des changements précis, cohérents avec les conventions du dépôt.
  Ne pas écraser ni annuler les modifications préexistantes de l'utilisateur.
- Mettre à jour la documentation directement liée au changement. Ne pas
  modifier de fichiers sans rapport avec la demande.
- Ne pas masquer les erreurs de validation. Si une commande échoue, examiner
  son résultat et ne relancer que si une action corrective ou une autre
  vérification est nécessaire.

## Salle blanche (compatibilité Windows)

Winlux réimplémente l'API Windows par lui-même. Pour que ce travail reste
juridiquement irréprochable, il se fait en « salle blanche » :

- **Sources autorisées** : la documentation publique (Microsoft Learn, SDK
  publics et leurs en-têtes, spécifications publiées comme [MS-CFB],
  [MS-SHLLINK] ou PE/COFF), les livres et articles publiés, et le
  comportement observé d'un programme lancé dans Winlux (traces, codes de
  retour, captures).
- **Interdit** : lire, décompiler, désassembler ou copier du code ou des
  binaires Microsoft (DLL de Windows, pilotes, fichiers de symboles privés,
  code divulgué). Interdit aussi : s'inspirer de Wine, ReactOS ou d'une
  autre réimplémentation dont la licence ou l'origine poserait problème. Le
  désassemblage d'un programme tiers testé (PuTTY, un jeu…) reste permis
  pour comprendre ce qu'il attend de Winlux, jamais pour copier du code
  Microsoft qu'il contiendrait.
- **Textes** : les messages, libellés et boîtes de dialogue sont rédigés par
  Winlux, jamais recopiés mot pour mot de Windows. Les noms d'API, de
  constantes et de structures, ainsi que les codes d'erreur, ne sont pas
  concernés : ils sont nécessaires à la compatibilité.
- **Fichiers de test** : tout fichier binaire ajouté au dépôt (`.msi`,
  `.exe`, `.dll`, images, polices) est produit par nous ou d'origine libre
  documentée. Un script ou un `LISEZMOI` à côté indique comment il est
  généré (exemple : `runtime/src/msi/testdata/`).
- En cas de doute sur l'origine d'une information ou d'un fichier, ne pas
  l'utiliser et le signaler.

### Logiciels constructeurs de périphériques (#1531, décision du 10/11/2026)

Pour faire fonctionner les logiciels d'éclairage, de souris, de claviers et
de casques (iCUE, Synapse, G HUB, SteelSeries GG…), l'observation de leur
fonctionnement est permise **dans le seul but de l'interopérabilité**, dans
le cadre du droit de l'Union européenne (directive 2009/24/CE, art. 5 § 3 :
observer, étudier et tester le fonctionnement d'un programme qu'on a le droit
d'utiliser) :

- **Permis** : journaliser, avec le runtime Winlux, ce que le logiciel envoie
  à notre propre appareil virtuel (`DeviceIoControl`, rapports HID) ou au
  périphérique USB, y compris quand l'appareil virtuel se présente avec le
  VID:PID et un descripteur HID tirés d'une source publique ou d'un pilote
  libre.
- **Interdit** : désassembler ou décompiler un binaire de la marque ou de
  Microsoft ; copier un de leurs fichiers dans le dépôt ; extraire une
  ressource ou du code d'un installeur. La tolérance ci-dessus pour les
  « programmes tiers testés » ne s'applique pas à ces logiciels.
- **Résultat** : uniquement nos propres tables de traduction, écrites par
  nous. Chaque PR concernée rappelle ce cadre et note la provenance et la
  méthode (logiciel, version, éditeur, source officielle, SHA-256 de
  l'installeur, ce qui a été journalisé).
- **Installeurs téléchargés** : données non fiables. Dossier vide dédié,
  scripts ailleurs, chemins passés en arguments, Python avec `-I`. Jamais
  ajoutés au dépôt.
- Le reste du projet garde la règle des sources publiques ci-dessus.

## Marque et vocabulaire

Voir `docs/POSITION-JURIDIQUE.md` (position publique) et `docs/DEFENSE-DU-NOM.md` (dossier interne).

- **Aucun élément de Microsoft** dans le dépôt ni dans l'ISO : ni code, ni DLL, ni exécutable, ni police (Segoe…), ni icône, ni curseur, ni son, ni fond d'écran, ni logo, ni texte d'aide recopié.
- **Salle blanche** : voir la section précédente.
- **Vocabulaire permis** : « ouvre les programmes conçus pour Windows (.exe) », « compatible avec de nombreux programmes Windows », « la touche Windows », « ergonomie familière ».
- **Vocabulaire interdit** dans les textes visibles, le site, le wiki, les notes de version et les captures : « Windows gratuit », « Windows libre », « remplace Windows », « alternative à Windows », « comme Windows 11 », « comme sous Windows », « façon Windows 11 », « ressemble à Windows », « clone de Windows », toute explication du nom qui le relie à Windows, et les noms de produits ou de matières graphiques de Microsoft comme noms de fonctions (Mica, Acrylic, Fluent, Copilot, Cortana, Defender, WordPad). Dans les commentaires de code, préférer une description fonctionnelle.
- **Mention de non-affiliation** sur tout nouvel écran « À propos », installateur ou page publique : « Windows est une marque de Microsoft Corporation. Winlux est un projet indépendant : il n'est ni affilié à Microsoft, ni approuvé par elle. »
- **Aucun contournement** de protection technique, d'activation ou de licence.
