use std::collections::HashMap;
use std::sync::Arc;

use rand::prelude::*;
use rand::SeedableRng;
use rayon::prelude::*;

use crate::data::CachedData;
use crate::inference::InferenceEngine;
use crate::types::*;
use crate::veto::simulate_veto;

// ── Public entry point ────────────────────────────────────────────────────────

pub fn run_monte_carlo(
    teams: &[Team],
    format: &TournamentFormat,
    engine: &Arc<InferenceEngine>,
    map_statuses: &HashMap<String, MapStatus>,
    form_modifiers: &HashMap<String, f32>,
    substitutes: &HashMap<String, crate::types::Substitute>,
    data: &Arc<CachedData>,
    n_runs: u32,
    recency_weight: f32,
) -> SimulationResults {
    let ctx = Arc::new(SimCtx {
        teams: teams.to_vec(),
        format: format.clone(),
        map_statuses: map_statuses.clone(),
        active_maps: data.active_maps.clone(),
        form_modifiers: form_modifiers.clone(),
        substitutes: substitutes.clone(),
        recency_weight,
    });

    let runs: Vec<RunResult> = (0..n_runs)
        .into_par_iter()
        .map(|seed| {
            let mut rng = SmallRng::seed_from_u64(seed as u64);
            simulate_once(&ctx, engine, data, &mut rng)
        })
        .collect();

    aggregate(&runs, format, n_runs)
}

// ── Context (cheaply Arc-shared across threads) ───────────────────────────────

struct SimCtx {
    teams: Vec<Team>,
    format: TournamentFormat,
    map_statuses: HashMap<String, MapStatus>,
    active_maps: Vec<String>,
    form_modifiers: HashMap<String, f32>,
    substitutes: HashMap<String, crate::types::Substitute>,
    recency_weight: f32,
}

// ── Single tournament run ─────────────────────────────────────────────────────

fn simulate_once(
    ctx: &SimCtx,
    engine: &Arc<InferenceEngine>,
    data: &Arc<CachedData>,
    rng: &mut SmallRng,
) -> RunResult {
    let mut remaining: Vec<Team> = ctx.teams.clone();
    let mut result = RunResult::default();

    for stage in &ctx.format.stages {
        // Limit to teams_in (in case previous stage produced more)
        remaining.truncate(stage.teams_in);
        if remaining.is_empty() { break; }

        let (advancing, stage_survivors) = simulate_stage(
            &remaining,
            stage,
            engine,
            ctx,
            data,
            rng,
            &mut result.player_ratings,
        );

        // Record which teams reached this stage
        result.stage_survivors.insert(stage.name.clone(), stage_survivors);

        remaining = advancing;
        if remaining.is_empty() { break; }
    }

    result.champion = remaining.first().map(|t| t.name.clone()).unwrap_or_default();
    result
}

// ── Stage dispatch ────────────────────────────────────────────────────────────

fn simulate_stage(
    teams: &[Team],
    stage: &TournamentStage,
    engine: &Arc<InferenceEngine>,
    ctx: &SimCtx,
    data: &Arc<CachedData>,
    rng: &mut SmallRng,
    player_ratings: &mut HashMap<String, Vec<f32>>,
) -> (Vec<Team>, Vec<String>) {
    match stage.stage_type {
        StageType::Swiss            => simulate_swiss(teams, stage, engine, ctx, data, rng, player_ratings),
        StageType::GSL              => simulate_gsl(teams, stage, engine, ctx, data, rng, player_ratings),
        StageType::SingleElimination => simulate_single_elim(teams, stage, engine, ctx, data, rng, player_ratings),
        StageType::DoubleElimination => simulate_double_elim(teams, stage, engine, ctx, data, rng, player_ratings),
        StageType::RoundRobin       => simulate_round_robin(teams, stage, engine, ctx, data, rng, player_ratings),
    }
}

// ── Match simulation ──────────────────────────────────────────────────────────

fn simulate_match(
    a: &Team,
    b: &Team,
    format: SeriesFormat,
    engine: &InferenceEngine,
    ctx: &SimCtx,
    data: &CachedData,
    rng: &mut SmallRng,
    player_ratings: &mut HashMap<String, Vec<f32>>,
) -> bool {
    // Returns true if `a` wins
    let maps = simulate_veto(a, b, &ctx.map_statuses, &ctx.active_maps, format, rng);
    let wins_needed = format.wins_needed();
    let mut wins_a = 0usize;
    let mut wins_b = 0usize;

    let avg_a = crate::data::team_avg_rating(data, &a.name, &ctx.form_modifiers, &ctx.substitutes);
    let avg_b = crate::data::team_avg_rating(data, &b.name, &ctx.form_modifiers, &ctx.substitutes);

    for map in &maps {
        let p_a = engine.predict_match(avg_a, avg_b, a, b, map, ctx.recency_weight)
            .clamp(0.05, 0.95);
        let a_wins_map: bool = rng.gen::<f32>() < p_a;

        if a_wins_map { wins_a += 1; } else { wins_b += 1; }

        // Track player predicted ratings for this map
        record_player_ratings(a, b, map, engine, ctx, data, player_ratings, rng);

        if wins_a >= wins_needed || wins_b >= wins_needed { break; }
    }
    wins_a > wins_b
}

fn record_player_ratings(
    a: &Team, b: &Team, map: &str,
    engine: &InferenceEngine,
    ctx: &SimCtx,
    data: &CachedData,
    out: &mut HashMap<String, Vec<f32>>,
    rng: &mut SmallRng,
) {
    let opp_b_avg = crate::data::team_avg_rating(data, &b.name, &ctx.form_modifiers, &ctx.substitutes);
    let opp_a_avg = crate::data::team_avg_rating(data, &a.name, &ctx.form_modifiers, &ctx.substitutes);

    for (team_players, opp_avg) in [
        (crate::data::team_players(data, &a.name), opp_b_avg),
        (crate::data::team_players(data, &b.name), opp_a_avg),
    ] {
        for p in team_players {
            // Use sub rating and sub name when a stand-in is active for this slot
            let (base_rating, display_name) = if let Some(sub) = ctx.substitutes.get(&p.name) {
                (sub.rating, sub.sub_name.as_str())
            } else {
                (p.rating, p.name.as_str())
            };
            let fm = ctx.form_modifiers.get(&p.name).copied().unwrap_or(0.0);
            let effective_rating = (base_rating + fm).max(0.5);
            let pred = engine.predict_player_rating(
                effective_rating, p.kd, p.adr, p.kast, p.opening_duel_success,
                opp_avg, p.map_rating(map), fm, ctx.recency_weight,
            );
            // Add a little noise to simulate match variance
            let noisy = pred + rng.gen::<f32>() * 0.06 - 0.03;
            out.entry(display_name.to_string()).or_default().push(noisy);
        }
    }
}

// ── Swiss ─────────────────────────────────────────────────────────────────────

fn simulate_swiss(
    teams: &[Team], stage: &TournamentStage,
    engine: &Arc<InferenceEngine>, ctx: &SimCtx, data: &Arc<CachedData>,
    rng: &mut SmallRng, pr: &mut HashMap<String, Vec<f32>>,
) -> (Vec<Team>, Vec<String>) {
    let total = teams.len();
    let advance = stage.teams_advancing.min(total);
    let n_rounds = 5usize;

    let mut records: HashMap<String, (usize, usize)> = teams.iter()
        .map(|t| (t.name.clone(), (0, 0)))
        .collect();
    let mut team_map: HashMap<String, Team> = teams.iter()
        .map(|t| (t.name.clone(), t.clone()))
        .collect();
    let mut advancing: Vec<Team> = Vec::new();
    let mut eliminated: Vec<Team> = Vec::new();
    let wins_to_advance = 3;
    let losses_to_eliminate = 3;

    for round in 0..n_rounds {
        let format = stage.format_for_round(round);
        // Group teams still playing by record
        let mut active: Vec<String> = records.iter()
            .filter(|(_, &(w, l))| w < wins_to_advance && l < losses_to_eliminate)
            .map(|(n, _)| n.clone())
            .collect();
        active.sort_by_key(|n| {
            let &(w, l) = records.get(n).unwrap();
            (-(w as i32), l as i32) // sort by wins desc, losses asc
        });

        // Pair teams with same record; if odd count, give one team a bye
        let mut i = 0;
        while i + 1 < active.len() {
            let a_name = active[i].clone();
            let b_name = active[i + 1].clone();
            let a = team_map[&a_name].clone();
            let b = team_map[&b_name].clone();
            let a_wins = simulate_match(&a, &b, format, engine, ctx, data, rng, pr);
            if a_wins {
                records.get_mut(&a_name).unwrap().0 += 1;
                records.get_mut(&b_name).unwrap().1 += 1;
            } else {
                records.get_mut(&b_name).unwrap().0 += 1;
                records.get_mut(&a_name).unwrap().1 += 1;
            }
            i += 2;
        }

        // Check for resolved teams
        let just_done: Vec<String> = records.iter()
            .filter(|(_, &(w, l))| w >= wins_to_advance || l >= losses_to_eliminate)
            .map(|(n, _)| n.clone())
            .filter(|n| team_map.contains_key(n))
            .collect();

        for name in just_done {
            let &(w, _) = records.get(&name).unwrap();
            let t = team_map.remove(&name).unwrap();
            if w >= wins_to_advance { advancing.push(t); } else { eliminated.push(t); }
        }
    }

    // Any remaining active teams: rank by wins
    let mut leftovers: Vec<(String, (usize, usize))> = team_map.iter()
        .map(|(n, _)| (n.clone(), *records.get(n).unwrap()))
        .collect();
    leftovers.sort_by(|a, b| b.1.0.cmp(&a.1.0));
    for (name, _) in leftovers {
        advancing.push(team_map[&name].clone());
    }

    advancing.truncate(advance);
    let survivors: Vec<String> = advancing.iter().map(|t| t.name.clone()).collect();
    (advancing, survivors)
}

// ── GSL ───────────────────────────────────────────────────────────────────────

fn simulate_gsl(
    teams: &[Team], stage: &TournamentStage,
    engine: &Arc<InferenceEngine>, ctx: &SimCtx, data: &Arc<CachedData>,
    rng: &mut SmallRng, pr: &mut HashMap<String, Vec<f32>>,
) -> (Vec<Team>, Vec<String>) {
    // Split into groups of 4, run standard GSL (5 matches), take top 2 per group
    let group_size = 4;
    let advance_per_group = 2;
    let mut groups: Vec<Vec<Team>> = Vec::new();
    let mut i = 0;
    while i < teams.len() {
        let end = (i + group_size).min(teams.len());
        groups.push(teams[i..end].to_vec());
        i += group_size;
    }

    let mut all_advancing: Vec<Team> = Vec::new();
    for group in &groups {
        let (adv, _) = gsl_group(group, stage, engine, ctx, data, rng, pr, advance_per_group);
        all_advancing.extend(adv);
    }

    all_advancing.truncate(stage.teams_advancing);
    let survivors: Vec<String> = all_advancing.iter().map(|t| t.name.clone()).collect();
    (all_advancing, survivors)
}

fn gsl_group(
    group: &[Team], stage: &TournamentStage,
    engine: &Arc<InferenceEngine>, ctx: &SimCtx, data: &Arc<CachedData>,
    rng: &mut SmallRng, pr: &mut HashMap<String, Vec<f32>>,
    advance: usize,
) -> (Vec<Team>, Vec<Team>) {
    // Needs at least 4 teams; pad or truncate
    if group.len() < 2 { return (group.to_vec(), vec![]); }

    let fmt = |r| stage.format_for_round(r);

    let m1_a_wins = simulate_match(&group[0], &group[1], fmt(0), engine, ctx, data, rng, pr);
    let m2_a_wins = if group.len() >= 4 {
        simulate_match(&group[2], &group[3], fmt(0), engine, ctx, data, rng, pr)
    } else {
        simulate_match(&group[1], &group[0], fmt(0), engine, ctx, data, rng, pr)
    };

    let (w1, l1) = if m1_a_wins { (&group[0], &group[1]) } else { (&group[1], &group[0]) };
    let (w2, l2) = if group.len() >= 4 {
        if m2_a_wins { (&group[2], &group[3]) } else { (&group[3], &group[2]) }
    } else {
        (w1, l1) // fallback: only 2 teams
    };

    // Winners match: w1 vs w2
    let wm_a_wins = simulate_match(w1, w2, fmt(1), engine, ctx, data, rng, pr);
    let (top, wm_loser) = if wm_a_wins { (w1, w2) } else { (w2, w1) };

    // Elimination match: l1 vs l2
    let em_a_wins = simulate_match(l1, l2, fmt(2), engine, ctx, data, rng, pr);
    let (em_winner, _bottom) = if em_a_wins { (l1, l2) } else { (l2, l1) };

    // Decider: wm_loser vs em_winner
    let dec_a_wins = simulate_match(wm_loser, em_winner, fmt(3), engine, ctx, data, rng, pr);
    let second = if dec_a_wins { wm_loser } else { em_winner };

    let advancing = vec![top.clone(), second.clone()];
    (advancing[..advance.min(advancing.len())].to_vec(), vec![])
}

// ── Single Elimination ────────────────────────────────────────────────────────

fn simulate_single_elim(
    teams: &[Team], stage: &TournamentStage,
    engine: &Arc<InferenceEngine>, ctx: &SimCtx, data: &Arc<CachedData>,
    rng: &mut SmallRng, pr: &mut HashMap<String, Vec<f32>>,
) -> (Vec<Team>, Vec<String>) {
    let mut bracket: Vec<Team> = teams.to_vec();
    let mut round_idx = 0usize;

    while bracket.len() > 1 {
        let fmt = stage.format_for_round(round_idx);
        let mut winners = Vec::new();
        let mut i = 0;
        while i + 1 < bracket.len() {
            let a_wins = simulate_match(&bracket[i], &bracket[i + 1], fmt, engine, ctx, data, rng, pr);
            winners.push(if a_wins { bracket[i].clone() } else { bracket[i + 1].clone() });
            i += 2;
        }
        // Bye if odd number
        if bracket.len() % 2 == 1 { winners.push(bracket.last().unwrap().clone()); }
        bracket = winners;
        round_idx += 1;
    }

    let survivors: Vec<String> = bracket.iter().map(|t| t.name.clone()).collect();
    (bracket, survivors)
}

// ── Double Elimination ────────────────────────────────────────────────────────

fn simulate_double_elim(
    teams: &[Team], stage: &TournamentStage,
    engine: &Arc<InferenceEngine>, ctx: &SimCtx, data: &Arc<CachedData>,
    rng: &mut SmallRng, pr: &mut HashMap<String, Vec<f32>>,
) -> (Vec<Team>, Vec<String>) {
    let mut upper: Vec<Team> = teams.to_vec();
    let mut lower: Vec<Team> = Vec::new();
    let mut round_idx = 0usize;

    while upper.len() + lower.len() > 1 {
        let fmt = stage.format_for_round(round_idx);

        // Upper bracket round
        if upper.len() > 1 {
            let mut u_winners = Vec::new();
            let mut i = 0;
            while i + 1 < upper.len() {
                let a_wins = simulate_match(&upper[i], &upper[i+1], fmt, engine, ctx, data, rng, pr);
                u_winners.push(if a_wins { upper[i].clone() } else { upper[i+1].clone() });
                lower.push(if a_wins { upper[i+1].clone() } else { upper[i].clone() });
                i += 2;
            }
            if upper.len() % 2 == 1 { u_winners.push(upper.last().unwrap().clone()); }
            upper = u_winners;
        }

        // Lower bracket round
        if lower.len() > 1 {
            let mut l_winners = Vec::new();
            let mut i = 0;
            while i + 1 < lower.len() {
                let a_wins = simulate_match(&lower[i], &lower[i+1], fmt, engine, ctx, data, rng, pr);
                l_winners.push(if a_wins { lower[i].clone() } else { lower[i+1].clone() });
                i += 2;
            }
            if lower.len() % 2 == 1 { l_winners.push(lower.last().unwrap().clone()); }
            lower = l_winners;
        }

        round_idx += 1;

        // Stop when only 1 team remains in each bracket
        if upper.len() <= 1 && lower.len() <= 1 { break; }
    }

    // Grand final: upper vs lower
    let champion = if upper.len() == 1 && lower.len() == 1 {
        let gf_fmt = stage.format_for_round(round_idx);
        let a_wins = simulate_match(&upper[0], &lower[0], gf_fmt, engine, ctx, data, rng, pr);
        if a_wins { upper[0].clone() } else { lower[0].clone() }
    } else if !upper.is_empty() {
        upper[0].clone()
    } else {
        lower[0].clone()
    };

    let survivors = vec![champion.name.clone()];
    (vec![champion], survivors)
}

// ── Round Robin ───────────────────────────────────────────────────────────────

fn simulate_round_robin(
    teams: &[Team], stage: &TournamentStage,
    engine: &Arc<InferenceEngine>, ctx: &SimCtx, data: &Arc<CachedData>,
    rng: &mut SmallRng, pr: &mut HashMap<String, Vec<f32>>,
) -> (Vec<Team>, Vec<String>) {
    let fmt = stage.format_for_round(0);
    let mut wins: HashMap<String, usize> = teams.iter().map(|t| (t.name.clone(), 0)).collect();

    for i in 0..teams.len() {
        for j in (i + 1)..teams.len() {
            let a_wins = simulate_match(&teams[i], &teams[j], fmt, engine, ctx, data, rng, pr);
            if a_wins {
                *wins.get_mut(&teams[i].name).unwrap() += 1;
            } else {
                *wins.get_mut(&teams[j].name).unwrap() += 1;
            }
        }
    }

    let mut ranked: Vec<Team> = teams.to_vec();
    ranked.sort_by(|a, b| wins[&b.name].cmp(&wins[&a.name]));
    ranked.truncate(stage.teams_advancing);

    let survivors: Vec<String> = ranked.iter().map(|t| t.name.clone()).collect();
    (ranked, survivors)
}

// ── Result aggregation ────────────────────────────────────────────────────────

fn aggregate(runs: &[RunResult], _format: &TournamentFormat, n_runs: u32) -> SimulationResults {
    let n = n_runs as f32;
    let mut win_counts: HashMap<String, u32> = HashMap::new();
    let mut stage_counts: HashMap<String, HashMap<String, u32>> = HashMap::new();
    let mut player_sum: HashMap<String, f32> = HashMap::new();
    let mut player_sum2: HashMap<String, f32> = HashMap::new();
    let mut player_count: HashMap<String, u32> = HashMap::new();

    for run in runs {
        *win_counts.entry(run.champion.clone()).or_insert(0) += 1;

        for (stage_name, survivors) in &run.stage_survivors {
            let stage_map = stage_counts.entry(stage_name.clone()).or_default();
            for team in survivors {
                *stage_map.entry(team.clone()).or_insert(0) += 1;
            }
        }

        for (player, ratings) in &run.player_ratings {
            let sum: f32 = ratings.iter().sum();
            let count = ratings.len() as f32;
            *player_sum.entry(player.clone()).or_insert(0.0) += sum / count.max(1.0);
            *player_sum2.entry(player.clone()).or_insert(0.0) += (sum / count.max(1.0)).powi(2);
            *player_count.entry(player.clone()).or_insert(0) += 1;
        }
    }

    let win_prob: HashMap<String, f32> = win_counts.iter()
        .map(|(k, &v)| (k.clone(), v as f32 / n))
        .collect();

    let stage_reach: HashMap<String, HashMap<String, f32>> = stage_counts.iter()
        .map(|(stage, teams)| {
            let probs = teams.iter()
                .map(|(t, &c)| (t.clone(), c as f32 / n))
                .collect();
            (stage.clone(), probs)
        })
        .collect();

    let player_perf: HashMap<String, (f32, f32)> = player_sum.iter()
        .map(|(name, &sum)| {
            let cnt = *player_count.get(name).unwrap_or(&1) as f32;
            let mean = sum / cnt;
            let mean2 = player_sum2.get(name).copied().unwrap_or(0.0) / cnt;
            let std = (mean2 - mean * mean).max(0.0).sqrt();
            (name.clone(), (mean, std))
        })
        .collect();

    SimulationResults { win_prob, stage_reach, player_perf }
}
