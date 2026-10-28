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

Le programme x64 est testé dans une VM jetable avec son dossier officiel
complet, sans réseau. C'est un diagnostic de démarrage, pas une installation
MSIX complète. L'ancien blocage `WINHTTP.dll` est dépassé. Le suivi des images
PE par les API mémoire permet maintenant de dépasser l'initialisation de
`chrome_elf.dll`, qui échouait avec LastError 487.

Les essais suivants ont successivement rencontré `RtlFormatCurrentUserKeyPath`,
`NtCreateKey` et `NtQueryValueKey`. Ces opérations sont désormais reliées au
registre persistant existant (voir [REGISTRY.md](REGISTRY.md)). L'essai suivant
rencontrait un piège logiciel `SIGTRAP`, RVA `0x249990` dans ChatGPT.exe, juste
après la recherche infructueuse de `shell32!CommandLineToArgvW`.

Après implémentation réelle de cette fonction (UTF-16, arguments vides,
guillemets, antislashs et allocation libérable par `LocalFree`), le nouvel essai
dépasse ce piège et s'arrête sur `kernel32!GetPrivateProfileStringW`, non encore
implémentée. Aucun contrôle du programme n'a été désactivé.

**ChatGPT Windows n'est pas fonctionnel sur Winlux à ce stade.** Le test ne
valide ni l'écran de connexion, ni les conversations, ni son bac à sable.
Les résultats locaux structurés se trouvent dans
`build/compatibility/native-registry-2026-10-28/results.json` ; les journaux
d'étapes sont `/tmp/winlux-image-vendor-vm.log`,
`/tmp/winlux-registry-vendor-vm.log`, `/tmp/winlux-create-vendor-vm.log` et
`/tmp/winlux-query-vendor-vm.log`, `/tmp/winlux-trap-vendor-vm.log` et
`/tmp/winlux-argv-vendor-vm.log`.

## Corrections et suite

- [x] Corriger les redirections API-set COM, registre, RTL, sécurité et downlevel
  vers les backends correspondants, sans inventer leurs exports absents.
- [x] Fournir `EventRegister`, `EventUnregister`, `EventWriteTransfer` et le contrat
  eventing pour des fournisseurs sans session de capture. Les handles sont
  validés et retirés ; ce n'est pas un contrôleur ETW ni une capture ETL.
- [x] Fournir `rpcrt4!UuidCreate` avec de véritables UUID v4 aléatoires, sans
  prétendre implémenter le transport RPC.
- [x] Fournir un premier backend WinHTTP synchrone HTTP/HTTPS via curl, avec
  validation TLS et tests HTTP réels, lecture, envoi et rejet d'un certificat
  non fiable. Les proxys nommés sont partiels ; PAC/WPAD et mode asynchrone
  restent à implémenter. Ce premier backend n'est pas WinHTTP complet.
- [x] Suivre les images PE dans `VirtualQuery`/`VirtualProtect` et relier les
  opérations NT de mémoire à de vraies protections, sans faux succès.
- [x] Relier les premières opérations NT du registre au backend Win32 existant.
- [x] Fournir `CommandLineToArgvW` et confirmer que le véritable programme
  dépasse le piège observé après sa recherche dynamique.
- [ ] Implémenter la lecture Unicode des fichiers INI (`GetPrivateProfileStringW`).
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

## Claude Windows

Le véritable `ClaudeSetup.exe` officiel est testé séparément du véritable
`claude.exe` extrait de son MSIX, avec son dossier d'application complet. Les
deux restent exclus du dépôt et des images publiées.

- L'installateur Go dépasse les précédents manques `LoadLibraryEx`,
  `bcryptprimitives!ProcessPrng` et les API de mode d'erreur. Il s'arrête sur
  `kernel32!AddVectoredExceptionHandler` : l'enregistrement existant dans ntdll
  ne constitue pas encore une vraie distribution des exceptions Windows.
- L'application s'arrête au chargement de `CRYPT32.dll`, encore absente.
  Il faut implémenter les opérations cryptographiques et de certificats utiles,
  sans inventer une validation de confiance réussie.

**Claude Windows n'est pas fonctionnel non plus.** Ces essais utilisent
uniquement le moteur natif Winlux, sans Wine et sans remplacement par un site
web ou une application Linux.
