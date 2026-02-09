use std::env;
use std::io::{BufRead, BufReader};
use std::process::{Command, ExitCode, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

// ---------------------------------------------------------------------------
// LLM-powered progress parsing
// ---------------------------------------------------------------------------

/// Structured progress extracted by the LLM.
#[derive(Clone, Debug)]
struct Progress {
    percent: f64,
    current: Option<f64>,
    total: Option<f64>,
    summary: String,
}

/// Ask the LLM to parse progress from an output chunk.
/// Returns None if parsing fails or no progress is detectable.
fn llm_parse_progress(output_chunk: &str, label: &str) -> Option<Progress> {
    let truncated: String = output_chunk
        .lines()
        .rev()
        .take(50)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect::<Vec<_>>()
        .join("\n");

    if truncated.trim().is_empty() {
        return None;
    }

    let prompt = format!(
        r#"You are a progress parser. Analyze this process output and extract progress information.

Process label: "{label}"

Output (last 50 lines):
```
{truncated}
```

Respond with ONLY a JSON object (no markdown, no explanation):
{{"percent": <0-100 or null if unknown>, "current": <current step or null>, "total": <total steps or null>, "summary": "<1-line status summary with key metrics>"}}

Rules:
- percent: estimate completion 0-100. Use step/total if available, or infer from context. null if truly unknown.
- current/total: extract if there's a clear X/Y pattern (epochs, steps, batches, files, etc). null otherwise.
- summary: concise 1-line status. Include key metrics (loss, accuracy, speed, ETA) if visible.
- If output shows an error or crash, set percent to null and describe the error in summary.
- If output has no discernible progress, set percent to null."#
    );

    // Use openclaw sessions spawn for LLM access
    let result = Command::new("openclaw")
        .args([
            "sessions", "spawn",
            "--task", &prompt,
            "--cleanup", "delete",
        ])
        .output()
        .ok()?;

    let raw = String::from_utf8_lossy(&result.stdout).trim().to_string();

    // Extract JSON from response (handle potential markdown wrapping)
    let json_str = if let Some(start) = raw.find('{') {
        if let Some(end) = raw.rfind('}') {
            &raw[start..=end]
        } else {
            return None;
        }
    } else {
        return None;
    };

    // Simple JSON parsing (avoid adding serde dependency)
    parse_progress_json(json_str)
}

/// Parse the JSON response manually (no serde needed).
fn parse_progress_json(json: &str) -> Option<Progress> {
    let get_f64 = |key: &str| -> Option<f64> {
        let pattern = format!("\"{}\"", key);
        let idx = json.find(&pattern)?;
        let after_key = &json[idx + pattern.len()..];
        let colon = after_key.find(':')?;
        let after_colon = after_key[colon + 1..].trim_start();
        if after_colon.starts_with("null") {
            return None;
        }
        // Extract number
        let num_end = after_colon
            .find(|c: char| !c.is_ascii_digit() && c != '.' && c != '-')
            .unwrap_or(after_colon.len());
        after_colon[..num_end].trim().parse().ok()
    };

    let get_string = |key: &str| -> Option<String> {
        let pattern = format!("\"{}\"", key);
        let idx = json.find(&pattern)?;
        let after_key = &json[idx + pattern.len()..];
        let colon = after_key.find(':')?;
        let after_colon = after_key[colon + 1..].trim_start();
        if !after_colon.starts_with('"') {
            return None;
        }
        let content = &after_colon[1..];
        // Find closing quote (handle escaped quotes)
        let mut end = 0;
        let mut escaped = false;
        for (i, c) in content.char_indices() {
            if escaped {
                escaped = false;
                continue;
            }
            if c == '\\' {
                escaped = true;
                continue;
            }
            if c == '"' {
                end = i;
                break;
            }
        }
        Some(content[..end].replace("\\\"", "\"").replace("\\n", " "))
    };

    let percent = get_f64("percent");
    let summary = get_string("summary").unwrap_or_default();

    // If no percent and no summary, parsing failed
    if percent.is_none() && summary.is_empty() {
        return None;
    }

    Some(Progress {
        percent: percent.unwrap_or(-1.0),
        current: get_f64("current"),
        total: get_f64("total"),
        summary,
    })
}

/// Fast regex-based fallback when LLM is unavailable.
fn regex_parse_progress(line: &str) -> Option<Progress> {
    // Pattern: X/Y
    let re_xy = regex_lite::Regex::new(
        r"(?i)(?:epoch|step|batch|iter|iteration|sample|chunk|file|item)?\s*(\d+)\s*/\s*(\d+)"
    ).ok()?;
    if let Some(caps) = re_xy.captures(line) {
        let current: f64 = caps[1].parse().ok()?;
        let total: f64 = caps[2].parse().ok()?;
        if total > 0.0 {
            return Some(Progress {
                percent: (current / total) * 100.0,
                current: Some(current),
                total: Some(total),
                summary: line.trim().to_string(),
            });
        }
    }

    // Pattern: N%
    let re_pct = regex_lite::Regex::new(r"(\d+(?:\.\d+)?)\s*%").ok()?;
    if let Some(caps) = re_pct.captures(line) {
        let pct: f64 = caps[1].parse().ok()?;
        if (0.0..=100.0).contains(&pct) {
            return Some(Progress {
                percent: pct,
                current: None,
                total: None,
                summary: line.trim().to_string(),
            });
        }
    }

    None
}

// ---------------------------------------------------------------------------
// Milestone logic
// ---------------------------------------------------------------------------

fn milestone_thresholds() -> Vec<f64> {
    (1..=10).map(|i| i as f64 * 10.0).collect()
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn env_or(name: &str, fallback: &str) -> String {
    env::var(name).unwrap_or_else(|_| fallback.to_string())
}

/// Send a message via OpenClaw CLI. Non-blocking (fire-and-forget thread).
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
    thread::spawn(move || {
        let _ = Command::new(&args[0]).args(&args[1..]).output();
    });
}

fn signal_name(code: i32) -> String {
    match code {
        1 => "SIGHUP".into(), 2 => "SIGINT".into(), 6 => "SIGABRT".into(),
        9 => "SIGKILL".into(), 11 => "SIGSEGV".into(), 15 => "SIGTERM".into(),
        n => format!("signal {n}"),
    }
}

fn elapsed_str(d: Duration) -> String {
    let secs = d.as_secs_f64();
    if secs >= 3600.0 { format!("{:.1}h", secs / 3600.0) }
    else if secs >= 60.0 { format!("{:.1}min", secs / 60.0) }
    else { format!("{:.0}s", secs) }
}

fn tail_lines(text: &str, n: usize) -> String {
    let lines: Vec<&str> = text.lines().collect();
    let start = lines.len().saturating_sub(n);
    lines[start..].join("\n")
}

// ---------------------------------------------------------------------------
// Core
// ---------------------------------------------------------------------------

struct Config {
    channel: String,
    target: String,
    label: String,
    parse_interval_sec: u64,   // how often to send output to LLM for parsing
    fallback_interval_sec: u64, // time-based fallback when LLM returns no progress
    plot_path: Option<String>,
    use_llm: bool,
}

struct SharedState {
    output_buf: String,
    last_parsed_len: usize,
    last_reported_milestone: f64,
    latest_progress: Option<Progress>,
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
        last_parsed_len: 0,
        last_reported_milestone: 0.0,
        latest_progress: None,
    }));
    let child_done = Arc::new(Mutex::new(false));
    let start = Instant::now();

    // --- stdout reader ---
    let stdout = child.stdout.take().unwrap();
    let state_out = Arc::clone(&state);
    let stdout_handle = thread::spawn(move || {
        let reader = BufReader::new(stdout);
        for line in reader.lines().flatten() {
            println!("{}", line);
            let mut s = state_out.lock().unwrap();
            s.output_buf.push_str(&line);
            s.output_buf.push('\n');
        }
    });

    // --- stderr reader ---
    let stderr = child.stderr.take().unwrap();
    let state_err = Arc::clone(&state);
    let stderr_handle = thread::spawn(move || {
        let reader = BufReader::new(stderr);
        for line in reader.lines().flatten() {
            eprintln!("{}", line);
            let mut s = state_err.lock().unwrap();
            s.output_buf.push_str(&line);
            s.output_buf.push('\n');
        }
    });

    // --- LLM parsing + notification thread ---
    let state_parse = Arc::clone(&state);
    let child_done_parse = Arc::clone(&child_done);
    let parse_interval = config.parse_interval_sec;
    let fallback_interval = config.fallback_interval_sec;
    let channel = config.channel.clone();
    let target = config.target.clone();
    let label = config.label.clone();
    let plot = config.plot_path.clone();
    let use_llm = config.use_llm;
    let thresholds = milestone_thresholds();

    let _parse_handle = thread::spawn(move || {
        let mut last_fallback_send = Instant::now();

        loop {
            thread::sleep(Duration::from_secs(parse_interval));

            if *child_done_parse.lock().unwrap() {
                break;
            }

            // Grab new output since last parse
            let (new_output, label_clone) = {
                let mut s = state_parse.lock().unwrap();
                let new = s.output_buf[s.last_parsed_len..].to_string();
                s.last_parsed_len = s.output_buf.len();
                (new, label.clone())
            };

            if new_output.trim().is_empty() {
                continue;
            }

            // Try LLM parse first, fall back to regex
            let progress = if use_llm {
                llm_parse_progress(&new_output, &label_clone)
                    .or_else(|| {
                        // Regex fallback on last line
                        new_output.lines().rev()
                            .find_map(|l| regex_parse_progress(l))
                    })
            } else {
                new_output.lines().rev()
                    .find_map(|l| regex_parse_progress(l))
            };

            let mut s = state_parse.lock().unwrap();

            if let Some(ref p) = progress {
                s.latest_progress = Some(p.clone());

                if p.percent >= 0.0 {
                    // Check milestones
                    for &t in &thresholds {
                        if t > s.last_reported_milestone && p.percent >= t {
                            s.last_reported_milestone = t;
                            let elapsed = elapsed_str(start.elapsed());
                            let step_info = match (p.current, p.total) {
                                (Some(c), Some(t)) => format!(" ({}/{})", c, t),
                                _ => String::new(),
                            };
                            let msg = format!(
                                "📊 **{}** — {:.0}%{} · {}\n{}",
                                label, p.percent, step_info, elapsed, p.summary
                            );
                            openclaw_send(&channel, &target, &msg, plot.as_deref());
                            last_fallback_send = Instant::now();
                            break;
                        }
                    }
                } else {
                    // LLM returned progress with summary but no percent
                    // Use time-based with the summary
                    if last_fallback_send.elapsed() >= Duration::from_secs(fallback_interval) {
                        let elapsed = elapsed_str(start.elapsed());
                        let msg = format!("📊 **{}** · {}\n{}", label, elapsed, p.summary);
                        openclaw_send(&channel, &target, &msg, plot.as_deref());
                        last_fallback_send = Instant::now();
                    }
                }
            } else {
                // No progress detected — time-based fallback with raw tail
                if last_fallback_send.elapsed() >= Duration::from_secs(fallback_interval) {
                    let elapsed = elapsed_str(start.elapsed());
                    let tail = tail_lines(&new_output, 5);
                    let msg = format!("📊 **{}** · {}\n```\n{}\n```", label, elapsed, tail);
                    openclaw_send(&channel, &target, &msg, plot.as_deref());
                    last_fallback_send = Instant::now();
                }
            }
        }
    });

    // --- Wait for child (event-based) ---
    let status = child.wait().expect("failed to wait on child");
    *child_done.lock().unwrap() = true;

    let _ = stdout_handle.join();
    let _ = stderr_handle.join();

    // Small delay to let any in-flight sends complete
    thread::sleep(Duration::from_millis(500));

    let elapsed = elapsed_str(start.elapsed());
    let s = state.lock().unwrap();
    let exit_code;

    if let Some(code) = status.code() {
        if code == 0 {
            let progress_info = s.latest_progress.as_ref()
                .and_then(|p| match (p.current, p.total) {
                    (Some(c), Some(t)) => Some(format!(" ({}/{})", c, t)),
                    _ => None,
                })
                .unwrap_or_default();
            let summary = s.latest_progress.as_ref()
                .map(|p| format!("\n{}", p.summary))
                .unwrap_or_default();
            let msg = format!("✅ **{}** finished{} in {}{}", config.label, progress_info, elapsed, summary);
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

    // Let send threads finish
    thread::sleep(Duration::from_secs(2));
    ExitCode::from(exit_code)
}

// ---------------------------------------------------------------------------
// CLI
// ---------------------------------------------------------------------------

fn print_usage() {
    eprintln!("Usage: ocnotify [OPTIONS] -- <command> [args...]");
    eprintln!();
    eprintln!("Wraps any command, monitors its output with an LLM, and sends");
    eprintln!("smart progress notifications via OpenClaw.");
    eprintln!();
    eprintln!("The LLM reads the process output and extracts progress, metrics,");
    eprintln!("and status summaries. Reports are sent at percentage milestones");
    eprintln!("(10%, 20%, ...) or on a time interval as fallback.");
    eprintln!();
    eprintln!("Options:");
    eprintln!("  --label <name>       Label for notifications (default: command name)");
    eprintln!("  --parse-every <sec>  How often to send output to LLM (default: 10)");
    eprintln!("  --fallback <secs>    Time-based fallback interval (default: 300)");
    eprintln!("  --plot <path>        Attach plot image to reports");
    eprintln!("  --no-llm             Disable LLM; use regex-only parsing");
    eprintln!("  --channel <ch>       OpenClaw channel (or OPENCLAW_PROGRESS_CHANNEL)");
    eprintln!("  --target <tgt>       OpenClaw target (or OPENCLAW_PROGRESS_TARGET)");
    eprintln!();
    eprintln!("Environment:");
    eprintln!("  OPENCLAW_PROGRESS_CHANNEL       Default channel");
    eprintln!("  OPENCLAW_PROGRESS_TARGET        Default target");
    eprintln!("  OPENCLAW_PROGRESS_PARSE_SEC     Default LLM parse interval");
    eprintln!("  OPENCLAW_PROGRESS_FALLBACK_SEC  Default fallback interval");
    eprintln!();
    eprintln!("Examples:");
    eprintln!("  ocnotify --label 'MNIST' --plot loss.png -- python train.py");
    eprintln!("  ocnotify -- make -j4");
    eprintln!("  ocnotify --no-llm -- cargo build --release");
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
    let mut parse_interval: Option<u64> = None;
    let mut fallback_interval: Option<u64> = None;
    let mut plot_path: Option<String> = None;
    let mut use_llm = true;
    let mut channel: Option<String> = None;
    let mut target: Option<String> = None;

    let mut i = 0;
    while i < opts.len() {
        match opts[i].as_str() {
            "--label" => { i += 1; label = opts.get(i).cloned(); }
            "--parse-every" => { i += 1; parse_interval = opts.get(i).and_then(|v| v.parse().ok()); }
            "--fallback" => { i += 1; fallback_interval = opts.get(i).and_then(|v| v.parse().ok()); }
            "--plot" => { i += 1; plot_path = opts.get(i).cloned(); }
            "--no-llm" => { use_llm = false; }
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
        parse_interval_sec: parse_interval.unwrap_or_else(|| env_or("OPENCLAW_PROGRESS_PARSE_SEC", "10").parse().unwrap_or(10)),
        fallback_interval_sec: fallback_interval.unwrap_or_else(|| env_or("OPENCLAW_PROGRESS_FALLBACK_SEC", "300").parse().unwrap_or(300)),
        plot_path,
        use_llm,
    };

    if config.channel.is_empty() || config.target.is_empty() {
        eprintln!("ocnotify: channel and target required (use --channel/--target or env vars)");
        return ExitCode::from(1);
    }

    run_wrap(config, child_argv)
}
