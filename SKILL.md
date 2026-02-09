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

# Remote node
ssh grunt@<node> 'cd ~/project && nohup python train.py > run.log 2>&1 & echo $!'
```

Use `PYTHONUNBUFFERED=1` for Python scripts so output appears in real time.

After launch, send a summary to the user: what's running, where, what PID, what to expect.

## Checking progress

Periodically tail the log and grab any plots:

```bash
# Check log
ssh grunt@<node> 'tail -20 ~/project/run.log'

# Grab plot for visual analysis
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

## Multi-node runs

When distributing work across the fleet:

1. Track what's running where (node, PID, task, start time)
2. Check all nodes in one pass
3. Collect results via SCP when done
4. Report a single consolidated summary
