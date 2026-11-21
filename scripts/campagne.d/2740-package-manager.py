# #2740 : Windows.Management.Deployment.PackageManager est servi (recherches
# vides, déploiement en E_NOTIMPL nommé) et décrit pour un programme x86,
# l'outil de déploiement d'Office. Comportement gardé par les tests
# winrt_package_manager::tests::package_manager_sans_paquet_installe et
# wow32::com_winrt::tests::winrt_decrit_pour_le_code_x86.
pm_2740 = (ROOT / "runtime" / "src" / "dlls" / "winrt_package_manager.rs").read_text(encoding="utf-8")
x86_2740 = (ROOT / "runtime" / "src" / "wow32" / "com_winrt.rs").read_text(encoding="utf-8")
tests_2740 = (ROOT / "runtime" / "src" / "dlls" / "winrt_package_manager" / "tests.rs").read_text(encoding="utf-8")
ok('"Windows.Management.Deployment.PackageManager"' in pm_2740
   and "fn package_manager_sans_paquet_installe" in tests_2740
   and "static IPACKAGEMANAGER: Iface" in x86_2740
   and "static IPACKAGEMANAGER2: Iface" in x86_2740,
   "PackageManager servi, x86 compris (#2740)")
