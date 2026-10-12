"""Converts WOFF 1.0 to TTF/OTF (zlib only, no dependencies)."""
import struct, sys, zlib

src, dst = sys.argv[1], sys.argv[2]
data = open(src, "rb").read()
sig, flavor, length, num, _, total, *_ = struct.unpack(">4sIIHHI", data[:20]) + (None,)
assert sig == b"wOFF", "not a WOFF 1.0 file"
tables = []
for i in range(num):
    tag, off, comp, orig, cks = struct.unpack(">4sIIII", data[44 + i * 20: 64 + i * 20])
    raw = data[off:off + comp]
    tables.append((tag, cks, raw if comp == orig else zlib.decompress(raw)))

tables.sort(key=lambda t: t[0])
es = max(k for k in range(16) if 2 ** k <= num)
sr = 2 ** es * 16
out = bytearray(struct.pack(">IHHHH", flavor, num, sr, es, num * 16 - sr))
offset = 12 + 16 * num
body = bytearray()
for tag, cks, tb in tables:
    out += struct.pack(">4sIII", tag, cks, offset + len(body), len(tb))
    body += tb + b"\0" * ((4 - len(tb) % 4) % 4)
open(dst, "wb").write(out + body)
print(dst, len(out + body))
