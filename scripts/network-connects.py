#!/usr/bin/env python3
"""Summarise an `strace -f -e trace=connect` capture: who did the app try to reach?

    strace -f -qq -e trace=connect -o connects.txt scripts/dev.sh      # run, use the app, quit
    scripts/network-connects.py connects.txt

Prints each distinct internet destination with a count, and exits 1 if any is not this computer.
On a dev run the only expected destination is the Vite server (127.0.0.1 and ::1, port 1420);
DNS (port 53) would show up here as a destination too. This is the runtime half of DESIGN.md §8's
"no silent data paths" (S13.4); `src-tauri/tests/no_silent_network.rs` is the structural half.
"""
import collections
import re
import sys

LOCAL = {"127.0.0.1", "::1"}
destinations = collections.Counter()
for line in open(sys.argv[1]):
    if not re.search(r"sa_family=AF_INET6?\b", line):
        continue
    address = re.search(r'inet_addr\("([^"]+)"\)|inet_pton\(AF_INET6, "([^"]+)"', line)
    port = re.search(r"sin6?_port=htons\((\d+)\)", line)
    host = (address.group(1) or address.group(2)) if address else "?"
    destinations[(host, port.group(1) if port else "?")] += 1

outside = False
for (host, port), count in sorted(destinations.items()):
    local = host in LOCAL and port != "53"
    outside |= not local
    print(f"{'ok   ' if local else 'OUT  '} {host}:{port}  x{count}")
print("no connection left this computer" if not outside else "SOMETHING LEFT THIS COMPUTER")
sys.exit(1 if outside else 0)
