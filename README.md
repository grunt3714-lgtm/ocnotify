# ocnotify ⚒️

Smart progress monitoring for long-running tasks, built for [OpenClaw](https://github.com/openclaw/openclaw).

Two approaches, depending on your needs:

| Approach | When to use | How it works |
|---|---|---|
| **CLI wrapper** (`ocnotify`) | Automated pipelines, unattended runs | Wraps any command, parses output, sends milestone notifications |
| **Agent skill** (`SKILL.md`) | Interactive monitoring, complex decisions | Agent monitors jobs via SSH/logs/plots, decides when to intervene |

## CLI Wrapper

A single Rust binary (~740KB) that wraps any command and sends progress notifications to Discord, Telegram, Slack, or any OpenClaw channel.

### Install

```bash
# From source
cargo build --release
sudo ln -sf $(pwd)/target/release/ocnotify /usr/local/bin/ocnotify
```

### Usage

```bash
ocnotify [OPTIONS] -- <command> [args...]
```

### Examples

```bash
# Training run with plot attachment
ocnotify --label "MNIST Training" --plot plots/loss.png -- python train.py

# Build with regex-only parsing (no LLM)
ocnotify --no-llm --label "Kernel build" -- make -j8

# Long backup with time-based updates
ocnotify --label "Backup" --fallback 60 -- rsync -avz /src /dst
```

### Options

| Option | Description | Default |
|---|---|---|
| `--label <name>` | Label for notifications | Command name |
| `--parse-every <sec>` | How often to send output to LLM for parsing | `10` |
| `--fallback <sec>` | Time-based fallback interval when no progress detected | `300` |
| `--plot <path>` | Attach plot image to milestone reports | — |
| `--no-llm` | Disable LLM parsing; use regex-only | LLM enabled |
| `--channel <ch>` | OpenClaw channel | env var |
| `--target <tgt>` | OpenClaw target | env var |

### Environment Variables

| Variable | Description |
|---|---|
| `OPENCLAW_PROGRESS_CHANNEL` | Default channel (discord, telegram, etc.) |
| `OPENCLAW_PROGRESS_TARGET` | Default target (user ID, channel ID) |
| `OPENCLAW_PROGRESS_PARSE_SEC` | Default LLM parse interval in seconds |
| `OPENCLAW_PROGRESS_FALLBACK_SEC` | Default fallback interval in seconds |

### How It Works

1. **Spawns** your command with captured stdout/stderr
2. **Parses** output for progress patterns:
   - LLM-powered: sends output chunks to an OpenClaw session for intelligent parsing
   - Regex fallback: detects `Epoch 5/30`, `45%`, `Step 100/1000`, etc.
3. **Reports** at milestone percentages (10%, 20%, ... 100%) with metrics and elapsed time
4. **Falls back** to time-based reporting when no progress pattern is detected
5. **On exit**: reports success with final stats, or crash details (signal, exit code, last 10 lines)

### Notification Examples

```
📊 MNIST Training — 50% (15/30) · 2.1min
Epoch 15/30 | loss: 0.4493 | accuracy: 92.5%

✅ MNIST Training finished (30/30) in 4.2min

❌ Big Model killed by SIGKILL (9) (likely OOM) after 12.3min
```

### Cross-compilation

```bash
rustup target add x86_64-unknown-linux-gnu
sudo apt install gcc-x86-64-linux-gnu
CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_LINKER=x86_64-linux-gnu-gcc \
  cargo build --release --target x86_64-unknown-linux-gnu
```

## Agent Skill

See [SKILL.md](SKILL.md) for the agent-side monitoring approach. Instead of wrapping commands, the OpenClaw agent monitors jobs externally — tailing logs, analyzing plots with vision, and making decisions about when to stop or continue.

Best for:
- Jobs running on remote nodes (SSH + SCP)
- Complex convergence decisions that need visual analysis
- Multi-node fleet monitoring
- Tasks where you want human-like judgment, not just pattern matching

## Requirements

- [OpenClaw](https://github.com/openclaw/openclaw) installed and configured
- A configured messaging channel (Discord, Telegram, Slack, etc.)

## License

MIT
