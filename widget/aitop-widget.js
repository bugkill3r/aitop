// aitop Scriptable iOS Widget
// Displays AI spend stats from aitop --serve
//
// Setup:
// 1. Run `aitop --serve` on your machine (reachable via Tailscale etc.)
// 2. Set SERVER_URL below to your machine's IP/hostname
// 3. Add this script in the Scriptable app
// 4. Add a Scriptable widget to your home screen and select this script

const SERVER_URL = "http://YOUR_TAILSCALE_IP:8080";
const API_ENDPOINT = `${SERVER_URL}/api/stats`;
const CACHE_KEY = "aitop_last_stats";

// Colors
const BG_COLOR = new Color("#1a1a2e");
const TEXT_WHITE = new Color("#ffffff");
const TEXT_DIM = new Color("#888899");
const ACCENT_ORANGE = new Color("#ff9933");
const GREEN_LIVE = new Color("#44dd55");
const GRAY_IDLE = new Color("#666677");

async function fetchStats() {
  try {
    const req = new Request(API_ENDPOINT);
    req.timeoutInterval = 5;
    const data = await req.loadJSON();
    // Cache for offline fallback
    Keychain.set(CACHE_KEY, JSON.stringify(data));
    return data;
  } catch (e) {
    // Try cached data
    if (Keychain.contains(CACHE_KEY)) {
      return JSON.parse(Keychain.get(CACHE_KEY));
    }
    return null;
  }
}

function formatCost(value) {
  if (value >= 1000) {
    return `$${Math.round(value)}`;
  } else if (value >= 100) {
    return `$${Math.round(value)}`;
  } else {
    return `$${value.toFixed(2)}`;
  }
}

function formatCostCompact(value) {
  if (value >= 1000) {
    return `$${Math.round(value)}`;
  } else if (value >= 100) {
    return `$${Math.round(value)}`;
  } else if (value >= 10) {
    return `$${value.toFixed(1)}`;
  } else {
    return `$${value.toFixed(2)}`;
  }
}

function buildSmallWidget(data) {
  const w = new ListWidget();
  w.backgroundColor = BG_COLOR;
  w.setPadding(12, 14, 12, 14);

  // Header row: "aitop" left, live dot right
  const header = w.addStack();
  header.layoutHorizontally();
  header.centerAlignContent();

  const title = header.addText("aitop");
  title.font = Font.boldSystemFont(13);
  title.textColor = TEXT_DIM;

  header.addSpacer();

  const dotColor = data.is_live ? GREEN_LIVE : GRAY_IDLE;
  const statusLabel = data.is_live ? "LIVE" : "IDLE";
  const dot = header.addText(`● ${statusLabel}`);
  dot.font = Font.boldSystemFont(11);
  dot.textColor = dotColor;

  w.addSpacer(6);

  // Today's spend
  const spend = w.addText(formatCost(data.spend_today));
  spend.font = Font.boldMonospacedSystemFont(28);
  spend.textColor = ACCENT_ORANGE;
  spend.minimumScaleFactor = 0.6;

  const todayLabel = w.addText("today");
  todayLabel.font = Font.systemFont(12);
  todayLabel.textColor = TEXT_DIM;

  w.addSpacer(4);

  // Burn rate
  const rate = w.addText(`${formatCostCompact(data.burn_rate)}/hr`);
  rate.font = Font.boldMonospacedSystemFont(16);
  rate.textColor = TEXT_WHITE;

  w.addSpacer();

  return w;
}

function buildMediumWidget(data) {
  const w = new ListWidget();
  w.backgroundColor = BG_COLOR;
  w.setPadding(12, 14, 12, 14);

  // Header row
  const header = w.addStack();
  header.layoutHorizontally();
  header.centerAlignContent();

  const title = header.addText("aitop");
  title.font = Font.boldSystemFont(13);
  title.textColor = TEXT_DIM;

  header.addSpacer();

  const dotColor = data.is_live ? GREEN_LIVE : GRAY_IDLE;
  const statusLabel = data.is_live ? "LIVE" : "IDLE";
  const dot = header.addText(`● ${statusLabel}`);
  dot.font = Font.boldSystemFont(11);
  dot.textColor = dotColor;

  w.addSpacer(6);

  // Row 1: today + week
  const row1 = w.addStack();
  row1.layoutHorizontally();
  row1.centerAlignContent();

  const todaySpend = row1.addText(`${formatCost(data.spend_today)} today`);
  todaySpend.font = Font.boldMonospacedSystemFont(18);
  todaySpend.textColor = ACCENT_ORANGE;
  todaySpend.minimumScaleFactor = 0.7;

  row1.addSpacer();

  const weekSpend = row1.addText(`${formatCostCompact(data.spend_this_week)}/wk`);
  weekSpend.font = Font.mediumMonospacedSystemFont(14);
  weekSpend.textColor = TEXT_WHITE;

  w.addSpacer(3);

  // Row 2: burn rate + cache
  const row2 = w.addStack();
  row2.layoutHorizontally();
  row2.centerAlignContent();

  const rate = row2.addText(`${formatCostCompact(data.burn_rate)}/hr`);
  rate.font = Font.boldMonospacedSystemFont(14);
  rate.textColor = TEXT_WHITE;

  row2.addSpacer();

  const cache = row2.addText(`${Math.round(data.cache_hit_ratio)}% cache`);
  cache.font = Font.mediumMonospacedSystemFont(12);
  cache.textColor = TEXT_DIM;

  w.addSpacer(3);

  // Row 3: top model
  if (data.top_models && data.top_models.length > 0) {
    const m = data.top_models[0];
    const row3 = w.addStack();
    row3.layoutHorizontally();
    row3.centerAlignContent();

    const modelName = row3.addText(m.model);
    modelName.font = Font.mediumSystemFont(12);
    modelName.textColor = TEXT_DIM;

    row3.addSpacer(6);

    const modelCost = row3.addText(`${formatCostCompact(m.cost)} (${Math.round(m.percentage)}%)`);
    modelCost.font = Font.mediumMonospacedSystemFont(12);
    modelCost.textColor = TEXT_DIM;

    row3.addSpacer();
  }

  w.addSpacer();

  return w;
}

// Main
const data = await fetchStats();

if (!data) {
  const w = new ListWidget();
  w.backgroundColor = BG_COLOR;
  const msg = w.addText("aitop\nNo data");
  msg.font = Font.systemFont(14);
  msg.textColor = TEXT_DIM;
  Script.setWidget(w);
  Script.complete();
} else {
  const family = config.widgetFamily;
  let widget;

  if (family === "medium") {
    widget = buildMediumWidget(data);
  } else {
    // small or fallback
    widget = buildSmallWidget(data);
  }

  Script.setWidget(widget);

  if (config.runsInApp) {
    // Preview as medium when running in-app
    widget.presentMedium();
  }

  Script.complete();
}
