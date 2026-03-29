// aitop Scriptable iOS Widget
// Displays AI spend stats from aitop --serve
//
// Setup:
// 1. Run `aitop --serve` on your machine
// 2. Set SERVER_URL below to your machine's IP/hostname
// 3. Add this script in the Scriptable app
// 4. Add a Scriptable widget to your home screen and select this script

const SERVER_URL = "http://100.88.137.69:8080";
const API_ENDPOINT = `${SERVER_URL}/api/stats`;
const CACHE_KEY = "aitop_last_stats";

// Theme — matches aitop menu bar / TUI
const C = {
  bg:       new Color("#1a1a2e"),
  card:     new Color("#242440"),
  accent:   new Color("#ff9933"),
  white:    new Color("#ffffff"),
  dim:      new Color("#888899"),
  faint:    new Color("#555566"),
  live:     new Color("#44dd55"),
  idle:     new Color("#666677"),
  barFill:  new Color("#ff9933", 0.5),
  divider:  new Color("#ffffff", 0.06),
};

const F = {
  hero:       Font.boldMonospacedSystemFont(30),
  cardVal:    Font.boldMonospacedSystemFont(14),
  cardLabel:  Font.systemFont(10),
  sectionVal: Font.boldMonospacedSystemFont(15),
  label:      Font.systemFont(11),
  title:      Font.boldSystemFont(13),
  tiny:       Font.systemFont(10),
  tinyBold:   Font.boldSystemFont(10),
  tiniest:    Font.systemFont(9),
  modelName:  Font.regularMonospacedSystemFont(11),
  modelCost:  Font.boldMonospacedSystemFont(11),
  sessionProj:Font.mediumSystemFont(11),
  sessionDim: Font.systemFont(10),
  sessionCost:Font.boldMonospacedSystemFont(11),
};

// --- Data ---

async function fetchStats() {
  try {
    const req = new Request(API_ENDPOINT);
    req.timeoutInterval = 5;
    const data = await req.loadJSON();
    Keychain.set(CACHE_KEY, JSON.stringify(data));
    return data;
  } catch (e) {
    if (Keychain.contains(CACHE_KEY)) {
      return JSON.parse(Keychain.get(CACHE_KEY));
    }
    return null;
  }
}

// --- Formatters ---

function fmt(v) {
  if (v >= 1000) return `$${Math.round(v)}`;
  if (v >= 100)  return `$${v.toFixed(1)}`;
  return `$${v.toFixed(2)}`;
}

function fmtCompact(v) {
  if (v >= 1000) return `$${(v/1000).toFixed(1)}k`;
  if (v >= 100)  return `$${Math.round(v)}`;
  if (v >= 10)   return `$${v.toFixed(1)}`;
  return `$${v.toFixed(2)}`;
}

function relativeTime(isoStr) {
  const d = new Date(isoStr);
  const now = new Date();
  const mins = Math.floor((now - d) / 60000);
  if (mins < 1)  return "now";
  if (mins < 60) return `${mins}m`;
  const hrs = Math.floor(mins / 60);
  if (hrs < 24)  return `${hrs}h`;
  return `${Math.floor(hrs / 24)}d`;
}

// --- Shared UI helpers ---

function addDivider(parent) {
  const d = parent.addStack();
  d.addSpacer();
  const line = d.addStack();
  line.backgroundColor = C.divider;
  line.size = new Size(0, 1);
  line.addSpacer();
  d.addSpacer();
}

function addHeader(parent, data) {
  const h = parent.addStack();
  h.layoutHorizontally();
  h.centerAlignContent();

  const t = h.addText("aitop");
  t.font = F.title;
  t.textColor = C.dim;

  h.addSpacer();

  const dotColor = data.is_live ? C.live : C.idle;
  const label = data.is_live ? "LIVE" : "IDLE";
  const s = h.addText(`● ${label}`);
  s.font = F.tinyBold;
  s.textColor = dotColor;
}

function addHero(parent, data) {
  const row = parent.addStack();
  row.layoutHorizontally();
  row.bottomAlignContent();

  const spend = row.addText(fmt(data.spend_today));
  spend.font = F.hero;
  spend.textColor = C.accent;
  spend.minimumScaleFactor = 0.5;

  row.addSpacer(6);

  const lbl = row.addText("today");
  lbl.font = F.label;
  lbl.textColor = C.dim;
}

function addSpendCard(parent, value, label) {
  const card = parent.addStack();
  card.layoutVertically();
  card.centerAlignContent();
  card.backgroundColor = C.card;
  card.cornerRadius = 8;
  card.setPadding(8, 10, 8, 10);
  card.size = new Size(0, 0);

  const v = card.addText(value);
  v.font = F.cardVal;
  v.textColor = C.white;
  v.lineLimit = 1;
  v.minimumScaleFactor = 0.7;

  card.addSpacer(2);

  const l = card.addText(label);
  l.font = F.cardLabel;
  l.textColor = C.dim;
}

function addSpendGrid(parent, data) {
  const row = parent.addStack();
  row.layoutHorizontally();
  row.spacing = 8;

  addSpendCard(row, fmtCompact(data.burn_rate), "/hr");
  addSpendCard(row, fmtCompact(data.spend_this_week), "/wk");
  addSpendCard(row, fmtCompact(data.spend_all_time), "total");
}

function addModelBar(parent, model, maxPct, availWidth) {
  const row = parent.addStack();
  row.layoutHorizontally();
  row.centerAlignContent();
  row.spacing = 6;

  // Proportional bar
  const barW = Math.max(4, (model.percentage / Math.max(maxPct, 1)) * (availWidth * 0.25));
  const bar = row.addStack();
  bar.backgroundColor = C.barFill;
  bar.cornerRadius = 2;
  bar.size = new Size(barW, 12);

  // Model name
  const name = row.addText(model.model);
  name.font = F.modelName;
  name.textColor = C.dim;
  name.lineLimit = 1;

  row.addSpacer();

  // Cost
  const cost = row.addText(fmtCompact(model.cost));
  cost.font = F.modelCost;
  cost.textColor = C.white;

  row.addSpacer(4);

  // Percentage
  const pct = row.addText(`${Math.round(model.percentage)}%`);
  pct.font = F.tiny;
  pct.textColor = C.faint;
}

function addSessionRow(parent, session) {
  const row = parent.addStack();
  row.layoutHorizontally();
  row.centerAlignContent();

  const proj = row.addText(session.project);
  proj.font = F.sessionProj;
  proj.textColor = C.white;
  proj.lineLimit = 1;

  row.addSpacer(4);

  const model = row.addText(session.model);
  model.font = F.sessionDim;
  model.textColor = C.faint;
  model.lineLimit = 1;

  row.addSpacer();

  const cost = row.addText(fmtCompact(session.cost));
  cost.font = F.sessionCost;
  cost.textColor = C.white;

  row.addSpacer(4);

  const time = row.addText(relativeTime(session.updated_at));
  time.font = F.tiniest;
  time.textColor = C.faint;
}

function addSectionLabel(parent, text) {
  const l = parent.addText(text);
  l.font = F.tinyBold;
  l.textColor = C.faint;
}

// --- Small Widget ---

function buildSmall(data) {
  const w = new ListWidget();
  w.backgroundColor = C.bg;
  w.setPadding(14, 14, 14, 14);

  addHeader(w, data);
  w.addSpacer(8);
  addHero(w, data);
  w.addSpacer(6);

  const rate = w.addText(`${fmtCompact(data.burn_rate)}/hr`);
  rate.font = F.sectionVal;
  rate.textColor = C.white;

  w.addSpacer(4);

  const row = w.addStack();
  row.layoutHorizontally();
  row.centerAlignContent();

  const wk = row.addText(`${fmtCompact(data.spend_this_week)}/wk`);
  wk.font = F.label;
  wk.textColor = C.dim;

  row.addSpacer();

  const cache = row.addText(`${Math.round(data.cache_hit_ratio)}% cache`);
  cache.font = F.tiny;
  cache.textColor = C.faint;

  w.addSpacer();
  return w;
}

// --- Medium Widget ---

function buildMedium(data) {
  const w = new ListWidget();
  w.backgroundColor = C.bg;
  w.setPadding(12, 16, 12, 16);

  addHeader(w, data);
  w.addSpacer(8);

  // Two-column layout
  const body = w.addStack();
  body.layoutHorizontally();
  body.spacing = 14;

  // Left: hero + cache
  const left = body.addStack();
  left.layoutVertically();

  const spend = left.addText(fmt(data.spend_today));
  spend.font = F.hero;
  spend.textColor = C.accent;
  spend.minimumScaleFactor = 0.5;

  const todayLbl = left.addText("today");
  todayLbl.font = F.label;
  todayLbl.textColor = C.dim;

  left.addSpacer(6);

  const cacheRow = left.addStack();
  cacheRow.layoutHorizontally();
  cacheRow.centerAlignContent();
  const cacheVal = cacheRow.addText(`${Math.round(data.cache_hit_ratio)}%`);
  cacheVal.font = F.cardVal;
  cacheVal.textColor = C.white;
  cacheRow.addSpacer(3);
  const cacheLbl = cacheRow.addText("cache");
  cacheLbl.font = F.tiny;
  cacheLbl.textColor = C.faint;

  left.addSpacer();

  // Right: spend cards + top models
  const right = body.addStack();
  right.layoutVertically();
  right.spacing = 6;

  const cards = right.addStack();
  cards.layoutHorizontally();
  cards.spacing = 6;

  addSpendCard(cards, fmtCompact(data.burn_rate), "/hr");
  addSpendCard(cards, fmtCompact(data.spend_this_week), "/wk");
  addSpendCard(cards, fmtCompact(data.spend_all_time), "total");

  right.addSpacer(2);

  if (data.top_models && data.top_models.length > 0) {
    addSectionLabel(right, "MODELS");
    const maxPct = data.top_models[0].percentage;
    const count = Math.min(data.top_models.length, 2);
    for (let i = 0; i < count; i++) {
      addModelBar(right, data.top_models[i], maxPct, 180);
    }
  }

  right.addSpacer();
  w.addSpacer();
  return w;
}

// --- Large Widget ---

function buildLarge(data) {
  const w = new ListWidget();
  w.backgroundColor = C.bg;
  w.setPadding(14, 16, 14, 16);

  addHeader(w, data);
  w.addSpacer(10);

  addHero(w, data);
  w.addSpacer(10);

  addSpendGrid(w, data);
  w.addSpacer(8);

  // Cache row
  const cacheRow = w.addStack();
  cacheRow.layoutHorizontally();
  cacheRow.centerAlignContent();
  const cacheVal = cacheRow.addText(`${Math.round(data.cache_hit_ratio)}%`);
  cacheVal.font = F.cardVal;
  cacheVal.textColor = C.accent;
  cacheRow.addSpacer(4);
  const cacheLbl = cacheRow.addText("prompt cache hit ratio");
  cacheLbl.font = F.tiny;
  cacheLbl.textColor = C.dim;

  w.addSpacer(10);
  addDivider(w);
  w.addSpacer(10);

  // Models
  if (data.top_models && data.top_models.length > 0) {
    addSectionLabel(w, "MODELS");
    w.addSpacer(6);
    const maxPct = data.top_models[0].percentage;
    const count = Math.min(data.top_models.length, 4);
    for (let i = 0; i < count; i++) {
      addModelBar(w, data.top_models[i], maxPct, 300);
      if (i < count - 1) w.addSpacer(4);
    }
  }

  w.addSpacer(10);
  addDivider(w);
  w.addSpacer(10);

  // Recent sessions
  if (data.recent_sessions && data.recent_sessions.length > 0) {
    addSectionLabel(w, "RECENT");
    w.addSpacer(6);
    const count = Math.min(data.recent_sessions.length, 4);
    for (let i = 0; i < count; i++) {
      addSessionRow(w, data.recent_sessions[i]);
      if (i < count - 1) w.addSpacer(3);
    }
  }

  w.addSpacer();
  return w;
}

// --- No Data ---

function buildNoData() {
  const w = new ListWidget();
  w.backgroundColor = C.bg;
  w.setPadding(16, 16, 16, 16);

  const t = w.addText("aitop");
  t.font = F.title;
  t.textColor = C.dim;

  w.addSpacer(8);

  const msg = w.addText("Cannot reach server");
  msg.font = F.label;
  msg.textColor = C.faint;

  w.addSpacer(4);

  const hint = w.addText("Run: aitop --serve");
  hint.font = Font.regularMonospacedSystemFont(10);
  hint.textColor = C.dim;

  w.addSpacer();
  return w;
}

// --- Main ---

const data = await fetchStats();

if (!data) {
  const w = buildNoData();
  Script.setWidget(w);
  if (config.runsInApp) w.presentLarge();
  Script.complete();
} else {
  const family = config.widgetFamily;
  let widget;

  if (family === "large") {
    widget = buildLarge(data);
  } else if (family === "medium") {
    widget = buildMedium(data);
  } else {
    widget = buildSmall(data);
  }

  Script.setWidget(widget);

  if (config.runsInApp) {
    // Show large preview in-app for full dashboard
    const preview = buildLarge(data);
    preview.presentLarge();
  }

  Script.complete();
}
