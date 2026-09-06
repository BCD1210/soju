#!/usr/bin/env python3
"""Galaxy uses exit 255 for a successful handoff to an already running client.

Preserve every other failure. Match its explicit success message, not just 255,
and stream output instead of retaining an unbounded copy of the client log.
"""
import subprocess
import sys

SUCCESS = b"returned exit code -1 (Message passed in argument has been sent successfully to another client.)"


def main():
    if len(sys.argv) < 2:
        return 64
    forwarded = False
    with subprocess.Popen(sys.argv[1:], stdout=subprocess.PIPE, stderr=subprocess.STDOUT) as process:
        for line in process.stdout:
            forwarded = forwarded or SUCCESS in line
            sys.stdout.buffer.write(line)
            sys.stdout.buffer.flush()
        code = process.wait()
    return 0 if code == 255 and forwarded else code


if __name__ == '__main__':
    sys.exit(main())
