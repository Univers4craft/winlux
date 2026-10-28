# Compatibilité des versions Windows de Claude et ChatGPT

Objectif demandé : exécuter leurs vrais programmes Windows dans Winlux.
Les clients Linux et les pages web ne valident pas cet objectif.

## Vérification du 26 octobre 2026

### ChatGPT Windows Classic

- Page officielle : https://chatgpt.com/download/
- Installateur Windows lié par cette page :
  https://get.microsoft.com/installer/download/9NT1R1C2HH7J?cid=website_cta_psi
- SHA-256 du fichier examiné :
  `e54f883db0b89e94b3888822cf8f4d0c7f33cd23ca7a981ddd76282455f311f3`.
- Analyse statique : PE32 managé, marqueur `.NETFramework,Version=v4.7.2`,
  références `PresentationFramework`, `PresentationCore`, `WindowsBase`,
  `System.Runtime.WindowsRuntime` et `Windows.Management.Deployment`.
- Il s'agit du lanceur d'installation Store, pas du binaire installé de ChatGPT.
- `winlux --inspect` affiche `gui` : cette classification ne prouve absolument
  pas l'exécution. L'installateur n'a pas été exécuté ni installé sur l'hôte.
- Blocages : CLR Windows/WPF et déploiement Store/AppX non disponibles dans
  Winlux. Le pont Mono Linux actuel ne remplit pas ces dépendances.

La nouvelle application ChatGPT Work et l'application Classic sont des produits
distincts sur la page de téléchargement ; ce relevé ne couvre que le lien
Classic Windows ci-dessus. Il faut obtenir et analyser le paquet exact voulu
avant d'étendre ces conclusions à un autre installateur.

### Claude Windows

- Source officielle : https://claude.com/download
- Lien Windows x64 :
  https://claude.ai/api/desktop/win32/x64/setup/latest/redirect
- La requête depuis l'environnement de vérification a renvoyé HTTP 403.
  Aucun exécutable Claude n'a donc été examiné et aucun résultat d'exécution
  n'est revendiqué.
- La documentation de déploiement officielle décrit des paquets MSIX et
  indique que Cowork dépend de Virtual Machine Platform et de services Windows :
  https://support.claude.com/en/articles/12622703-deploy-claude-desktop-for-windows
- Le fonctionnement du chat et celui de Cowork devront être validés séparément.

## Travail restant

- [ ] Obtenir l'installateur Claude et le paquet exact de ChatGPT visé.
- [ ] Relever versions, architectures, signatures et dépendances des binaires
  réellement installés, au-delà de leurs lanceurs de téléchargement.
- [ ] Implémenter et tester les dépendances Windows requises (CLR/WPF,
  AppX/MSIX/WinRT, puis services et graphismes selon les paquets observés).
- [ ] Tester dans une VM jetable : installation, lancement de l'interface,
  connexion interactive par l'utilisateur, discussion et redémarrage.
- [ ] Tester mise à jour et désinstallation sans perte de données personnelles.

Aucun statut « compatible » avant réussite de ces tests. Les binaires
propriétaires téléchargés restent des artefacts locaux ignorés par Git.
