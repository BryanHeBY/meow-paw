#!/usr/bin/python3
"""Private filesystem-only fixture for the Linux proxy worker smoke test."""
import json
import os
from pathlib import Path
import sys

state = Path(os.environ["PAW_TEST_GSETTINGS_STATE"])
defaults = dict(zip(
    ("mode http/host http/port https/host https/port use-same-proxy ignore-hosts "
     "autoconfig-url http/use-authentication ftp/host ftp/port socks/host socks/port").split(),
    ("'none'", "''", "8080", "''", "0", "true", "['localhost']", "''", "false", "''", "0", "''", "0"),
))
values = json.loads(state.read_text())
if Path(sys.argv[0]).name == "dconf":
    if sys.argv[2] != "-d":
        assert sys.argv[2].startswith("/system/proxy/")
        print(values.get(sys.argv[2].removeprefix("/system/proxy/"), ""))
else:
    verb, schema, key = sys.argv[1:4]
    schema = schema.removeprefix("org.gnome.system.proxy").lstrip(".")
    name = f"{schema}/{key}" if schema else key
    assert name in defaults
    if verb == "writable":
        print("true")
    elif verb == "get":
        print(values.get(name, defaults[name]))
    elif verb == "set":
        values[name] = sys.argv[4]
        state.write_text(json.dumps(values))
    elif verb == "reset":
        del values[name]
        state.write_text(json.dumps(values))
    else:
        raise AssertionError(verb)
