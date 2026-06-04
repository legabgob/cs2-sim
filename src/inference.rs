use std::path::Path;
use std::sync::Mutex;

use anyhow::Result;
use ort::session::Session;
use ort::value::Tensor;

use crate::data::{FeatureSchema, ScalerParams};
use crate::types::Team;

// ── Session pool ──────────────────────────────────────────────────────────────
// Session::run takes &mut self, so we wrap in Mutex for thread-safe sharing.

pub struct OnnxSessions {
    match_session: Mutex<Session>,
    player_session: Mutex<Session>,
    pub match_scaler: ScalerParams,
    pub player_scaler: ScalerParams,
}

// ── Engine ────────────────────────────────────────────────────────────────────

pub enum InferenceEngine {
    Onnx(OnnxSessions),
    Formula,
}

// SAFETY: Session's underlying ONNX Runtime is thread-safe; Mutex guards &mut access.
unsafe impl Send for InferenceEngine {}
unsafe impl Sync for InferenceEngine {}

impl InferenceEngine {
    pub fn load(data_dir: &Path, schema: Option<&FeatureSchema>) -> Self {
        let match_path = data_dir.join("model.onnx");
        let player_path = data_dir.join("player_model.onnx");

        if match_path.exists() && player_path.exists() {
            match try_load_sessions(&match_path, &player_path, schema) {
                Some(s) => {
                    eprintln!("[inference] ONNX models loaded.");
                    return InferenceEngine::Onnx(s);
                }
                None => eprintln!("[inference] Failed to load ONNX models — using formula predictor."),
            }
        } else {
            eprintln!("[inference] ONNX models not found — using formula predictor. Run: python pipeline/run.py fetch+train");
        }
        InferenceEngine::Formula
    }

    pub fn is_onnx(&self) -> bool {
        matches!(self, InferenceEngine::Onnx(_))
    }

    pub fn predict_match(
        &self,
        avg_rating_a: f32,
        avg_rating_b: f32,
        team_a: &Team,
        team_b: &Team,
        map: &str,
        recency_weight: f32,
    ) -> f32 {
        let features = build_match_features(avg_rating_a, avg_rating_b, team_a, team_b, map, recency_weight);
        match self {
            InferenceEngine::Onnx(sess) => {
                let scaled = sess.match_scaler.transform(&features);
                run_onnx_f32(&sess.match_session, &scaled, "float_input", "win_probability")
                    .unwrap_or_else(|_| formula_predict(&features))
            }
            InferenceEngine::Formula => formula_predict(&features),
        }
    }

    pub fn predict_player_rating(
        &self,
        player_rating: f32,
        player_kd: f32,
        player_adr: f32,
        player_kast: f32,
        player_od_success: f32,
        opponent_avg_rating: f32,
        map_familiarity: f32,
        form_modifier: f32,
        recency_weight: f32,
    ) -> f32 {
        let features = [
            player_rating, player_kd, player_adr, player_kast, player_od_success,
            opponent_avg_rating, map_familiarity, form_modifier, recency_weight,
        ];
        match self {
            InferenceEngine::Onnx(sess) => {
                let scaled = sess.player_scaler.transform(&features);
                run_onnx_f32(&sess.player_session, &scaled, "float_input", "rating_prediction")
                    .unwrap_or(player_rating + form_modifier * 0.05)
            }
            InferenceEngine::Formula => player_rating + form_modifier * 0.05,
        }
    }
}

// ── Feature construction ──────────────────────────────────────────────────────

fn build_match_features(
    avg_rating_a: f32, avg_rating_b: f32,
    team_a: &Team, team_b: &Team,
    map: &str, recency_weight: f32,
) -> [f32; 10] {
    [
        avg_rating_a,
        avg_rating_b,
        team_a.blended_wr(recency_weight),
        team_b.blended_wr(recency_weight),
        team_a.h2h_wr(&team_b.name),
        team_a.map_wr(map),
        team_b.map_wr(map),
        if team_a.map_pool.iter().any(|m| m == map) { 1.0 } else { 0.0 },
        if team_b.map_pool.iter().any(|m| m == map) { 1.0 } else { 0.0 },
        recency_weight,
    ]
}

fn formula_predict(features: &[f32]) -> f32 {
    let diff = (features[0] - features[1]) * 2.5
        + (features[2] - features[3]) * 1.2
        + (features[4] - 0.5) * 0.8
        + (features[5] - features[6]) * 0.6
        + (features[7] - features[8]) * 0.15;
    1.0 / (1.0 + (-diff).exp())
}

// ── ONNX helpers ──────────────────────────────────────────────────────────────

fn try_load_sessions(
    match_path: &Path,
    player_path: &Path,
    schema: Option<&FeatureSchema>,
) -> Option<OnnxSessions> {
    let match_session = Session::builder().ok()?.commit_from_file(match_path).ok()?;
    let player_session = Session::builder().ok()?.commit_from_file(player_path).ok()?;

    let (ms, ps) = if let Some(s) = schema {
        (
            ScalerParams { mean: s.match_scaler.mean.clone(), scale: s.match_scaler.scale.clone() },
            ScalerParams { mean: s.player_scaler.mean.clone(), scale: s.player_scaler.scale.clone() },
        )
    } else {
        (
            ScalerParams { mean: vec![0.0; 10], scale: vec![1.0; 10] },
            ScalerParams { mean: vec![0.0; 9],  scale: vec![1.0; 9] },
        )
    };

    Some(OnnxSessions {
        match_session: Mutex::new(match_session),
        player_session: Mutex::new(player_session),
        match_scaler: ms,
        player_scaler: ps,
    })
}

fn run_onnx_f32(
    session: &Mutex<Session>,
    features: &[f32],
    input_name: &str,
    output_name: &str,
) -> Result<f32> {
    let n = features.len();
    let tensor = Tensor::<f32>::from_array((vec![1i64, n as i64], features.to_vec()))?;
    let inputs = ort::inputs![input_name => tensor];
    let mut sess = session.lock().map_err(|_| anyhow::anyhow!("Mutex poisoned"))?;
    let outputs = sess.run(inputs)?;
    let (_, data) = outputs[output_name].try_extract_tensor::<f32>()?;
    data.first().copied().ok_or_else(|| anyhow::anyhow!("empty output tensor"))
}
