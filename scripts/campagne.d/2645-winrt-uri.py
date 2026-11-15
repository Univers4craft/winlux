# #2645 : la classe Windows.Foundation.Uri s'active par sa fabrique et sert
# IUriRuntimeClass, IUriRuntimeClassWithAbsoluteCanonicalUri et IStringable.
# Le comportement est gardé par les tests unitaires nommés.
uri = (runtime_dlls / "winrt_uri.rs").read_text(encoding="utf-8")
ok("fn classe_uri_2645" in uri, "test de la classe Windows.Foundation.Uri (#2645)")
ok("fn decoupage_rfc3986" in uri, "test du découpage d'URI selon la RFC 3986 (#2645)")
