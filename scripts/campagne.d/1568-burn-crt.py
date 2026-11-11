# Bundle de Python : CRT universel fourni par Winlux, paquet ucrt.msi (téléchargé à part) sauté (#1568).
ok('vars.entry("CRTInstalled".into())' in (ROOT / "runtime/src/setup/burn.rs").read_text(),
   "bundle Burn : CRTInstalled vrai, ucrt.msi non réclamé (#1568)")
