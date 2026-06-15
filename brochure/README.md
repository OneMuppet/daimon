# Daimon brochure

A native **pdfkit** generator (no HTML middle-step) that renders the Daimon
white paper as a branded A4 PDF — cover, clickable TOC, outline bookmarks,
per-page footers, and vector charts. Same visual system as the rest of the
`reins`/`daimon` family.

- `brand.json` — palette, fonts, metadata (edit branding here)
- `content.json` — all copy and chart data (edit text here)
- `generate.js` — pure layout (rarely needs editing)
- `fonts/` — Fraunces (display), Inter (body), IBM Plex Mono (code)

## Build

```bash
npm install        # once (pdfkit)
npm run build      # -> Daimon-brochure-v<version>.pdf
# or: node generate.js
```

The output is `Daimon-brochure-v0.1.0.pdf` in this directory.
