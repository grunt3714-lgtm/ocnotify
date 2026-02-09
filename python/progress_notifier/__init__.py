#!/usr/bin/env python3
import json
import os
import shutil
import subprocess
import sys
import time
import urllib.error
import urllib.request


def _env(name, default=None):
    v = os.environ.get(name)
    return v if v is not None and v != "" else default


class ProgressNotifier:
    """Send periodic progress notifications via OpenClaw.

    Delivery methods (tried in order):
    1. ``openclaw message send`` CLI (works on gateway or any machine with
       openclaw configured — run ``openclaw doctor --fix`` first).
    2. HTTP POST to the gateway API (set ``OPENCLAW_GATEWAY_URL`` and
       ``OPENCLAW_GATEWAY_TOKEN``).  Useful on nodes/remotes that don't have
       channel config.
    3. Silent no-op if neither method is available.

    A startup self-test runs on first ``send_now`` / ``maybe_send`` call and
    prints a warning to stderr if delivery will fail.
    """

    def __init__(self, interval_sec: int = 300, plot_interval_sec: int | None = None):
        self.interval_sec = interval_sec
        self.plot_interval_sec = plot_interval_sec if plot_interval_sec is not None else interval_sec
        self.last_sent = 0.0
        self.last_plot_sent = 0.0
        self.channel = _env("OPENCLAW_PROGRESS_CHANNEL") or _env("OPENCLAW_DEFAULT_DM_CHANNEL")
        self.target = _env("OPENCLAW_PROGRESS_TARGET") or _env("OPENCLAW_DEFAULT_DM_TARGET")

        # HTTP fallback config
        self.gateway_url = _env("OPENCLAW_GATEWAY_URL")  # e.g. http://192.168.1.94:18789
        self.gateway_token = _env("OPENCLAW_GATEWAY_TOKEN")

        # Delivery state
        self._tested = False
        self._use_http = False  # True = skip CLI, use HTTP fallback
        self._cli_available = shutil.which("openclaw") is not None

    def _selftest(self):
        """Run once: verify that at least one delivery method works."""
        if self._tested:
            return
        self._tested = True

        if not self.channel or not self.target:
            print(
                "[progress_notifier] WARNING: No channel/target configured. "
                "Set OPENCLAW_PROGRESS_CHANNEL and OPENCLAW_PROGRESS_TARGET. "
                "Notifications will be silent.",
                file=sys.stderr, flush=True,
            )
            return

        # Try CLI first
        if self._cli_available:
            try:
                r = subprocess.run(
                    ["openclaw", "message", "send", "--channel", self.channel,
                     "--target", self.target, "--message", "🔔 Progress notifier connected.", "--dry-run"],
                    capture_output=True, text=True, timeout=10,
                )
                if r.returncode == 0:
                    return  # CLI works
                # CLI failed — check if it's a channel config issue
                if "Unknown channel" in (r.stderr + r.stdout):
                    print(
                        f"[progress_notifier] WARNING: openclaw CLI found but channel "
                        f"'{self.channel}' not configured on this machine.\n"
                        f"  Fix: run 'openclaw doctor --fix' or set OPENCLAW_GATEWAY_URL "
                        f"for HTTP fallback.",
                        file=sys.stderr, flush=True,
                    )
                else:
                    print(
                        f"[progress_notifier] WARNING: openclaw CLI dry-run failed: "
                        f"{(r.stderr or r.stdout)[:200]}",
                        file=sys.stderr, flush=True,
                    )
            except Exception as exc:
                print(f"[progress_notifier] WARNING: openclaw CLI test failed: {exc}",
                      file=sys.stderr, flush=True)

            # CLI didn't work — try HTTP fallback
            self._use_http = True

        if not self._cli_available:
            self._use_http = True

        if self._use_http:
            if self.gateway_url and self.gateway_token:
                print(
                    f"[progress_notifier] Using HTTP fallback → {self.gateway_url}",
                    file=sys.stderr, flush=True,
                )
            else:
                print(
                    "[progress_notifier] WARNING: openclaw CLI not available/configured "
                    "and no OPENCLAW_GATEWAY_URL set. Notifications will be silent.\n"
                    "  To fix on a node/remote machine, either:\n"
                    "    1. Install openclaw (npm i -g openclaw) and run 'openclaw doctor --fix'\n"
                    "    2. Set OPENCLAW_GATEWAY_URL and OPENCLAW_GATEWAY_TOKEN env vars",
                    file=sys.stderr, flush=True,
                )

    def _send_http(self, message: str, media_path: str | None = None):
        """Send via gateway HTTP API."""
        if not self.gateway_url or not self.gateway_token:
            return
        url = f"{self.gateway_url.rstrip('/')}/api/v1/message/send"
        payload = {
            "channel": self.channel,
            "target": self.target,
            "message": message,
        }
        if media_path and os.path.isfile(media_path):
            payload["mediaPath"] = os.path.abspath(media_path)
        data = json.dumps(payload).encode()
        req = urllib.request.Request(
            url, data=data,
            headers={
                "Content-Type": "application/json",
                "Authorization": f"Bearer {self.gateway_token}",
            },
        )
        try:
            urllib.request.urlopen(req, timeout=15)
        except Exception:
            pass  # best-effort

    def _send_message(self, args: list[str], media_path: str | None = None):
        self._selftest()
        if self._use_http:
            # Extract message from args
            msg = ""
            for i, a in enumerate(args):
                if a == "--message" and i + 1 < len(args):
                    msg = args[i + 1]
                    break
            self._send_http(msg, media_path=media_path)
            return
        try:
            subprocess.run(args, check=False)
        except Exception:
            pass

    def send_now(self, message: str, plot_path: str | None = None):
        if not self.channel or not self.target:
            return
        cmd = [
            "openclaw", "message", "send",
            "--channel", self.channel,
            "--target", self.target,
            "--message", message,
        ]
        if plot_path:
            cmd += ["--media", plot_path]
        self._send_message(cmd, media_path=plot_path)

    def maybe_send(self, message: str, plot_path: str | None = None):
        if not self.channel or not self.target:
            return
        now = time.time()
        send_text = now - self.last_sent >= self.interval_sec
        send_plot = plot_path and now - self.last_plot_sent >= self.plot_interval_sec

        if send_text and not send_plot:
            self.last_sent = now
            self._send_message([
                "openclaw", "message", "send",
                "--channel", self.channel,
                "--target", self.target,
                "--message", message,
            ])
        elif send_text and send_plot:
            self.last_sent = now
            self.last_plot_sent = now
            self._send_message([
                "openclaw", "message", "send",
                "--channel", self.channel,
                "--target", self.target,
                "--message", message,
                "--media", plot_path,
            ], media_path=plot_path)
        elif send_plot:
            self.last_plot_sent = now
            self._send_message([
                "openclaw", "message", "send",
                "--channel", self.channel,
                "--target", self.target,
                "--message", message,
                "--media", plot_path,
            ], media_path=plot_path)


def main():
    if len(sys.argv) < 2:
        print("Usage: progress_notifier.py <message> [--plot /path/to.png]", file=sys.stderr)
        sys.exit(1)

    plot_path = None
    args = sys.argv[1:]
    if "--plot" in args:
        idx = args.index("--plot")
        if idx + 1 >= len(args):
            print("--plot requires a file path", file=sys.stderr)
            sys.exit(1)
        plot_path = args[idx + 1]
        args = args[:idx] + args[idx + 2 :]

    message = " ".join(args)
    notifier = ProgressNotifier(
        interval_sec=int(_env("OPENCLAW_PROGRESS_INTERVAL_SEC", "300")),
        plot_interval_sec=int(
            _env("OPENCLAW_PROGRESS_PLOT_INTERVAL_SEC", _env("OPENCLAW_PROGRESS_INTERVAL_SEC", "300"))
        ),
    )
    notifier.maybe_send(message, plot_path=plot_path)


if __name__ == "__main__":
    main()
