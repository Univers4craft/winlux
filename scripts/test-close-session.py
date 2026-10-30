#!/usr/bin/python3
"""Check that shutdown waits for windows and aborts when they refuse."""
import runpy
from pathlib import Path
from unittest.mock import patch

module = runpy.run_path(str(Path(__file__).with_name("winlux-close-session.py")))
main = module["main"]
namespace = main.__globals__
with patch.dict(namespace, {"windows": iter([{"0x1"}, set()]).__next__}), \
     patch.object(namespace["subprocess"], "run") as close:
    assert main() == 0
    assert close.call_args.args[0] == ["wmctrl", "-ic", "0x1"]
with patch.dict(namespace, {"windows": lambda: {"0x1"}}), \
     patch.object(namespace["subprocess"], "run"), \
     patch.object(namespace["time"], "monotonic", side_effect=[0, 31]):
    assert main() == 1
print("OK: orderly close and refusal cancels shutdown")
