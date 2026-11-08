# gdi32 : comportements de Windows reproduits exprès

Détails documentés de Windows que Winlux reproduit parce que des programmes
réels en dépendent. Ne pas les « corriger » sans relire la source citée.

| Fonction | Cas | Comportement Windows reproduit | Source publique | Programme qui en dépend |
|---|---|---|---|---|
| `CreateCompatibleBitmap` | largeur ou hauteur nulle | rend une bitmap 1x1 (monochrome sous Windows), pas une erreur | documentation Microsoft Learn de `CreateCompatibleBitmap`, section *Remarks* : « If either the width or the height of the bitmap is zero, CreateCompatibleBitmap returns the handle to a 1-by-1 pixel, monochrome bitmap. » | VCL Delphi (`Vcl.Graphics`, copie d'une `TBitmap` vide) : WinSCP x86 levait `EOutOfResources « Paramètre invalide »` (#905, PR #1000) |
| `CreateBitmap` | largeur ou hauteur nulle | idem, bitmap 1x1 | documentation Microsoft Learn de `CreateBitmap`, section *Remarks* : « If the width or height is zero, the function returns the handle to a 1-by-1 pixel, monochrome bitmap. » | même chemin de la VCL (bitmap monochrome) |
| `CreateDIBSection` | `ppvBits` nul | la DIB est créée ; seul le pointeur de pixels n'est pas rendu | documentation de `CreateDIBSection` (*ppvBits* : pointeur facultatif vers la variable qui reçoit l'adresse des pixels) | PR #1000 |
| `GetObject` sur une DIB section, programme x86 | tampon d'au moins 84 octets | rend un `DIBSECTION` x86 de 84 octets (BITMAP 24, BITMAPINFOHEADER 40, dsBitfields 12, dshSection 4, dsOffset 4) | documentation de `GetObject` et de la structure `DIBSECTION` | VCL Delphi (TBitmap) (#905, PR #966) |

Écart connu : la bitmap 1x1 de Winlux a la profondeur de l'écran, pas 1 bit
par pixel ; aucun programme relevé ne lit cette profondeur.
