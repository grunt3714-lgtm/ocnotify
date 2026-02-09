# ocnotify ⚒️

A Rust CLI that wraps any command and sends smart progress notifications via [OpenClaw](https://github.com/openclaw/openclaw).

Wrap a training run, a build, a download — anything. `ocnotify` watches the output, detects progress automatically, and sends milestone updates (with optional plot attachments) to Discord, Telegram, Slack, or any OpenClaw channel.

## Features

- **Auto progress detection** — Parses `Epoch 5/30`, `45%`, `Step 100/1000`, and similar patterns from stdout
- **Milestone reporting** — Sends updates at 10% increments (configurable) when progress is detected
- **Time-based fallback** — Falls back to periodic reports (default: every 5 min) when no progress pattern is found
- **Plot attachments** — Attach live-updating plots to milestone reports
- **Crash detection** — Event-based (not polling). Reports SIGKILL/OOM, segfaults, non-zero exits with the last 10 lines of output
- **LLM summaries** — Optionally summarizes output via an isolated OpenClaw session instead of raw log tails
- **Passthrough** — All stdout/stderr passes through to your terminal unchanged
- **Single binary** — No runtime dependencies. ~740KB static binary

## Install

```bash
# From source
cargo install --path .

# Or build and symlink
cargo build --release
sudo ln -sf $(pwd)/target/release/ocnotify /usr/local/bin/ocnotify
```

## Usage

```bash
ocnotify [OPTIONS] -- <command> [args...]
```

### Examples

**Training run with plot:**
```bash
ocnotify --label "MNIST Training" --plot plots/loss.png -- python train.py
```

**Compile:**
```bash
ocnotify --label "Kernel build" -- make -j8
```

**Download:**
```bash
ocnotify -- wget https://example.com/dataset.tar.gz
```

**Long backup with time-based updates:**
```bash
ocnotify --label "Backup" --no-milestones --interval 60 -- rsync -avz /src /dst
```

**Any binary:**
```bash
ocnotify -- ./my_program --flag value
```

## Options

| Option | Description | Default |
|---|---|---|
| `--label <name>` | Label for notifications | Command name |
| `--interval <secs>` | Time-based fallback interval (0 = off) | `300` |
| `--plot <path>` | Attach plot image to milestone reports | — |
| `--no-milestones` | Disable auto progress detection | Enabled |
| `--no-summary` | Send raw output tail instead of LLM summary | Summary on |
| `--channel <ch>` | OpenClaw channel | env var |
| `--target <tgt>` | OpenClaw target | env var |

## Environment Variables

| Variable | Description |
|---|---|
| `OPENCLAW_PROGRESS_CHANNEL` | Default channel (discord, telegram, etc.) |
| `OPENCLAW_PROGRESS_TARGET` | Default target (user ID, channel ID) |
| `OPENCLAW_PROGRESS_INTERVAL_SEC` | Default fallback interval in seconds |

## How It Works

1. **Spawns** your command as a child process with captured stdout/stderr
2. **Parses** each output line for progress patterns:
   - `Epoch 5/30`, `Step 100/1000`, `Batch 3/50` → X/Y ratio
   - `45%`, `45.2%` → percentage
   - `progress: 0.45` → fraction
3. **Reports** at milestone percentages (10%, 20%, ... 100%) with the triggering line and elapsed time
4. **Falls back** to time-based reporting if no progress pattern is detected
5. **On exit**: reports success with final stats, or crash details (signal, exit code, last 10 lines)

### Example Notifications

```
📊 MNIST Training — 50% (15/30) · 2.1min
`Epoch  15/30 | loss: 0.4493 | accuracy: 92.5%`

✅ MNIST Training finished (30/30) in 4.2min

❌ Big Model killed by SIGKILL (9) (likely OOM) after 12.3min
```

## Progress Pattern Support

| Pattern | Example | Detection |
|---|---|---|
| Step counter | `Epoch 5/30`, `Step 100/1000` | ✅ Milestone |
| Percentage | `45%`, `[=====>   ] 67.2%` | ✅ Milestone |
| Fraction | `progress: 0.45` | ✅ Milestone |
| No pattern | Raw compiler output, logs | ⏱️ Time-based fallback |

## Cross-compilation

Build for x86_64 from ARM (e.g., Raspberry Pi):

```bash
rustup target add x86_64-unknown-linux-gnu
sudo apt install gcc-x86-64-linux-gnu
CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_LINKER=x86_64-linux-gnu-gcc \
  cargo build --release --target x86_64-unknown-linux-gnu
```

## Requirements

- [OpenClaw](https://github.com/openclaw/openclaw) installed and configured (the `openclaw` CLI must be in PATH)
- A configured messaging channel (Discord, Telegram, Slack, etc.)

## License

MIT
