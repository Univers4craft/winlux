# Objets OLE incorporés (#1236) : OleCreate, OleRun, OleDraw, OleSave puis
# OleLoad dans un stockage composé ; porte-avis OLE et de données.
step "Objets OLE incorporés : création, enregistrement, rechargement (#1236)" cargo test -q -p winlux -- objet_incorpore_cree_enregistre_et_recharge porte_avis_ole_et_donnees
