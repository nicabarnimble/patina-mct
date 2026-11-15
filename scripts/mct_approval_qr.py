"""Authoritative version-4/M QR renders for MCT approval capability URLs."""
from io import BytesIO
from pathlib import Path
import sys

sys.path.insert(0, str(Path(__file__).resolve().parent / "third_party"))
import segno

ESC = "\x1b"


def code(url):
    qr = segno.make(url, error="m", boost_error=False)
    if qr.version != 4 or qr.error != "M":
        raise ValueError("approval URL no longer fits the version-4/M QR contract")
    return qr


def svg(url, title="Scan on your phone"):
    output = BytesIO()
    code(url).save(output, kind="svg", scale=5, border=4, dark="#000", light="#fff",
                   svgclass=None, lineclass=None, xmldecl=False, title=title)
    return output.getvalue()


def png(url, scale=8):
    output = BytesIO()
    code(url).save(output, kind="png", scale=scale, border=4, dark="#000", light="#fff")
    return output.getvalue()


def terminal(url, ansi=True):
    matrix = [list(row) for row in code(url).matrix_iter(scale=1, border=4, verbose=False)]
    if len(matrix) % 2:
        matrix.append([0] * len(matrix[0]))
    glyphs = {(1, 1): "█", (1, 0): "▀", (0, 1): "▄", (0, 0): " "}
    rows = ["".join(glyphs[(top, bottom)] for top, bottom in zip(matrix[y], matrix[y + 1]))
            for y in range(0, len(matrix), 2)]
    if ansi:
        return "".join(f"{ESC}[30;107m{row}{ESC}[0m\n" for row in rows)
    return "\n".join(rows) + "\n"
