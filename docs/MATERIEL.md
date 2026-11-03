# Support du matériel des PC

Winlux doit démarrer et être utilisable sur les PC réels : les fixes récents
et anciens, les portables, les tablettes Atom et les machines virtuelles. Ce
document décrit ce que l'image embarque, comment c'est vérifié et ce qui reste
à faire.

## Principes

1. **Un pilote oublié laisse l'appareil mort sans aucun message.** On
   embarque donc des **dossiers entiers** de pilotes du noyau Ubuntu, plutôt
   qu'une liste de noms qu'il faudrait tenir à jour pour des milliers de
   puces (contrôleurs de pavés tactiles, cartes Ethernet, capteurs des
   tablettes…).
2. **udev ne charge que les pilotes dont le matériel est présent.**
   Embarquer un pilote ne coûte que de la place sur le disque. Rien ne change
   au démarrage, en VM comme sur un vrai PC.
3. **On exclut seulement ce qu'aucun PC n'a** : réseaux de centres de
   données (Mellanox, Chelsio, QLogic…), cartes SAN Fibre Channel, bus des
   systèmes embarqués (CAN, 802.15.4), systèmes de fichiers de grappe.
4. **Les firmwares** viennent en entier de `linux-firmware` (Ubuntu les pose
   dans `/usr/lib/firmware`, et `/lib/firmware` pointe dessus depuis #376).
5. **Ce qui ne tient pas dans l'image s'installe après coup**, depuis les
   dépôts Ubuntu signés : Paramètres > Matériel et pilotes (pilotes complets
   du noyau, NVIDIA, VA-API, microcode, fwupd, empreintes, scanners, pilotes
   d'imprimantes).
6. **Ce qui n'est pas pris en charge est détecté et masqué**, jamais grisé
   (règle générale de Winlux). `winlux-hardware-check` explique pourquoi.

## Ce que fait la construction de l'image

`scripts/build-live-iso.sh` embarque les pilotes du noyau de construction en
quatre passes :

| Passe | Fichier | Contenu |
|---|---|---|
| Pilotes des VM et de démarrage | `build-live-iso.sh`, `build-live-root.sh` | virtio, e1000, bochs/qxl/vmwgfx, ahci, nvme, vmd, mmc, dm-crypt (initramfs) |
| Pilotes nommés | `scripts/hardware-modules.list` | cartes graphiques, Ethernet et Wi-Fi courants (#314), avec leurs dépendances |
| Son, Wi-Fi, crypto et pare-feu | boucle `for directory in sound …` | tout ALSA/ASoC/SoundWire (#376) ; Wi-Fi, crypto et netfilter (#398, fil pilotes) |
| **Dossiers entiers** | `scripts/hardware-module-dirs.list` → `scripts/stage-kernel-modules.py` | tout le reste du matériel des PC, dépendances comprises |

Les modules de la dernière passe restent compressés tels qu'Ubuntu les livre
(`.ko.zst`, lus par modprobe et udev). Ils prennent ainsi 3 à 4 fois moins de
place sur le disque installé. Un module déjà embarqué par une passe
précédente n'est pas recopié.

Ensuite, `scripts/check-image-hardware.py` relit **la racine finale** (après
`stage-apt-root.py`) et fait **échouer la construction** si un élément de
`scripts/hardware-essentials.list` manque :

- `module NOM` doit être intégré au noyau, ou présent dans
  `/lib/modules/<version>` et connu de `modules.dep` (sinon modprobe ne le
  trouverait pas) ;
- `firmware MOTIF` doit correspondre à au moins un fichier de
  `/lib/firmware`.

Un module inconnu du noyau de construction (renommé par une nouvelle version)
est signalé sans échec, pour qu'on corrige la liste.

`scripts/test-hardware-image.py` (CI) vérifie les trois outils. Sur une
machine qui a un noyau Ubuntu complet (`/lib/modules/*-generic` ou
`WINLUX_HOST_MODULES`), il vérifie aussi que chaque pilote essentiel est bien
embarqué par l'une des passes.

### Ajouter un pilote

- Il est dans un dossier déjà listé : rien à faire.
- Sinon, ajoutez son dossier à `hardware-module-dirs.list`, ou son nom à
  `hardware-modules.list` s'il est seul de son espèce.
- S'il est indispensable à une famille courante de PC, ajoutez
  `module NOM` à `hardware-essentials.list` : il ne pourra plus disparaître
  sans casser la construction.

## Audit de la v0.2.17 (3 novembre 2026)

Audit fait sur l'ISO publiée, comparée au noyau Ubuntu `7.0.0-42-generic`
complet. L'image embarquait **911 modules sur 6 901**. Les firmwares étaient
complets, sauf `regulatory.db` (Wi-Fi, #398). Les tailles sont celles des
modules compressés manquants.

| Catégorie | Ce qui manquait | Effet pour l'utilisateur | Coût |
|---|---|---|---|
| Broches et bus I2C/SMBus | `pinctrl-sunrisepoint` (Intel 6e–9e gén.), icelake, jasperlake, geminilake, elkhartlake, broxton, lynxpoint ; `intel-lpss-acpi`, `i2c-i801`, `i2c-designware-pci`, `dw_dmac` | Pavé tactile et écran tactile muets sur les portables Intel de 2015 à 2019, sur les Ice/Jasper/Gemini Lake et sur les tablettes Atom | 1 Mio |
| Pavés et écrans tactiles | `rmi_smbus`, `hid-rmi`, `hid-alps`, `hid-elan`, `goodix_ts`, `silead`, `elants` (intégré), `intel-quicki2c/quickspi/thc` (Lunar Lake), `bcm5974`, `appletouch` | Pavé Synaptics ou Elan réduit à la souris PS/2 (pas de défilement à deux doigts), écran tactile des tablettes muet, pavé des portables 2025 muet | 2,5 Mio |
| Stylets et tablettes graphiques | `wacom`, `hid-uclogic` (Huion, XP-Pen), `wacom_serial4` | Stylet et tablette graphique inutilisables | 0,2 Mio |
| Capteurs et rotation | `intel-ish*`, `amd_sfh`, `hid-sensor-*`, accéléromètres IIO (`bmc150`, `kxcjk-1013`, `mxc4005`, `st_accel`…), capteurs de lumière | Pas de rotation de l'écran ni de luminosité automatique sur les tablettes et les convertibles | 5 Mio |
| Touches et mode tablette | `intel-hid`, `intel-vbtn`, `soc_button_array`, toshiba, samsung, sony, fujitsu, msi, lg, huawei, alienware, eeepc, `x86-android-tablets`… | Touches de fonction, bouton marche et volume des tablettes, mode tablette sans effet | 1 Mio |
| Batteries et alimentation | `axp288_fuel_gauge`, `axp288_charger`, `bq27xxx`, `max17042`, `sbs`, PMIC Cherry Trail | Batterie invisible sur les tablettes Atom | 0,7 Mio |
| Températures et ventilateurs | `x86_pkg_temp_thermal`, `int340x/int3400`, `intel_pch_thermal`, `dell-smm-hwmon`, `nct6683`, `asus_ec_sensors`, `applesmc`, `drivetemp` (285 pilotes hwmon) | Ventilateurs et températures absents du Gestionnaire des tâches, régulation thermique Intel incomplète | 2 Mio |
| Ethernet | `asix`, `smsc95xx`, `lan78xx`, `aqc111`, `atl1c/atl1e/atl1/atl2`, `sky2/skge`, `forcedeth`, `8139too`, `jme`, `via-rhine`, `sis190`, `e100`, `pcnet32`, `ixgbe/i40e/ice`, `dwmac-intel` | Pas de réseau filaire sur les adaptateurs USB courants, les portables Atheros/Marvell et les anciens PC nForce/VIA/SiS | 9 Mio |
| Partage de connexion et 4G | `ipheth` (iPhone), `cdc_mbim`, `qmi_wwan`, `huawei_cdc_ncm`, `cdc_eem`, PPPoE | Pas de partage de connexion par câble avec un iPhone, modem 4G/5G intégré muet, pas de PPPoE | 0,5 Mio |
| Bluetooth | `uhid` (claviers et souris Bluetooth LE), `btintel_pcie` (Intel 2024+), `btmtksdio`, `btmtkuart`, `hci_bcm4377` (Mac), `ath3k` | Claviers et souris Bluetooth LE ne marchent pas ; Bluetooth des portables récents absent | 0,3 Mio |
| Stockage | `md` (RAID 0/1/5/10, `dm-raid` pour Intel RST et AMD RAID), `ata` en mode IDE et SATA anciens (`pata_*`, `sata_nv/via/sil`), `rtsx_usb_sdmmc`, `alcor`, `memstick`, contrôleurs SAS courants | Disques en RAID invisibles (dual-boot avec Windows en RST), anciens PC sans disque visible, lecteurs de cartes USB Realtek muets | 7 Mio |
| USB, USB-C et Thunderbolt | `typec_displayport`, `intel_pmc_mux`, `ucsi_ccg`, `xhci-pci-renesas`, `cdc-acm`, `usb-serial` (`ftdi_sio`, `ch341`, `cp210x`, `pl2303`), `usblp`, `ums-*` | Écran externe en USB-C absent sur des portables Intel, Arduino et ESP32 invisibles, cartes USB 3 Renesas muettes | 3 Mio |
| Webcams | `gspca` (anciennes), `pwc`, IPU6 et IVSC (MIPI), capteurs `ov*`, `hi556` | Anciennes webcams USB muettes ; webcams MIPI des portables Intel de 12e à 14e génération muettes | 4 Mio |
| Graphique | `gma500_gfx` (anciens Atom), `udl`/`gud` (DisplayLink), `ast`/`mgag200`, `hyperv_drm`, `vboxvideo`, `cirrus-qemu`, `appletbdrm` | Netbooks Atom et écrans DisplayLink sans affichage accéléré, Hyper-V et VirtualBox en mode dégradé | 1 Mio |
| Machines virtuelles | `hv_vmbus`, `hv_netvsc`, `hv_storvsc`, `hyperv-keyboard`, `vboxguest`, `vboxsf`, `vmxnet3`, `vmw_pvscsi`, `virtiofs`, `9p` | Winlux dans Hyper-V sans disque ni réseau ; VirtualBox et VMware sans dossiers partagés | 1 Mio |
| Systèmes de fichiers | `binfmt_misc`, `btrfs`, `xfs`, `f2fs`, `jfs`, `hfs`, pages de codes `nls_cp*` | `./programme.exe` dans un terminal ne marche pas ; disques Linux illisibles | 6 Mio |
| Divers | `mei_me` (Intel ME), `ee1004` (SPD de la mémoire), `lis3lv02d` (HP), `ntsync` | Infos mémoire absentes ; primitives de synchronisation de Windows (ntsync) non disponibles | 0,8 Mio |
| Wi-Fi, crypto, pare-feu | 143 pilotes Wi-Fi, `ccm`/`cmac`, `regulatory.db`, `nf_tables` | Aucun réseau WPA2, pare-feu inactif | voir #398 |

**Coût total de la passe « dossiers entiers » : 3 581 modules, 49 Mio
compressés.** L'ISO passe d'environ 1,27 Go à environ 1,32 Go, et à moins de
1,4 Go avec le Wi-Fi de #398.

## Après l'installation

- **Pilotes complets du noyau** (`winlux-drivers install modules`, installé
  automatiquement au premier démarrage hors VM). Depuis le noyau 7.0, Ubuntu
  ne fournit plus qu'un seul paquet, `linux-modules-<version>` (environ
  170 Mo), et `linux-modules-extra` n'existe plus. Avant cette correction,
  Winlux demandait les deux paquets et apt-get échouait à chaque fois.
- **NVIDIA propriétaire, VA-API, microcode, fwupd, empreintes** : #314.
- **Scanners** (`sane-utils`, `sane-airscan`) et **pilotes d'imprimantes**
  (Gutenprint, HP, Brother, Samsung) : proposés dans Matériel et pilotes.
  L'impression driverless (IPP Everywhere, IPP sur USB) marche sans eux.

## Diagnostic sur une machine

```
winlux-hardware-check            # rapport lisible (langue du système)
sudo winlux-hardware-check       # lit aussi le journal du noyau : firmwares manquants
winlux-hardware-check --save rapport.txt
winlux-hardware-check --json     # pour un script ou un signalement
winlux-hardware-check --all      # liste aussi tous les appareils et leur pilote
```

Le même rapport s'ouvre depuis Paramètres > Matériel et pilotes > Lancer le
diagnostic. L'outil liste :

- les appareils PCI, USB, I2C et SPI sans pilote, avec la raison : pilote
  présent mais pas chargé, pilote installable avec « pilotes complets », ou
  appareil non pris en charge par le noyau Linux ;
- les firmwares que le noyau n'a pas trouvés ;
- ce que l'utilisateur remarque : affichage accéléré, son, pavé tactile,
  batterie, luminosité, Wi-Fi et interrupteur, Bluetooth, capteur de
  rotation, Thunderbolt non autorisé, webcam (dont les MIPI non prises en
  charge), disques et mode RAID.

Le code de sortie vaut 1 s'il y a au moins un problème. Rien n'est modifié.

## Reste à faire

- **Rotation automatique de l'écran** : les capteurs sont maintenant là,
  mais il manque le service qui tourne l'écran (iio-sensor-proxy ou lecture
  directe de l'IIO).
- **Thunderbolt** : sans `boltd`, un dock sur un portable en niveau de
  sécurité « user » reste non autorisé (le diagnostic le signale).
- **Webcams MIPI Intel (IPU6/IPU7)** : il faut les modules
  `linux-main-modules-ipu6/ipu7/vision`, libcamera et son greffon PipeWire.
- **RAID logiciel** : les pilotes sont là ; `mdadm` doit encore rejoindre
  l'image pour assembler les volumes.
- **Microcode en session live** : il n'est chargé qu'après l'installation.
