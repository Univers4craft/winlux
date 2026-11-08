# #1196 : MachineGuid présent, dérivé de machine-id et stable.
ok("machine_guid(data, &linux_machine_id())" in (ROOT / "runtime/src/registry/live.rs").read_text(encoding="utf-8")
   and "machine_guid_derive_de_machine_id_et_stable" in (SCRIPTS / "test-all.sh").read_text(encoding="utf-8"),
   "Registre : HKLM\\Software\\Microsoft\\Cryptography\\MachineGuid présent et stable (#1196)")
