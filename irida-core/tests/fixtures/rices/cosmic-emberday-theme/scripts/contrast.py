#!/usr/bin/env python3
"""Ember Day palette table for the README (stdout) with contrast ratios on stderr.
Exit 1 if a readable role is under 4.5:1 on the window background."""
import sys

WINDOW = "#E9E6DF"
PANE = "#DDD9D1"

SURFACES = [
    ("Background", "Stone", ["#E9E6DF"]),
    ("Containers", "Linen / Pane", ["#E2DED6", "#DDD9D1"]),
    ("Terminal", "Pane, opaque", ["#DDD9D1"]),
]

# (role, name, hex, reads) where reads=True means a user reads it as text.
ROLES = [
    ("Text", "Ink", "#2B2E36", True),
    ("Accent", "Ember", "#A4441A", True),
    ("Success", "Moss", "#4C6B45", True),
    ("Warning", "Amber", "#8A5E12", True),
    ("Error", "Brick", "#9E3A3A", True),
    ("Blue", "Slate blue", "#2F5C8A", True),
    ("Magenta", "Heather", "#6E4E8C", True),
    ("Cyan", "Verdigris", "#2E6E6A", True),
    ("Muted", "Muted", "#6B6F78", False),
    ("Comment", "Comment", "#8C9099", False),
]


def swatch(hexv):
    hx = hexv.lstrip("#").lower()
    return f"![{hexv}](https://placehold.co/12x12/{hx}/{hx}.png)"


def lum(h):
    r, g, b = (int(h[i:i + 2], 16) / 255 for i in (1, 3, 5))
    f = lambda c: c / 12.92 if c <= 0.03928 else ((c + 0.055) / 1.055) ** 2.4
    return 0.2126 * f(r) + 0.7152 * f(g) + 0.0722 * f(b)


def ratio(a, b):
    la, lb = sorted((lum(a), lum(b)), reverse=True)
    return (la + 0.05) / (lb + 0.05)


def main():
    bad = []
    print("| Role | Color | Hex |")
    print("|------|-------|-----|")
    for role, name, hexes in SURFACES:
        hx = " / ".join(f"`{x}`" for x in hexes)
        print(f"| {role} | {swatch(hexes[0])} {name} | {hx} |")
    for role, name, hexv, reads in ROLES:
        w, p = ratio(hexv, WINDOW), ratio(hexv, PANE)
        print(f"| {role} | {swatch(hexv)} {name} | `{hexv}` |")
        print(f"  {role}: {w:.1f}:1 on window, {p:.1f}:1 on pane", file=sys.stderr)
        if reads and w < 4.5:
            bad.append((role, w))
    for role, w in bad:
        print(f"FAIL {role} {w:.1f} < 4.5 on window", file=sys.stderr)
    return 1 if bad else 0


if __name__ == "__main__":
    sys.exit(main())
