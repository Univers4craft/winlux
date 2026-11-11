# #1725 : AppxMetadata\CodeIntegrity.cat est un fichier d'empreinte, absent
# du block map ; il ne doit plus rendre un paquet « altéré » (Teams).
# #1726 : racines de signature de code de Microsoft reconnues pour la
# signature de code seulement.
msix = (ROOT / "runtime/src/setup/msix.rs").read_text(encoding="utf-8")
sig = (ROOT / "runtime/src/setup/appx_signature.rs").read_text(encoding="utf-8")
test = (SCRIPTS / "test-msix.sh").read_text(encoding="utf-8")
ok('"appxmetadata\\\\codeintegrity.cat"' in msix
   and 'z.writestr("AppxMetadata/CodeIntegrity.cat", CATALOG)' in test,
   "MSIX : CodeIntegrity.cat accepté hors du block map (#1725)")
ok("CODE_ROOTS" in sig and "hostssl::verify(&signer.der" not in sig
   and (ROOT / "runtime/src/setup/racines_code/microsoft-root-2011.cer").is_file(),
   "Signature de code : racines Microsoft reconnues, hors TLS (#1726)")
