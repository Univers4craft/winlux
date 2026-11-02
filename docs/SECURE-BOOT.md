# Secure Boot

Winlux démarre avec Secure Boot activé grâce à la chaîne de confiance standard
des distributions Linux :

```
micrologiciel UEFI → shim signé (BOOTX64.EFI) → GRUB signé (grubx64.efi) → noyau signé
                         └─ MokManager (mmx64.efi) : enregistrement de clés locales
```

Tout se fait localement : aucun compte, aucune connexion réseau, aucune donnée
envoyée. Le shim et le GRUB signés viennent des paquets Ubuntu `shim-signed` et
`grub-efi-amd64-signed`. Le shim est contresigné par l'autorité UEFI tierce
présente dans les micrologiciels des PC du commerce : c'est la seule façon
qu'un PC accepte de démarrer un système avec Secure Boot sans réglage manuel.
Pour s'en passer complètement, voir « Clés entièrement personnelles » plus bas.

## Construire une ISO compatible Secure Boot

```sh
sudo apt install shim-signed grub-efi-amd64-signed grub-pc-bin mtools dosfstools xorriso
WINLUX_SECUREBOOT=1 WINLUX_KERNEL=/boot/vmlinuz-<version> WINLUX_KERNEL_RELEASE=<version> \
    scripts/build-live-iso.sh
```

- L'ISO reste hybride : BIOS (El Torito), UEFI et clé USB (`dd`).
- Le noyau doit être signé. Un noyau Ubuntu (`linux-image-*`) l'est déjà ; un
  noyau compilé soi-même doit être signé avec une clé MOK (ci-dessous).
- Sans `WINLUX_SECUREBOOT=1`, la construction reste identique à avant
  (`grub-mkrescue`, GRUB non signé : il faut désactiver Secure Boot).

## PC à micrologiciel UEFI 32 bits (tablettes Atom)

Certaines tablettes et petits PC (Intel Atom Bay Trail / Cherry Trail) ont un
processeur 64 bits mais un micrologiciel UEFI 32 bits. Winlux reste 64 bits ;
seul le premier chargeur existe aussi en 32 bits, `EFI/BOOT/BOOTIA32.EFI`, à
côté de `BOOTX64.EFI` (paquet `grub-efi-ia32-bin`). L'ISO, la clé USB, le
disque installé, la clé de secours et la réinitialisation portent les deux, et
le micrologiciel prend seul celui de son mode. Sur une partition EFI partagée
avec Windows, l'installateur pose uniquement le chargeur du mode détecté
(`/sys/firmware/efi/fw_platform_size`). Le noyau Ubuntu démarre dans ce mode
mixte (`CONFIG_EFI_MIXED`).

Il n'existe pas de shim signé en 32 bits : sur ces machines, **désactivez
Secure Boot** dans le menu du micrologiciel. La chaîne signée 64 bits n'est
pas touchée. La validation de `release.yml` démarre l'ISO et le disque
installé sous QEMU avec OVMF 32 bits (`ovmf-ia32`), et
`scripts/check-iso-boot-paths.sh` fait échouer la construction si l'un des
trois chemins (BIOS, UEFI 64 bits, UEFI 32 bits) manque.

L'installateur (graphique ou `scripts/install.sh`) copie la même chaîne sur
l'ESP du disque cible quand `/usr/lib/shim/shimx64.efi.signed` est présent.
Le mode récupération (« Réparer le démarrage ») la réinstalle aussi.

## Vérifier

```sh
winlux-secureboot status   # état du micrologiciel et clés MOK enregistrées
winlux-secureboot check    # shim, MokManager et GRUB signés présents
sh scripts/test-secureboot-qemu.sh                     # QEMU + OVMF, Secure Boot imposé
WINLUX_TEST_BIOS=1 sh scripts/test-secureboot-qemu.sh  # même image en BIOS
```

Le test construit une petite ISO avec le vrai `grub.cfg` du live et vérifie,
depuis le noyau démarré, que la variable `SecureBoot` vaut 1.

## Signer un noyau ou un module avec une clé locale (MOK)

```sh
sudo winlux-secureboot gen-mok              # /var/lib/winlux/secureboot/MOK.{key,crt,der}
sudo winlux-secureboot sign-kernel /boot/vmlinuz
sudo winlux-secureboot enroll               # choisir un mot de passe temporaire
sudo reboot
```

Au redémarrage, l'écran bleu **MokManager** apparaît : *Enroll MOK* →
*Continue* → *Yes* → mot de passe → *Reboot*. La clé est alors acceptée par le
shim. Pour un module noyau :

```sh
/usr/src/linux-headers-$(uname -r)/scripts/sign-file sha256 \
    /var/lib/winlux/secureboot/MOK.key /var/lib/winlux/secureboot/MOK.der module.ko
```

`MOK.key` est la clé privée : elle reste sur la machine (droits 600), elle ne
doit pas être publiée ni ajoutée au dépôt.

## Clés entièrement personnelles

Pour ne dépendre d'aucune autorité extérieure, on peut remplacer les clés du
micrologiciel : mettre le PC en *Setup Mode* dans le menu UEFI, enregistrer sa
propre clé de plateforme (PK), KEK et base `db`, puis signer directement GRUB
et le noyau avec `sbsign`. C'est plus robuste vis-à-vis des tiers mais propre
à chaque machine (une mise à jour du micrologiciel peut réinitialiser les
clés), donc non automatisé ici.

## Limites connues

- Le GRUB signé n'accepte que ses modules intégrés : `grub.cfg` n'utilise que
  `search`, `linux`, `initrd`, `configfile`, `echo`, `sleep`, `serial`.
- Sous Secure Boot, le noyau active le *lockdown* : les modules non signés sont
  refusés. Les pilotes Ubuntu sont signés ; un pilote externe (NVIDIA DKMS…)
  doit être signé avec la clé MOK.
