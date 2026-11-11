# #1533 : appareils virtuels des pilotes constructeurs (\\.\Nom, DeviceIoControl).
step "Appareils virtuels : noms, codes de contrôle (#1533)" cargo test -q -p winlux -- vdev::
