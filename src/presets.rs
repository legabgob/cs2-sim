use crate::types::{RoundConfig, SeriesFormat, StageType, TournamentFormat, TournamentStage};

pub fn preset_names() -> &'static [&'static str] {
    &["CS2 Major", "ESL Pro League Season", "IEM Single Site"]
}

pub fn load_preset(name: &str) -> Option<TournamentFormat> {
    match name {
        "CS2 Major" => Some(cs2_major()),
        "ESL Pro League Season" => Some(esl_pro_league()),
        "IEM Single Site" => Some(iem_single_site()),
        _ => None,
    }
}

// ── CS2 Major (16-team, Swiss + Playoffs) ────────────────────────────────────
//
// Challengers / Legends Stage: 16-team Swiss, top 8 advance
// Playoffs: 8-team Single Elimination (QF Bo3, SF Bo3, F Bo5)

fn cs2_major() -> TournamentFormat {
    TournamentFormat {
        name: "CS2 Major".into(),
        stages: vec![
            TournamentStage {
                name: "Challengers / Legends Stage".into(),
                stage_type: StageType::Swiss,
                teams_in: 16,
                teams_advancing: 8,
                round_configs: vec![
                    RoundConfig { label: "Round 1".into(), format: SeriesFormat::Bo1 },
                    RoundConfig { label: "Round 2".into(), format: SeriesFormat::Bo3 },
                    RoundConfig { label: "Round 3".into(), format: SeriesFormat::Bo3 },
                    RoundConfig { label: "Round 4".into(), format: SeriesFormat::Bo3 },
                    RoundConfig { label: "Round 5".into(), format: SeriesFormat::Bo3 },
                ],
            },
            TournamentStage {
                name: "Playoffs".into(),
                stage_type: StageType::SingleElimination,
                teams_in: 8,
                teams_advancing: 1,
                round_configs: vec![
                    RoundConfig { label: "Quarterfinals".into(), format: SeriesFormat::Bo3 },
                    RoundConfig { label: "Semifinals".into(),    format: SeriesFormat::Bo3 },
                    RoundConfig { label: "Grand Final".into(),   format: SeriesFormat::Bo5 },
                ],
            },
        ],
    }
}

// ── ESL Pro League (24-team, 4 GSL groups → Single Elim playoffs) ────────────
//
// Group Stage: 4 × GSL group (6 teams each, top 3 advance)
// Playoffs: 12-team Double Elimination

fn esl_pro_league() -> TournamentFormat {
    TournamentFormat {
        name: "ESL Pro League Season".into(),
        stages: vec![
            TournamentStage {
                name: "Group Stage".into(),
                stage_type: StageType::GSL,
                teams_in: 16,
                teams_advancing: 8,
                round_configs: vec![
                    RoundConfig { label: "Opening Match".into(),  format: SeriesFormat::Bo1 },
                    RoundConfig { label: "Winners Match".into(),  format: SeriesFormat::Bo3 },
                    RoundConfig { label: "Elimination Match".into(), format: SeriesFormat::Bo3 },
                    RoundConfig { label: "Decider Match".into(),  format: SeriesFormat::Bo3 },
                ],
            },
            TournamentStage {
                name: "Playoffs".into(),
                stage_type: StageType::DoubleElimination,
                teams_in: 8,
                teams_advancing: 1,
                round_configs: vec![
                    RoundConfig { label: "Upper/Lower R1".into(), format: SeriesFormat::Bo3 },
                    RoundConfig { label: "Upper/Lower R2".into(), format: SeriesFormat::Bo3 },
                    RoundConfig { label: "Upper Final".into(),    format: SeriesFormat::Bo3 },
                    RoundConfig { label: "Grand Final".into(),    format: SeriesFormat::Bo5 },
                ],
            },
        ],
    }
}

// ── IEM Single Site (8-team, GSL groups → Single Elim) ───────────────────────

fn iem_single_site() -> TournamentFormat {
    TournamentFormat {
        name: "IEM Single Site".into(),
        stages: vec![
            TournamentStage {
                name: "Group Stage".into(),
                stage_type: StageType::GSL,
                teams_in: 8,
                teams_advancing: 4,
                round_configs: vec![
                    RoundConfig { label: "Opening Match".into(),  format: SeriesFormat::Bo1 },
                    RoundConfig { label: "Winners Match".into(),  format: SeriesFormat::Bo3 },
                    RoundConfig { label: "Elimination Match".into(), format: SeriesFormat::Bo3 },
                    RoundConfig { label: "Decider Match".into(),  format: SeriesFormat::Bo3 },
                ],
            },
            TournamentStage {
                name: "Playoffs".into(),
                stage_type: StageType::SingleElimination,
                teams_in: 4,
                teams_advancing: 1,
                round_configs: vec![
                    RoundConfig { label: "Semifinals".into(),  format: SeriesFormat::Bo3 },
                    RoundConfig { label: "Grand Final".into(), format: SeriesFormat::Bo5 },
                ],
            },
        ],
    }
}
