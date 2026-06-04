"""Train PyTorch MLPs and export to ONNX. Also writes feature_schema.json."""
import json
import logging
from pathlib import Path

import numpy as np
import torch
import torch.nn as nn
from torch.utils.data import DataLoader, TensorDataset
from sklearn.model_selection import train_test_split
from sklearn.preprocessing import StandardScaler

from .features import (
    build_match_training_data,
    build_player_training_data,
    MATCH_FEATURE_NAMES,
    PLAYER_FEATURE_NAMES,
    N_MATCH_FEATURES,
    N_PLAYER_FEATURES,
)

logger = logging.getLogger(__name__)

DATA_DIR = Path(__file__).parent.parent / "data"
DATA_DIR.mkdir(parents=True, exist_ok=True)


# ── Models ────────────────────────────────────────────────────────────────────

class MatchNet(nn.Module):
    def __init__(self, in_dim: int = N_MATCH_FEATURES):
        super().__init__()
        self.net = nn.Sequential(
            nn.Linear(in_dim, 128), nn.ReLU(), nn.Dropout(0.2),
            nn.Linear(128, 64),    nn.ReLU(), nn.Dropout(0.2),
            nn.Linear(64, 32),     nn.ReLU(),
            nn.Linear(32, 1),      nn.Sigmoid(),
        )

    def forward(self, x: torch.Tensor) -> torch.Tensor:
        return self.net(x)


class PlayerNet(nn.Module):
    def __init__(self, in_dim: int = N_PLAYER_FEATURES):
        super().__init__()
        self.net = nn.Sequential(
            nn.Linear(in_dim, 64), nn.ReLU(), nn.Dropout(0.15),
            nn.Linear(64, 32),     nn.ReLU(),
            nn.Linear(32, 1),
        )

    def forward(self, x: torch.Tensor) -> torch.Tensor:
        return self.net(x)


# ── Helpers ───────────────────────────────────────────────────────────────────

def _train_loop(model: nn.Module, loader: DataLoader, criterion, optimizer, epochs: int) -> None:
    model.train()
    for epoch in range(epochs):
        total_loss = 0.0
        for xb, yb in loader:
            optimizer.zero_grad()
            pred = model(xb).squeeze(-1)
            loss = criterion(pred, yb)
            loss.backward()
            optimizer.step()
            total_loss += loss.item()
        if (epoch + 1) % 10 == 0:
            logger.info(f"  epoch {epoch+1}/{epochs}  loss={total_loss/len(loader):.4f}")


def _accuracy(model: nn.Module, X: torch.Tensor, y: torch.Tensor) -> float:
    model.eval()
    with torch.no_grad():
        preds = (model(X).squeeze(-1) > 0.5).float()
    return (preds == y).float().mean().item()


def _export_onnx(model: nn.Module, in_dim: int, path: Path, input_name: str, output_name: str) -> None:
    import onnx
    model.eval()
    dummy = torch.zeros(1, in_dim)
    # Use the legacy TorchScript exporter (dynamo=False) for a self-contained
    # single-file .onnx — avoids the external .data file the dynamo exporter creates.
    torch.onnx.export(
        model, dummy, str(path),
        input_names=[input_name],
        output_names=[output_name],
        dynamic_axes={input_name: {0: "batch"}, output_name: {0: "batch"}},
        opset_version=18,
        dynamo=False,
    )
    # Confirm the file is self-contained (no external data refs).
    m = onnx.load(str(path))
    logger.info(f"  Exported {path.name}  (opset {m.opset_import[0].version})")


# ── Public API ────────────────────────────────────────────────────────────────

def train_and_export() -> None:
    print("Building match training data...")
    X_match, y_match = build_match_training_data(recency_weight=0.5)

    # Add augmented recency samples
    extra_X, extra_y = [], []
    for rw in [0.0, 0.25, 0.75, 1.0]:
        Xa, ya = build_match_training_data(recency_weight=rw)
        extra_X.append(Xa)
        extra_y.append(ya)
    X_match = np.concatenate([X_match] + extra_X)
    y_match = np.concatenate([y_match] + extra_y)

    print(f"  Match samples: {len(X_match)}")

    scaler_m = StandardScaler()
    X_m_scaled = scaler_m.fit_transform(X_match).astype(np.float32)

    Xtr, Xval, ytr, yval = train_test_split(X_m_scaled, y_match, test_size=0.15, random_state=42)
    tr_ds = TensorDataset(torch.from_numpy(Xtr), torch.from_numpy(ytr))
    tr_dl = DataLoader(tr_ds, batch_size=256, shuffle=True)

    match_model = MatchNet()
    opt = torch.optim.Adam(match_model.parameters(), lr=1e-3, weight_decay=1e-4)
    criterion = nn.BCELoss()

    print("Training match model...")
    logging.basicConfig(level=logging.INFO)
    _train_loop(match_model, tr_dl, criterion, opt, epochs=60)

    val_acc = _accuracy(match_model, torch.from_numpy(Xval), torch.from_numpy(yval))
    print(f"  Validation accuracy: {val_acc:.3f}")

    # ── Player model ──
    print("Building player training data...")
    X_player, y_player = build_player_training_data()
    print(f"  Player samples: {len(X_player)}")

    scaler_p = StandardScaler()
    X_p_scaled = scaler_p.fit_transform(X_player).astype(np.float32)

    tr_ds_p = TensorDataset(
        torch.from_numpy(X_p_scaled),
        torch.from_numpy(y_player.astype(np.float32)),
    )
    tr_dl_p = DataLoader(tr_ds_p, batch_size=256, shuffle=True)

    player_model = PlayerNet()
    opt_p = torch.optim.Adam(player_model.parameters(), lr=1e-3)
    print("Training player model...")
    _train_loop(player_model, tr_dl_p, nn.MSELoss(), opt_p, epochs=60)

    # ── Export ONNX ──
    print("Exporting ONNX models...")
    _export_onnx(match_model,  N_MATCH_FEATURES,  DATA_DIR / "model.onnx",        "float_input", "win_probability")
    _export_onnx(player_model, N_PLAYER_FEATURES, DATA_DIR / "player_model.onnx", "float_input", "rating_prediction")

    # ── Save scalers + schema ──
    schema = {
        "match_features": MATCH_FEATURE_NAMES,
        "player_features": PLAYER_FEATURE_NAMES,
        "match_scaler": {
            "mean": scaler_m.mean_.tolist(),
            "scale": scaler_m.scale_.tolist(),
        },
        "player_scaler": {
            "mean": scaler_p.mean_.tolist(),
            "scale": scaler_p.scale_.tolist(),
        },
    }
    with open(DATA_DIR / "feature_schema.json", "w") as f:
        json.dump(schema, f, indent=2)
    print(f"  Saved feature_schema.json")
    print("Done.")
