# #1363 : Paint.NET ouvre le profil sRGB par mscms.dll.
step "mscms.dll : profils de couleur ICM (#1363)" cargo test -q -p winlux -- profil_ouvert_lu_puis_ferme
