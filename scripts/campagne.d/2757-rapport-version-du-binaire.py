# #2757 : le rapport de plantage nomme le binaire winlux qui a tourné, avec le
# début de son sha256, pas l'image installée. Comportement vérifié en chargeant
# winlux-report.py avec un binaire témoin : l'interpréteur Python, qui répond à
# --version, et aucune image installée.
ok((lambda sortie: sortie.startswith("Python 3") and ", sha256 " in sortie)(subprocess.run(
       [sys.executable, "-I", "-c",
        "import runpy, sys; print(runpy.run_path(sys.argv[1], run_name='report_test')['winlux_version']())",
        str(SCRIPTS / "winlux-report.py")],
       env={**os.environ, "WINLUX_REPORT_BINARY": sys.executable, "WINLUX_BUILD_INFO": "/nonexistent"},
       capture_output=True, text=True, timeout=60).stdout),
   "rapport : version et empreinte du binaire qui a planté (#2757)")
