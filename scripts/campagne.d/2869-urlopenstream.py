# URLOpenStreamW/A pousse les données au rappel (OnDataAvailable) : chemin
# de repli de l'installeur de OneDrive sans BITS (#2869).
_u = (runtime_dlls / "url_stream.rs").read_text(encoding="utf-8")
_w = (runtime_dlls / "wininet.rs").read_text(encoding="utf-8")
ok('"URLOpenStreamW"' in _w and '"URLOpenStreamA"' in _w
   and "fn url_open_stream_w(" in _u and "fn url_open_stream_a(" in _u,
   "urlmon exporte URLOpenStreamW et URLOpenStreamA (#2869)")
ok("fn flux_pousse_au_rappel_2869" in _u and "fn flux_annule_et_url_nulle_2869" in _u,
   "tests : données poussées dans un IStream, annulation par E_ABORT, URL nulle (#2869)")
