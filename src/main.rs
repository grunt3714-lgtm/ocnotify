use std::env;
use std::io::{BufRead, BufReader};
use std::process::{Command, ExitCode, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

// ---------------------------------------------------------------------------
// Progress detection
// ---------------------------------------------------------------------------

/// Parsed progress from a single output line.
#[derive(Clone, Debug)]
struct Progress {
    current: f64,
    total: f64,
    percent: f64,
    raw_line: String,
}

/// Try to extract progress from a line.  Recognises:
///   - "Epoch 5/30", "Step 100/1000", "Batch 3/50", etc.  (X/Y pattern)
///   - "45%", "45.2%", "[=====>    ] 45%"                  (percentage)
///   - "progress: 0.45"                                     (fraction 0-1)
fn parse_progress(line: &str) -> Option<Progress> {
    // Pattern 1: X/Y  (with optional keyword prefix)
    // Matches: "Epoch 5/30", "5/30", "Step 100/1000", "Batch 3 / 50"
    let re_xy = regex_lite::Regex::new(
        r"(?i)(?:epoch|step|batch|iter|iteration|sample|chunk|file|item)?\s*(\d+)\s*/\s*(\d+)"
    ).ok()?;
    if let Some(caps) = re_xy.captures(line) {
        let current: f64 = caps[1].parse().ok()?;
        let total: f64 = caps[2].parse().ok()?;
        if total > 0.0 {
            return Some(Progress {
                current,
                total,
                percent: (current / total) * 100.0,
                raw_line: line.to_string(),
            });
        }
    }

    // Pattern 2: N%
    let re_pct = regex_lite::Regex::new(r"(\d+(?:\.\d+)?)\s*%").ok()?;
    if let Some(caps) = re_pct.captures(line) {
        let pct: f64 = caps[1].parse().ok()?;
        if (0.0..=100.0).contains(&pct) {
            return Some(Progress {
                current: pct,
                total: 100.0,
                percent: pct,
                raw_line: line.to_string(),
            });
        }
    }

    // Pattern 3: "progress: 0.XX" (fraction between 0 and 1)
    let re_frac = regex_lite::Regex::new(r"(?i)progress\s*[:=]\s*(0\.\d+|1\.0*)").ok()?;
    if let Some(caps) = re_frac.captures(line) {
        let frac: f64 = caps[1].parse().ok()?;
        if (0.0..=1.0).contains(&frac) {
            return Some(Progress {
                current: frac * 100.0,
                total: 100.0,
                percent: frac * 100.0,
                raw_line: line.to_string(),
            });
        }
    }

    None
}

/// Decides which percentage milestones to report at, based on total steps.
/// More steps → fewer reports.  Returns a set of percentage thresholds.
fn milestone_thresholds(total: f64) -> Vec<f64> {
    if total <= 5.0 {
        // Very few steps: report every step (20% increments)
        vec![20.0, 40.0, 60.0, 80.0, 100.0]
    } else if total <= 20.0 {
        // Report ~every 25%
        vec![25.0, 50.0, 75.0, 100.0]
    } else if total <= 100.0 {
        // Report every 10%
        (1..=10).map(|i| i as f64 * 10.0).collect()
    } else {
        // Many steps: every 10%
        (1..=10).map(|i| i as f64 * 10.0).collect()
    }
}

// ---------------------------------------------------------------------------
// Shared state for the monitoring thread
// ---------------------------------------------------------------------------

struct SharedState {
    output_buf: String,
    latest_progress: Option<Progress>,
    last_reported_milestone: f64,  // last % milestone we reported at
    progress_detected: bool,
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn env_or(name: &str, fallback: &str) -> String {
    env::var(name).unwrap_or_else(|_| fallback.to_string())
}

/// Send a message via OpenClaw CLI.  Always non-blocking (spawns a thread).
fn openclaw_send(channel: &str, target: &str, message: &str, media: Option<&str>) {
    let mut args = vec![
        "openclaw".to_string(), "message".to_string(), "send".to_string(),
        "--channel".to_string(), channel.to_string(),
        "--target".to_string(), target.to_string(),
        "--message".to_string(), message.to_string(),
    ];
    if let Some(path) = media {
        if std::path::Path::new(path).exists() {
            args.push("--media".to_string());
            args.push(path.to_string());
        }
    }
    // Fire-and-forget in a background thread so we never block the reader
    thread::spawn(move || {
        let _ = Command::new(&args[0]).args(&args[1..]).output();
    });
}

fn signal_name(code: i32) -> String {
    match code {
        1 => "SIGHUP".into(),
        2 => "SIGINT".into(),
        6 => "SIGABRT".into(),
        9 => "SIGKILL".into(),
        11 => "SIGSEGV".into(),
        15 => "SIGTERM".into(),
        n => format!("signal {n}"),
    }
}

fn elapsed_str(d: Duration) -> String {
    let secs = d.as_secs_f64();
    if secs >= 3600.0 {
        format!("{:.1}h", secs / 3600.0)
    } else if secs >= 60.0 {
        format!("{:.1}min", secs / 60.0)
    } else {
        format!("{:.0}s", secs)
    }
}

fn tail_lines(text: &str, n: usize) -> String {
    let lines: Vec<&str> = text.lines().collect();
    let start = lines.len().saturating_sub(n);
    lines[start..].join("\n")
}

// ---------------------------------------------------------------------------
// Core wrap logic
// ---------------------------------------------------------------------------

struct Config {
    channel: String,
    target: String,
    label: String,
    interval_sec: u64,
    plot_path: Option<String>,
    summary: bool,
    milestones: bool,  // auto milestone-based reporting
}

fn run_wrap(config: Config, child_argv: Vec<String>) -> ExitCode {
    if child_argv.is_empty() {
        eprintln!("ocnotify: nothing to run");
        return ExitCode::from(1);
    }

    let mut child = match Command::new(&child_argv[0])
        .args(&child_argv[1..])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(c) => c,
        Err(e) => {
            let msg = format!("❌ **{}** — failed to launch `{}`: {}", config.label, child_argv[0], e);
            openclaw_send(&config.channel, &config.target, &msg, None);
            return ExitCode::from(1);
        }
    };

    let state = Arc::new(Mutex::new(SharedState {
        output_buf: String::new(),
        latest_progress: None,
        last_reported_milestone: 0.0,
        progress_detected: false,
    }));
    let start = Instant::now();

    // --- stdout reader ---
    let stdout = child.stdout.take().unwrap();
    let state_stdout = Arc::clone(&state);
    let milestones_enabled = config.milestones;
    let channel_m = config.channel.clone();
    let target_m = config.target.clone();
    let label_m = config.label.clone();
    let plot_m = config.plot_path.clone();
    let start_m = start;

    let stdout_handle = thread::spawn(move || {
        let reader = BufReader::new(stdout);
        let mut thresholds: Option<Vec<f64>> = None;

        for line in reader.lines().flatten() {
            println!("{}", line);

            let progress = parse_progress(&line);

            let mut s = state_stdout.lock().unwrap();
            s.output_buf.push_str(&line);
            s.output_buf.push('\n');

            if let Some(ref p) = progress {
                s.latest_progress = Some(p.clone());

                if milestones_enabled && !s.progress_detected {
                    s.progress_detected = true;
                    // First time we see progress — compute milestones
                    thresholds = Some(milestone_thresholds(p.total));
                }

                // Check if we crossed a milestone
                if milestones_enabled {
                    if let Some(ref ts) = thresholds {
                        for &t in ts {
                            if t > s.last_reported_milestone && p.percent >= t {
                                s.last_reported_milestone = t;
                                let elapsed = elapsed_str(start_m.elapsed());
                                let msg = format!(
                                    "📊 **{}** — {:.0}% ({}/{}) · {}\n`{}`",
                                    label_m, p.percent, p.current, p.total, elapsed,
                                    // Last meaningful line
                                    line.trim()
                                );
                                openclaw_send(&channel_m, &target_m, &msg, plot_m.as_deref());
                                break;
                            }
                        }
                    }
                }
            }
        }
    });

    // --- stderr reader ---
    let stderr = child.stderr.take().unwrap();
    let state_stderr = Arc::clone(&state);
    let stderr_handle = thread::spawn(move || {
        let reader = BufReader::new(stderr);
        for line in reader.lines().flatten() {
            eprintln!("{}", line);
            let mut s = state_stderr.lock().unwrap();
            s.output_buf.push_str(&line);
            s.output_buf.push('\n');
        }
    });

    // --- Time-based periodic thread (fallback when no progress detected) ---
    let state_periodic = Arc::clone(&state);
    let interval = config.interval_sec;
    let channel_p = config.channel.clone();
    let target_p = config.target.clone();
    let label_p = config.label.clone();
    let plot_p = config.plot_path.clone();
    let do_summary = config.summary;

    let _periodic_handle = if interval > 0 {
        Some(thread::spawn(move || {
            let mut last_len = 0usize;
            loop {
                thread::sleep(Duration::from_secs(interval));
                let s = state_periodic.lock().unwrap();

                // Skip time-based reports if milestone-based is active
                if s.progress_detected {
                    drop(s);
                    continue;
                }

                let current_len = s.output_buf.len();
                if current_len > last_len {
                    let new_output = s.output_buf[last_len..].to_string();
                    last_len = current_len;
                    drop(s);

                    if do_summary {
                        // LLM summary
                        let truncated: String = new_output.chars().rev().take(3000)
                            .collect::<String>().chars().rev().collect();
                        let prompt = format!(
                            "Summarize this process output concisely (1-3 lines) for a Discord status update. \
                             Process label: \"{}\". Focus on progress indicators, errors, or key metrics. \
                             Just output the summary, nothing else.\n\n```\n{}\n```",
                            label_p, truncated
                        );
                        let result = Command::new("openclaw")
                            .args(["sessions", "spawn", "--task", &prompt, "--cleanup", "delete"])
                            .output();
                        let summary = result.ok()
                            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
                            .unwrap_or_default();
                        let msg = if !summary.is_empty() {
                            format!("📊 **{}**\n{}", label_p, summary)
                        } else {
                            format!("📊 **{}**\n```\n{}\n```", label_p, tail_lines(&new_output, 5))
                        };
                        openclaw_send(&channel_p, &target_p, &msg, plot_p.as_deref());
                    } else {
                        let msg = format!("📊 **{}**\n```\n{}\n```", label_p, tail_lines(&new_output, 10));
                        openclaw_send(&channel_p, &target_p, &msg, plot_p.as_deref());
                    }
                } else {
                    drop(s);
                }
            }
        }))
    } else {
        None
    };

    // --- Wait for child (event-based) ---
    let status = child.wait().expect("failed to wait on child");
    let _ = stdout_handle.join();
    let _ = stderr_handle.join();

    let elapsed = elapsed_str(start.elapsed());
    let s = state.lock().unwrap();
    let exit_code;

    if let Some(code) = status.code() {
        if code == 0 {
            let progress_info = s.latest_progress.as_ref()
                .map(|p| format!(" ({}/{})", p.current, p.total))
                .unwrap_or_default();
            let msg = format!("✅ **{}** finished{} in {}", config.label, progress_info, elapsed);
            openclaw_send(&config.channel, &config.target, &msg, config.plot_path.as_deref());
            exit_code = 0;
        } else {
            let tail = tail_lines(&s.output_buf, 10);
            let msg = format!(
                "❌ **{}** exited with code {} after {}\n```\n{}\n```",
                config.label, code, elapsed, tail
            );
            openclaw_send(&config.channel, &config.target, &msg, None);
            exit_code = code as u8;
        }
    } else {
        #[cfg(unix)]
        {
            use std::os::unix::process::ExitStatusExt;
            let sig = status.signal().unwrap_or(0);
            let sig_name = signal_name(sig);
            let hint = if sig == 9 { " (likely OOM)" } else { "" };
            let tail = tail_lines(&s.output_buf, 10);
            let msg = format!(
                "❌ **{}** killed by {} ({}){} after {}\n```\n{}\n```",
                config.label, sig_name, sig, hint, elapsed, tail
            );
            openclaw_send(&config.channel, &config.target, &msg, None);
            exit_code = (128 + sig) as u8;
        }
        #[cfg(not(unix))]
        {
            let msg = format!("❌ **{}** killed after {}", config.label, elapsed);
            openclaw_send(&config.channel, &config.target, &msg, None);
            exit_code = 1;
        }
    };

    ExitCode::from(exit_code)
}

// ---------------------------------------------------------------------------
// CLI
// ---------------------------------------------------------------------------

fn print_usage() {
    eprintln!("Usage: ocnotify [OPTIONS] -- <command> [args...]");
    eprintln!();
    eprintln!("Wraps any command, monitors its output, and sends progress");
    eprintln!("notifications to Discord/Telegram/etc. via OpenClaw.");
    eprintln!();
    eprintln!("Progress detection is automatic: if the output contains patterns");
    eprintln!("like 'Epoch 5/30', '45%', or 'Step 100/1000', reports are sent");
    eprintln!("at percentage milestones (10%, 20%, ...). Otherwise falls back");
    eprintln!("to time-based reporting (default: every 5 min).");
    eprintln!();
    eprintln!("Options:");
    eprintln!("  --label <name>       Label for notifications (default: command name)");
    eprintln!("  --interval <secs>    Time-based fallback interval, 0=off (default: 300)");
    eprintln!("  --plot <path>        Attach plot image to milestone reports");
    eprintln!("  --no-milestones      Disable auto progress detection; use time-based only");
    eprintln!("  --no-summary         Send raw output tail instead of LLM summary");
    eprintln!("  --channel <ch>       OpenClaw channel (or OPENCLAW_PROGRESS_CHANNEL)");
    eprintln!("  --target <tgt>       OpenClaw target (or OPENCLAW_PROGRESS_TARGET)");
    eprintln!();
    eprintln!("Environment:");
    eprintln!("  OPENCLAW_PROGRESS_CHANNEL       Default channel");
    eprintln!("  OPENCLAW_PROGRESS_TARGET        Default target");
    eprintln!("  OPENCLAW_PROGRESS_INTERVAL_SEC  Default fallback interval");
    eprintln!();
    eprintln!("Examples:");
    eprintln!("  ocnotify --label 'MNIST' --plot loss.png -- python train.py");
    eprintln!("  ocnotify -- make -j4");
    eprintln!("  ocnotify --label 'Backup' --no-milestones --interval 60 -- rsync -avz /src /dst");
}

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();

    if args.is_empty() || args.contains(&"--help".to_string()) || args.contains(&"-h".to_string()) {
        print_usage();
        return ExitCode::from(if args.is_empty() { 1 } else { 0 });
    }

    let separator = args.iter().position(|a| a == "--");
    let (opts, child_argv): (Vec<String>, Vec<String>) = match separator {
        Some(pos) => (args[..pos].to_vec(), args[pos + 1..].to_vec()),
        None => (vec![], args),
    };

    let mut label: Option<String> = None;
    let mut interval: Option<u64> = None;
    let mut plot_path: Option<String> = None;
    let mut summary = true;
    let mut milestones = true;
    let mut channel: Option<String> = None;
    let mut target: Option<String> = None;

    let mut i = 0;
    while i < opts.len() {
        match opts[i].as_str() {
            "--label" => { i += 1; label = opts.get(i).cloned(); }
            "--interval" => { i += 1; interval = opts.get(i).and_then(|v| v.parse().ok()); }
            "--plot" => { i += 1; plot_path = opts.get(i).cloned(); }
            "--no-summary" => { summary = false; }
            "--no-milestones" => { milestones = false; }
            "--channel" => { i += 1; channel = opts.get(i).cloned(); }
            "--target" => { i += 1; target = opts.get(i).cloned(); }
            _ => {
                eprintln!("Unknown option: {}", opts[i]);
                print_usage();
                return ExitCode::from(1);
            }
        }
        i += 1;
    }

    let resolved_label = label.unwrap_or_else(|| {
        child_argv.first()
            .map(|s| std::path::Path::new(s).file_name().unwrap_or_default().to_string_lossy().to_string())
            .unwrap_or_else(|| "process".to_string())
    });

    let config = Config {
        channel: channel.unwrap_or_else(|| env_or("OPENCLAW_PROGRESS_CHANNEL", &env_or("OPENCLAW_DEFAULT_DM_CHANNEL", ""))),
        target: target.unwrap_or_else(|| env_or("OPENCLAW_PROGRESS_TARGET", &env_or("OPENCLAW_DEFAULT_DM_TARGET", ""))),
        label: resolved_label,
        interval_sec: interval.unwrap_or_else(|| env_or("OPENCLAW_PROGRESS_INTERVAL_SEC", "300").parse().unwrap_or(300)),
        plot_path,
        summary,
        milestones,
    };

    if config.channel.is_empty() || config.target.is_empty() {
        eprintln!("ocnotify: channel and target required (use --channel/--target or env vars)");
        return ExitCode::from(1);
    }

    run_wrap(config, child_argv)
}
