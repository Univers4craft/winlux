# _get_osfhandle (#1283) : handle Windows d'un descripteur CRT, même fichier et
# même position ; l'export WAV d'Audacity (libsndfile, sf_open_fd) n'est plus vide.
step "_get_osfhandle d'un fichier (#1283)" cargo test -q -p winlux -- get_osfhandle_d_un_fichier
