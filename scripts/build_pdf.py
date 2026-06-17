#!/usr/bin/env python3
"""Render RESEARCH.md to a publication-grade PDF with ReportLab.

Matches the FrostOak whitepaper's design language (kicker · oversized title with
an accent underline · running header/footer with rules · mono metadata lines)
but in **Daimon's own palette**: the violet "mind" brand orb, a coral section
accent, warm ink on near-white, an elegant serif body. Unicode-heavy text and
the box-drawing Figure 1 render via embedded DejaVu fonts.

    python3 scripts/build_pdf.py            # -> Daimon-RESEARCH.pdf
"""
import os
import re
import sys
import datetime

import matplotlib
from reportlab.lib.pagesizes import A4
from reportlab.lib import colors
from reportlab.lib.styles import ParagraphStyle
from reportlab.lib.enums import TA_LEFT, TA_JUSTIFY, TA_RIGHT
from reportlab.platypus import (
    BaseDocTemplate, PageTemplate, Frame, Paragraph, Spacer, Table, TableStyle,
    Preformatted, PageBreak, KeepTogether, HRFlowable,
)
from reportlab.platypus.tableofcontents import TableOfContents
from reportlab.pdfbase import pdfmetrics
from reportlab.pdfbase.ttfonts import TTFont
from reportlab.pdfgen import canvas

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
SRC = os.path.join(ROOT, "RESEARCH.md")
OUT = os.path.join(ROOT, "Daimon-RESEARCH.pdf")

# ---- palette (Daimon) -----------------------------------------------------
INK = colors.HexColor("#16131f")      # warm near-black
ACCENT = colors.HexColor("#5b3df0")   # Daimon brand violet (the mind orb)
CORAL = colors.HexColor("#ef6a3d")    # Daimon section accent
MUTED = colors.HexColor("#6f6a7d")
RULE = colors.HexColor("#d9d6e2")
CODEBG = colors.HexColor("#f4f2fb")
HEADBG = colors.HexColor("#ece9f8")
STRIPE = colors.HexColor("#faf9fe")

# ---- fonts (DejaVu: full Unicode incl box-drawing + math) -----------------
TTF = os.path.join(os.path.dirname(matplotlib.__file__), "mpl-data/fonts/ttf")
def _reg(name, fn):
    pdfmetrics.registerFont(TTFont(name, os.path.join(TTF, fn)))
_reg("Body", "DejaVuSerif.ttf")
_reg("Body-Bold", "DejaVuSerif-Bold.ttf")
_reg("Body-Italic", "DejaVuSerif-Italic.ttf")
_reg("Body-BoldItalic", "DejaVuSerif-BoldItalic.ttf")
pdfmetrics.registerFontFamily("Body", normal="Body", bold="Body-Bold",
                              italic="Body-Italic", boldItalic="Body-BoldItalic")
_reg("Sans", "DejaVuSans.ttf")
_reg("Sans-Bold", "DejaVuSans-Bold.ttf")
pdfmetrics.registerFontFamily("Sans", normal="Sans", bold="Sans-Bold",
                              italic="Sans", boldItalic="Sans-Bold")
_reg("Mono", "DejaVuSansMono.ttf")
_reg("Mono-Bold", "DejaVuSansMono-Bold.ttf")

# ---- geometry -------------------------------------------------------------
PW, PH = A4
LM = RM = 52
TM, BM = 74, 58
CW = PW - LM - RM  # content width

# ---- styles ---------------------------------------------------------------
BODY = ParagraphStyle("BODY", fontName="Body", fontSize=9.7, leading=14.2,
                      alignment=TA_JUSTIFY, textColor=INK, spaceAfter=5)
H2 = ParagraphStyle("H2", fontName="Sans-Bold", fontSize=14, leading=17,
                    textColor=INK, spaceBefore=15, spaceAfter=3, keepWithNext=True)
H3 = ParagraphStyle("H3", fontName="Sans-Bold", fontSize=11, leading=14,
                    textColor=INK, spaceBefore=11, spaceAfter=2, keepWithNext=True)
H4 = ParagraphStyle("H4", fontName="Sans-Bold", fontSize=9.7, leading=13,
                    textColor=INK, spaceBefore=7, spaceAfter=1, keepWithNext=True)
BULLET = ParagraphStyle("BULLET", parent=BODY, leftIndent=16, bulletIndent=3,
                        spaceAfter=3)
REF = ParagraphStyle("REF", fontName="Body", fontSize=8.5, leading=11.4,
                     alignment=TA_LEFT, textColor=INK, leftIndent=13,
                     firstLineIndent=-13, spaceAfter=3.5)
CODE = ParagraphStyle("CODE", fontName="Mono", fontSize=6.4, leading=8.1, textColor=INK)
CELL = ParagraphStyle("CELL", fontName="Body", fontSize=7.9, leading=10, textColor=INK)
CELLH = ParagraphStyle("CELLH", fontName="Sans-Bold", fontSize=7.9, leading=10, textColor=INK)
CAP = ParagraphStyle("CAP", parent=BODY, fontSize=8.6, leading=11.5, textColor=MUTED,
                     alignment=TA_LEFT, spaceBefore=3, spaceAfter=8)

# ---- inline markdown -> reportlab mini-markup -----------------------------
def esc(s):
    return s.replace("&", "&amp;").replace("<", "&lt;").replace(">", "&gt;")

def inline(s):
    codes = []
    def grab(m):
        codes.append(m.group(1))
        return f"\x00{len(codes)-1}\x00"
    s = re.sub(r"`([^`]+)`", grab, s)
    s = esc(s)
    s = re.sub(r"\[([^\]]+)\]\(([^)]+)\)",
               lambda m: f'<a href="{m.group(2)}"><font color="#5b3df0">{m.group(1)}</font></a>', s)
    s = re.sub(r"\*\*([^*]+)\*\*", r"<b>\1</b>", s)
    s = re.sub(r"(?<!\*)\*([^*\n]+)\*(?!\*)", r"<i>\1</i>", s)
    def give(m):
        return f'<font face="Mono" size="8.4" backColor="#f1eef9">{esc(codes[int(m.group(1))])}</font>'
    s = re.sub(r"\x00(\d+)\x00", give, s)
    return s

# ---- table builder --------------------------------------------------------
def split_row(line):
    """Split a Markdown table row on '|' — but NOT on pipes inside `code` spans
    (e.g. the cell `|S| ≤ 2√2`), which would otherwise shred the cell.
    Assumes backticks balance within a row (true for all RESEARCH.md tables); an
    unclosed span would swallow later delimiters."""
    line = line.strip()
    if line.startswith("|"):
        line = line[1:]
    if line.endswith("|"):
        line = line[:-1]
    cells, cur, in_code = [], "", False
    for ch in line:
        if ch == "`":
            in_code = not in_code
            cur += ch
        elif ch == "|" and not in_code:
            cells.append(cur.strip())
            cur = ""
        else:
            cur += ch
    cells.append(cur.strip())
    return cells


def make_table(rows):
    header, body = rows[0], rows[1:]
    ncols = len(header)
    if len(header[0]) <= 3:
        w0 = 42
        rest = (CW - w0) / (ncols - 1) if ncols > 1 else CW
        widths = [w0] + [rest] * (ncols - 1)
    else:
        widths = [CW / ncols] * ncols
    data = [[Paragraph(inline(c), CELLH) for c in header]]
    for r in body:
        r = (r + [""] * ncols)[:ncols]
        data.append([Paragraph(inline(c), CELL) for c in r])
    t = Table(data, colWidths=widths, repeatRows=1)
    style = [
        ("BACKGROUND", (0, 0), (-1, 0), HEADBG),
        ("LINEBELOW", (0, 0), (-1, 0), 0.7, ACCENT),
        ("GRID", (0, 0), (-1, -1), 0.4, RULE),
        ("VALIGN", (0, 0), (-1, -1), "TOP"),
        ("LEFTPADDING", (0, 0), (-1, -1), 5),
        ("RIGHTPADDING", (0, 0), (-1, -1), 5),
        ("TOPPADDING", (0, 0), (-1, -1), 2.5),
        ("BOTTOMPADDING", (0, 0), (-1, -1), 2.5),
    ]
    for i in range(2, len(data), 2):
        style.append(("BACKGROUND", (0, i), (-1, i), STRIPE))
    t.setStyle(TableStyle(style))
    return t

def code_card(text):
    pre = Preformatted(text, CODE)
    t = Table([[pre]], colWidths=[CW])
    t.setStyle(TableStyle([
        ("BACKGROUND", (0, 0), (-1, -1), CODEBG),
        ("BOX", (0, 0), (-1, -1), 0.5, RULE),
        ("LEFTPADDING", (0, 0), (-1, -1), 8),
        ("RIGHTPADDING", (0, 0), (-1, -1), 8),
        ("TOPPADDING", (0, 0), (-1, -1), 6),
        ("BOTTOMPADDING", (0, 0), (-1, -1), 6),
    ]))
    return t

# ---- markdown body -> flowables -------------------------------------------
def parse_body(lines):
    flow = []
    i, n = 0, len(lines)
    in_refs = False
    while i < n:
        ln = lines[i]
        s = ln.rstrip("\n")
        # fenced code
        if s.strip().startswith("```"):
            i += 1
            buf = []
            while i < n and not lines[i].strip().startswith("```"):
                buf.append(lines[i].rstrip("\n"))
                i += 1
            i += 1
            flow.append(code_card("\n".join(buf)))
            continue
        # table
        if s.lstrip().startswith("|") and i + 1 < n and re.match(r"^\s*\|?[\s:|-]+\|", lines[i + 1]):
            rows = []
            while i < n and lines[i].lstrip().startswith("|"):
                rows.append(split_row(lines[i]))
                i += 1
            sep = rows.pop(1) if len(rows) > 1 else None  # drop |---| row
            _ = sep
            flow.append(make_table(rows))
            flow.append(Spacer(0, 4))
            continue
        # headings
        m = re.match(r"^(#{2,4})\s+(.*)$", s)
        if m:
            level = len(m.group(1))
            text = m.group(2).strip()
            if re.match(r"^10\.?\s+References", text) or text.lower().startswith("references"):
                in_refs = True
            if level == 2:
                flow.append(Paragraph(inline(text), H2))
                flow.append(HRFlowable(width="100%", thickness=0.7, color=RULE,
                                       spaceBefore=1, spaceAfter=6, lineCap="round"))
            elif level == 3:
                flow.append(Paragraph(inline(text), H3))
            else:
                flow.append(Paragraph(inline(text), H4))
            i += 1
            continue
        # hr
        if re.match(r"^(---+|\*\*\*+)\s*$", s):
            flow.append(Spacer(0, 3))
            i += 1
            continue
        # blank
        if not s.strip():
            i += 1
            continue
        # list (bullet or ordered)
        lm = re.match(r"^(\s*)([-*+]|\d+\.)\s+(.*)$", s)
        if lm:
            marker = lm.group(2)
            text = lm.group(3)
            i += 1
            # gather soft-wrapped continuation lines
            while i < n:
                nxt = lines[i].rstrip("\n")
                if (not nxt.strip() or re.match(r"^(\s*)([-*+]|\d+\.)\s+", nxt)
                        or re.match(r"^#{2,4}\s", nxt) or nxt.strip().startswith("```")
                        or nxt.lstrip().startswith("|") or re.match(r"^---+\s*$", nxt)):
                    break
                text += " " + nxt.strip()
                i += 1
            bullet = "•" if marker in "-*+" else marker
            flow.append(Paragraph(inline(text), BULLET, bulletText=bullet))
            continue
        # paragraph (gather soft-wrapped lines)
        text = s.strip()
        i += 1
        while i < n:
            nxt = lines[i].rstrip("\n")
            if (not nxt.strip() or re.match(r"^#{2,4}\s", nxt) or nxt.strip().startswith("```")
                    or nxt.lstrip().startswith("|") or re.match(r"^(\s*)([-*+]|\d+\.)\s+", nxt)
                    or re.match(r"^---+\s*$", nxt)):
                break
            text += " " + nxt.strip()
            i += 1
        # caption styling for "Figure N."
        if re.match(r"^\*?Figure \d", text):
            flow.append(Paragraph(inline(text), CAP))
        else:
            flow.append(Paragraph(inline(text), REF if in_refs else BODY))
    return flow

# ---- title page + running deco (drawn on the canvas) ----------------------
KICKER_L = "AUTONOMOUS GAME AI   ·   COGNITIVE ARCHITECTURE"
KICKER_R = "TECHNICAL REPORT"
TITLE = "Daimon"
SUBTITLE = "A Self-Authoring Cognitive Architecture for Autonomous Game Agents"
AUTHOR = "David Borgenvik   ·   Independent research"
LEDE = ("Daimon is a deterministic, CPU-only, pure-Rust cognitive architecture in "
        "which a game agent authors its own concepts, goals, world-model, and even "
        "its values from lived experience. This report states plainly what is "
        "<b>measured</b>, what is <b>proved</b>, and what is honestly deferred.")
QUOTE = ("“Can the agent carve up its own world, set its own ends, model its own "
         "dynamics, and revise its own values — and can we <i>prove</i> it did, "
         "against things it was never built for?”")
META1 = "Version 2.0      ·      16 June 2026      ·      9 theorems machine-checked"
META2 = "47 ablation criteria   ·   82 tests   ·   deterministic   ·   pure-Rust   ·   A4"
DATEISO = "2026-06-16"


def draw_title(c):
    c.saveState()
    # top rule
    c.setStrokeColor(INK)
    c.setLineWidth(1.4)
    c.line(LM, PH - 96, PW - RM, PH - 96)
    # kicker
    c.setFont("Sans-Bold", 9)
    c.setFillColor(ACCENT)
    c.drawString(LM, PH - 112, KICKER_L)
    c.setFont("Sans", 9)
    c.setFillColor(MUTED)
    c.drawRightString(PW - RM, PH - 112, KICKER_R)
    # title
    c.setFont("Sans-Bold", 54)
    c.setFillColor(INK)
    c.drawString(LM - 2, PH - 188, TITLE)
    # the violet mind-orb as a brand mark, set clear to the right of the wordmark
    tw = pdfmetrics.stringWidth(TITLE, "Sans-Bold", 54)
    ox = LM - 2 + tw + 26
    c.setFillColor(ACCENT)
    c.circle(ox, PH - 172, 8, stroke=0, fill=1)
    c.setFillColor(colors.white)
    c.circle(ox, PH - 172, 2.7, stroke=0, fill=1)
    # coral accent underline
    c.setFillColor(CORAL)
    c.rect(LM, PH - 205, 150, 5, stroke=0, fill=1)
    # subtitle
    c.setFont("Sans", 14)
    c.setFillColor(INK)
    c.drawString(LM, PH - 232, SUBTITLE)
    # author
    c.setFont("Sans-Bold", 11)
    c.setFillColor(ACCENT)
    c.drawString(LM, PH - 258, AUTHOR)
    # lede (wrapped)
    p = Paragraph(LEDE, ParagraphStyle("lede", fontName="Body", fontSize=10.5,
                                       leading=15.5, textColor=INK, alignment=TA_LEFT))
    w, h = p.wrap(360, 200)
    p.drawOn(c, LM, PH - 285 - h)
    # blockquote with a violet left bar
    qy = PH - 285 - h - 28
    q = Paragraph(QUOTE, ParagraphStyle("q", fontName="Body-Italic", fontSize=11.5,
                                        leading=16.5, textColor=colors.HexColor("#2c2738"),
                                        alignment=TA_LEFT))
    qw, qh = q.wrap(360, 200)
    c.setFillColor(ACCENT)
    c.rect(LM, qy - qh, 3, qh, stroke=0, fill=1)
    q.drawOn(c, LM + 14, qy - qh)
    # bottom rule + metadata
    c.setStrokeColor(RULE)
    c.setLineWidth(0.8)
    c.line(LM, 132, PW - RM, 132)
    c.setFont("Mono-Bold", 9)
    c.setFillColor(INK)
    c.drawString(LM, 116, META1)
    c.setFont("Mono", 8)
    c.setFillColor(MUTED)
    c.drawString(LM, 102, META2)
    c.restoreState()


def draw_running(c, page, total):
    c.saveState()
    # header
    hy = PH - 44
    c.setFillColor(ACCENT)
    c.circle(LM + 3, hy + 2.6, 3, stroke=0, fill=1)
    c.setFont("Sans-Bold", 8)
    c.setFillColor(INK)
    c.drawString(LM + 11, hy, "DAIMON")
    c.setFont("Sans", 8)
    c.setFillColor(MUTED)
    c.drawString(LM + 11 + pdfmetrics.stringWidth("DAIMON", "Sans-Bold", 8) + 6, hy,
                 "A Self-Authoring Cognitive Architecture")
    c.drawRightString(PW - RM, hy, "Technical Report  ·  v2.0")
    c.setStrokeColor(RULE)
    c.setLineWidth(0.6)
    c.line(LM, hy - 6, PW - RM, hy - 6)
    # footer
    fy = 40
    c.setStrokeColor(RULE)
    c.line(LM, fy + 12, PW - RM, fy + 12)
    c.setFont("Mono", 7.5)
    c.setFillColor(MUTED)
    c.drawString(LM, fy, DATEISO)
    c.drawCentredString(PW / 2, fy, f"{page} / {total}")
    c.drawRightString(PW - RM, fy, "deterministic  ·  reproducible")
    c.restoreState()


class DaimonCanvas(canvas.Canvas):
    def __init__(self, *a, **k):
        super().__init__(*a, **k)
        self._saved = []
    def showPage(self):
        self._saved.append(dict(self.__dict__))
        self._startPage()
    def save(self):
        total = len(self._saved)
        for st in self._saved:
            self.__dict__.update(st)
            if self._pageNumber == 1:
                draw_title(self)
            else:
                draw_running(self, self._pageNumber, total)
            super().showPage()
        super().save()


class DaimonDoc(BaseDocTemplate):
    def afterFlowable(self, flowable):
        if isinstance(flowable, Paragraph):
            st = flowable.style.name
            txt = flowable.getPlainText()
            if st == "H2":
                key = f"h2-{self.page}-{hash(txt) & 0xffff}"
                self.canv.bookmarkPage(key)
                self.canv.addOutlineEntry(txt, key, level=0)
                self.notify("TOCEntry", (0, txt, self.page))
            elif st == "H3":
                key = f"h3-{self.page}-{hash(txt) & 0xffff}"
                self.canv.bookmarkPage(key)
                self.canv.addOutlineEntry(txt, key, level=1)
                self.notify("TOCEntry", (1, txt, self.page))


def main():
    with open(SRC, encoding="utf-8") as f:
        lines = f.readlines()
    # body starts at the Abstract heading (title/author/keywords go on the title page)
    start = next(i for i, l in enumerate(lines) if l.strip().startswith("## Abstract"))
    body = parse_body(lines[start:])

    toc = TableOfContents()
    toc.levelStyles = [
        ParagraphStyle("toc0", fontName="Sans-Bold", fontSize=10, leading=20,
                       textColor=INK),
        ParagraphStyle("toc1", fontName="Body", fontSize=9, leading=15,
                       leftIndent=18, textColor=colors.HexColor("#3a3548")),
    ]

    story = [PageBreak(),  # leave page 1 for the canvas-painted title
             Paragraph("Contents", H2),
             HRFlowable(width="100%", thickness=0.7, color=RULE, spaceAfter=8),
             toc, PageBreak()]
    story += body

    doc = DaimonDoc(OUT, pagesize=A4, leftMargin=LM, rightMargin=RM,
                    topMargin=TM, bottomMargin=BM, title="Daimon — Technical Report",
                    author="David Borgenvik")
    frame = Frame(LM, BM, CW, PH - TM - BM, id="body",
                  leftPadding=0, rightPadding=0, topPadding=0, bottomPadding=0)
    doc.addPageTemplates([PageTemplate(id="main", frames=[frame])])
    doc.multiBuild(story, canvasmaker=DaimonCanvas)
    print(f"wrote {OUT}  ({os.path.getsize(OUT)//1024} KB)")


if __name__ == "__main__":
    main()
