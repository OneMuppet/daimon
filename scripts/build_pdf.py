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

# ---- Figure 1: the cognitive-cycle flowchart (vector, palette-matched) ----
from reportlab.platypus.flowables import Flowable

BOXFILL = colors.HexColor("#ece9f8")   # light violet tint
BOXFILL2 = colors.HexColor("#fdeee7")  # light coral tint (System-2)
BANDFILL = colors.HexColor("#f4f2fb")  # praxis footer band

# the 7 steps: (title, sub-label lines)
_STEPS = [
    ("Perceive", ["world-model", "(beliefs)"]),
    ("Appraise", ["drives · affect", "homeostat"]),
    ("Reflex", ["fast, safe responses", "(System 1)"]),
    ("Decide", ["System-1", "arbitration"]),
    ("Plan", ["forward-model", "planning"]),
    ("Act", ["the chosen", "action"]),
    ("Reflect", ["memory: Hebbian +", "ACT-R + replay"]),
]


class CognitiveCycle(Flowable):
    """A clean vector flowchart of the seven-step cognitive cycle.

    Replaces the old box-drawing ASCII (kept as a fallback in RESEARCH.md).
    Two rows of rounded boxes (4 + 3), arrows along each row, a wrap arrow
    from the end of row 1 to the start of row 2, a loop-back arrow to World,
    a downward 'escalate?' branch from Decide to a coral System-2 box, and a
    Praxis footer band spanning the full width.
    """

    def __init__(self, width):
        Flowable.__init__(self)
        self.fig_w = width
        # vertical budget — tuned to read well at column width
        self.row_h = 50          # box height
        self.gap_y = 64          # vertical gap between the two box rows
        self.world_h = 22        # World pill height
        self.s2_h = 38           # System-2 box height
        self.band_h = 34         # praxis footer band height
        self.top_pad = 6
        self.height = (self.top_pad + self.world_h + 22 + self.row_h
                       + self.gap_y + self.row_h + 30 + self.s2_h + 18
                       + self.band_h)
        self.width = self.fig_w

    def wrap(self, availWidth, availHeight):
        return (self.fig_w, self.height)

    # -- drawing helpers ----------------------------------------------------
    def _box(self, c, x, y, w, h, title, subs, fill, stroke, r=6):
        c.setFillColor(fill)
        c.setStrokeColor(stroke)
        c.setLineWidth(1.1)
        c.roundRect(x, y, w, h, r, stroke=1, fill=1)
        cx = x + w / 2.0
        c.setFillColor(INK)
        c.setFont("Sans-Bold", 9.2)
        # title sits in upper portion
        c.drawCentredString(cx, y + h - 15, title)
        c.setFont("Sans", 6.3)
        c.setFillColor(MUTED)
        sy = y + h - 26
        for line in subs:
            c.drawCentredString(cx, sy, line)
            sy -= 8.2

    def _arrow(self, c, x1, y1, x2, y2, color=ACCENT, lw=1.3, head=4.2):
        import math
        c.setStrokeColor(color)
        c.setFillColor(color)
        c.setLineWidth(lw)
        c.line(x1, y1, x2, y2)
        ang = math.atan2(y2 - y1, x2 - x1)
        for da in (math.radians(150), math.radians(-150)):
            hx = x2 + head * math.cos(ang + da)
            hy = y2 + head * math.sin(ang + da)
            c.line(x2, y2, hx, hy)
        # filled triangle head
        p = c.beginPath()
        p.moveTo(x2, y2)
        p.lineTo(x2 + head * math.cos(ang + math.radians(150)),
                 y2 + head * math.sin(ang + math.radians(150)))
        p.lineTo(x2 + head * math.cos(ang - math.radians(150)),
                 y2 + head * math.sin(ang - math.radians(150)))
        p.close()
        c.drawPath(p, stroke=0, fill=1)

    def draw(self):
        c = self.canv
        W = self.fig_w
        H = self.height
        c.saveState()

        n_top = 4
        bw_gap = 14           # horizontal gap between boxes in a row
        # row 1 has 4 boxes; row 2 has 3 boxes — size boxes to the wider row
        bw = (W - (n_top - 1) * bw_gap) / n_top
        r1 = _STEPS[:4]
        r2 = _STEPS[4:]

        # -- y coordinates (origin bottom-left of the flowable) ------------
        world_y = H - self.top_pad - self.world_h
        row1_y = world_y - 22 - self.row_h
        row2_y = row1_y - self.gap_y - self.row_h
        s2_y = row2_y - 30 - self.s2_h
        band_y = 0

        # -- World pill (top) ----------------------------------------------
        world_w = 96
        world_x = 0
        c.setFillColor(colors.white)
        c.setStrokeColor(INK)
        c.setLineWidth(1.1)
        c.roundRect(world_x, world_y, world_w, self.world_h, 10, stroke=1, fill=1)
        c.setFillColor(INK)
        c.setFont("Sans-Bold", 9.2)
        c.drawCentredString(world_x + world_w / 2, world_y + 6.5, "World")

        # -- Row 1 boxes ----------------------------------------------------
        r1_x = []
        for k, (title, subs) in enumerate(r1):
            x = k * (bw + bw_gap)
            r1_x.append(x)
            self._box(c, x, row1_y, bw, self.row_h, title, subs, BOXFILL, ACCENT)

        # World -> Perceive (down then arrow into box 0)
        px0 = r1_x[0] + bw / 2
        self._arrow(c, world_x + world_w / 2, world_y,
                    px0, row1_y + self.row_h)

        # arrows between row-1 boxes
        for k in range(n_top - 1):
            x_from = r1_x[k] + bw
            x_to = r1_x[k + 1]
            ymid = row1_y + self.row_h / 2
            self._arrow(c, x_from, ymid, x_to, ymid)

        # -- wrap arrow: end of row1 -> start of row2 ----------------------
        last1_cx = r1_x[-1] + bw / 2
        first2_x = []
        n_bot = 3
        for k in range(n_bot):
            first2_x.append(k * (bw + bw_gap))
        wrap_x_right = r1_x[-1] + bw + 8
        # down the right edge, across, into top of first row-2 box
        c.setStrokeColor(ACCENT)
        c.setLineWidth(1.3)
        ymid1 = row1_y + self.row_h / 2
        ymid2 = row2_y + self.row_h / 2
        # from right of last row1 box -> out -> down -> to above first row2 box
        c.line(r1_x[-1] + bw, ymid1, wrap_x_right, ymid1)
        midy = (row1_y + row2_y + self.row_h) / 2
        c.line(wrap_x_right, ymid1, wrap_x_right, midy)
        col2_cx = first2_x[0] + bw / 2
        c.line(wrap_x_right, midy, col2_cx, midy)
        self._arrow(c, col2_cx, midy, col2_cx, row2_y + self.row_h)

        # -- Row 2 boxes ----------------------------------------------------
        for k, (title, subs) in enumerate(r2):
            x = first2_x[k]
            self._box(c, x, row2_y, bw, self.row_h, title, subs, BOXFILL, ACCENT)

        # arrows between row-2 boxes
        for k in range(n_bot - 1):
            x_from = first2_x[k] + bw
            x_to = first2_x[k + 1]
            self._arrow(c, x_from, ymid2, x_to, ymid2)

        # -- loop-back arrow: last row2 box (Reflect) -> World -------------
        last2_x = first2_x[-1]
        loop_x_right = last2_x + bw + 8
        c.setStrokeColor(MUTED)
        c.setLineWidth(1.2)
        c.line(last2_x + bw, ymid2, loop_x_right, ymid2)
        c.line(loop_x_right, ymid2, loop_x_right, world_y + self.world_h / 2)
        # back across the very top to World's right edge
        self._arrow(c, loop_x_right, world_y + self.world_h / 2,
                    world_x + world_w, world_y + self.world_h / 2,
                    color=MUTED, lw=1.2)
        c.setFont("Sans-Italic" if False else "Sans", 6.2)
        c.setFillColor(MUTED)
        c.drawCentredString(loop_x_right, ymid2 + (world_y + self.world_h / 2 - ymid2) / 2,
                            "")  # (label omitted; geometry reads as a loop)

        # -- escalate? branch: Decide (row1 box 3) -> System-2 -------------
        decide_x = r1_x[3]
        decide_cx = decide_x + bw / 2
        # System-2 box sits below the second row, under Decide column-ish
        s2_w = bw * 1.45
        s2_x = decide_cx - s2_w / 2
        if s2_x < 0:
            s2_x = 0
        if s2_x + s2_w > W:
            s2_x = W - s2_w
        # vertical drop from Decide bottom, routed down past row2 to the S2 box
        c.setStrokeColor(CORAL)
        c.setLineWidth(1.3)
        c.line(decide_cx, row1_y, decide_cx, s2_y + self.s2_h + 14)
        self._arrow(c, decide_cx, s2_y + self.s2_h + 14,
                    s2_x + s2_w / 2, s2_y + self.s2_h, color=CORAL, lw=1.3)
        # 'escalate?' label on the branch
        c.setFont("Sans-Bold", 6.6)
        c.setFillColor(CORAL)
        c.drawString(decide_cx + 4, row2_y - 2, "escalate?")
        # System-2 box (coral)
        self._box(c, s2_x, s2_y, s2_w, self.s2_h, "System-2 deliberator",
                  ["the slow, deliberate planner"], BOXFILL2, CORAL)

        # -- Praxis footer band (full width) -------------------------------
        c.setFillColor(BANDFILL)
        c.setStrokeColor(ACCENT)
        c.setLineWidth(0.9)
        c.roundRect(0, band_y, W, self.band_h, 5, stroke=1, fill=1)
        c.setFillColor(ACCENT)
        c.setFont("Sans-Bold", 8.6)
        c.drawCentredString(W / 2, band_y + self.band_h - 14,
                            "Praxis — concept · affordance · goal genesis")
        c.setFillColor(MUTED)
        c.setFont("Sans", 6.6)
        c.drawCentredString(W / 2, band_y + 7,
                            "empowerment · imagination · meta-motivation · "
                            "quantum cognition · criticality all attach to this spine")

        c.restoreState()

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
            info = s.strip()[3:].strip()  # info-string after the fence
            i += 1
            buf = []
            while i < n and not lines[i].strip().startswith("```"):
                buf.append(lines[i].rstrip("\n"))
                i += 1
            i += 1
            # drawn-figure marker: swap in a vector flowchart (ASCII kept as fallback)
            if info == "figure:cognitive-cycle":
                flow.append(KeepTogether(CognitiveCycle(CW)))
            else:
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
META1 = "Version 2.4      ·      18 June 2026      ·      9 theorems machine-checked"
META2 = "45 ablation criteria   ·   88 tests   ·   deterministic   ·   pure-Rust   ·   A4"
DATEISO = "2026-06-18"


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
    c.drawRightString(PW - RM, hy, "Technical Report  ·  v2.4")
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
