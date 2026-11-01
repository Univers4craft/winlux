#!/usr/bin/env python3
"""Lanceur du créateur de clé USB Winlux (sources, paquet .deb et exécutable Windows)."""
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.realpath(__file__)))

from winlux_usb.cli import main  # noqa: E402

if __name__ == "__main__":
    sys.exit(main())
