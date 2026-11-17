# #2743 : l'image ubuntu-base installe avec « path-exclude=/usr/share/doc/* »
# puis « path-include=/usr/share/doc/*/copyright » ; dpkg n'écrivait pas les
# notices de winlux-desktop, dpkg --verify les disait manquantes et la mise à
# jour revenait à la version précédente. Le paquet n'a plus que copyright sous
# /usr/share/doc, et sa construction refuse tout fichier que dpkg filtrerait.
notices_tool = runpy.run_path(str(SCRIPTS / "stage-notices.py"))
apt_tool = runpy.run_path(str(SCRIPTS / "stage-apt-root.py"))
with tempfile.TemporaryDirectory() as temp_2743:
    base_2743 = Path(temp_2743) / "base"
    (base_2743 / "etc/dpkg/dpkg.cfg.d").mkdir(parents=True)
    (base_2743 / "etc/dpkg/dpkg.cfg.d/excludes").write_text(
        "path-exclude=/usr/share/man/*\npath-exclude=/usr/share/locale/*/LC_MESSAGES/*.mo\n"
        "path-exclude=/usr/share/doc/*\npath-include=/usr/share/doc/*/copyright\n")
    rules_2743 = apt_tool["dpkg_rules"](base_2743)
    crate_2743 = Path(temp_2743) / "crate"; crate_2743.mkdir()
    (crate_2743 / "Cargo.toml").write_text("")
    (crate_2743 / "LICENSE-MIT").write_text("MIT\n")
    meta_2743 = {"packages": [
        {"id": "w", "name": "winlux", "version": "0", "source": None, "license": "GPL-3.0-or-later"},
        {"id": "c", "name": "simd-adler32", "version": "0.3.7", "source": "registry", "license": "MIT",
         "authors": [], "manifest_path": str(crate_2743 / "Cargo.toml")}],
        "resolve": {"nodes": [{"id": "w", "deps": [{"pkg": "c", "dep_kinds": [{"kind": None}]}]},
                              {"id": "c", "deps": []}]}}
    image_2743 = Path(temp_2743) / "image"
    notices_tool["stage"](image_2743, meta_2743)
    posed_2743 = sorted(str(p.relative_to(image_2743)) for p in image_2743.rglob("*") if p.is_file())
    filtered_2743 = [p for p in posed_2743 if apt_tool["dpkg_filtered"](p, rules_2743)]
ok("usr/share/winlux/notices/NOTICES" in posed_2743
   and "usr/share/winlux/notices/crates/simd-adler32-0.3.7/LICENSE-MIT" in posed_2743
   and "usr/share/doc/winlux-desktop/copyright" in posed_2743 and filtered_2743 == []
   and apt_tool["dpkg_filtered"]("usr/share/doc/winlux-desktop/NOTICES", rules_2743)
   and not apt_tool["dpkg_filtered"]("usr/share/doc/winlux-desktop/copyright", rules_2743),
   f"Notices du paquet hors des chemins exclus par dpkg, seul copyright sous /usr/share/doc (#2743) {filtered_2743}")
