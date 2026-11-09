# #1319 : le graphe d'un effet Direct2D personnalisé appelle ses nœuds par
# leur interface ID2D1Transform (objets .NET de Paint.NET).
step "Nœud de transformation par ID2D1Transform (#1319)" cargo test -q -p winlux -- noeud_appele_par_son_interface_id2d1transform
