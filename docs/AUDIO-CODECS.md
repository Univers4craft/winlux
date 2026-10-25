# MP3 et AAC dans Winlux

Premier lot de compatibilité multimédia : `winmm!PlaySoundA/W` décode des fichiers
locaux WAVE, MP3 (libmpg123) et AAC ADTS mono/stéréo (libfaad2), puis utilise ALSA.
Ce n'est ni une implémentation Media Foundation/DirectShow, ni une chaîne vidéo.
Les conteneurs MP4/M4A, le streaming, les boucles et le son en mémoire restent exclus.

## Sécurité et dépendances

- Types FFI C natifs (`long`, `unsigned long`), structure FAAD complète avec
  `channel_position[64]`, configuration obtenue depuis la bibliothèque.
- Initialisation mpg123 unique ; pas d'arrêt global concurrent. Un décodeur par
  appel, détruit par RAII sur succès comme sur erreur. Les API chargées restent
  disponibles jusqu'à la fin du processus.
- Limites par clip : entrée 32 Mio, PCM 64 Mio, 32768 trames ADTS ; mono/stéréo,
  fréquence de sortie entre 8 et 192 kHz, format constant pendant le clip.
- ADTS incomplet ou incohérent refusé avant décodage, allocation lisible avec
  marge FAAD en fin d'entrée. Les bibliothèques natives doivent rester à jour :
  ces contrôles ne constituent pas un bac à sable pour des fichiers malveillants.
- Le rééchantillonnage SBR implicite est désactivé ; le mono AAC-LC dupliqué
  par certaines versions de FAAD est ramené en mono en l'absence de PS déclaré.
- Une bibliothèque manquante ou un format refusé produit une erreur PlaySound,
  sans inventer de réussite. WAVE fonctionne sans ces décodeurs optionnels.
- Le constructeur ISO inclut explicitement mpg123, faad et ALSA avec leurs
  dépendances, car `ldd` du runtime ne voit pas les appels `dlopen`.
  Les anciennes ISO ne sont pas modifiées par ce changement de code.

Paquets de développement/test Debian/Ubuntu : `libmpg123-0`, `libfaad2`.
Pour produire l'ISO, ALSA (`libasound2` ou `libasound2t64`) est aussi nécessaire.
La lecture audible nécessite un périphérique ALSA utilisable ; les tests de
décodage ne prouvent pas le fonctionnement d'une carte son physique.

## Vérification

```sh
ALSA_CONFIG_PATH="$PWD/runtime/tests/audio/alsa-null.conf" cargo test -p winlux winmm::codecs -- --include-ignored
```

Le test de fichiers réels est explicitement ignoré dans la suite par défaut,
car les bibliothèques sont optionnelles pour le runtime. La CI les installe et
exécute ce test obligatoirement : sinus mono/stéréo, limites PCM, entrée invalide,
troncature AAC et huit décodages simultanés. Un autre test appelle PlaySoundA/W
avec un périphérique ALSA `null` silencieux (aucune carte physique nécessaire).
Les fixtures sont générées, sans
enregistrement ni musique provenant d'un tiers.

Les bibliothèques natives gardent leurs licences respectives :
[mpg123 (LGPL)](https://www.mpg123.de/), [FAAD2 (GPL)](https://github.com/knik0/faad2).
La distribution doit conserver leurs notices et respecter leurs obligations.
Le constructeur copie les notices disponibles des paquets système ; fournir
les sources correspondantes ou une offre conforme reste requis pour distribuer
les binaires GPL/LGPL. Ce lot n'a pas produit de nouvelle ISO distribuée.

Références ABI : [mpg123.h](https://github.com/madebr/mpg123/blob/master/src/include/mpg123.h),
[neaacdec.h](https://github.com/knik0/faad2/blob/master/include/neaacdec.h).
