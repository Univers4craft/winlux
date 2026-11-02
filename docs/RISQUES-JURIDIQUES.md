# Risques juridiques de Winlux

*Analyse du 2 novembre 2026, faite sur le dépôt à l'état du commit `c706919`.*

> **Avertissement.** Ce document a été rédigé par Claude, un assistant d'IA,
> pas par un avocat ni par un conseil en propriété industrielle. Il sert à
> repérer les risques et à préparer une consultation, pas à la remplacer.
> Aucune jurisprudence ni aucun numéro de dépôt n'a été inventé : chaque
> source citée a été vérifiée en ligne le 2 novembre 2026 (liste en fin de
> document). Quand une chose n'a pas pu être vérifiée, c'est écrit.
> Le droit cité est surtout français et européen, avec les États-Unis quand
> ils comptent (GitHub, Microsoft et les IA y sont établis).

## En bref

| # | Risque | Gravité | Face à qui | À faire maintenant |
|---|--------|---------|-----------|--------------------|
| 1 | Le nom « Winlux », combiné à l'interface Windows 11 et à la promesse « vos .exe » | **Élevée** | Microsoft | Consulter un conseil en propriété industrielle **avant** d'acheter `winlux-os.org` ou de déposer une marque ; ajouter tout de suite une mention de non-affiliation ; préparer un nom de repli |
| 2 | ISO distribuée sans les sources des logiciels GPL qu'elle contient | **Élevée** (certaine, mais facile à corriger) | Ayants droit GPL (noyau, BusyBox, GRUB…), Software Freedom Conservancy | Publier les sources avec chaque version, corriger `NOTICES` |
| 3 | Assistant Claude et ChatGPT intégrés | **Moyenne** | Anthropic, OpenAI | Accepter les conditions commerciales d'Anthropic, renommer « Assistant Claude » en « Assistant IA », écrire l'information aux utilisateurs |
| 4 | Imitation de l'interface de Windows 11 | **Moyenne** | Microsoft | Ne jamais reprendre un fichier Microsoft (icône, son, police, fond) ; garder une différence visible ; ne pas suivre au pixel les nouveautés récentes |
| 5 | Installation de logiciels tiers, dont MSIX sans vérifier la signature | **Moyenne**, qui monte si le projet devient commercial | Utilisateurs, éventuellement autorités (règlement européen sur la cyber-résilience) | Bloquer par défaut ou avertir clairement « éditeur non vérifié » |
| 6 | Base Ubuntu redistribuée modifiée | **Moyenne-faible** | Canonical | Retirer les marques Ubuntu de l'image, écrire « construit à partir de paquets Ubuntu » |
| 7 | Réimplémentation des API Windows | **Faible** tant que la méthode reste propre | Microsoft | Écrire une règle de « salle blanche » dans `AGENTS.md` et la respecter |
| 8 | Vie privée (RGPD) et mentions légales du site | **Faible** aujourd'hui | CNIL, utilisateurs | Une page « Confidentialité » et une page « Mentions légales » sur le site |
| 9 | Composants tiers (shim, VC++, NVIDIA, FFmpeg, polices, modèle Qwen…) | **Faible** dans l'état actuel | Leurs éditeurs, porteurs de brevets vidéo | Ne rien ajouter dans l'ISO sans vérifier sa licence ; compléter `NOTICES` |
| 10 | Licence du dépôt, code écrit par IA | **Faible** | — | Rien d'urgent ; ajouter un DCO si d'autres contributeurs arrivent |

**Le seul risque qui peut vraiment arrêter le projet est le nom.** Les autres
se corrigent avec du travail ordinaire, sans avocat. Pour le nom, un avis de
conseil en propriété industrielle est justifié dès que Winlux se
« professionnalise » (domaine, marque, site vitrine, dons, vente).

Correction d'un point de départ : **le dépôt a déjà une licence**, la GPL
version 3 ou ultérieure (`LICENSE`, `NOTICES`, badge du `README.md`).

---

## 1. Le nom « Winlux »

### Ce qui a été vérifié

- La recherche précédente du projet n'a trouvé aucune marque « Winlux » en
  vigueur en classes 9 (logiciels) et 42 (services informatiques) à l'INPI,
  à l'EUIPO et à l'USPTO. **Je n'ai pas pu interroger moi-même les bases de
  l'INPI et de l'EUIPO** (elles ne s'interrogent pas depuis cet environnement).
  Je reprends donc ce résultat sans l'avoir refait.
- Une recherche web le 2 novembre 2026 a trouvé :
  - une marque américaine **WINLUX n° 85700940**, de Parlux Optoelectronics,
    en classe 11 (éclairage), **annulée le 18 décembre 2020** : sans effet ;
  - une société suisse **Win-Lux**, présentée comme « IT & Software »
    (`win-lux.ch`) ;
  - un petit logiciel Windows appelé **WinLux** (paquet winget
    `Lparksi-WinLux`) ;
  - une société américaine Winlux Living LLC, titulaire de marques dont je
    n'ai pas vérifié les classes.

  Aucun ne paraît être un vrai obstacle, mais la société suisse et le
  logiciel WinLux montrent que **le nom n'est pas libre de tout usage** dans
  l'informatique. Une recherche d'antériorité professionnelle (marques,
  dénominations sociales, noms de domaine, usages) doit le confirmer.

### Le vrai risque : Microsoft et la marque « Windows »

Le risque ne vient pas d'un homonyme, il vient de **Microsoft**. Une marque
est contrefaite quand un signe **similaire** est utilisé pour des produits
**identiques ou similaires** et qu'il existe un **risque de confusion** dans
l'esprit du public (règlement (UE) 2017/1001 sur la marque de l'Union
européenne, article 9 ; Code de la propriété intellectuelle, article
L. 713-3). Une marque renommée, ce qu'est « Windows », est en plus protégée
contre l'usage qui **profite indûment de sa renommée**, même sans confusion.

**Le précédent Lindows est presque le même cas.** En décembre 2001, Microsoft
a attaqué Lindows.com, qui vendait un système Linux capable de lancer des
programmes Windows, au motif que « Lindows » imitait « Windows ». Aux
États-Unis, les juges ont refusé à Microsoft une interdiction provisoire et
ont émis des « doutes sérieux » sur la validité de la marque « Windows »,
parce que le mot servait à décrire les interfaces à fenêtres avant 1985.
Mais Microsoft a aussi attaqué en Suède, en France, en Belgique, au
Luxembourg, aux Pays-Bas et au Canada, et a obtenu dans certains pays le
blocage des ventes. En juillet 2004, Microsoft a payé environ 20 millions de
dollars, Lindows lui a cédé sa marque et s'est renommé **Linspire**.

Ce qu'il faut en retenir :

1. Lindows a gagné l'argument juridique aux États-Unis et **a quand même dû
   changer de nom**, parce que Microsoft pouvait plaider dans plusieurs
   pays à la fois. Un projet tenu par une seule personne ne peut pas
   soutenir ce type de procédure, même avec de bons arguments.
2. En Europe, la question « Windows est-il un mot générique ? » ne joue
   pas comme aux États-Unis : je n'ai trouvé aucune décision européenne qui
   affaiblisse la marque, et Lindows y a perdu des procédures.
3. « Winlux » est construit **exactement comme « Lindows »** : un morceau de
   « Windows » + un morceau de « Linux », pour le même produit (un système
   d'exploitation), avec la même promesse (lancer des programmes Windows).

### Ce qui aggrave le risque

- **La promesse « vos .exe se lancent d'un double-clic »** : elle renforce le
  lien avec Windows dans l'esprit du public. Il faut la garder (c'est le
  produit) mais la formuler comme une compatibilité, pas comme une identité.
- **L'interface volontairement « façon Windows 11 »** : un nom proche + un
  aspect proche + le même produit, c'est l'addition que les juges regardent
  pour apprécier la confusion. Chaque élément pris seul est défendable ;
  l'ensemble l'est beaucoup moins.
- **Ajouter « -os »** (`winlux-os.org`) **n'aide pas** : « OS » est un mot
  descriptif qui souligne au contraire que le produit est le même que
  Windows. Il ne rend pas le nom plus distinctif.
- **« Win » en tête de nom** : beaucoup de logiciels commencent par « Win »
  (WinRAR, WinZip…) sans procès connu, mais ce sont des **programmes pour
  Windows**, pas des **systèmes concurrents** de Windows. Ce n'est pas le
  même rapport de force.
- **La professionnalisation** : tant que Winlux est un projet personnel peu
  visible, Microsoft a peu de raisons d'agir. Un domaine, une marque
  déposée, des dons ou une vente font changer cela. **Un dépôt de marque
  « Winlux » serait publié et très probablement repéré par la surveillance
  de marques de Microsoft**, qui pourrait former opposition.

### Ce qui l'atténue

- Le nom ne reprend pas « dows » ; la fin « lux » est propre au projet.
- Aucun logo, drapeau ou élément graphique de Microsoft (logo « Prisme »,
  fonds d'écran et sons propres, voir section 4).
- Projet gratuit, non commercial, sous licence libre.
- Une mention claire de non-affiliation réduit le risque de confusion
  (elle ne le supprime pas).

### Que faire

**Tout de suite, sans frais :**

1. Ajouter sur le site, dans le `README.md`, dans « À propos » et dans
   l'installateur :
   > Windows est une marque de Microsoft Corporation. Winlux est un projet
   > indépendant, il n'est ni affilié à Microsoft, ni approuvé par elle.
2. Parler de « compatibilité avec les programmes Windows » plutôt que de
   « Windows sous Linux », et éviter « le Windows libre » ou « Windows 11
   gratuit » dans toute communication.
3. **Ne pas acheter `winlux-os.org` ni déposer « Winlux » avant l'étape 4.**
   Un domaine coûte peu, mais l'investissement en image (site, guide, liens)
   se perd si le nom doit changer.

**Avant de professionnaliser :**

4. **Faire faire par un conseil en propriété industrielle** (profession
   réglementée, liste sur le site de la CNCPI) une recherche d'antériorité
   et un avis sur « Winlux » face à « Windows ». C'est le seul point de ce
   document où un professionnel est vraiment nécessaire.
5. Préparer en parallèle **un nom de repli sans « Win »** ni « dows », qui
   ne rappelle pas Windows, par exemple autour de l'identité « Prisme »
   déjà en place. Changer de nom aujourd'hui coûte quelques heures de
   recherche-remplacement ; dans deux ans, cela coûtera la notoriété acquise.

Mon avis franc : **si Winlux doit devenir un vrai produit public, il est
plus sûr de changer de nom maintenant** que d'attendre une lettre de
Microsoft. Le précédent Lindows montre que même en ayant raison, on finit
par céder.

---

## 2. Réimplémenter les API Windows

### L'état du droit

**Le principe est favorable : réécrire une interface est permis, copier du
code ne l'est pas.**

- **Union européenne.** La directive 2009/24/CE sur les programmes
  d'ordinateur protège l'*expression* d'un programme (son code), pas les
  idées et principes qui sous-tendent ses interfaces (considérant 11,
  article 1er §2). Dans l'arrêt **SAS Institute c. World Programming**
  (CJUE, 2 mai 2012, C-406/10), la Cour a jugé que **la fonctionnalité d'un
  programme, son langage de programmation et le format de ses fichiers de
  données ne sont pas protégés** par le droit d'auteur sur le logiciel ;
  et qu'un utilisateur licencié peut **observer, étudier et tester** le
  programme pour comprendre ses principes (article 5 §3), sans pouvoir
  l'interdire par contrat. En droit français, ces règles sont aux articles
  L. 122-6-1 et L. 122-6-2 du Code de la propriété intellectuelle.
- **Décompilation.** L'article 6 de la directive ne l'autorise que si les
  **trois** conditions sont réunies : la personne a le droit d'utiliser le
  programme ; l'information nécessaire à l'interopérabilité n'était pas
  déjà facilement accessible ; l'opération se limite aux parties
  nécessaires. L'information obtenue ne peut servir **qu'à
  l'interopérabilité**, ne peut pas être diffusée au-delà, et ne peut pas
  servir à écrire un programme « substantiellement similaire dans son
  expression ».
- **États-Unis.** Dans **Google LLC c. Oracle America** (Cour suprême,
  5 avril 2021, 593 U.S. 1), Google avait recopié les lignes de
  *déclaration* d'environ 11 500 lignes de l'API Java pour Android. La Cour
  n'a **pas** tranché si ces déclarations sont protégeables ; elle a jugé
  que leur reprise était un **usage loyal (« fair use »)**, parce qu'il
  s'agissait de permettre aux programmeurs de réutiliser leurs
  connaissances dans un nouveau contexte. C'est favorable à Winlux, mais
  c'est une décision au cas par cas, pas une règle générale.

### Ce qui est permis, ce qui est interdit

| Permis | Interdit ou risqué |
|--------|--------------------|
| Écrire de zéro une fonction qui porte le même nom, prend les mêmes paramètres et produit le même effet que la fonction Windows | Copier, même partiellement, du code de Windows (y compris les sources de Windows XP qui ont fuité en 2020, ou d'autres fuites) |
| Reprendre les noms de fonctions, les constantes, les numéros d'erreur, la disposition des structures en mémoire : ce sont des faits nécessaires à l'interopérabilité | Recopier les fichiers d'en-tête du **SDK Windows** de Microsoft (leur licence limite leur usage au développement pour Windows) |
| Lire la documentation publique de Microsoft pour comprendre ce que fait une fonction | Recopier des paragraphes de cette documentation dans le code ou dans le wiki |
| Observer un programme Windows tourner, tracer ses appels, tester des cas | Décompiler une DLL de Windows au-delà de ce que l'article 6 autorise, puis s'en inspirer pour écrire le code |
| S'appuyer sur les en-têtes de MinGW-w64 (domaine public ou licences libres) pour compiler les tests | Livrer une DLL de Microsoft dans l'ISO |

### Ce que j'ai vérifié dans le dépôt

- **Aucune DLL, aucun `.sys`, aucun exécutable Microsoft n'est versionné.**
  Les seuls binaires Windows sont des fichiers de test (`sample.msi`,
  `tables.msi`, une icône `t.ico`), à faire vérifier par leur auteur
  (voir plus bas).
- **Aucune mention de décompilation, de désassemblage, de Ghidra, d'IDA ou
  d'ingénierie inverse** dans le code, les scripts ou la documentation.
- **Aucun en-tête du SDK Windows** : le runtime est en Rust et ses
  déclarations sont écrites à la main ; les tests C se compilent avec
  MinGW-w64 (`scripts/build-mingw-tests.sh`).
- **Aucun lien vers `learn.microsoft.com` ou `docs.microsoft.com`** dans le
  code Rust, donc pas de signe de documentation recopiée.
- `NOTICES` affirme qu'aucun code Wine, Proton ou autre couche tierce n'est
  utilisé, et la CI le vérifie (« aucune dépendance à Wine »).

### Où le risque est le plus réel

1. **Les messages d'erreur système.** `runtime/src/dlls/kernel32_misc.rs`
   (vers la ligne 482) contient des textes comme « Le fichier spécifié est
   introuvable. », qui reproduisent le texte français de Windows. Une
   phrase isolée et banale n'est pas une œuvre protégée, mais une table
   complète recopiée pourrait l'être. **Action** : reformuler ces messages
   avec des mots propres à Winlux (les programmes lisent le *numéro*
   d'erreur, pas le texte).
2. **Les fichiers de test.** `runtime/src/msi/testdata/sample.msi` et
   `tables.msi` : il faut s'assurer qu'ils ont été **fabriqués par le
   projet** (par exemple avec un outil libre) et non tirés d'un produit
   Microsoft ou d'un SDK. Je n'ai pas pu établir leur origine depuis
   l'historique.
3. **Les contributeurs.** Le danger classique d'un projet comme Winlux est
   qu'une personne ayant vu du code source de Windows (fuite, accès
   professionnel, programme de licence de sources Microsoft) écrive le code
   correspondant. **Action** : ajouter dans `AGENTS.md` et dans le guide de
   contribution une règle de « salle blanche » : ne jamais consulter de
   code source Microsoft, fuité ou non ; ne jamais décompiler une DLL
   Microsoft pour écrire du code ; documenter la source de chaque
   comportement reproduit (test, documentation publique, observation).
4. **Les brevets**, plus que le droit d'auteur. Microsoft détient des
   brevets logiciels, surtout aux États-Unis, et en a déjà fait valoir
   contre Linux (affaire TomTom en 2009, sur le format FAT, réglée à
   l'amiable). En Europe, les logiciels « en tant que tels » ne sont pas
   brevetables, ce qui limite le risque. Je ne connais pas de brevet précis
   visé par Winlux ; il n'y a rien à faire à ce stade, sinon **ne jamais
   chercher de brevets** (savoir qu'un brevet existe aggrave la
   responsabilité aux États-Unis).
5. **Le contournement de protections.** Les jeux et logiciels protégés
   (anti-triche, DRM, activation) sont un piège : contourner une mesure
   technique de protection est interdit en lui-même (directive 2001/29/CE,
   article 6 ; en France, articles L. 335-3-1 et L. 335-3-2 du CPI ; aux
   États-Unis, le DMCA, section 1201). Le dépôt n'en contient aucun.
   **Règle à garder** : Winlux peut *faire tourner* un anti-triche qui
   accepte de tourner, jamais le *tromper* ni le désactiver.
6. **Les DLL de Microsoft fournies par les applications** (redistribuables
   VC++, `coreclr.dll` d'une appli .NET autonome, DLL de System32 apportées
   par un installateur) : Winlux les **exécute** pour l'utilisateur, qui a
   reçu ces fichiers avec sa licence. C'est acceptable. Ce qui ne l'est pas :
   les mettre dans l'ISO ou les héberger.

**Conclusion** : la méthode actuelle du projet est la bonne et le risque est
faible. Il remonterait brutalement si un seul fichier ou un seul morceau de
code Microsoft entrait dans le dépôt.

---

## 3. L'interface « façon Windows 11 »

### Les règles qui s'appliquent

- **Droit d'auteur sur l'interface.** Dans l'arrêt **BSA c. Ministerstvo
  kultury** (CJUE, 22 décembre 2010, C-393/09), la Cour a jugé qu'une
  interface graphique **n'est pas protégée comme programme d'ordinateur**,
  mais **peut l'être comme œuvre ordinaire** si elle est originale ; et que
  les éléments dictés uniquement par leur fonction ne sont pas protégés.
  Un menu Démarrer, une barre des tâches, des fenêtres avec leurs boutons
  sont largement fonctionnels ; des icônes, des illustrations, des sons ou
  des animations précis sont, eux, des œuvres.
- **Dessins et modèles.** Depuis la réforme européenne (règlement (UE)
  2024/2822 et directive (UE) 2024/2823, en vigueur depuis le 1er mai 2025
  pour l'essentiel), **les interfaces graphiques, les icônes et les
  animations sont expressément des « produits » protégeables** comme
  dessins ou modèles. Un modèle enregistré peut durer jusqu'à 25 ans ; un
  modèle communautaire **non enregistré** dure 3 ans à partir de sa
  divulgation dans l'Union. L'apparence de Windows 11 présentée en 2021
  n'est donc plus protégée sans enregistrement, mais **les nouveautés des
  trois dernières années peuvent l'être**, et Microsoft a pu enregistrer
  certains éléments. Je ne connais pas la liste de ses dépôts : je ne peux
  pas l'affirmer élément par élément.
- **« Look and feel » aux États-Unis.** Apple avait attaqué Microsoft pour
  l'apparence de Windows ; la cour d'appel du 9e circuit a rejeté
  l'essentiel en 1994 (*Apple Computer, Inc. c. Microsoft Corp.*), en
  excluant les éléments fonctionnels et les idées. Une simple « ergonomie
  semblable » est donc difficile à attaquer sur le seul droit d'auteur.
- **Concurrence déloyale et parasitisme (France).** Même sans droit de
  propriété intellectuelle, un juge français peut sanctionner celui qui
  « se place dans le sillage » d'un concurrent pour profiter de ses efforts
  sans bourse délier (article 1240 du Code civil). C'est l'argument le plus
  probable contre une imitation très poussée, **surtout combiné au nom**
  (section 1).

### Ce que j'ai vérifié dans le dépôt

- **Logo** : logo propre « Prisme » (`assets/brand`), le logo Windows à
  quatre carreaux a été retiré (issue #170).
- **Fonds d'écran** : `assets/wallpapers`, propres au projet.
- **Sons** : les 11 sons de `assets/sounds` sont propres (son de
  démarrage « Prisme ») ; aucun son de Windows.
- **Icônes** : thème d'icônes « Winlux » généré par
  `integration/native/icons` (script `render-icons.py`), qui hérite
  d'Adwaita (GNOME). Les icônes du shell PE sont dessinées par le code.
  Je n'ai pas comparé chaque icône pixel par pixel aux icônes Fluent de
  Microsoft : à vérifier visuellement pour l'Explorateur et les
  Paramètres.
- **Polices** : **aucune police n'est versionnée et aucune police
  Microsoft n'est livrée.** « Segoe UI » apparaît seulement comme **nom**
  remplacé par Liberation Sans ou DejaVu Sans
  (`integration/native/config/fontconfig/conf.d/60-winlux-windows-fonts.conf`),
  et comme nom dans la liste de polices du site (`site/static/style.css`,
  utilisée seulement si la police est déjà installée chez le visiteur).
  C'est correct : la licence de Segoe UI ne permet pas de la redistribuer
  hors de Windows, mais citer son nom pour la remplacer est permis.
  `scripts/build-live-iso.sh` le rappelle (« pas de Segoe UI »).
- **Raccourcis clavier** (« touche Windows », Win+V…) : on peut décrire
  une touche par son nom, c'est un usage descriptif.

### Que faire

1. **Règle absolue** : aucun fichier venant de Windows (icône, curseur,
   son, police, fond d'écran, animation, texte d'aide) dans le dépôt ni
   dans l'ISO. Même « juste pour tester ».
2. **Garder une différence visible** : couleurs d'accent, logo, typographie
   (Inter, déjà utilisée par la marque), formes des icônes. Le but est
   qu'un utilisateur reconnaisse l'ergonomie de Windows, mais ne puisse pas
   croire qu'il est *sur* Windows.
3. **Ne pas reproduire au pixel les nouveautés récentes** de Windows 11
   (moins de trois ans) : ce sont celles qui peuvent encore être protégées
   comme modèles non enregistrés.
4. Sur le site, préférer « une ergonomie familière, inspirée de Windows 11 »
   à « comme Windows 11 » (`site/content/*/guide-desktop.md`, `README.md`).

---

## 4. Les composants tiers

**Règle générale** : redistribuer un logiciel libre dans une ISO est permis,
à condition de **respecter sa licence** (notices, et pour la GPL, fournir
les sources). Pour un logiciel propriétaire, il faut une autorisation
explicite ; le plus sûr est de **le faire télécharger par l'utilisateur
après installation**, depuis la source officielle, ce que Winlux fait déjà
pour plusieurs composants.

| Composant | Où dans Winlux | Licence | Peut-on le mettre dans l'ISO ? | Condition / action |
|-----------|---------------|---------|-------------------------------|--------------------|
| Noyau Linux, BusyBox, GRUB | ISO | GPL-2.0 / GPL-3.0 | Oui | **Fournir les sources correspondantes** (voir section 6) |
| Paquets Ubuntu | ISO + installation | Variées, surtout libres ; politique de marques de Canonical | Oui | Sources GPL ; retirer les marques Ubuntu (section 6) |
| Shim Secure Boot (`shim-signed`), GRUB et noyau signés | ISO (`build-live-iso.sh`) | Libres ; signature Microsoft UEFI CA sur le shim de Canonical | Oui, **sans modification** | La signature ne vaut que pour les fichiers inchangés. Modifier GRUB ou le noyau casse Secure Boot (problème technique, pas juridique). Écrire dans `NOTICES` d'où viennent ces fichiers |
| Micrologiciels (`/lib/firmware`) | ISO | Licences diverses (fichier `WHENCE` de linux-firmware) | Oui en général | Garder le fichier `WHENCE` et les licences dans l'image |
| Microcode Intel / AMD | Installé au premier démarrage | Propriétaire, redistribuable | Oui, mais Winlux ne le met pas dans l'ISO | Rien à faire |
| Pilote NVIDIA propriétaire | Installé au premier démarrage depuis le dépôt Ubuntu (`winlux-first-boot.sh`) | Propriétaire NVIDIA | **Ne pas l'ajouter à l'ISO** sans lire la licence NVIDIA en vigueur | Le téléchargement par l'utilisateur depuis Ubuntu est la bonne solution |
| Redistribuables Visual C++ | **Pas livrés** : exécutés quand une application les apporte | Licence Microsoft : redistribution réservée aux titulaires de Visual Studio, avec leur application | **Non** | Ne jamais les mettre dans l'ISO, le Store ou le site. Exécuter la copie fournie par l'application est acceptable |
| Mesa | ISO | MIT | Oui | Notice |
| FFmpeg | ISO (binaire copié depuis la machine de construction, `build-live-iso.sh` ligne ~276) | LGPL ou **GPL** selon la compilation (celle d'Ubuntu est GPL) | Oui | Sources à fournir. **Brevets** : voir ci-dessous |
| Samba | Paquet | GPL-3.0 | Oui | Sources |
| KDE Connect | Paquet (#297) | GPL | Oui | Sources |
| llama.cpp | Téléchargé (#300) | MIT | Oui | Notice |
| Modèle Qwen2.5-1.5B-Instruct | Téléchargé (#300, 1,1 Go) | **Apache-2.0** | Oui | Garder la licence et la notice. **Attention : Qwen2.5-3B est sous licence de recherche non commerciale et Qwen2.5-72B sous licence Qwen** : ne jamais passer à ces tailles sans relire la licence |
| Police emoji (Noto Color Emoji) | Téléchargée à la demande (paquet `fonts-noto-color-emoji`) | SIL Open Font License 1.1 | Oui | La mettre dans l'ISO serait permis (question ouverte du projet) |
| Inter, Liberation, DejaVu | ISO / marque | OFL 1.1 / licences libres | Oui | Notices |
| Icônes Adwaita (thème parent) | ISO | CC-BY-SA 3.0 ou LGPL | Oui | Mentionner l'attribution dans `NOTICES` |
| Données emojibase / CLDR | Dépôt | MIT + licence Unicode v3 | Oui | Déjà dans `NOTICES` |
| Microsoft Edge, Google Chrome | Téléchargés par le Store depuis l'éditeur | Propriétaires | **Non** | Déjà le cas (`WINLUX_EDGE_DEB` n'est pas activé par défaut) |
| Claude Code | Téléchargé par l'utilisateur | Conditions d'Anthropic | **Non** | Déjà le cas (« rien dans l'ISO »). Voir section 5 |
| Roue Python `pycryptodome-…whl` à la racine du dépôt | Dépôt | BSD / domaine public | Oui | Rien d'illégal, mais un binaire à la racine surprend : le déplacer ou l'expliquer dans `NOTICES` |

### Brevets des codecs vidéo (H.264, HEVC)

H.264 et HEVC sont couverts par des brevets gérés par des groupements de
licences (Via LA, anciennement MPEG LA, et Access Advance pour HEVC). Les
grandes distributions réagissent différemment : Ubuntu publie FFmpeg avec
ces décodeurs, Fedora les retire. En Europe, les brevets logiciels ont une
portée limitée, et un projet gratuit est une cible peu probable.
**Risque faible aujourd'hui, à revoir si Winlux est vendu** ou préinstallé
sur des machines. Le décodage matériel (VA-API, NVDEC) est couvert par la
licence payée par le fabricant de la carte. Je n'ai pas vérifié les dates
d'expiration des brevets restants.

---

## 5. Les assistants d'IA intégrés

### Claude (Anthropic)

Winlux télécharge Claude Code **à la demande**, l'exécute **sans le
modifier**, et l'utilisateur se connecte **avec son propre compte**, par le
parcours de connexion d'Anthropic (`claude auth login`). C'est la bonne
architecture. Ce que disent les conditions d'Anthropic (page « Legal and
compliance » de Claude Code, consultée le 2 novembre 2026) :

- Pré-installer ou **faire tourner Claude Code dans un produit** suppose
  d'**accepter les conditions commerciales d'Anthropic** et de respecter
  deux règles : ne pas modifier le binaire ni retirer une méthode de
  connexion ; **chaque utilisateur paie sa propre utilisation** avec ses
  propres identifiants. Winlux respecte les deux règles, **mais je ne
  sais pas si damien a accepté les conditions commerciales** : c'est
  l'action à faire.
- Il est explicitement permis qu'un utilisateur final se connecte au
  binaire Claude Code non modifié avec son propre abonnement.
- Il est interdit à un développeur de **récupérer, stocker ou relayer** les
  jetons de connexion Claude.ai. Winlux ne doit donc jamais lire
  `~/.claude` pour réutiliser le jeton dans un autre programme. Le code
  actuel laisse la connexion à Claude Code : à garder ainsi.
- **Le nom et le logo.** On peut écrire en texte simple que Winlux « fait
  tourner Claude Code ». On ne peut pas utiliser les noms ou logos
  d'Anthropic « comme nom d'une fonction de votre produit » ni d'une
  manière qui suggère un partenariat. **« Assistant Claude »** comme nom de
  fonction et l'icône orange `winlux-claude-code.svg` qui rappelle les
  couleurs de Claude sont donc **à revoir** : préférer « Assistant IA »,
  avec « fonctionne avec Claude » dans la description, et une icône
  neutre.
- Couper la télémétrie par variables d'environnement
  (`scripts/winlux-assistant.py`, ligne ~264) utilise des réglages prévus
  par Claude Code : ce n'est pas une modification du binaire.

### ChatGPT / Codex (OpenAI)

Les mêmes principes valent (compte de l'utilisateur, logiciel officiel non
modifié, pas de reprise du logo ni du nom comme nom de fonction).
**Je n'ai pas relu les conditions d'OpenAI pour cette analyse** : à faire
avant de fusionner #300.

### Le modèle local (Qwen2.5-1.5B via llama.cpp)

Licence Apache-2.0, données qui restent sur la machine : c'est le choix le
plus simple juridiquement. Garder la licence du modèle à côté du fichier.

### Ce qu'il faut écrire aux utilisateurs

Au moment d'activer un assistant en ligne, un écran court doit dire :

1. que les questions, et les fichiers ou textes que l'utilisateur choisit
   d'envoyer, **partent chez Anthropic ou OpenAI**, hors de l'ordinateur ;
2. que c'est **le compte de l'utilisateur** et **ses conditions** qui
   s'appliquent (lien vers elles), pas celles de Winlux ;
3. que Winlux ne voit, ne garde et ne transmet rien lui-même ;
4. que les réponses d'une IA peuvent être fausses, et que Winlux demande
   confirmation avant toute action sur le système (déjà le cas, capture
   `assistant-claude/7-action-a-confirmer.png`) ;
5. que l'IA locale, elle, ne fait rien sortir de la machine.

---

## 6. Licences, Ubuntu et code source

### La licence du dépôt

Le dépôt est sous **GPL-3.0-or-later** (`LICENSE`, `NOTICES`). C'est un bon
choix pour un système d'exploitation : quiconque redistribue Winlux modifié
doit publier ses modifications. Deux remarques :

- **Code écrit par une IA.** Une grande partie du code a été écrite par
  Claude sous la direction de damien. Aux États-Unis, l'Office du droit
  d'auteur refuse la protection aux éléments sans auteur humain ; en
  France, la question n'est pas tranchée. Concrètement, cela pourrait
  affaiblir la possibilité de *faire respecter* la GPL sur certaines
  parties, pas le droit de les utiliser. Pas d'action urgente.
- **Contributions extérieures.** Si d'autres personnes contribuent, un
  « Developer Certificate of Origin » (une ligne `Signed-off-by` par commit)
  suffit à garder une trace de leur accord.

### Le risque certain : les sources de l'ISO

L'ISO contient du code sous GPL (noyau Linux, BusyBox, GRUB, FFmpeg,
Samba…). **Distribuer un binaire GPL oblige à fournir son code source
correspondant**, soit avec le binaire, soit par une offre écrite (GPL-2.0,
article 3), soit par un accès équivalent que le distributeur garantit
(GPL-3.0, article 6). Aujourd'hui :

- `NOTICES` dit que le noyau, GRUB et BusyBox sont « fournis séparément »,
  mais **aucune source n'est publiée** avec les versions GitHub ;
- je n'ai trouvé ni offre écrite ni archive de sources dans
  `release.yml` ou les scripts.

C'est le manquement le plus *certain* du projet, et c'est exactement ce qui
a valu des procès : la Software Freedom Conservancy a poursuivi en 2009
plusieurs fabricants qui distribuaient BusyBox sans ses sources. Le risque
d'un procès contre un projet libre de bonne foi est faible, mais une
demande publique serait embarrassante.

**Action** (pour le fil des versions, qui possède `release.yml`) :

1. Publier avec chaque version une archive des sources des paquets inclus
   (`apt-get source` sur la liste exacte des paquets de l'ISO), ou au
   minimum une **offre écrite valable 3 ans** dans `NOTICES` et dans l'ISO.
2. Pointer seulement vers les miroirs d'Ubuntu est insuffisant au sens
   strict : ces sources peuvent disparaître quand Ubuntu passe à une
   nouvelle version.
3. Corriger `NOTICES` : lister les vraies composantes de l'ISO (FFmpeg,
   Samba, Mesa, paquets Ubuntu, micrologiciels) au lieu de la liste
   courte actuelle.

### Canonical et la marque Ubuntu

La politique de propriété intellectuelle de Canonical (consultée le
2 novembre 2026) permet de redistribuer Ubuntu **sans modification** ; pour
une version modifiée non approuvée, **il faut retirer et remplacer les
marques** de Canonical. Winlux est une version modifiée. La politique
précise qu'elle ne réduit pas les droits donnés par les licences libres
des paquets.

**Action** : dans l'image installée, vérifier que `/etc/os-release`,
l'écran de démarrage, les fonds et les textes ne présentent pas le système
comme « Ubuntu », et écrire plutôt « construit à partir de paquets
Ubuntu ». Garder les dépôts Ubuntu officiels comme source de mises à jour
est permis.

---

## 7. Logiciels tiers installés par Winlux, et MSIX non signés

### Le constat

`runtime/src/setup/msix.rs` installe un paquet MSIX en vérifiant les
empreintes du *block map*, mais **pas la signature `AppxSignature.p7x`**
(le journal le dit : « présente, pas encore vérifiée »). Un paquet modifié
par un tiers pourrait donc être installé comme s'il venait de son éditeur.
Plus largement, Winlux installe des paquets Ubuntu, des `.exe`, des `.msi`
et des applications du Store.

### La responsabilité

- **Aujourd'hui**, Winlux est un logiciel libre, gratuit, livré avec
  l'exclusion de garantie de la GPL (articles 15 et 16) et un avertissement
  clair de « préversion » dans le `README.md`. La nouvelle directive
  européenne sur la responsabilité du fait des produits défectueux
  (directive (UE) 2024/2853) **exclut les logiciels libres fournis en dehors
  d'une activité commerciale**. Le risque est donc faible.
- **Si Winlux devient commercial** (vente, abonnement, dons réguliers
  importants, préinstallation), deux textes entrent en jeu :
  - la même directive 2024/2853, qui couvre alors les logiciels, y compris
    l'absence de mise à jour de sécurité ;
  - le **règlement européen sur la cyber-résilience** (règlement (UE)
    2024/2847), qui impose des obligations de sécurité aux « produits
    comportant des éléments numériques » mis sur le marché dans le cadre
    d'une activité commerciale : signalement des failles activement
    exploitées à partir de septembre 2026, obligations complètes à partir
    de décembre 2027. Les logiciels libres non monétisés en sont exclus.

  Installer du code non vérifié sans avertissement serait alors une faute
  difficile à défendre.

### Que faire

1. **Vérifier la signature MSIX** (chaîne de certificats et empreinte du
   paquet) ; en attendant, **refuser par défaut** un MSIX dont la signature
   n'est pas vérifiée, ou afficher un avertissement bloquant « Éditeur non
   vérifié » avec le nom tel qu'il figure dans le manifeste.
2. Ne jamais présenter un logiciel installé depuis le Store ou un MSIX comme
   « approuvé par Winlux » s'il n'a pas été vérifié.
3. Garder la règle actuelle : les gros composants propriétaires sont
   **téléchargés par l'utilisateur, depuis l'éditeur**, après installation.

---

## 8. Vie privée (RGPD) et site web

### Ce que Winlux envoie aujourd'hui

| Flux | Où | Données | Par défaut |
|------|----|---------|-----------|
| Recherche de mises à jour | GitHub (`winlux-update-apt.py`) | Adresse IP, version | Oui |
| Paquets et Store | Dépôts Ubuntu, éditeurs | Adresse IP, paquets demandés | À l'usage |
| Météo des widgets | **wttr.in**, service tiers (`winlux-widgets.py`) | Adresse IP, lieu (déduit de l'IP si non réglé), langue | **Oui** |
| Signalement de problème | Formulaire GitHub (issue **publique**), relais prévu | Contenu du rapport, compte GitHub | Non : affiché et masqué avant envoi, avec accord |
| Assistants Claude / ChatGPT | Anthropic / OpenAI | Questions de l'utilisateur | Non, désactivés par défaut |
| Lien avec le téléphone | Réseau local | Données échangées avec le téléphone | Non |

Le signalement est déjà bien conçu (rapport montré avant envoi, données
personnelles masquées, rien sans accord).

### Ce que dit le droit

Tant que damien **ne reçoit lui-même aucune donnée** (les flux vont
directement de l'ordinateur de l'utilisateur vers GitHub, Ubuntu, wttr.in,
Anthropic ou OpenAI), il n'est en principe **pas responsable de
traitement** au sens du RGPD pour ces flux. Il le devient dès qu'il
**héberge quelque chose qui reçoit des données** : le relais de
signalement (`docs/RELAIS-SIGNALEMENT.md`), un serveur de mises à jour, des
statistiques de téléchargement ou de visite, un forum.

Même sans obligation stricte, l'utilisateur doit **savoir** ce qui sort de
sa machine.

### Que faire

1. **Météo** : ne contacter wttr.in qu'après un premier choix de
   l'utilisateur (« Afficher la météo ? Votre adresse IP sera envoyée au
   service wttr.in »), ou masquer le widget météo jusqu'à ce choix.
2. **Une page « Confidentialité »** sur le site et dans Paramètres,
   reprenant le tableau ci-dessus en mots simples.
3. **Avant de mettre le relais en ligne** : écrire qui le gère, ce qu'il
   garde (rien, d'après sa documentation), combien de temps, et rappeler
   que l'issue créée sur GitHub est **publique**.
4. **Mentions légales du site** (loi n° 2004-575 pour la confiance dans
   l'économie numérique, article 6-III) : un éditeur non professionnel peut
   rester anonyme envers le public à condition d'indiquer le nom et
   l'adresse de l'hébergeur (GitHub, Inc.) et d'avoir donné son identité à
   celui-ci. Dès que le site devient professionnel (domaine propre, dons,
   vente), il faut l'identité complète de l'éditeur. **Action** : ajouter
   une page « Mentions légales » avec l'hébergeur, un contact, la mention
   de non-affiliation à Microsoft et le lien vers la licence. Le site
   actuel ne charge aucun outil de statistiques ni de police externe
   (vérifié dans `site/`) : pas de bandeau cookies nécessaire.

---

## Actions prioritaires

**Cette semaine (gratuit, dans le dépôt) :**

1. Ajouter la **mention de non-affiliation à Microsoft** (site, `README.md`,
   « À propos », installateur).
2. **Ne pas acheter `winlux-os.org` et ne pas déposer la marque** avant
   l'avis d'un conseil en propriété industrielle.
3. Publier **les sources GPL** avec les versions, ou une offre écrite, et
   corriger `NOTICES` (fil des versions).
4. Renommer « Assistant Claude » en « **Assistant IA** » et remplacer son
   icône ; accepter les **conditions commerciales d'Anthropic** ; relire
   celles d'OpenAI avant de fusionner #300.
5. **Bloquer par défaut les MSIX non vérifiés** ou afficher « Éditeur non
   vérifié ».
6. Ajouter la **règle de salle blanche** dans `AGENTS.md` ; reformuler les
   messages d'erreur recopiés de Windows ; confirmer l'origine des fichiers
   `sample.msi` et `tables.msi`.

**Ce mois-ci :**

7. Pages **Confidentialité** et **Mentions légales** du site ; météo
   activée seulement après accord.
8. Retirer les **marques Ubuntu** de l'image installée.

**Avant toute professionnalisation (domaine, marque, dons, vente) :**

9. **Consulter un conseil en propriété industrielle** sur le nom, et
   décider de garder « Winlux » ou de passer à un nom de repli.
10. Revoir les brevets de codecs, la directive 2024/2853 et le règlement
    sur la cyber-résilience, qui changent de portée dès qu'il y a une
    activité commerciale.

---

## Sources vérifiées le 2 novembre 2026

- Microsoft c. Lindows : [Wikipédia, *Microsoft Corp. v. Lindows.com, Inc.*](https://en.wikipedia.org/wiki/Microsoft_Corp._v._Lindows.com,_Inc.) ;
  [ITPro Today, « Microsoft Settles Lindows Case for $20 Million »](https://itprotoday.com/microsoft-windows/microsoft-settles-lindows-case-for-20-million) ;
  [InformationWeek, « Microsoft Settles With Lindows »](https://www.informationweek.com/software-services/microsoft-settles-with-lindows)
- Marque WINLUX n° 85700940 (annulée) : [Markinton](https://markinton.com/trademark/winlux-85700940) ;
  Win-Lux (Suisse) : [webwiki](https://webwiki.de/win-lux.ch) ;
  WinLux (winget) : [wingetgui](https://wingetgui.com/apps/Lparksi-WinLux)
- Google c. Oracle : [résumé du Copyright Office des États-Unis](https://www.copyright.gov/fair-use/summaries/google-llc-oracle-am-inc-2021.pdf)
- SAS Institute c. World Programming (C-406/10) : [Wikipédia](https://en.wikipedia.org/wiki/SAS_Institute_Inc_v_World_Programming_Ltd) ;
  [Society for Computers and Law](https://www.scl.org/2451-sas-institute-inc-v-world-programming-ltd-report-and-analysis/)
- Directive 2009/24/CE, article 6 : [texte](https://www.legislation.gov.uk/eudr/2009/24/article/6)
- BSA c. Ministerstvo kultury (C-393/09) : [Fieldfisher](https://www.fieldfisher.com/en/insights/graphical-user-interfaces-can-be-protected-by-copyright-rules-european-court) ;
  [Kluwer Copyright Blog](https://legalblogs.wolterskluwer.com/copyright-blog/protection-of-guis-graphical-user-interfaces-some-comments-about-the-ecj-s-preliminary-ruling-in-bsa-v-ministervo-kultury/)
- Réforme européenne des dessins et modèles : [Dreyfus, guide de la réforme](https://www.dreyfus.fr/en/2026/01/05/eu-design-protection-complete-guide-to-the-2025-reform/) ;
  [Casalonga](https://www.casalonga.com/actualites-et-documentation-pi/modeles/modele-francais-218/actualites/designs-models-package-key-changes.html)
- Segoe UI : [fiche Microsoft](https://learn.microsoft.com/typography/font-list/segoe-ui)
- Redistribuables Visual C++ : [Microsoft, « Redistributing Visual C++ files »](https://learn.microsoft.com/cpp/windows/redistributing-visual-cpp-files)
- Licences Qwen2.5 : [annonce Qwen2.5](https://qwenlm.github.io/blog/qwen2.5-llm)
- Conditions de Claude Code : [Legal and compliance](https://code.claude.com/docs/en/legal-and-compliance) ;
  contexte : [The Register, 20 février 2026](https://www.theregister.com/2026/02/20/anthropic_clarifies_ban_third_party_claude_access)
- Politique de Canonical : [Intellectual property rights policy](https://canonical.com/legal/intellectual-property-policy)

**Cités de mémoire, non revérifiés en ligne pour ce document** (à confirmer
par un professionnel avant de s'en servir) : *Apple Computer, Inc. c.
Microsoft Corp.* (9e circuit, 1994) ; les procès BusyBox de la Software
Freedom Conservancy (2009) ; l'affaire Microsoft c. TomTom (2009) ; les
numéros et dates du règlement (UE) 2024/2847 sur la cyber-résilience et de
la directive (UE) 2024/2853 ; l'article 6-III de la loi pour la confiance
dans l'économie numérique.
