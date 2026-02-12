---
name: task-monitor
description: "Monitor, report on, and make decisions about long-running tasks. Use when launching training runs, builds, data processing, or any job that takes more than a few minutes."
---

# Task Monitor

You are the monitoring system. No libraries or wrappers needed — use your existing tools (exec, SSH, image analysis, messaging) to watch jobs, report progress, and decide when to intervene.

## Launching a task

Always log output to a file so you can check it later:

```bash
# Local
nohup python train.py > run.log 2>&1 &
echo $!  # save the PID

# Remote node (preferred: OpenClaw node RPC)
# Use OpenClaw node RPC for quick launches when possible (better observability).
# (Exact command will vary; use the `nodes` tool in-chat.)

# SSH fallback (when RPC timeout is too short or you need a full shell)
ssh grunt@<node> 'cd ~/project && nohup python train.py > run.log 2>&1 & echo $!'
```

Use `PYTHONUNBUFFERED=1` for Python scripts so output appears in real time.

After launch, send a summary to the user: what's running, where, what PID, what to expect.

## Checking progress

Periodically tail the log and grab any plots.

Prefer OpenClaw’s built-in node RPC (`nodes` tool) for quick status/log reads; fall back to SSH/SCP when you need longer operations or file transfer.

```bash
# Check log (SSH fallback)
ssh grunt@<node> 'tail -20 ~/project/run.log'

# Grab plot for visual analysis (SCP fallback)
scp grunt@<node>:~/project/plots/loss.png /tmp/check.png
```

Then use the `image` tool to analyze the plot.

## Making decisions

When you look at a plot or read metrics, decide:

| Signal | Action |
|---|---|
| Loss dropping steadily | Let it run |
| Loss flat for 3+ check-ins | Likely converged — stop it |
| Loss increasing from best | Diverging — stop it |
| Accuracy at known ceiling | Target reached — stop it |
| Accept rate <1% (search methods) | Diminishing returns — stop it |
| Unusual spikes or oscillation | Alert the user, don't auto-kill |

**Use judgment, not thresholds.** You can see the curve shape, compare to baselines, and factor in how long it's been running.

## Stopping a task

```bash
ssh grunt@<node> 'kill <PID>'
```

Then report:
- What the plot/metrics showed
- Why you stopped it
- Final numbers and where results are saved

## Reporting

Keep updates concise. One message with the key metrics:

```
⚒️ Hillclimb Node 1 | Step 3000/5000
LSR sum: 127.34 (plateau since step 500)
Accept rate: 0.5%
→ Killed — no improvement in 2500 steps
```

For batch launches across multiple nodes, send one grouped summary instead of N individual messages.

## When to check

- After launch: verify the process started and first output appeared
- Periodically: every 5-15 min for fast jobs, every 30-60 min for slow ones
- Use heartbeats or cron jobs to remind yourself to check
- When a notification arrives with a plot attachment — always look at it

## Automating monitoring (generic)

If the user asks for “monitor this every X minutes” or “monitor whatever is running”, prefer **one generic monitor loop** rather than bespoke one-off scripts.

Two reliable patterns:

### Pattern A — registry-based (works for *any* program)

When you launch a long-running task, record a job entry (host, PID, cwd, log path, label, optional artifact paths) in a small JSON file on the gateway (example path):

- `/home/grunt/.openclaw/workspace/memory/monitor-registry.json`

Then a cron/heartbeat tick can:

1) Check if each PID is alive (remote `ps -p <pid>`)
2) Tail the log
3) Pull/generate any plots
4) Message the user **only if something is running**
5) When finished, mark the entry complete

This avoids noisy “scan the whole machine” behavior and works for non-Python jobs too.

### Pattern B — heuristic fallback (useful for legacy jobs)

If there’s no registry, you can scan for known signatures (e.g. `pgrep -af "python.*-m src.train"`) and infer log paths from argv.

Use this only as a fallback—registry is the stable approach.

### Intervention policy

Unless the user explicitly authorizes it, **ask before killing** a run. If you recommend stopping, include:

- what plateau/divergence signal you observed
- how long it has persisted (time window / gens)
- what you recommend (stop / keep going / change params)

## Multi-node runs

When distributing work across the fleet:

1. Track what's running where (node, PID, task, start time)
2. Check all nodes in one pass
3. Collect results via SCP when done
4. Report a single consolidated summary
