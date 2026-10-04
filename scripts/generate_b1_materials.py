"""Generate small constant and horizontal-ramp PNGs for the Blender B1 check."""

import argparse
import binascii
import struct
import zlib
from pathlib import Path


WIDTH = 64
HEIGHT = 64


def png_chunk(kind, data):
    payload = kind + data
    return struct.pack(">I", len(data)) + payload + struct.pack(">I", binascii.crc32(payload))


def write_rgba_png(path, pixels):
    rows = b"".join(
        b"\0" + bytes(pixels[y * WIDTH * 4 : (y + 1) * WIDTH * 4])
        for y in range(HEIGHT)
    )
    header = struct.pack(">IIBBBBB", WIDTH, HEIGHT, 8, 6, 0, 0, 0)
    payload = (
        b"\x89PNG\r\n\x1a\n"
        + png_chunk(b"IHDR", header)
        + png_chunk(b"IDAT", zlib.compress(rows, level=9))
        + png_chunk(b"IEND", b"")
    )
    path.write_bytes(payload)


def height_pixels(profile):
    pixels = []
    for y in range(HEIGHT):
        for x in range(WIDTH):
            value = 128 if profile == "constant" else round(255 * x / (WIDTH - 1))
            pixels.extend((value, value, value, 255))
    return pixels


def albedo_pixels():
    pixels = []
    for y in range(HEIGHT):
        for x in range(WIDTH):
            if ((x // 8) + (y // 8)) % 2 == 0:
                pixels.extend((205, 145, 75, 255))
            else:
                pixels.extend((65, 110, 175, 255))
    return pixels


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output-dir", required=True, type=Path)
    args = parser.parse_args()
    color = albedo_pixels()
    for profile in ("constant", "ramp"):
        root = args.output_dir / profile
        root.mkdir(parents=True, exist_ok=True)
        write_rgba_png(root / "Textures_h.png", height_pixels(profile))
        write_rgba_png(root / "Textures_a.png", color)
        print(f"wrote {profile} material under {root}")


if __name__ == "__main__":
    main()
