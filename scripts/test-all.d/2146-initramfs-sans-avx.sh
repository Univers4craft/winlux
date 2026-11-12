# #2146 : initramfs construit avec les paquets amd64v3 de l'hôte, panique sans AVX.
step "Initramfs sans AVX, binaires de la racine native (#2146)" python3 scripts/test-initramfs-baseline.py
