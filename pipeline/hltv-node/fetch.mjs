/**
 * fetch.mjs — Fetch HLTV team rankings, rosters, and recent match results.
 * Uses gigobyte/HLTV v3 (named export { HLTV }, date-range pagination).
 *
 * Outputs a single JSON object to stdout.
 * Progress / warnings go to stderr so Python can separate them.
 *
 * Usage:
 *   node fetch.mjs [--months N] [--count N]
 *
 *   --months  How many months of result history to fetch (default 3).
 *   --count   How many top-ranked teams to fetch (default 30).
 */

import { HLTV } from 'hltv';

// ── CLI args ──────────────────────────────────────────────────────────────────

const MONTHS_BACK = (() => {
  const idx = process.argv.indexOf('--months');
  return idx !== -1 ? Math.max(1, parseInt(process.argv[idx + 1], 10) || 3) : 3;
})();

/** How many top-ranked teams to fetch (default 30, capped at whatever HLTV returns). */
const TEAM_COUNT = (() => {
  const idx = process.argv.indexOf('--count');
  return idx !== -1 ? Math.max(1, parseInt(process.argv[idx + 1], 10) || 30) : 30;
})();

// Current CS2 active map pool — used to filter HLTV map stats (drops Overpass, Vertigo, etc.)
const ACTIVE_MAPS = new Set([
  'Dust2', 'Mirage', 'Inferno', 'Nuke', 'Ancient', 'Anubis', 'Train',
]);

// Base delay between requests. Cloudflare starts rate-limiting below ~1.5 s.
const DELAY_MS = 2500;

const sleep = (ms) => new Promise(r => setTimeout(r, ms));
function err(msg) { process.stderr.write(msg + '\n'); }

// ── Retry helper ──────────────────────────────────────────────────────────────

/**
 * Run `fn` up to `maxTries` times with exponential back-off.
 * Only retries on errors whose message contains "Access denied" or "ECONNRESET"
 * (Cloudflare transient blocks). Other errors propagate immediately.
 */
async function withRetry(fn, maxTries = 3, baseDelay = 4000) {
  let lastErr;
  for (let attempt = 1; attempt <= maxTries; attempt++) {
    try {
      return await fn();
    } catch (e) {
      lastErr = e;
      const transient = /access denied|econnreset|socket hang up|timeout/i.test(e.message);
      if (!transient || attempt === maxTries) throw e;
      const wait = baseDelay * Math.pow(2, attempt - 1);   // 4 s, 8 s, 16 s …
      err(`  CF block on attempt ${attempt}/${maxTries} — retrying in ${wait / 1000}s…`);
      await sleep(wait);
    }
  }
  throw lastErr;
}

// ── Helpers ───────────────────────────────────────────────────────────────────

function toDateStr(date) {
  return date.toISOString().slice(0, 10);
}

function monthsAgo(n) {
  const d = new Date();
  d.setMonth(d.getMonth() - n);
  return d;
}

// ── Main ──────────────────────────────────────────────────────────────────────

async function main() {
  // 1. Team rankings (1 request — rarely blocked)
  err('Fetching team rankings…');
  let ranking;
  try {
    ranking = await withRetry(() => HLTV.getTeamRanking());
  } catch (e) {
    err(`Fatal: getTeamRanking failed after retries: ${e.message}`);
    process.exit(1);
  }
  const topN = ranking.slice(0, TEAM_COUNT);
  err(`  Got ${topN.length} ranked teams (requested ${TEAM_COUNT}).`);

  // 2. Full team page per ranked team → starter rosters (1 request each)
  err(`Fetching rosters for ${topN.length} teams…`);
  const teams = [];
  for (const entry of topN) {
    await sleep(DELAY_MS);
    try {
      const full = await withRetry(() => HLTV.getTeam({ id: entry.team.id }));

      // p.type is 'Starter' | 'Substitute' | 'Benched' | 'Coach' | undefined
      const starters = (full.players ?? [])
        .filter(p => !p.type || p.type === 'Starter')
        .map(p => ({ id: p.id, name: p.name }));

      teams.push({
        id:      entry.team.id,
        name:    full.name,
        ranking: entry.place,
        points:  entry.points,
        change:  entry.change ?? 0,
        country: full.country?.name ?? '',
        players: starters,
      });
      err(`  [${String(entry.place).padStart(2)}] ${full.name} — ${starters.length} starters`);
    } catch (e) {
      // Keep the team without a roster rather than aborting the whole run.
      err(`  [${String(entry.place).padStart(2)}] ${entry.team.name} — BLOCKED (${e.message.slice(0, 60)})`);
      teams.push({
        id:      entry.team.id,
        name:    entry.team.name,
        ranking: entry.place,
        points:  entry.points,
        change:  entry.change ?? 0,
        country: '',
        players: [],   // Python will fill from overrides.json
      });
    }
  }

  // 3. Recent match results — two-stage strategy:
  //
  //    Stage A: week-by-week getResults (hltv.org/results).
  //      Passes teamIds for server-side pre-filtering and requires BOTH teams to
  //      be in our ranked set. Stops on the first Cloudflare block.
  //
  //    Stage B: getMatchesStats (hltv.org/stats/matches) — different subdomain,
  //      often less aggressively CF-protected. Fetches the full date range in
  //      one paginated call; rankingFilter keeps results relevant. Per-row is a
  //      single MAP played (round scores), which gives richer training data.
  //      Post-filtered by both team names being in our ranked set.
  const teamIds   = teams.map(t => t.id);
  const teamNames = new Set(teams.map(t => t.name));
  const overallEnd   = new Date();
  const overallStart = monthsAgo(MONTHS_BACK);

  const results = [];

  // ── Stage A: getResults week-by-week ─────────────────────────────────────
  err(`Stage A: getResults week-by-week for the last ${MONTHS_BACK} month(s) (${teamIds.length} teams)…`);
  let stageABlocked = false;
  let weekEnd = new Date(overallEnd);

  while (weekEnd > overallStart && results.length < 600) {
    const weekStart = new Date(weekEnd);
    weekStart.setDate(weekStart.getDate() - 7);
    if (weekStart < overallStart) weekStart.setTime(overallStart.getTime());

    const s = toDateStr(weekStart);
    const e = toDateStr(weekEnd);

    try {
      await sleep(DELAY_MS);
      const raw = await withRetry(() =>
        HLTV.getResults({
          startDate: s,
          endDate:   e,
          teamIds,
          delayBetweenPageRequests: DELAY_MS,
        }),
        2, 4000
      );
      let added = 0;
      for (const m of raw) {
        if (!m.team1?.name || !m.team2?.name || m.result == null) continue;
        if (!teamNames.has(m.team1.name) || !teamNames.has(m.team2.name)) continue;
        const t1wins = (m.result.team1 ?? 0) > (m.result.team2 ?? 0);
        results.push({
          team1:  m.team1.name,
          team2:  m.team2.name,
          score1: m.result.team1 ?? 0,
          score2: m.result.team2 ?? 0,
          winner: t1wins ? m.team1.name : m.team2.name,
          map:    m.map    ?? null,
          format: m.format ?? null,
          date:   m.date   ?? null,
          stars:  m.stars  ?? 0,
        });
        added++;
      }
      err(`  ${s} → ${e}: ${added} results`);
    } catch (e2) {
      err(`  ${s} → ${e}: blocked — moving to Stage B.`);
      stageABlocked = true;
      break;
    }

    weekEnd = new Date(weekStart);
    weekEnd.setDate(weekEnd.getDate() - 1);
  }

  err(`  Stage A total: ${results.length} series results`);

  // ── Stage B: getMatchesStats (hltv.org/stats/matches) ────────────────────
  //  Only run when Stage A yielded nothing useful.
  if (results.length < 50) {
    // Pick the tightest applicable rankingFilter so fewer irrelevant pages load.
    const rankingFilter =
      TEAM_COUNT >= 50 ? 'Top50' :
      TEAM_COUNT >= 30 ? 'Top30' :
      TEAM_COUNT >= 20 ? 'Top20' : 'Top10';

    const startStr = toDateStr(overallStart);
    const endStr   = toDateStr(overallEnd);
    err(`Stage B: getMatchesStats (stats subdomain, ${startStr} → ${endStr}, rankingFilter=${rankingFilter})…`);

    try {
      await sleep(DELAY_MS);
      // getMatchesStats auto-paginates until it finds an empty page.
      // Each row is one MAP played (round scores, not series scores).
      const statsRaw = await withRetry(() =>
        HLTV.getMatchesStats({
          startDate: startStr,
          endDate:   endStr,
          rankingFilter,
          delayBetweenPageRequests: DELAY_MS,
        }),
        2, 5000
      );

      let added = 0;
      for (const m of statsRaw) {
        if (!m.team1?.name || !m.team2?.name || m.result == null) continue;
        if (!teamNames.has(m.team1.name) || !teamNames.has(m.team2.name)) continue;
        const t1wins = m.result.team1 > m.result.team2;
        results.push({
          team1:  m.team1.name,
          team2:  m.team2.name,
          score1: m.result.team1,    // round score on this map, e.g. 16
          score2: m.result.team2,    // e.g. 10
          winner: t1wins ? m.team1.name : m.team2.name,
          map:    m.map  ?? null,
          format: 'map',             // per-map, not per-series
          date:   m.date ?? null,
          stars:  0,
        });
        added++;
      }
      err(`  Stage B: ${added} map-level results (from ${statsRaw.length} total rows before filtering)`);
    } catch (e3) {
      err(`  Stage B also blocked: ${e3.message.slice(0, 80)}`);
    }
  }

  err(`  Total results: ${results.length}`);

  // 4. Emit everything as JSON to stdout — Python reads this.
  process.stdout.write(JSON.stringify({ teams, results }, null, 2));
  err('Done.');
}

main().catch(e => {
  err(`Fatal: ${e.stack ?? e}`);
  process.exit(1);
});
