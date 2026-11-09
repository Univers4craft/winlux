# #1331 : ID2D1DeviceContext::CreateCommandList et ID2D1CommandList
# (Paint.NET, ColorsForm.SetColorAddIcon).
step "Liste de commandes Direct2D (#1331)" cargo test -q -p winlux -- liste_de_commandes_dessinee_puis_lue_comme_image
