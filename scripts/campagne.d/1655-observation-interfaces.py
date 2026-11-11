# #1655 : une interface de pilote absent ne se présente que pour une passe d'observation, et n'exécute rien.
vdev = (runtime_dlls / "vdev.rs").read_text(encoding="utf-8")
noeuds = (runtime_dlls / "devnodes.rs").read_text(encoding="utf-8")
ok('std::env::var_os("WINLUX_VDEV_OBSERVE")' in vdev and "out.extend(observation_nodes());" in noeuds,
   "observation : interfaces présentées seulement avec WINLUX_VDEV_OBSERVE (#1655)")
corps = vdev[vdev.index("impl VirtualDevice for Observation"):]
corps = corps[:corps.index("\n}\n")]
ok(corps.count("Some(Reply::Data(Vec::new()))") == 2 and "saisie" not in corps and "inject" not in corps,
   "observation : écritures et codes acceptés sans effet, rien n'est tapé (#1655)")
ok('" ; données = {}"' in vdev and "crate::dlls::vdev::write_file(h, buf, len, written, ov)" in (runtime_dlls / "kernel32.rs").read_text(encoding="utf-8"),
   "observation : WriteFile vers un appareil virtuel journalisé (WINLUX_VDEV_DUMP) (#1655)")
