# OpenClaw Progress Notifier

Time-based progress updates for long-running scripts, delivered via `openclaw message send` to any channel (Discord, Telegram, Slack, Signal, etc.).

## Install

```bash
pip install -e /path/to/openclaw-progress-notifier
```

Or copy `scripts/progress_notifier.py` into your project.

## Configuration

Set environment variables (or use a `.env` file):

| Variable | Required | Description |
|---|---|---|
| `OPENCLAW_PROGRESS_CHANNEL` | Yes* | Channel plugin (e.g. `discord`, `telegram`) |
| `OPENCLAW_PROGRESS_TARGET` | Yes* | Target (e.g. `channel:123`, `user:456`) |
| `OPENCLAW_PROGRESS_INTERVAL_SEC` | No | Min seconds between updates (default: 300) |
| `OPENCLAW_PROGRESS_PLOT_INTERVAL_SEC` | No | Min seconds between plot sends (default: same as interval) |
| `OPENCLAW_DEFAULT_DM_CHANNEL` | No | Fallback for channel |
| `OPENCLAW_DEFAULT_DM_TARGET` | No | Fallback for target |

*If neither channel/target nor defaults are set, the notifier is a silent no-op.

## Python Usage

```python
from progress_notifier import ProgressNotifier

notifier = ProgressNotifier(interval_sec=120)

# Bypass interval — always sends
notifier.send_now("🚀 Starting training run")

# Respects interval — skips if too soon
for epoch in range(100):
    loss = train_one_epoch()
    notifier.maybe_send(f"Epoch {epoch} | loss {loss:.4f}", plot_path="plots/loss.png")

notifier.send_now("✅ Run complete", plot_path="plots/final.png")
```

## CLI Usage

```bash
# One-off message
python scripts/progress_notifier.py "epoch 5 | loss 0.1234"

# With plot attachment
python scripts/progress_notifier.py "epoch 5 | loss 0.1234" --plot plots/loss.png
```

## How It Works

- Calls `openclaw message send` under the hood
- `maybe_send()` throttles by wall-clock time (interval_sec)
- Plot sends have a separate throttle (plot_interval_sec)
- If `openclaw` CLI isn't available, silently does nothing
- No dependencies beyond Python stdlib

## License

MIT
