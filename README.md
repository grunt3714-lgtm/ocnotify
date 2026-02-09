# ocnotify ⚒️

An [OpenClaw](https://github.com/openclaw/openclaw) agent skill for monitoring long-running tasks — training runs, builds, data processing, anything that takes more than a few minutes.

No libraries. No wrappers. No dependencies in your scripts. The agent monitors externally using its existing tools (SSH, log tailing, plot analysis, messaging) and makes human-like decisions about when to intervene.

## How It Works

Your scripts just print to stdout and save plots. That's it. The agent handles everything else:

1. **Launch** jobs via `nohup` (local or remote via SSH), capture PIDs
2. **Monitor** by periodically tailing logs and grabbing plots via SCP
3. **Analyze** plots with vision to assess convergence, plateaus, divergence
4. **Decide** whether to let it run, stop it, or alert the user
5. **Report** concise status updates to Discord/Telegram/Slack

## Decision Framework

| Signal | Action |
|---|---|
| Loss dropping steadily | Let it run |
| Loss flat for 3+ check-ins | Likely converged — stop it |
| Loss increasing from best | Diverging — stop it |
| Accuracy at known ceiling | Target reached — stop it |
| Accept rate <1% (search methods) | Diminishing returns — stop it |
| Unusual spikes or oscillation | Alert the user, don't auto-kill |

## Example

```bash
# Your script — no special imports, no wrappers
python train.py > run.log 2>&1 &

# Agent checks periodically
ssh node 'tail -20 ~/project/run.log'
scp node:~/project/plots/loss.png /tmp/
# → analyzes plot with vision
# → sends update: "⚒️ Step 3000/5000 | LSR 127.34 (plateau) | Killed"
```

## Install

Copy `SKILL.md` into your OpenClaw skills directory, or point your agent config at this repo.

See [SKILL.md](SKILL.md) for the full agent instructions.

## Demo Scripts

- `demo/demo_training.py` — Fake training loop with loss curves and progress output
- `demo/demo_messy.py` — Messy pipeline with no clean progress patterns

Use these to test your monitoring setup.

## Why Not a Wrapper?

We tried that. An agent that can *see* a plot and decide "this has plateaued for 2500 steps, kill it" beats any regex or even LLM-parsed progress bar. The agent already has SSH, vision, and messaging — wrapping commands just adds complexity for no gain.

## License

MIT
