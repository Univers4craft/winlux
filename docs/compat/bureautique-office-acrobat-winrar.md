# Bureautique : WinRAR, Acrobat Reader, Word, Excel, PowerPoint, Outlook

Inventaire du 11 novembre 2026 (master 9e8b080), demandé par damien.
**Objectif immédiat (consigne de damien) : que les six s'installent** ;
il prend ensuite la main pour se connecter.
Compatibilité .exe pendant le gel 0.2.21 : aucune nouveauté d'OS, chaque
défaut constaté devient une issue française, la PR qui le corrige dit
`Fixes #N`.

Sources : documentation publique des éditeurs seulement (pages de
téléchargement, guides de déploiement en entreprise, CLUF). Rien n'a été
désassemblé ni lu dans un binaire d'éditeur ; les besoins exacts seront
**mesurés au banc**, dans la VM, par `winlux --setup --info` et
`scripts/compat-imports.py` (fiche
`winlux/tests/bureautique-1-winrar-acrobat-office-mesure.md`). Les colonnes
« probable » ci-dessous sont des hypothèses à confirmer par cette mesure.

## Ordre de passage

| Rang | Logiciel | Pourquoi à ce rang | Licence |
|---|---|---|---|
| 1 | **WinRAR** (x64) | Win32 classique, un seul processus, pas de réseau ni de compte | Essai de 40 jours, puis rappel à chaque lancement ; licence payante chez win.rar GmbH |
| 2 | **Acrobat Reader** (x64) | Gratuit sans compte, mais MSI + correctif `.msp`, processus bac à sable, CEF | Gratuit, CLUF à accepter, compte Adobe facultatif |
| 3 | **Nouvel Outlook** | WebView2 dans un paquet MSIX : chemin déjà ouvert par #236 et la boutique | Gratuit avec un compte Microsoft (ou un compte IMAP) |
| 4 | **Word, Excel, PowerPoint** (+ Outlook classique) | Click-to-Run : service système, virtualisation du registre, COM partout, connexion au compte | Compte de damien (Microsoft 365 ou Office 2024) : il se connecte une fois l'installation faite |

## 1. WinRAR

- **Obtention** : `winrar-x64-<ver>fr.exe` sur rarlab.com (page « WinRAR and
  RAR archiver downloads », versions localisées). Version, éditeur
  (win.rar GmbH), source et SHA-256 notés au banc.
- **Installeur** : exécutable auto-extractible propre à WinRAR (format RAR),
  qui lance ensuite son propre programme d'installation. Winlux ne reconnaît
  pas ce format (`setup::detect` connaît NSIS, Inno, 7z, Squirrel, MSIX) : il
  le charge donc **comme un programme ordinaire**, ce qui est le chemin voulu
  (le module SFX est un vrai programme Win32 qui décompresse lui-même). À
  vérifier : que l'assistant d'installation s'affiche, écrit dans
  `C:\Program Files\WinRAR`, crée les raccourcis et la clé `Uninstall`.
- **Ce que l'appli demande (probable)** : `comctl32` complet (ListView en
  mode détails, TreeView, barre d'outils, barre d'état), dialogues
  `comdlg32`, glisser-déposer OLE, presse-papiers de fichiers
  (`CF_HDROP`), `SHFileOperation`, `SHGetFileInfo`, icônes système
  (`SHGetImageList`), associations de fichiers (HKCR), notification de
  changement de dossier. L'extension de menu contextuel (`RarExt.dll`,
  serveur COM de l'Explorateur Windows) n'a pas d'hôte chez nous : le
  gestionnaire de fichiers natif n'appelle pas d'extension shell. Ce n'est
  pas un défaut ; le menu contextuel « Extraire ici » existe déjà côté natif
  pour ZIP (`runtime/src/archive`), et RAR passe par `unrar` s'il est là.
- **Licence** : essai de 40 jours ; ensuite une fenêtre de rappel à chaque
  lancement. Pour le banc, l'essai suffit. Aucune licence n'est à saisir.

## 2. Adobe Acrobat Reader

- **Obtention** : programme d'installation **hors ligne** de la page
  publique « Acrobat Reader enterprise download » d'Adobe
  (`AcroRdrDCx64<ver>_fr_FR.exe`), pas le petit téléchargeur de
  get.adobe.com, qui passe par un gestionnaire de téléchargement. Gratuit.
- **Installeur** : exécutable auto-extractible d'Adobe, qui dépose un
  `setup.exe`, un `setup.ini`, la base `AcroPro.msi`, ses `.cab` et un
  **correctif `AcroRdrDCx64Upd<ver>.msp`** appliqué par-dessus.
  **Écart connu** : notre msiexec ne prend pas encore en charge les
  correctifs `.msp` ni les transformations `.mst` (`docs/MSIEXEC.md`,
  « Pas encore pris en charge »). Sans le `.msp`, on installerait la version
  de base, plus ancienne, qui voudra se mettre à jour. C'est le premier
  chantier probable.
- **Ce que l'appli demande (probable)** :
  - mode protégé : un processus courtier et un processus de rendu à jeton
    restreint, objets de travail (job objects), niveau d'intégrité bas.
    Adobe documente la clé de déploiement
    `HKLM\SOFTWARE\Policies\Adobe\Acrobat Reader\DC\FeatureLockDown`,
    `bProtectedMode = 0` : si le bac à sable bloque, c'est la porte de
    secours, à proposer explicitement (règle « aucun réglage caché ») et
    non à poser en silence ;
  - CEF (Chromium Embedded) pour l'accueil et les outils : même famille que
    libcef et #236 ;
  - service de mise à jour Adobe (`AdobeARMservice`) et tâche planifiée :
    services Winlux (#1643, #1675) ;
  - rendu par Direct2D/Direct3D 11, polices DirectWrite, impression ;
  - associations `.pdf`, aperçu et vignettes (extensions shell, pas d'hôte).
- **Licence** : gratuit. Le compte Adobe n'est pas nécessaire pour lire un
  PDF ; on le refuse au premier lancement.

## 3. Outlook

Deux produits portent ce nom :

- **Nouvel Outlook pour Windows** : application WebView2 distribuée en
  paquet MSIX (boutique, ou programme d'installation public de Microsoft).
  Il demande WebView2 (notre moteur WebKitGTK, #236), le déploiement MSIX
  (`setup/msix.rs`), et la connexion à un compte (Microsoft, ou un compte
  IMAP tiers, ce qui évite de dépendre de Microsoft au premier essai).
  C'est **le chemin le plus court vers « emails et calendrier »**.
- **Outlook classique** : fait partie de Microsoft 365 / Office, mêmes
  exigences que Word (section 4), plus MAPI, le profil de messagerie et le
  cache `.ost`.

## 4. Word, Excel, PowerPoint (Microsoft 365 ou Office 2024)

- **Obtention** : Click-to-Run. Pour le banc, l'**outil de déploiement
  d'Office** (Office Deployment Tool, publié par Microsoft, documenté
  publiquement) avec un fichier `configuration.xml` : `setup.exe /download`
  récupère les fichiers sans licence, `setup.exe /configure` installe. Pas
  de MSI.
- **Ce que l'installation demande (probable)** :
  - le service `ClickToRunSvc` (`OfficeClickToRun.exe`) lancé en compte
    système, qui installe et répare tout ;
  - une couche de virtualisation : registre sous
    `HKLM\SOFTWARE\Microsoft\Office\ClickToRun\REGISTRY`, fichiers dans
    `Program Files\Microsoft Office\root` ; à mesurer ;
  - des fichiers de données de plus de 2 Gio (le flux de produits
    `stream.x64.*.dat`) : à surveiller, Winlux a déjà refusé des fichiers
    de cette taille ailleurs ;
  - téléchargement par HTTPS (WinHTTP, Schannel, #235), BITS possible.
- **Ce que les applis demandent (probable)** : COM complet et hors
  processus, OLE (incorporation, glisser-déposer, presse-papiers riche),
  automatisation (#16), DirectWrite et Direct2D partout, `mso.dll`
  partagée par toutes les applis, VBA, compléments, et la connexion :
  Office passe par le gestionnaire de comptes Web de Windows (WAM, WinRT),
  avec repli documenté par Microsoft vers une fenêtre de connexion
  intégrée (WebView2). Rien de cela n'est petit ; c'est le chantier XL.
- **Licence — consigne de damien (11 nov.)** : l'objectif est que
  l'installation aboutisse ; c'est damien qui se connecte ensuite à son
  compte (abonnement Microsoft 365 ou licence Office 2024). Le banc va
  jusqu'à l'écran « Se connecter pour activer Office » et s'arrête. Nous ne
  créons ni compte ni essai à sa place.

## Ce qui ne bouge pas

Aucun fichier d'éditeur dans le dépôt ni dans `/mnt/project-files` : seuls
nos journaux et les listes produites par `compat-imports.py` en sortent.
VLC et 7-Zip marchent déjà ; Discord, WhatsApp Desktop et Teams relèvent du
fil Electron.
