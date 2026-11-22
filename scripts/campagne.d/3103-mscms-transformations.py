# mscms transforme les couleurs entre profils RVB matrice + courbes
# (CreateMultiProfileTransform, TranslateColors, TranslateBitmapBits,
# CheckColors…) et énumère les profils du dossier des couleurs (#3103).
mscms_src = (ROOT / "runtime/src/dlls/mscms.rs").read_text(encoding="utf-8")
ok("fn transformations_de_couleurs_3103" in mscms_src,
   "mscms : transformations de couleurs servies (#3103)")
ok("fn enumeration_des_profils_3103" in mscms_src,
   "mscms : profils énumérés selon les critères (#3103)")
