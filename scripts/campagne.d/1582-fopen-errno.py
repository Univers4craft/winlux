# CRT : fopen et _wfopen d'un fichier absent posent errno = ENOENT (Python 3.12, pyvenv.cfg) (#1582).
ok("fn open_failed" in (runtime_dlls / "crt_stdio.rs").read_text()
   and "fn fopen_d_un_fichier_absent_pose_enoent" in (runtime_dlls / "crt_stdio.rs").read_text(),
   "CRT : fopen d'un fichier absent pose errno ENOENT, Python 3.12 démarre (#1582)")
