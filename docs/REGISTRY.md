# Registre Windows et configuration Linux

Winlux possède déjà un registre Windows persistant, implémenté dans
`runtime/src/dlls/advapi32.rs`. Les données sont enregistrées dans
`~/.local/share/winlux/registry.tsv` (ou le répertoire de données Winlux du
profil). `WINLUX_REGISTRY_PATH` permet aux tests de choisir un fichier isolé.
Ce n'est pas un format de ruche Windows natif ni un éditeur Regedit complet.

## Stockage commun aux API Windows

Les API `RegCreateKeyEx`, `RegOpenKeyEx`, `RegSetValueEx`, `RegQueryValueEx`,
suppression et fermeture utilisent ce stockage. Les valeurs conservent leur
type et leurs octets ; les noms sont insensibles à la casse. Les modifications
sont persistées avec verrouillage et remplacement du fichier.

Les appels bas niveau suivants réutilisent maintenant ce même backend :

- `RtlFormatCurrentUserKeyPath` alloue un chemin Unicode, libérable par
  `RtlFreeUnicodeString`.
- `NtOpenKey` et `NtOpenKeyEx` ouvrent une clé existante ; ils n'inventent pas
  les clés manquantes. Les options étendues non prises en charge sont refusées.
- `NtCreateKey` crée réellement une clé persistante, ou ouvre une clé déjà
  présente, et indique laquelle de ces opérations a eu lieu.
- `NtQueryValueKey` lit les valeurs dans les formats Basic, Full et Partial,
  avec les tailles requises et les erreurs de tampon trop petit ou tronqué.
- `NtClose` ferme aussi les handles de registre et signale un handle invalide.

Les chemins `\Registry\Machine` sont reliés à HKLM. Le chemin utilisateur
`\Registry\User\S-1-22-1-<uid Linux effectif>` est relié à HKCU. Il représente
l'identité Linux locale ; cela n'implémente ni les jetons de sécurité Windows,
ni l'impersonation, ni un domaine Active Directory. Un autre utilisateur n'est
pas redirigé vers HKCU. Les ruches restent dans le fichier du profil Winlux ;
HKLM n'est pas encore un magasin machine partagé entre tous les comptes Linux.

Les clés volatiles, les options backup/restore et les métadonnées de classe de
`NtCreateKey` ne sont pas prises en charge. Les autres formats de requête,
l'écriture NT des valeurs et les ACL Windows restent à compléter ; les fonctions
Win32 de lecture/écriture existent déjà.

## Côté Linux

Le fichier du registre peut être consulté depuis Linux et le catalogue
d'applications Winlux le lit déjà. Cela ne transforme pas les configurations
Linux en clés de registre : les paramètres système Winlux utilisent notamment
`/etc/winlux/input.conf`, `preferences.conf` et `display.conf`, et le bureau
natif utilise aussi Xfconf et les fichiers de configuration du profil.

Un éditeur graphique unifié et la synchronisation explicite de certains
paramètres Windows/Linux restent à développer. Il ne faut pas annoncer ces
fonctions comme disponibles sur la seule base du stockage `registry.tsv`.

## Vérifications

Le test `persiste_valeurs_binaires_et_respecte_la_casse_insensible` vérifie la
persistance, l'ouverture d'une même clé par les API Win32 et NT, la lecture
des mêmes octets, la création NT visible depuis Win32, les dispositions
créée/existante, la libération du chemin Unicode et la fermeture des handles.
Il vérifie aussi qu'un chemin utilisateur inconnu n'ouvre pas HKCU et que les
options non implémentées ne renvoient pas un faux succès.
Les lectures NT sont vérifiées dans les trois formats, y compris les tailles,
les données binaires et l'absence d'écriture au-delà du tampon fourni.
