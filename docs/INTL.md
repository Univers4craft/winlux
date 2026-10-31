# Accents, polices, langue, clavier et heure

Les programmes Windows suivent les réglages de Linux et échangent texte et
polices avec lui sans casser les accents. Tout est écrit de zéro dans
`runtime/src/intl/` (issue #183).

## Encodages (`codepage.rs`, `text.rs`)

- Pages de code : Windows-1252 (ANSI), 850 et 858 (OEM d'Europe de l'Ouest),
  437, ISO 8859-1 et 8859-15, ASCII et UTF-8. `CP_ACP`/`CP_OEMCP` prennent la
  page de la locale.
- `MultiByteToWideChar` / `WideCharToMultiByte` : `MB_ERR_INVALID_CHARS`,
  `WC_ERR_INVALID_CHARS`, caractère par défaut et `lpUsedDefaultChar`,
  substitution « meilleur ajustement » de Windows (Ł → L) sauf avec
  `WC_NO_BEST_FIT_CHARS`. Aussi `GetCPInfoEx`, `CharToOem`/`OemToChar`,
  `IsTextUnicode`.
- Console : page OEM de la locale par défaut, comme Windows ; les octets sont
  convertis en UTF-8 pour le terminal Linux selon `SetConsoleOutputCP`.
- ZIP : un nom sans l'indicateur UTF-8 venant de Windows (hôte FAT/NTFS) est
  lu dans la page OEM ; le champ « Unicode Path » (0x7075) de 7-Zip et WinRAR
  est prioritaire quand son CRC correspond.
- Fichiers texte : `winlux --text info|linux|windows [--utf16|--ansi] FICHIER…`
  détecte BOM, UTF-16 avec ou sans BOM, UTF-8 ou page ANSI, et convertit CRLF ↔
  LF. Le fichier est remplacé de façon atomique.

## Langue (`locale.rs`)

La langue vient de Paramètres (`/etc/winlux/locale.conf`, `LANG=`) ; la région
vient de glibc (`fr_BE.UTF-8`…) quand elle a la même langue. Locales décrites :
fr-FR, fr-BE, fr-CH, fr-CA, en-US, en-GB, es-ES, es-MX, de-DE, de-AT, de-CH,
it-IT, pt-PT, pt-BR et l'invariante. `GetLocaleInfo(A/W/Ex)` répond aux LCTYPE
courants (séparateurs, formats, jours et mois, codes ISO, pages de code,
`LOCALE_RETURN_NUMBER`) ; `GetDateFormat(A/W/Ex)`, `GetTimeFormat(A/W/Ex)` et
`GetNumberFormat(W/Ex)` appliquent les images Windows. Une locale non décrite
est refusée (`ERROR_INVALID_PARAMETER`) plutôt que remplacée par le français.
`WINLUX_LOCALE=de-CH` force une locale pour un test.

## Clavier (`keyboard.rs`)

La disposition vient de `/etc/winlux/input.conf` (Paramètres), puis de
`/etc/default/keyboard`. `GetKeyboardLayout` renvoie le HKL Windows
(`040C040C` pour l'AZERTY), `GetKeyboardLayoutName` le KLID (`0000040C`).
Les touches mortes sont composées comme le pilote Windows : `^` puis `e` donne
`ê`, `^` puis espace donne `^`, `^` puis `x` donne `^x`.

## Heure (`tz.rs`)

Les .exe lisent le même fuseau que Linux : `TZ`, sinon `/etc/localtime` réglé
par Paramètres ou l'installateur. La règle POSIX finale du fichier TZif
(« CET-1CEST,M3.5.0,M10.5.0/3 ») donne `TIME_ZONE_INFORMATION` ; le nom IANA
est traduit en nom Windows (« Romance Standard Time »). L'horloge matérielle
reste en UTC, partagée par Linux et les .exe : il n'y a pas de seconde horloge
Windows. `FileTimeToLocalFileTime` applique le décalage actuel (comme Windows),
`SystemTimeToTzSpecificLocalTime` celui de la date convertie. `_get_timezone`,
`_get_daylight` et `_get_dstbias` pour la CRT.

`winlux --intl` affiche ce que voient les .exe (locale, pages de code,
clavier, fuseau).

## Polices

- `C:\Windows\Fonts` et `%LOCALAPPDATA%\Microsoft\Windows\Fonts` sont ajoutés
  à fontconfig (`~/.config/fontconfig/conf.d/60-winlux-windows-fonts.conf`) :
  une police installée par un programme Windows est visible de toutes les
  applications Linux, et l'appli Polices la liste et peut la désinstaller.
- Les .exe voient déjà les polices Linux (`EnumFontFamilies` lit les dossiers
  de fontconfig).
- Remplacements aux métriques compatibles : Arial, Helvetica → Liberation Sans ;
  Times New Roman → Liberation Serif ; Courier New → Liberation Mono ; Segoe
  UI, Tahoma, MS Shell Dlg, Calibri, Cambria, Consolas… vers les polices
  libres installées (paquet `fonts-liberation`).

## Reste à faire

- Clés de registre `TimeZoneInformation`, `Nls\CodePage`, `Keyboard Layout` et
  `Fonts` en vue dynamique.
- `ToUnicode`, `VkKeyScan` et `MapVirtualKey` selon la disposition réelle.
- Pages 1250, 1251, 866 et pages asiatiques à plusieurs octets.
