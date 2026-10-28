# Diagnostic du véritable ChatGPT Windows — 28 octobre 2026

## Fichiers officiels testés

Les fichiers restent dans `build/compatibility/chatgpt/`, exclus du dépôt et de
l'ISO. Aucun programme tiers renommé, lanceur Linux ou site web n'est substitué
au programme Windows.

- Installateur **ChatGPT Classic**, lié depuis
  [la page officielle OpenAI](https://chatgpt.com/download/) :
  `https://get.microsoft.com/installer/download/9NT1R1C2HH7J?cid=website_cta_psi`.
  SHA-256 : `13838bc895273e460d7441a25da281e4958f78967d521173f7c3c0a0b42c8143`.
  C'est un installateur Microsoft Store PE32 managé, avec des références WPF.
- Véritable **ChatGPT.exe x64** extrait sans modification du paquet officiel
  `https://persistent.oaistatic.com/codex-app-prod/ChatGPT-x64.msix`, publié dans
  [la documentation de déploiement OpenAI](https://learn.chatgpt.com/docs/enterprise/windows-deployment).
  Ce paquet concerne la nouvelle application ChatGPT avec Codex, distincte de
  Classic : identité `OpenAI.Codex`, nom affiché ChatGPT, version `26.1023.6123.0`.
  SHA-256 MSIX : `9890e869a0d40fc00f396ce48ba05dc45ccb8bb36d151dcd1a2ff23d270f2e4d`.
  SHA-256 de `app/ChatGPT.exe` : `5871c61efd9db4026e30bfef21774ff9e6bd815127fffbb4d04262c42d7cddde`.

## Résultat réel

L'installateur Classic s'arrête sur l'absence de Mono/.NET Framework dans l'image.
Ajouter Mono ne fournirait pas à lui seul WPF ni les services Microsoft Store.

Le programme x64 a été tenté dans une VM jetable, avec ses DLL officielles
voisines. C'est un diagnostic de chargement, pas une installation MSIX complète.
Après les corrections ci-dessous, il quitte avec le code 1 :

```text
winlux[error/loader]: chargement impossible : DLL introuvable : WINHTTP.dll
(ni intégrée à Winlux, ni présente à côté du programme)
```

**ChatGPT Windows n'est pas fonctionnel sur Winlux à ce stade.** Le test ne
valide ni l'écran de connexion, ni les conversations, ni son bac à sable.
Le journal du dernier essai local est `build/native-vm.lEHOui/` et
`/tmp/winlux-chatgpt-rpc-vm.log` ; les essais précédents se trouvent dans les
autres répertoires `build/native-vm.*`.

## Corrections et suite

- [x] Corriger les redirections API-set COM, registre, RTL, sécurité et downlevel
  vers les backends correspondants, sans inventer leurs exports absents.
- [x] Fournir `EventRegister`, `EventUnregister`, `EventWriteTransfer` et le contrat
  eventing pour des fournisseurs sans session de capture. Les handles sont
  validés et retirés ; ce n'est pas un contrôleur ETW ni une capture ETL.
- [x] Fournir `rpcrt4!UuidCreate` avec de véritables UUID v4 aléatoires, sans
  prétendre implémenter le transport RPC.
- [ ] Implémenter le sous-ensemble WinHTTP requis, avec HTTPS, validation des
  certificats, proxys et tests réseau réels. Ne pas contourner TLS.
- [ ] Relever puis implémenter les prochaines dépendances et fonctions Windows
  utilisées par le programme et les DLL Chromium, dont les piles graphiques.
- [ ] Fournir une installation MSIX et une activation adaptées ; ne pas confondre
  l'extraction d'un exécutable avec son installation.
- [ ] Tester le démarrage complet, la connexion utilisateur, les conversations,
  les processus auxiliaires et les mises à jour.

Les fonctions ETW suivent le comportement documenté par Microsoft lorsqu'aucune
session ne consomme les événements :
[EventRegister](https://learn.microsoft.com/en-us/windows/win32/api/evntprov/nf-evntprov-eventregister),
[EventWriteTransfer](https://learn.microsoft.com/en-us/windows/win32/api/evntprov/nf-evntprov-eventwritetransfer).

Pour reproduire le diagnostic avec les fichiers déjà téléchargés :

```sh
WINLUX_EXTERNAL_EXE="$PWD/build/compatibility/chatgpt/ChatGPT.exe" \
WINLUX_EXTERNAL_DLLS=1 sh scripts/test-native-session-vm.sh
```

Les DLL voisines sont ajoutées uniquement au disque temporaire. Ne pas présenter
la réussite des tests du bureau comme une validation de compatibilité ChatGPT.
