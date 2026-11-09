# _get_osfhandle : numéro de descripteur réutilisé pour un autre fichier,
# et test stable dans la suite parallèle (#1320).
step "_get_osfhandle et descripteur réutilisé (#1320)" cargo test -q -p winlux -- get_osfhandle
