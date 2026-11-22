# #3164 : extraire l'icône d'un installateur ne charge pas tout le .exe.
_r_3164 = subprocess.run([sys.executable, "-B", str(SCRIPTS / "test-icone-sans-lecture.py")], capture_output=True, text=True)
print(_r_3164.stdout.strip(), _r_3164.stderr.strip()[-300:])
ok(_r_3164.returncode == 0, "icône d'un .exe de 300 Mo extraite en lisant moins de 1 Mo, sans pic de mémoire (#3164)")
