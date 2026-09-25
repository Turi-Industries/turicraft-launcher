#!/usr/bin/env python3
"""Images des installeurs, tirées du logo (scripts/branding.sh) :

  src-tauri/branding/sidebar.bmp   164×314  Windows, pages d'accueil et de fin
  src-tauri/branding/header.bmp    150×57   Windows, en-tête des autres pages
  src-tauri/branding/dmg.png       660×400  macOS, fenêtre du .dmg

Contraste : l'en-tête de Windows et la fenêtre du .dmg ont un fond CLAIR —
Windows y écrit ses titres, le Finder ses noms d'icônes, en noir. Seul le
bandeau latéral est sombre : il n'a que notre texte, clair.

Usage : installer-images.py <logo.png>   (Pillow : pip install pillow)
Le logo n'est pas sous GPL : ces images non plus, elles restent hors du dépôt.
"""
import os
import sys

from PIL import Image, ImageDraw, ImageFont

ROOT = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..")
OUT = os.path.join(ROOT, "src-tauri", "branding")
FONT = os.path.join(ROOT, "node_modules", "@fontsource", "silkscreen", "files", "silkscreen-latin-400-normal.woff2")

DARK = (18, 19, 22)        # --bg du launcher, #121316
YELLOW = (245, 197, 24)    # --yellow, #f5c518
LIGHT = (241, 241, 241)
GREY = (163, 166, 173)
GOLD_ON_WHITE = (138, 106, 0)  # « CRAFT » sur fond clair : le jaune n'y contraste pas assez


def font(size: int) -> ImageFont.FreeTypeFont:
    try:
        return ImageFont.truetype(FONT, size)
    except OSError:
        return ImageFont.load_default(size)


def logo(src: Image.Image, size: int) -> Image.Image:
    return src.resize((size, size), Image.LANCZOS)


def title(draw: ImageDraw.ImageDraw, x: int, y: int, size: int, turi, craft, center: bool = False) -> None:
    """« TURI CRAFT », « CRAFT » dans l'autre couleur, comme le logo texte du launcher."""
    f = font(size)
    w1 = draw.textlength("TURI ", font=f)
    w = w1 + draw.textlength("CRAFT", font=f)
    x0 = x - w / 2 if center else x
    draw.text((x0, y), "TURI ", font=f, fill=turi)
    draw.text((x0 + w1, y), "CRAFT", font=f, fill=craft)


def sidebar(src: Image.Image) -> None:
    im = Image.new("RGB", (164, 314), DARK)
    d = ImageDraw.Draw(im)
    lg = logo(src, 116)
    im.paste(lg, ((164 - 116) // 2, 56), lg)
    title(d, 82, 190, 17, LIGHT, YELLOW, center=True)
    d.rectangle((0, 310, 164, 314), fill=YELLOW)  # filet jaune, comme le bandeau du launcher
    im.save(os.path.join(OUT, "sidebar.bmp"))


def header(src: Image.Image) -> None:
    im = Image.new("RGB", (150, 57), (255, 255, 255))
    d = ImageDraw.Draw(im)
    lg = logo(src, 45)
    im.paste(lg, (4, 6), lg)
    f = font(13)
    d.text((56, 12), "TURI", font=f, fill=DARK)
    d.text((56, 28), "CRAFT", font=f, fill=GOLD_ON_WHITE)
    im.save(os.path.join(OUT, "header.bmp"))


def dmg(src: Image.Image) -> None:
    # Positions des icônes : tauri.branding.conf.json (appPosition…).
    im = Image.new("RGB", (660, 400), (246, 246, 247))
    d = ImageDraw.Draw(im)
    lg = logo(src, 48)
    im.paste(lg, (24, 20), lg)
    title(d, 84, 34, 22, DARK, GOLD_ON_WHITE)
    # Flèche de l'app vers Applications (icônes centrées en 180 et 480, y 200).
    d.line((250, 200, 400, 200), fill=DARK, width=6)
    d.polygon([(412, 200), (392, 186), (392, 214)], fill=DARK)
    msg = "Glisse Turi Craft dans Applications"
    f = font(19)
    d.text(((660 - d.textlength(msg, font=f)) / 2, 312), msg, font=f, fill=DARK)
    d.rectangle((0, 394, 660, 400), fill=YELLOW)
    im.save(os.path.join(OUT, "dmg.png"))


def main() -> None:
    src = Image.open(sys.argv[1]).convert("RGBA")
    os.makedirs(OUT, exist_ok=True)
    sidebar(src)
    header(src)
    dmg(src)
    print(f"images des installeurs : {OUT}")


if __name__ == "__main__":
    main()
