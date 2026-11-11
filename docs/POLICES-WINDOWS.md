# Polices de Windows sur Winlux : ce qu'on a le droit de faire

État des lieux du 11 novembre 2026, demandé par damien : « vérifie qu'on peut
installer toutes les polices d'écriture qui sont disponibles dans Windows, sous
condition qu'on ait le droit de les installer ». Ce n'est pas l'avis d'un
juriste.

## 1. Ce que disent les licences

Sources lues :
- FAQ typographique de Microsoft (<https://learn.microsoft.com/typography/fonts/font-faq>,
  mise à jour le 19 novembre 2024) : « Apart from the document embedding rights
  described previously, you may not redistribute the Windows fonts. You may not
  copy them to other computers or servers. » Segoe UI Variable est en plus « not
  available for licensing or use outside of Microsoft products or on non-Windows
  platforms ». Les droits étendus se négocient auprès de Monotype.
- Termes de licence de Windows 11 (OEM, avril 2024) : les polices font partie du
  logiciel (section 1.a, « any fonts, icons, images or sound files included with
  the software ») et suivent ses restrictions : ni copie (hors sauvegarde) ni
  publication (section 2.c).

Conséquences :

| Cas | Verdict |
|---|---|
| Livrer dans l'ISO une police sous licence Windows (Segoe UI, Calibri, Tahoma, Consolas…) | **Impossible** |
| Proposer d'importer les polices d'une partition Windows de l'utilisateur | **Impossible** : c'est la copie hors de Windows que la FAQ interdit. Winlux ne doit ni l'offrir ni l'automatiser. |
| Core fonts for the Web (1996-2002 : Arial, Courier New, Times New Roman, Georgia, Verdana, Trebuchet MS, Comic Sans MS, Impact, Andale Mono, Arial Black, Webdings) | **Pas pour l'instant** : leur licence n'autorise que la redistribution des .exe d'origine, Microsoft ne les publie plus (il ne reste que des miroirs), donc pas de « source officielle de l'éditeur » au sens de nos règles. Versions de 2002, sans Tahoma. Ne serait envisageable que sur décision de damien. |
| Polices de Windows publiées par Microsoft sous licence libre : **Cascadia Code / Cascadia Mono** (OFL 1.1) | **Livrable** : ajoutée à l'image (paquet `fonts-cascadia-code`). |
| **Selawik** (Microsoft, OFL 1.1, métriques de Segoe UI) | **Livrable** mais pas empaquetée par Ubuntu : à télécharger depuis le dépôt GitHub de Microsoft au moment de construire l'ISO, avec SHA-256. Hors gel 0.2.21. |
| Police achetée ou libre que l'utilisateur possède (y compris un .ttf récupéré avec sa propre licence) | **Permis** : application Polices, `winlux-fonts install`, ou dépôt dans `C:\Windows\Fonts`. |
| Polices déposées par un logiciel installé (Office, Adobe…) dans `C:\Windows\Fonts` | **Permis** dans le cadre de la licence de ce logiciel : c'est lui qui les installe, Winlux les sert. |

En résumé : **Winlux livre Cascadia, peut livrer Selawik, et remplace tout le
reste par des polices libres** ; il ne propose pas d'importer les polices de
Windows.

## 2. Remplaçants libres, par police de Windows 11

« Métriques » = largeurs identiques, la mise en page ne bouge pas. « Proche » =
même genre, largeurs différentes. Les remplacements se font à trois endroits qui
doivent rester d'accord : fontconfig
(`integration/native/config/fontconfig/conf.d/60-winlux-windows-fonts.conf`),
la table `substitutes` de `runtime/src/dlls/dwrite.rs` (GDI et DirectWrite) et
l'énumération de `gdi32_fonts.rs` (#1670, #1768).

| Police Windows | Remplaçant | Type | Dans l'ISO |
|---|---|---|---|
| Arial, Helvetica | Liberation Sans (Arimo) | métriques | oui |
| Times New Roman | Liberation Serif (Tinos) | métriques | oui |
| Courier New | Liberation Mono (Cousine) | métriques | oui |
| Calibri, Calibri Light | Carlito | métriques | oui depuis #1770 |
| Cambria | Caladea | métriques | oui depuis #1770 |
| Cascadia Code, Cascadia Mono | la vraie police | — | oui (OFL) |
| Segoe UI (et MS Shell Dlg) | Selawik | métriques | non (à ajouter) ; Liberation Sans en attendant |
| Georgia | Gelasio | métriques | non empaquetée ; Liberation Serif |
| Palatino Linotype, Book Antiqua | P052 / TeX Gyre Pagella | métriques de Palatino | non |
| Symbol | Standard Symbols PS (URW) | métriques | non |
| Tahoma, Verdana, Microsoft Sans Serif | DejaVu Sans / Liberation Sans | proche | oui |
| Consolas, Lucida Console | Liberation Mono / DejaVu Sans Mono | proche (chasse fixe) | oui |
| Segoe UI Emoji | Noto Color Emoji | proche | par le Store |
| MS Gothic, Yu Gothic, Meiryo, SimSun, Microsoft YaHei, Malgun Gothic, MingLiU, Microsoft JhengHei | WenQuanYi Micro Hei, Noto CJK | proche | WenQuanYi oui |
| Nirmala UI, Ebrima, Gadugi, Leelawadee UI, Myanmar Text, Javanese Text, Sylfaen, Mongolian Baiti, Microsoft Himalaya… | famille Noto du script | proche | à la demande |
| Comic Sans MS | Comic Neue | proche | non |
| Arial Black, Impact, Trebuchet MS, Candara, Constantia, Corbel, Franklin Gothic, Bahnschrift, Sitka, Gabriola, Ink Free, MV Boli, Segoe Print, Segoe Script | — | **aucun** | — |
| Marlett, Wingdings, Webdings, Segoe MDL2 Assets, Segoe Fluent Icons | — | **aucun** (polices de symboles : les codes n'ont pas de sens ailleurs). Une copie apportée par un logiciel (Steam) est servie depuis #1807 ; une Marlett maison est possible plus tard. | — |

## 3. Chemin d'installation d'une police : ce qui existe

- **Dépôt par l'utilisateur** : application Polices et `winlux-fonts install`
  (`.ttf`, `.otf`, `.ttc`, `.otc`) ; les polices système sont protégées.
- **fontconfig** : `C:\Windows\Fonts` et le dossier par utilisateur de
  Windows 10+ sont indexés ; les polices ajoutées par AddFontResource sont
  signalées à fontconfig (FcConfigAppFontAddFile).
- **Registre** : `HKLM` et `HKCU\Software\Microsoft\Windows NT\CurrentVersion\Fonts`
  sont une vue dynamique des dossiers (#1045, #1046).
- **GDI** : `AddFontResourceA/W`, `AddFontResourceExA/W` (avec `FR_NOT_ENUM`
  depuis #1771), `RemoveFontResource*`, `AddFontMemResourceEx` /
  `RemoveFontMemResourceEx` (fichier privé 0700), `EnumFontFamilies(Ex)A/W`,
  `EnumFontsA/W`, `CreateFont(Indirect)A/W`, `CreateFontIndirectExA/W` (#1771),
  `GetGlyphIndicesA/W` (A depuis #1771), `GetFontUnicodeRanges` (lit la table
  `cmap`), `GetFontData`, `GetTextMetrics`, `GetOutlineTextMetricsW`,
  `GetCharABCWidths*`, `GetKerningPairs`, `GetFontResourceInfoW`.
- **DirectWrite** : collection système, ensembles de polices, collections
  personnalisées et repli par caractère ; GDI+ `PrivateFontCollection`.

## 4. Ce qui manque (issues ouvertes)

| Issue | Manque | Urgence |
|---|---|---|
| #1768 | Une police Windows remplacée était dite absente par EnumFontFamiliesEx (cause probable du Steam sans texte, #1670) | 1, corrigé par #1775, complété ici (MS Shell Dlg, Calibri Light, Cambria) |
| #1769 | Un nom résolu vers un remplaçant le restait après AddFontResource | 2, corrigé |
| #1770 | Calibri et Cambria vers Liberation au lieu de Carlito et Caladea | 2, corrigé |
| #1771 | FR_NOT_ENUM ignoré ; GetGlyphIndicesA, CreateFontIndirectExA/W absents | 3, corrigé |
| #1772 | .otf à contours CFF jamais dessinés par GDI ni DirectWrite | 2, après 0.2.21 |
| #1773 | Police installée pendant qu'un .exe tourne : invisible pour lui, pas de WM_FONTCHANGE ; IDWriteFactory5::CreateInMemoryFontFileLoader absent | 3 |
| #1774 | Polices bitmap .fon et Type 1 (.pfm\|.pfb) refusées | 3 |
| #1807 | Marlett (et toute police de symboles aux noms en codage « symbole ») rejetée : 0 face chez Steam | 2, corrigé |
| #1781 | Un programme qui ouvre directement `C:\Windows\Fonts\arial.ttf` (ou lit son nom dans la clé de registre Fonts) ne trouve aucun fichier : la clé ne liste que les vraies familles installées | 2 |
