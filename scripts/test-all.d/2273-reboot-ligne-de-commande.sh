# #2273 : reboot, poweroff, halt et shutdown (liens vers le shim systemctl) passent par winlux-power.
step "reboot/poweroff/halt en ligne de commande (#2273)" python3 scripts/test-systemctl-power.py
