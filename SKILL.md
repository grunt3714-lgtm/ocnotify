---
name: openclaw-progress-notifier
description: "Time-based progress updates for long-running scripts. Use when you need periodic progress notifications to any OpenClaw channel (Discord/Telegram/Slack/Signal/etc.) from Python or shell workflows."
---

# OpenClaw Progress Notifier

Use this skill to add **time-based progress updates** to long-running jobs and send them via `openclaw message send` to any channel.

## Quick usage (Python)

Import the helper and call `maybe_send()` each epoch/step (optionally attach a plot). Use `send_now()` for start/end summaries (bypasses interval):

```python
from progress_notifier import ProgressNotifier

notifier = ProgressNotifier(interval_sec=300)  # 5 min

notifier.send_now("starting run…")

# inside loop
notifier.maybe_send(f"epoch {epoch} | loss {loss:.6f}", plot_path="plots/loss.png")

notifier.send_now("run complete")
```

Configure delivery via env vars (channel-agnostic):

- `OPENCLAW_PROGRESS_CHANNEL` (e.g., `discord`, `telegram`, `slack`)
- `OPENCLAW_PROGRESS_TARGET` (e.g., `channel:123`, `user:123`, `+15551234567`)

If either is missing, the notifier is a no-op.

Optional defaults for "DM me" setups:

- `OPENCLAW_DEFAULT_DM_CHANNEL` (used if `OPENCLAW_PROGRESS_CHANNEL` is unset)
- `OPENCLAW_DEFAULT_DM_TARGET` (used if `OPENCLAW_PROGRESS_TARGET` is unset)

## Setup on remote machines / nodes

The notifier uses `openclaw message send` under the hood. On the **gateway** machine this works out of the box. On **nodes or remote machines**, you need one of:

### Option 1: Install and configure openclaw CLI (recommended)

```bash
npm install -g openclaw
openclaw doctor --fix --non-interactive
```

This creates a minimal `~/.openclaw/openclaw.json` with your channel config. If you only need one channel (e.g., Discord), you can create a minimal config:

```json
{
  "channels": {
    "discord": {
      "enabled": true,
      "token": "YOUR_BOT_TOKEN",
      "actions": { "messages": true }
    }
  }
}
```

Then run `openclaw doctor --fix` to finalize.

### Option 2: HTTP fallback via gateway API

If you don't want to install openclaw on every machine, set these env vars to route notifications through your gateway:

```bash
export OPENCLAW_GATEWAY_URL=http://192.168.1.94:18789
export OPENCLAW_GATEWAY_TOKEN=your-gateway-token
```

The notifier will POST to `$OPENCLAW_GATEWAY_URL/api/v1/message/send` when the CLI is unavailable or misconfigured.

### Self-test

On first send, the notifier runs a self-test and prints warnings to stderr if delivery will fail:

- `WARNING: No channel/target configured` → set the env vars
- `WARNING: channel 'X' not configured on this machine` → run `openclaw doctor --fix` or use HTTP fallback
- `WARNING: openclaw CLI not available` → install openclaw or set gateway URL

## CLI usage

Send a one-off update:

```bash
OPENCLAW_PROGRESS_CHANNEL=discord \
OPENCLAW_PROGRESS_TARGET=channel:123456789 \
python -m progress_notifier "epoch 5 | loss 0.1234"
```

With a plot:

```bash
python -m progress_notifier "epoch 5 | loss 0.1234" --plot plots/loss.png
```

## Delivery behavior

- **`send_now()`** — sends immediately (bypasses interval). Use for start/end messages.
- **`maybe_send()`** — sends only if `interval_sec` has elapsed since last send. Use in loops.
- **Plot attachments** — sent alongside messages when `plot_path` is provided. Plot frequency can differ from text frequency via `OPENCLAW_PROGRESS_PLOT_INTERVAL_SEC`.
- **Deduplication** — text and plot have independent timers; a single call can send both when both intervals have elapsed.

## Environment variables

| Variable | Required | Description |
|---|---|---|
| `OPENCLAW_PROGRESS_CHANNEL` | Yes | Channel name (discord, telegram, etc.) |
| `OPENCLAW_PROGRESS_TARGET` | Yes | Target (channel:ID, user:ID, phone, etc.) |
| `OPENCLAW_PROGRESS_INTERVAL_SEC` | No | Text send interval (default: 300) |
| `OPENCLAW_PROGRESS_PLOT_INTERVAL_SEC` | No | Plot send interval (default: same as text) |
| `OPENCLAW_GATEWAY_URL` | No | HTTP fallback gateway URL |
| `OPENCLAW_GATEWAY_TOKEN` | No | HTTP fallback auth token |
| `OPENCLAW_DEFAULT_DM_CHANNEL` | No | Fallback channel if PROGRESS_CHANNEL unset |
| `OPENCLAW_DEFAULT_DM_TARGET` | No | Fallback target if PROGRESS_TARGET unset |

## Active Monitoring & Early Stopping

When long-running jobs send progress notifications with plot attachments, **you are the early-stop mechanism**. Don't just forward notifications — actively analyze them.

### What to do when a plot arrives

1. **Look at the plot** using vision — don't just read the numeric summary
2. **Assess the training dynamics:**
   - Is the loss still decreasing meaningfully, or has it plateaued?
   - Is there divergence or oscillation that suggests instability?
   - Has the metric hit a reasonable target already?
   - Is the accept rate (for hillclimb/search) too low to make further progress?
3. **Decide whether to intervene:**
   - **Let it run** — curve is still improving at a meaningful rate
   - **Kill it** — clearly plateaued, diverging, or target reached
   - **Alert the user** — ambiguous case, unusual behavior worth flagging

### How to stop a remote run

```bash
# Find and kill the process
ssh grunt@<node-ip> 'kill <PID>'
```

Then send a notification explaining the decision:
- What the plot showed (e.g., "LSR sum plateaued at 115 for last 500 steps")
- Why you stopped it (e.g., "No improvement in 10 updates, convergence reached")
- Final metrics and where the model/plots are saved

### Judgment guidelines

| Signal | Action |
|---|---|
| Loss dropping steadily | Let it run |
| Loss flat for >3 consecutive updates | Likely converged — consider stopping |
| Loss increasing from best | Diverging — stop unless it's early noise |
| Accuracy plateaued near known ceiling | Stop, target reached |
| Accept rate <1% (search/hillclimb) | Diminishing returns — stop |
| Unusual spikes or oscillation | Alert user, don't auto-kill |

### Key principle

You have context that coded heuristics don't — you can see the shape of the curve, compare it to paper baselines, and factor in how long the run has been going. **Use judgment, not thresholds.**

## Notes

- Keep messages short and consistent for clean notification feeds.
- For multi-run batch launches, prefix messages with a run label.
- The notifier is a no-op when channel/target aren't configured — safe to leave in code.
- When monitoring multiple runs, prioritize checking the ones closest to completion or showing concerning trends.

## Resources

- `progress_notifier/__init__.py` — library source
- `scripts/progress_notifier.py` — standalone CLI wrapper (legacy)
