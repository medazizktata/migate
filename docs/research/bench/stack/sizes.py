# Binary size and gzip -9 size (roughly what an npm/pip/brew download carries).
import os, gzip, sys
for f in sys.argv[1:]:
    raw = open(f, 'rb').read()
    print(f"{f:36} {len(raw)/1048576:7.2f} MB   gzip {len(gzip.compress(raw, 9))/1048576:6.2f} MB")
