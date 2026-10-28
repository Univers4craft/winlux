# Audit des mises à jour du 28 octobre 2026

## Cause de l'absence des ajouts dans la VM

La release GitHub `v0.2.2` (construction `40683903421`) est verte, mais son
ISO n'a pas été construite : le noyau `/boot/vmlinuz-6.8.0-147-generic`
était illisible par le compte de construction. Le workflow ignorait cette
erreur puis publiait un paquet `--runtime` de 4,6 Mo. Ce paquet change le
numéro de version et le moteur, sans remplacer `gui_demo.exe`, le menu GTK,
les applications ni la session native. Les releases v0.2.0 et v0.2.1 doivent
également être considérées comme incomplètes tant que leur contenu système
n'a pas été vérifié. Un statut Actions vert ne suffisait donc pas.

Le workflow copie désormais le noyau avec les droits nécessaires puis construit
sans privilèges. Tout échec de construction bloque la publication. Une release
doit contenir un paquet `scope=system`, avec bureau et applications, et passer
les validations ISO/installation. Aucun remplacement silencieux par un paquet
runtime n'est autorisé.

## Autres écarts constatés et corrections

- La page Souris modifiait uniquement `MOUSE_SPEED` en mémoire. Les trois
  choix appellent maintenant libinput via `winlux-control`, sauvegardent le
  réglage dans `/etc/winlux/input.conf` et le réappliquent au démarrage de la
  session native. Sans périphérique compatible, une erreur est signalée.
  Une tablette absolue de VM peut ne pas exposer de réglage d'accélération.
- Les catégories ajoutées au menu du shell PE ne modifiaient pas le menu GTK
  utilisé par le bureau natif. Les catégories sont désormais aussi affichées
  dans « Toutes les applications » du menu natif, à partir des vrais fichiers
  desktop installés. Recherche et actions contextuelles sont conservées.
- Les actions `pointer-normal`, `pointer-large`, `pointer-xlarge`,
  `contrast-on` et `contrast-off` proposées par les paramètres n'ont toujours
  pas de backend dans `winlux-control`. Leur implémentation reste à faire ;
  ce ne sont pas des fonctionnalités validées.
- Les effets et options du shell PE ne s'appliquent pas automatiquement au
  menu GTK, à Openbox et au panneau Xfce. Cette intégration reste à vérifier
  option par option, sans écraser les préférences personnelles existantes.

## Validation et livraison

La préparation de v0.2.3 a aussi révélé des pages de paramètres masquées par
des cas de navigation dupliqués. Écran, À propos, Exécutable, Accessibilité et
Effets visuels ont désormais des pages distinctes ; Mises à jour utilise à
nouveau son vrai contenu. Les lanceurs natifs ouvrent les pages 7 (mises à
jour) et 11 (navigateurs). Les identifiants des clics Souris correspondent
maintenant aux boutons dessinés. Les pseudo-boutons de conservation de
résolution et de fréquence, sans implémentation et qui débordaient ou
recouvraient d'autres contrôles, ont été retirés ; l'activation automatique
réelle du service de mise à jour est conservée. La compilation refuse les
cas de navigation dupliqués et le diagnostic VM vérifie les pages ouvertes
par les vrais lanceurs du bureau.

Les tests de contrôle couvrent l'appel libinput, la sauvegarde et la télémétrie
de la vitesse. Ils utilisent un périphérique simulé et ne remplacent pas un
essai de souris dans une VM. Les changements de source ne réparent pas les
anciens paquets publiés : une nouvelle construction système validée est
nécessaire avant de déclarer la VM à jour avec ces fonctionnalités.

La compatibilité native ChatGPT.exe et Claude.exe reste un chantier distinct,
sans Wine. Aucune réussite de démarrage complet n'est revendiquée ici.
