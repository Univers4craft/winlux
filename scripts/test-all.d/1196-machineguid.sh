# Identifiant de la machine (#1196, licences, inventaire) : MachineGuid
# dérivé de machine-id, au format GUID, jamais remplacé une fois posé.
step "MachineGuid dans le registre (#1196)" cargo test -q -p winlux -- machine_guid_derive_de_machine_id_et_stable
