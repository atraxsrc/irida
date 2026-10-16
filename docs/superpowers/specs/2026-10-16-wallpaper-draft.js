/* ---- wallpaper draft: palette first, the wallpaper only seeds hues ----
   Pixels -> OKLab -> k-means (k = 8, seeded, so the same image gives the same
   draft). Lightness and chroma come from Ember (dark) and Ember Day (light);
   the wallpaper decides the neutral cast, the accent and how far each
   semantic hue leans toward a nearby wallpaper colour. Every role is then
   moved in lightness until it meets irida's contrast minimum on both the
   window and the pane. */
const WP = (() => {
  const toLin = c => (c <= 0.04045 ? c / 12.92 : Math.pow((c + 0.055) / 1.055, 2.4));
  const toSrgb = c => (c <= 0.0031308 ? 12.92 * c : 1.055 * Math.pow(c, 1 / 2.4) - 0.055);
  function oklab(R, G, B) {
    const r = toLin(R / 255), g = toLin(G / 255), b = toLin(B / 255);
    const l = Math.cbrt(0.4122214708 * r + 0.5363325363 * g + 0.0514459929 * b);
    const m = Math.cbrt(0.2119034982 * r + 0.6806995451 * g + 0.1073969566 * b);
    const s = Math.cbrt(0.0883024619 * r + 0.2817188376 * g + 0.6299787005 * b);
    return [0.2104542553 * l + 0.7936177850 * m - 0.0040720468 * s,
            1.9779984951 * l - 2.4285922050 * m + 0.4505937099 * s,
            0.0259040371 * l + 0.7827717662 * m - 0.8086757660 * s];
  }
  function linRgb(L, a, b) {
    const l = (L + 0.3963377774 * a + 0.2158037573 * b) ** 3;
    const m = (L - 0.1055613458 * a - 0.0638541728 * b) ** 3;
    const s = (L - 0.0894841775 * a - 1.2914855480 * b) ** 3;
    return [4.0767416621 * l - 3.3077115913 * m + 0.2309699292 * s,
            -1.2684380046 * l + 2.6097574011 * m - 0.3413193965 * s,
            -0.0041960863 * l - 0.7034186147 * m + 1.7076147010 * s];
  }
  const inGamut = c => c.every(v => v >= -1e-4 && v <= 1 + 1e-4);
  /** OKLCH to #RRGGBB, lowering chroma until the colour fits sRGB. */
  function hexOf(L, C, h) {
    const rad = h * Math.PI / 180;
    let lo = 0, hi = C, best = linRgb(L, 0, 0);
    if (inGamut(linRgb(L, C * Math.cos(rad), C * Math.sin(rad)))) best = linRgb(L, C * Math.cos(rad), C * Math.sin(rad));
    else for (let i = 0; i < 18; i++) {
      const mid = (lo + hi) / 2, c = linRgb(L, mid * Math.cos(rad), mid * Math.sin(rad));
      if (inGamut(c)) { lo = mid; best = c; } else hi = mid;
    }
    return "#" + best.map(v => Math.round(Math.min(1, Math.max(0, toSrgb(Math.min(1, Math.max(0, v))))) * 255)
      .toString(16).padStart(2, "0").toUpperCase()).join("");
  }
  const lchOf = ([L, a, b]) => [L, Math.hypot(a, b), (Math.atan2(b, a) * 180 / Math.PI + 360) % 360];
  const hueDist = (x, y) => { const d = Math.abs(x - y) % 360; return d > 180 ? 360 - d : d; };
  const lerpHue = (from, to, t) => { let d = ((to - from + 540) % 360) - 180; return (from + d * t + 360) % 360; };

  /** k-means in OKLab over RGBA bytes; returns clusters sorted by weight. */
  function clusters(rgba, k = 8) {
    const pts = [];
    for (let i = 0; i < rgba.length; i += 4) if (rgba[i + 3] >= 128) pts.push(oklab(rgba[i], rgba[i + 1], rgba[i + 2]));
    if (!pts.length) return [];
    let seed = 0x9e3779b9;
    const rand = () => { seed = (Math.imul(seed, 1664525) + 1013904223) >>> 0; return seed / 4294967296; };
    const d2 = (p, q) => (p[0] - q[0]) ** 2 + (p[1] - q[1]) ** 2 + (p[2] - q[2]) ** 2;
    const cents = [pts[Math.floor(rand() * pts.length)].slice()];
    while (cents.length < Math.min(k, pts.length)) {
      const w = pts.map(p => Math.min(...cents.map(c => d2(p, c))));
      const total = w.reduce((s, v) => s + v, 0);
      if (total === 0) break;
      let t = rand() * total, i = 0;
      while (t > w[i] && i < w.length - 1) { t -= w[i]; i++; }
      cents.push(pts[i].slice());
    }
    const assign = new Array(pts.length).fill(0);
    for (let it = 0; it < 14; it++) {
      for (let i = 0; i < pts.length; i++) {
        let best = 0, bd = Infinity;
        for (let c = 0; c < cents.length; c++) { const d = d2(pts[i], cents[c]); if (d < bd) { bd = d; best = c; } }
        assign[i] = best;
      }
      const sum = cents.map(() => [0, 0, 0, 0]);
      for (let i = 0; i < pts.length; i++) { const s = sum[assign[i]]; s[0] += pts[i][0]; s[1] += pts[i][1]; s[2] += pts[i][2]; s[3]++; }
      sum.forEach((s, c) => { if (s[3]) cents[c] = [s[0] / s[3], s[1] / s[3], s[2] / s[3]]; });
    }
    const count = cents.map(() => 0);
    assign.forEach(c => count[c]++);
    return cents.map((c, i) => { const [L, C, h] = lchOf(c); return { L, C, h, a: c[1], b: c[2], weight: count[i] / pts.length, hex: hexOf(L, C, h) }; })
      .filter(c => c.weight > 0).sort((x, y) => y.weight - x.weight);
  }

  const STYLE = {
    dark:  { background: .205, pane: .235, container: .268, text: .885, muted: .606, comment: .49, accent: .68, sem: .70 },
    light: { background: .925, pane: .886, container: .902, text: .302, muted: .541, comment: .653, accent: .51, sem: .49 },
  };
  const SEM = { error: [23, .155], warning: [78, .125], success: [141, .075], cyan: [190, .062], blue: [250, .075], magenta: [310, .095] };

  /** Draft the 13 roles from clusters for a mode, every role meeting its minimum. */
  function draft(cl, mode, ratio) {
    const st = STYLE[mode], dark = mode === "dark";
    // neutral cast: the weighted average of the wallpaper, kept faint
    const a = cl.reduce((s, c) => s + c.a * c.weight, 0), b = cl.reduce((s, c) => s + c.b * c.weight, 0);
    const castC = Math.min(0.018, Math.hypot(a, b)), castH = (Math.atan2(b, a) * 180 / Math.PI + 360) % 360;
    const roles = {};
    for (const r of ["background", "pane", "container"]) roles[r] = hexOf(st[r], castC * (r === "background" ? .8 : 1), castH);
    const fit = (L, C, h, min) => {
      // move away from the surfaces until both ratios reach min (plus a hair, as check rounds down)
      let hex = hexOf(L, C, h);
      for (let i = 0; i < 120 && min && Math.min(ratio(hex, roles.background), ratio(hex, roles.pane)) < min + 0.02; i++) {
        L = dark ? Math.min(0.99, L + 0.005) : Math.max(0.02, L - 0.005);
        hex = hexOf(L, C, h);
      }
      return hex;
    };
    roles.text = fit(st.text, castC * .8, castH, 4.5);
    roles.muted = fit(st.muted, castC * 1.4, castH, 3.0);
    roles.comment = hexOf(st.comment, castC * 1.6, castH);
    // accent: the most vivid colour that covers some of the wallpaper
    const vivid = cl.filter(c => c.C > 0.045).sort((x, y) => y.C * Math.sqrt(y.weight) - x.C * Math.sqrt(x.weight))[0];
    const acc = vivid ? { h: vivid.h, C: Math.min(0.19, Math.max(0.11, vivid.C)) } : { h: 44, C: 0.15 };
    roles.accent = fit(st.accent, acc.C, acc.h, 4.5);
    for (const [r, [h0, c0]] of Object.entries(SEM)) {
      const near = cl.filter(c => c.C > 0.04 && hueDist(c.h, h0) < 35).sort((x, y) => y.weight - x.weight)[0];
      const h = near ? lerpHue(h0, near.h, 0.5) : h0;
      const C = near ? Math.min(c0 * 1.3, Math.max(c0 * 0.8, near.C)) : c0;
      roles[r] = fit(st.sem, C, h, 4.5);
    }
    return { roles, accentFrom: vivid ? vivid.hex : null };
  }
  /** "dark" when the wallpaper is mostly dark. */
  const modeOf = cl => cl.reduce((s, c) => s + c.L * c.weight, 0) < 0.55 ? "dark" : "light";
  return { clusters, draft, modeOf };
})();
