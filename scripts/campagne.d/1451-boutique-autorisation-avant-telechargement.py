# #1447 à #1451 : boutique, logiciels des éditeurs. L'autorisation administrateur
# est demandée avant tout téléchargement ; Annuler a le focus visible ; le
# résultat reste affiché ; la croix vide la recherche ; libellé allemand court.
store_py = (SCRIPTS / "winlux-store.py").read_text(encoding="utf-8")
repos_py = (SCRIPTS / "winlux-store-repos.py").read_text(encoding="utf-8")
ok('"check"' in store_py and "DENIED" in store_py and '"check"' in repos_py,
   "Boutique : l'autorisation précède le téléchargement (#1451)")
