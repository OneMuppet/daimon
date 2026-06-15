// Daimon concept brochure — native pdfkit (no HTML middlestep).
// Cover, clickable TOC, outline bookmarks, per-page footers, vector charts.
// Brand in brand.json, copy in content.json; this file is pure layout.
// Layout engine adapted from the Reins brochure generator (same visual system).

const fs = require("fs");
const path = require("path");
const PDFDocument = require("pdfkit");

const brand = JSON.parse(fs.readFileSync(path.join(__dirname, "brand.json")));
const C = brand.colors;
const content = JSON.parse(fs.readFileSync(path.join(__dirname, "content.json")));

// ---------- document ----------
const PAGE_W = 595.28, PAGE_H = 841.89; // A4 portrait
const M = 52;                            // margin
const W = PAGE_W - M * 2;                // content width

const out = path.join(__dirname, `Daimon-brochure-v${brand.version}.pdf`);
const doc = new PDFDocument({
  size: "A4",
  margins: { top: M, bottom: M, left: M, right: M },
  bufferPages: true,
  autoFirstPage: false,
  info: {
    Title: `${brand.name} — ${brand.subtitle}`,
    Author: brand.company,
    Subject: brand.tagline,
  },
});
doc.pipe(fs.createWriteStream(out));

// fonts
for (const [k, p] of Object.entries(brand.fonts)) {
  doc.registerFont(k, path.join(__dirname, p));
}

const tocEntries = []; // {title, page}
let currentSection = "";

// ---------- helpers ----------
function shadowBox(x, y, w, h, opts = {}) {
  const off = opts.offset ?? 4;
  const shadow = opts.shadow ?? C.ink;
  const fill = opts.fill ?? C.card;
  const stroke = opts.stroke ?? C.ink;
  doc.save();
  doc.rect(x + off, y + off, w, h).fill(shadow);
  doc.rect(x, y, w, h).fillAndStroke(fill, stroke);
  doc.restore();
}

function mono(txt, x, y, opts = {}) {
  doc.font(opts.medium ? "monoMedium" : "mono")
    .fontSize(opts.size ?? 6.6)
    .fillColor(opts.color ?? C.muted)
    .text(txt.toUpperCase(), x, y, { characterSpacing: 1.4, width: opts.width, align: opts.align });
}

function rule(y, color = C.line, x1 = M, x2 = PAGE_W - M, w = 0.7) {
  doc.save().moveTo(x1, y).lineTo(x2, y).lineWidth(w).strokeColor(color).stroke().restore();
}

function sectionHeader(idx, kicker, title, lede) {
  doc.addPage();
  currentSection = title.replace(/\n/g, " ");
  doc.outline.addItem(`${idx} — ${currentSection}`);
  tocEntries.push({ title: currentSection, idx, page: doc.bufferedPageRange().count });

  // ghost numeral
  doc.font("display").fontSize(64).fillColor(C.ghost).text(idx, M, M - 8, { lineBreak: false });
  mono(kicker, M + 2, M + 64);

  const tx = M + 120;
  doc.font("displayMedium").fontSize(23).fillColor(C.ink)
    .text(title, tx, M + 2, { width: W - 120, lineGap: 2 });
  if (lede) {
    doc.moveDown(0.4);
    doc.font("body").fontSize(9.4).fillColor(C.muted)
      .text(lede, tx, doc.y, { width: W - 120, lineGap: 2.4 });
  }
  rule(doc.y + 16, C.ink, M, PAGE_W - M, 1);
  doc.y += 28;
  return doc.y;
}

// ---------- cover ----------
doc.addPage();
doc.rect(0, 0, PAGE_W, PAGE_H).fill(C.paper);

// hairline frame
doc.save().rect(26, 26, PAGE_W - 52, PAGE_H - 52).lineWidth(0.8).strokeColor(C.line).stroke().restore();

mono(`${brand.company} · ${brand.date} · v${brand.version}`, 52, 56, { color: C.muted });

// wordmark + coral period
doc.font("displayMedium").fontSize(58);
const wmW = doc.widthOfString(brand.name);
doc.fillColor(C.ink).text(brand.name, 52, 110, { lineBreak: false });
doc.circle(52 + wmW + 9, 110 + 47, 4.6).fill(C.coral);

doc.font("display").fontSize(29).fillColor(C.ink)
  .text("Your NPCs forget you the\nmoment you turn around.", 52, 205, { lineGap: 4 });
doc.font("displayItalic").fontSize(29).fillColor(C.blue)
  .text("Daimon doesn't.", 52, doc.y + 6);

doc.font("body").fontSize(10.5).fillColor(C.muted)
  .text("A cognitive architecture for autonomous game agents: drives that make them want, memory that makes them remember, theory of mind that makes them care — and a dual-process brain that thinks fast almost always and slow only when it matters, so a language-model mind is affordable for a thousand NPCs.",
    52, doc.y + 26, { width: 410, lineGap: 3 });

// cover stat band
const bandY = 560;
rule(bandY - 14, C.ink, 52, PAGE_W - 52, 1.2);
const bw = (PAGE_W - 104) / 4;
content.heroStats.forEach((s, i) => {
  const x = 52 + i * bw;
  mono(s.label, x, bandY, { color: C.muted, size: 5.8 });
  doc.font("displayMedium").fontSize(21).fillColor(C.ink).text(s.num, x, bandY + 14, { lineBreak: false });
  mono(s.sub, x, bandY + 44, { color: C.muted, size: 5.4 });
  if (i < 3) doc.save().moveTo(x + bw - 12, bandY).lineTo(x + bw - 12, bandY + 54).lineWidth(0.6).strokeColor(C.line).stroke().restore();
});

mono(brand.subtitle, 52, PAGE_H - 104, { color: C.coral });
doc.font("body").fontSize(8.5).fillColor(C.muted)
  .text("Every behaviour in this document is produced by the included Rust concept and reproducible from a single seed.", 52, PAGE_H - 88, { width: 470, height: 30 });

// ---------- TOC ----------
doc.addPage();
const tocPageIndex = doc.bufferedPageRange().count - 1;
mono("Contents", M, M);
doc.font("displayMedium").fontSize(30).fillColor(C.ink).text("What's inside", M, M + 16);

// ---------- 01 problem ----------
(function problem() {
  const p = content.problem;
  let y = sectionHeader("01", "the problem", p.title, p.lede);

  const gap = 12, cw = (W - gap * 2) / 3, ch = 178;
  p.stats.forEach((s, i) => {
    const x = M + i * (cw + gap);
    shadowBox(x, y, cw, ch, { offset: 4 });
    mono(s.kicker, x + 14, y + 14);
    doc.font("displayMedium").fontSize(26).fillColor(C.coral).text(s.big, x + 14, y + 26, { width: cw - 28 });
    doc.font("bodySemi").fontSize(8.6).fillColor(C.ink).text(s.head, x + 14, doc.y + 4, { width: cw - 28, lineGap: 1.6 });
    doc.font("body").fontSize(7.8).fillColor("#4d4956").text(s.body, x + 14, doc.y + 5, { width: cw - 28, lineGap: 1.8 });
    mono(s.src, x + 14, y + ch - 20, { size: 5.6 });
  });
  y += ch + 26;

  // the gap strip
  const stripH = 108;
  shadowBox(M, y, W, stripH, { fill: C.ink, shadow: C.coral, offset: 5 });
  mono("the gap", M + 18, y + 16, { color: C.gold, size: 7 });
  doc.font("body").fontSize(9.2).fillColor(C.paper)
    .text(p.clock, M + 96, y + 14, { width: W - 116, lineGap: 2.6 });
})();

// ---------- 02 architecture ----------
(function product() {
  const p = content.product;
  let y = sectionHeader("02", "the architecture", p.title, p.lede);

  // dashed label over the flow
  doc.save().rect(M + 60, y, W - 120, 26).lineWidth(1).dash(3, { space: 2 }).strokeColor(C.ink).stroke().undash().restore();
  doc.font("body").fontSize(7.4).fillColor(C.ink)
    .text(p.flow.lab, M + 70, y + 8, { width: W - 140, align: "center" });
  y += 40;

  // flow nodes
  const nodeW = (W - 110) / 3, nodeH = 62, arrowW = 55;
  p.flow.nodes.forEach((n, i) => {
    const x = M + i * (nodeW + arrowW);
    shadowBox(x, y, nodeW, nodeH, n.dark ? { fill: C.ink, shadow: C.coral } : {});
    doc.font("displayMedium").fontSize(11).fillColor(n.dark ? C.paper : C.ink)
      .text(n.name, x + 8, y + 10, { width: nodeW - 16, align: "center" });
    doc.font("body").fontSize(6.6).fillColor(n.dark ? "#cdc6d6" : C.muted)
      .text(n.sub, x + 8, y + 26, { width: nodeW - 16, align: "center", lineGap: 1 });
    if (i < 2) {
      doc.font("monoMedium").fontSize(8).fillColor(C.coral)
        .text(i === 0 ? "-percept->" : "-action->", x + nodeW + 2, y + nodeH / 2 - 5, { width: arrowW + 4, align: "center" });
    }
  });
  y += nodeH + 26;

  // feature grid 3x3 — the nine subsystems
  const gap = 1, cw = (W - 2) / 3, ch = 97;
  doc.save().rect(M - 0.5, y - 0.5, W + 1, ch * 3 + 3).lineWidth(1).strokeColor(C.ink).stroke().restore();
  p.features.forEach((f, i) => {
    const col = i % 3, row = Math.floor(i / 3);
    const x = M + col * (cw + gap), fy = y + row * (ch + gap);
    doc.rect(x, fy, cw, ch).fill(C.card);
    mono(f.k, x + 12, fy + 10, { color: C.blue, size: 5.8 });
    doc.font("displayMedium").fontSize(10.5).fillColor(C.ink).text(f.h, x + 12, fy + 22, { width: cw - 24 });
    doc.font("body").fontSize(7.2).fillColor("#4d4956").text(f.p, x + 12, doc.y + 3, { width: cw - 24, lineGap: 1.6 });
  });
})();

// ---------- 02b the code ----------
(function contract() {
  let y = sectionHeader("02b", "the seam", "The mind is a value you can replay.",
    "A whole Daimon — beliefs, drives, memory, social models — is a plain serialisable value. Same seed, same life, byte for byte. The one expensive part, System 2, is a single trait a language model implements.");

  const lines = content.product.yaml;
  const lh = 15.5, pad = 22;
  const boxH = lines.length * lh + pad * 2;
  shadowBox(M, y, W, boxH, { fill: C.codebg, offset: 6 });

  let ty = y + pad;
  const colorOf = t => t === "cm" ? C.codecomment : t === "k" ? C.codekey : t === "s" ? C.codestr : t === "n" ? C.codenum : C.codetext;
  doc.font("mono").fontSize(8.4);
  for (const segments of lines) {
    let tx = M + pad;
    for (const [type, text] of segments) {
      doc.fillColor(colorOf(type)).text(text, tx, ty, { lineBreak: false });
      tx += doc.widthOfString(text);
    }
    ty += lh;
  }

  y += boxH + 30;
  const notes = [
    ["DETERMINISTIC", "Every choice flows from one seeded PRNG. Emergent behaviour becomes testable, not merely anecdotal."],
    ["PLUGGABLE", "The offline HeuristicDeliberator ships so the repo runs with zero network; an LLM implements the same trait."],
    ["RATE-LIMITED", "The escalation policy reserves System 2 for ~10% of ticks — a few model calls per agent per minute."],
  ];
  const nw = (W - 24) / 3;
  notes.forEach((n, i) => {
    const x = M + i * (nw + 12);
    mono(n[0], x, y, { color: C.coral, size: 6 });
    doc.font("body").fontSize(7.8).fillColor("#4d4956").text(n[1], x, y + 13, { width: nw, lineGap: 1.8 });
  });
})();

// ---------- 03 results ----------
(function benchmarks() {
  const b = content.benchmarks;
  let y = sectionHeader("03", "the proof", b.title, b.lede);

  doc.font("body").fontSize(7.4).fillColor(C.muted).text(b.method, M, y, { width: W, lineGap: 1.8 });
  y = doc.y + 16;

  // --- conversations hbar chart ---
  const ovH = 214;
  shadowBox(M, y, W, ovH, { offset: 4 });
  doc.font("displayMedium").fontSize(12.5).fillColor(C.ink).text(b.overhead.title, M + 18, y + 14, { width: W - 36 });
  doc.font("body").fontSize(7.4).fillColor(C.muted).text(b.overhead.sub, M + 18, doc.y + 2, { width: W - 36 });
  let by = doc.y + 12;
  const trackW = W - 36 - 70;
  b.overhead.bars.forEach(bar => {
    doc.font(bar.blue ? "bodySemi" : "body").fontSize(7.6).fillColor(C.ink).text(bar.label, M + 18, by, { lineBreak: false });
    doc.font("mono").fontSize(7.2).fillColor(C.ink).text(bar.val, M + 18 + trackW + 8, by, { lineBreak: false });
    by += 11;
    doc.rect(M + 18, by, trackW, 11).fillAndStroke(C.track, C.line);
    doc.rect(M + 18, by, Math.max(trackW * bar.w, 2.5), 11).fill(bar.blue ? C.blue : C.barmute);
    by += 17;
  });
  doc.font("body").fontSize(7.2).fillColor(C.muted).text(b.overhead.note, M + 18, by + 2, { width: W - 36, lineGap: 1.6 });
  y += ovH + 20;

  // --- two vbar charts side by side ---
  const half = (W - 16) / 2, vbH = 196;
  const drawVbars = (x, spec) => {
    shadowBox(x, y, half, vbH, { offset: 4 });
    doc.font("displayMedium").fontSize(11.5).fillColor(C.ink).text(spec.title, x + 16, y + 13, { width: half - 32 });
    doc.font("body").fontSize(6.8).fillColor(C.muted).text(spec.sub, x + 16, doc.y + 2, { width: half - 32, lineGap: 1.2 });
    const chartTop = y + 64, chartBot = y + vbH - 38;
    const maxV = Math.max(...spec.bars.map(s => s.v));
    const bwid = (half - 32 - (spec.bars.length - 1) * 14) / spec.bars.length;
    spec.bars.forEach((s, i) => {
      const bx = x + 16 + i * (bwid + 14);
      const h = Math.max(((chartBot - chartTop) * s.v) / maxV, 3);
      doc.font("mono").fontSize(6.8).fillColor(C.ink).text(String(s.v), bx, chartBot - h - 10, { width: bwid, align: "center" });
      doc.rect(bx, chartBot - h, bwid, h).fillAndStroke(s.coral ? C.coral : C.blue, C.ink);
      mono(s.l, bx, chartBot + 5, { width: bwid, align: "center", size: 5.4 });
    });
    doc.font("body").fontSize(6.4).fillColor(C.muted).text(spec.note, x + 16, y + vbH - 20, { width: half - 32 });
  };
  drawVbars(M, b.percentiles);
  drawVbars(M + half + 16, b.optimisation);
  y += vbH + 20;

  // --- big numbers strip ---
  const bnW = (W - 24) / 3, bnH = 104;
  b.bignums.forEach((n, i) => {
    const x = M + i * (bnW + 12);
    shadowBox(x, y, bnW, bnH, { fill: C.ink, shadow: C.coral, offset: 4 });
    doc.font("displayMedium").fontSize(20).fillColor(C.gold).text(n.n, x + 14, y + 14);
    doc.font("body").fontSize(7).fillColor("#d9d3e0").text(n.p, x + 14, doc.y + 4, { width: bnW - 28, lineGap: 1.6 });
  });
})();

// ---------- 03b the economics ----------
(function scaling() {
  const s = content.benchmarks.scaling;
  let y = sectionHeader("03b", "the economics", s.title, s.sub);

  const chH = 224;
  shadowBox(M, y, W, chH, { offset: 5 });
  doc.font("displayMedium").fontSize(12.5).fillColor(C.ink)
    .text("LLM calls per minute vs. the fleet you can run", M + 20, y + 16, { width: W - 40 });
  doc.font("body").fontSize(7.2).fillColor(C.muted)
    .text("Lower is better. The bar is cost; the number on the right is how many NPCs you can run.", M + 20, doc.y + 2, { width: W - 40 });

  const rows = [
    { label: "LLM every frame (60 fps)", calls: "~3,600", w: 1.0, fleet: "≈ 1 agent", coral: true },
    { label: "LLM every second", calls: "~60", w: 0.46, fleet: "tens" },
    { label: "Daimon — per interesting moment", calls: "~3–10", w: 0.1, fleet: "hundreds–thousands", blue: true },
  ];
  let ry = doc.y + 18;
  const labelW = 196, trackW = W - 40 - labelW - 132;
  rows.forEach(r => {
    doc.font(r.blue ? "bodySemi" : "body").fontSize(8.4).fillColor(C.ink)
      .text(r.label, M + 20, ry + 3, { width: labelW - 8, lineBreak: false });
    const tx = M + 20 + labelW;
    doc.rect(tx, ry, trackW, 16).fillAndStroke(C.track, C.line);
    doc.rect(tx, ry, Math.max(trackW * r.w, 3), 16).fill(r.blue ? C.blue : r.coral ? C.coral : C.barmute);
    doc.font("mono").fontSize(7.4).fillColor(C.ink).text(r.calls, tx + 6, ry + 4, { lineBreak: false });
    doc.font(r.blue ? "displayMedium" : "displayMedium").fontSize(11).fillColor(r.blue ? C.blue : C.ink)
      .text(r.fleet, tx + trackW + 10, ry + 1, { width: 122, lineBreak: false });
    ry += 40;
  });
  doc.font("body").fontSize(6.8).fillColor(C.muted)
    .text("Order-of-magnitude estimates; the point is the ratio, not the digit.", M + 20, y + chH - 22, { width: W - 40 });
  y += chH + 22;

  doc.font("body").fontSize(9.2).fillColor("#4d4956").text(s.note, M, y, { width: W, lineGap: 2.5 });
  y = doc.y + 22;

  // the two systems
  const pw = (W - 16) / 2, ph = 172;
  const paths = [
    ["SYSTEM 1 — ALWAYS ON (THE ~90%)", "Reflexes and utility arbitration run every tick on local compute: perception, drives, locomotion, routine foraging, reflexive flight, holding a commitment. No model call, no network, no added latency. This is the floor the whole fleet stands on — and it is effectively free."],
    ["SYSTEM 2 — ONLY WHEN IT MATTERS", "An explicit escalation policy invokes the model only on surprise, high stakes, or genuine ambiguity, then debounces with a cooldown. ReAct / Reflexion / Tree-of-Thoughts run here. At ~10% of ticks that is a handful of calls per agent per minute — what turns a thousand reasoning NPCs from a fantasy into a budget line."],
  ];
  paths.forEach((p, i) => {
    const x = M + i * (pw + 16);
    shadowBox(x, y, pw, ph, { offset: 4 });
    mono(p[0], x + 16, y + 16, { color: i === 0 ? C.blue : C.coral, size: 6 });
    doc.font("body").fontSize(8).fillColor("#4d4956").text(p[1], x + 16, y + 34, { width: pw - 32, lineGap: 2.2 });
  });
})();

// ---------- 04 status ----------
(function security() {
  const s = content.security;
  let y = sectionHeader("04", "the status", s.title, s.lede);

  const rowH = 26, col1 = 168, col3 = 78;
  const col2 = W - col1 - col3;
  doc.save().rect(M, y, W, 20).fill(C.card).restore();
  doc.save().rect(M, y, W, rowH * s.rows.length + 20).lineWidth(1.2).strokeColor(C.ink).stroke().restore();
  mono("capability", M + 10, y + 7, { size: 5.6 });
  mono("what daimon does", M + col1 + 10, y + 7, { size: 5.6 });
  mono("status", M + col1 + col2 + 10, y + 7, { size: 5.6 });
  rule(y + 20, C.ink, M, M + W, 1);
  let ry = y + 20;
  s.rows.forEach(([req, how, status], i) => {
    if (i > 0) rule(ry, C.line);
    doc.font("bodySemi").fontSize(7.6).fillColor(C.ink).text(req, M + 10, ry + 7, { width: col1 - 16 });
    doc.font("body").fontSize(7.4).fillColor("#4d4956").text(how, M + col1 + 10, ry + 7, { width: col2 - 16 });
    const pillCol = status === "ok" ? C.green : "#b78a1e";
    const label = status === "ok" ? "SHIPPED" : "ROADMAP";
    doc.save().rect(M + col1 + col2 + 10, ry + 7, 54, 11).lineWidth(0.8).strokeColor(pillCol).stroke().restore();
    doc.font("mono").fontSize(5.6).fillColor(pillCol).text(label, M + col1 + col2 + 10, ry + 10, { width: 54, align: "center", characterSpacing: 1 });
    ry += rowH;
  });
  y = ry + 16;
  doc.font("body").fontSize(7.6).fillColor(C.muted).text(s.note, M, y, { width: W, lineGap: 2 });
})();

// ---------- 05 lineage ----------
(function voices() {
  const v = content.voices;
  let y = sectionHeader("05", "the lineage", v.title, null);

  const gap = 12, qw = W, qh = 100;
  v.quotes.forEach(q => {
    shadowBox(M, y, qw, qh, { offset: 4 });
    doc.font("displaySemi").fontSize(30).fillColor(C.coral).text("“", M + 18, y + 4, { lineBreak: false });
    doc.font("displayItalic").fontSize(10.5).fillColor(C.ink)
      .text(q.q, M + 52, y + 16, { width: qw - 250, lineGap: 2.4 });
    doc.font("bodySemi").fontSize(8.4).fillColor(C.ink).text(q.who, M + qw - 180, y + 22, { width: 160 });
    doc.font("body").fontSize(7.4).fillColor(C.muted).text(q.org, M + qw - 180, doc.y + 2, { width: 160 });
    y += qh + gap;
  });
  doc.font("mono").fontSize(6.2).fillColor(C.muted)
    .text("NB — " + v.disclaimer, M, y + 2, { width: W, characterSpacing: 0.4 });
})();

// ---------- 06 crates ----------
(function tooling() {
  const t = content.tooling;
  let y = sectionHeader("06", "the build", t.title, t.lede);

  const rowH = 128, gap = 16;
  t.cards.forEach(c => {
    shadowBox(M, y, W, rowH, { offset: 4 });
    const colName = M + 18, colCode = M + 132, codeW = 218, colText = colCode + codeW + 18;

    doc.font("displayMedium").fontSize(15).fillColor(C.ink).text(c.h, colName, y + 18, { width: 112, lineBreak: false });
    doc.font("mono").fontSize(5.6);
    const pillW = doc.widthOfString(c.pill) + c.pill.length * 0.8 + 14;
    doc.save().rect(colName, y + 42, pillW, 13).lineWidth(0.8).strokeColor(C.green).stroke().restore();
    doc.fillColor(C.green).text(c.pill, colName + 7, y + 46, { characterSpacing: 0.8, lineBreak: false });

    const codeLines = c.code.split("\n");
    const codeH = rowH - 28;
    doc.save();
    doc.rect(colCode, y + 14, codeW, codeH).fill(C.codebg);
    doc.rect(colCode, y + 14, codeW, codeH).clip();
    codeLines.forEach((l, li) => {
      doc.font("mono").fontSize(6.6).fillColor(C.codetext)
        .text(l, colCode + 10, y + 24 + li * 13, { lineBreak: false });
    });
    doc.restore();

    doc.font("body").fontSize(7.8).fillColor("#4d4956")
      .text(c.p, colText, y + 18, { width: M + W - colText - 16, lineGap: 2 });
    y += rowH + gap;
  });
})();

// ---------- 07 next ----------
(function engage() {
  const e = content.engage;
  let y = sectionHeader("07", "where this goes", e.title, e.lede);

  const gap = 14, cw = (W - gap * 2) / 3, ch = 318;
  e.tiers.forEach((t, i) => {
    const x = M + i * (cw + gap);
    const dark = !!t.hot;
    shadowBox(x, y, cw, ch, dark ? { fill: C.ink, shadow: C.coral, offset: 5 } : { offset: 4 });
    mono(t.kicker, x + 16, y + 16, { color: dark ? C.gold : C.muted, size: 5.8 });
    doc.font("displayMedium").fontSize(18).fillColor(dark ? C.paper : C.ink)
      .text(t.name, x + 16, y + 30, { width: cw - 32, lineGap: 1 });
    let iy = Math.max(y + 62, doc.y + 10);
    t.items.forEach(item => {
      doc.font("body").fontSize(7.6).fillColor(dark ? "#d9d3e0" : "#4d4956")
        .text(item, x + 16, iy, { width: cw - 32, lineGap: 1.6 });
      iy = doc.y + 6;
      doc.save().moveTo(x + 16, iy - 1).lineTo(x + cw - 16, iy - 1).lineWidth(0.5)
        .dash(2, { space: 2 }).strokeColor(dark ? "#3a3640" : C.line).stroke().undash().restore();
      iy += 6;
    });
    // CTA box with coral offset shadow
    const ctaY = y + ch - 40;
    doc.save().rect(x + 19, ctaY + 3, cw - 32, 24).fill(C.coral).restore();
    doc.save().rect(x + 16, ctaY, cw - 32, 24).fillAndStroke(C.ink, dark ? C.paper : C.ink).restore();
    doc.font("monoMedium").fontSize(6.4).fillColor(C.paper)
      .text(i === 1 ? "SEE ROADMAP.md" : i === 0 ? "READ THE WHITEPAPER" : "GOVERN WITH REINS",
        x + 16, ctaY + 9, { width: cw - 32, align: "center", characterSpacing: 1.2 });
  });

  // closing
  const cy = y + ch + 28;
  doc.font("display").fontSize(15).fillColor(C.ink)
    .text("Ready to meet a mind?", M, cy, { width: W, align: "center" });
  doc.font("mono").fontSize(8).fillColor(C.blue)
    .text(brand.contact, M, doc.y + 6, { width: W, align: "center", characterSpacing: 1.5 });
})();

// ---------- pass 2: TOC + footers ----------
doc.switchToPage(tocPageIndex);
let ty = M + 80;
tocEntries.forEach(e => {
  const short = e.title.includes(". ") ? e.title.split(". ")[0] + "." : e.title;
  doc.font("displayMedium").fontSize(13).fillColor(C.ink).text(e.idx, M, ty, { lineBreak: false });
  doc.font("display").fontSize(13).fillColor(C.ink).text(short, M + 52, ty, { lineBreak: false });
  const titleEnd = M + 52 + doc.widthOfString(short) + 10;
  const pageX = PAGE_W - M - 24;
  doc.save();
  for (let dx = titleEnd; dx < pageX - 8; dx += 7) doc.circle(dx, ty + 9, 0.7).fill(C.line);
  doc.restore();
  doc.font("mono").fontSize(9).fillColor(C.muted).text(String(e.page), pageX, ty + 2, { width: 24, align: "right", lineBreak: false });
  ty += 30;
});
mono("Every section is bookmarked in your PDF viewer's outline.", M, ty + 18);

// footers (skip cover)
const range = doc.bufferedPageRange();
for (let i = 1; i < range.count; i++) {
  doc.switchToPage(i);
  rule(PAGE_H - 40, C.line);
  doc.font("mono").fontSize(5.6).fillColor(C.muted)
    .text(`${brand.name} — ${brand.subtitle} · ${brand.company}`.toUpperCase(), M, PAGE_H - 32,
      { characterSpacing: 1.2, lineBreak: false });
  const right = `v${brand.version} · ${i + 1} / ${range.count}`.toUpperCase();
  const rw = doc.widthOfString(right) + (right.length - 1) * 1.2;
  doc.text(right, PAGE_W - M - rw, PAGE_H - 32, { characterSpacing: 1.2, lineBreak: false });
}

doc.end();
console.log("wrote", out);
