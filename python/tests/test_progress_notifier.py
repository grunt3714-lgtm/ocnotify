"""Unit and regression tests for ProgressNotifier."""

import os
import sys
import time
import unittest
from unittest.mock import patch, call

# Add scripts dir to path so we can import the module directly
sys.path.insert(0, os.path.join(os.path.dirname(__file__), "..", "scripts"))

from progress_notifier import ProgressNotifier, _env


class TestEnvHelper(unittest.TestCase):
    def test_env_returns_value(self):
        with patch.dict(os.environ, {"TEST_VAR": "hello"}):
            self.assertEqual(_env("TEST_VAR"), "hello")

    def test_env_returns_default_when_missing(self):
        os.environ.pop("MISSING_VAR", None)
        self.assertEqual(_env("MISSING_VAR", "fallback"), "fallback")

    def test_env_returns_default_when_empty(self):
        with patch.dict(os.environ, {"EMPTY_VAR": ""}):
            self.assertEqual(_env("EMPTY_VAR", "fallback"), "fallback")

    def test_env_returns_none_when_missing_no_default(self):
        os.environ.pop("MISSING_VAR", None)
        self.assertIsNone(_env("MISSING_VAR"))


class TestNotifierInit(unittest.TestCase):
    def test_defaults(self):
        with patch.dict(os.environ, {}, clear=True):
            n = ProgressNotifier(interval_sec=60)
            self.assertEqual(n.interval_sec, 60)
            self.assertEqual(n.plot_interval_sec, 60)
            self.assertIsNone(n.channel)
            self.assertIsNone(n.target)

    def test_reads_env_vars(self):
        env = {
            "OPENCLAW_PROGRESS_CHANNEL": "discord",
            "OPENCLAW_PROGRESS_TARGET": "channel:123",
        }
        with patch.dict(os.environ, env, clear=True):
            n = ProgressNotifier()
            self.assertEqual(n.channel, "discord")
            self.assertEqual(n.target, "channel:123")

    def test_falls_back_to_default_dm(self):
        env = {
            "OPENCLAW_DEFAULT_DM_CHANNEL": "telegram",
            "OPENCLAW_DEFAULT_DM_TARGET": "user:456",
        }
        with patch.dict(os.environ, env, clear=True):
            n = ProgressNotifier()
            self.assertEqual(n.channel, "telegram")
            self.assertEqual(n.target, "user:456")

    def test_explicit_overrides_default(self):
        env = {
            "OPENCLAW_PROGRESS_CHANNEL": "discord",
            "OPENCLAW_PROGRESS_TARGET": "channel:123",
            "OPENCLAW_DEFAULT_DM_CHANNEL": "telegram",
            "OPENCLAW_DEFAULT_DM_TARGET": "user:456",
        }
        with patch.dict(os.environ, env, clear=True):
            n = ProgressNotifier()
            self.assertEqual(n.channel, "discord")
            self.assertEqual(n.target, "channel:123")

    def test_custom_plot_interval(self):
        n = ProgressNotifier(interval_sec=60, plot_interval_sec=120)
        self.assertEqual(n.plot_interval_sec, 120)


class TestNotifierNoop(unittest.TestCase):
    """When channel/target are not configured, nothing should happen."""

    @patch("progress_notifier.subprocess.run")
    def test_send_now_noop_without_config(self, mock_run):
        with patch.dict(os.environ, {}, clear=True):
            n = ProgressNotifier()
            n.send_now("hello")
            mock_run.assert_not_called()

    @patch("progress_notifier.subprocess.run")
    def test_maybe_send_noop_without_config(self, mock_run):
        with patch.dict(os.environ, {}, clear=True):
            n = ProgressNotifier()
            n.maybe_send("hello")
            mock_run.assert_not_called()


class TestSendNow(unittest.TestCase):
    @patch("progress_notifier.subprocess.run")
    def test_send_now_calls_openclaw(self, mock_run):
        env = {
            "OPENCLAW_PROGRESS_CHANNEL": "discord",
            "OPENCLAW_PROGRESS_TARGET": "channel:123",
        }
        with patch.dict(os.environ, env, clear=True):
            n = ProgressNotifier()
            n.send_now("test message")
            mock_run.assert_called_once()
            args = mock_run.call_args[0][0]
            self.assertEqual(args[0], "openclaw")
            self.assertIn("--message", args)
            self.assertIn("test message", args)
            self.assertIn("--channel", args)
            self.assertIn("discord", args)

    @patch("progress_notifier.subprocess.run")
    def test_send_now_with_plot(self, mock_run):
        env = {
            "OPENCLAW_PROGRESS_CHANNEL": "discord",
            "OPENCLAW_PROGRESS_TARGET": "channel:123",
        }
        with patch.dict(os.environ, env, clear=True):
            n = ProgressNotifier()
            n.send_now("msg", plot_path="/tmp/plot.png")
            # Single call with both message and media
            self.assertEqual(mock_run.call_count, 1)
            args = mock_run.call_args[0][0]
            self.assertIn("--media", args)
            self.assertIn("/tmp/plot.png", args)
            self.assertIn("--message", args)


class TestMaybeSend(unittest.TestCase):
    @patch("progress_notifier.subprocess.run")
    def test_respects_interval(self, mock_run):
        env = {
            "OPENCLAW_PROGRESS_CHANNEL": "discord",
            "OPENCLAW_PROGRESS_TARGET": "channel:123",
        }
        with patch.dict(os.environ, env, clear=True):
            n = ProgressNotifier(interval_sec=9999)
            n.maybe_send("first")
            n.maybe_send("second")  # should be skipped
            # Only the first message + no plots = 1 call
            self.assertEqual(mock_run.call_count, 1)

    @patch("progress_notifier.subprocess.run")
    def test_sends_after_interval_expires(self, mock_run):
        env = {
            "OPENCLAW_PROGRESS_CHANNEL": "discord",
            "OPENCLAW_PROGRESS_TARGET": "channel:123",
        }
        with patch.dict(os.environ, env, clear=True):
            n = ProgressNotifier(interval_sec=0)  # no throttle
            n.maybe_send("first")
            n.maybe_send("second")
            self.assertEqual(mock_run.call_count, 2)

    @patch("progress_notifier.subprocess.run")
    def test_plot_throttled_separately(self, mock_run):
        env = {
            "OPENCLAW_PROGRESS_CHANNEL": "discord",
            "OPENCLAW_PROGRESS_TARGET": "channel:123",
        }
        with patch.dict(os.environ, env, clear=True):
            n = ProgressNotifier(interval_sec=0, plot_interval_sec=9999)
            n.maybe_send("first", plot_path="/tmp/p.png")
            n.maybe_send("second", plot_path="/tmp/p.png")
            # 2 calls: first has media, second text-only (plot throttled)
            self.assertEqual(mock_run.call_count, 2)
            first_args = mock_run.call_args_list[0][0][0]
            second_args = mock_run.call_args_list[1][0][0]
            self.assertIn("--media", first_args)
            self.assertNotIn("--media", second_args)


class TestSubprocessFailure(unittest.TestCase):
    """Notifier should never raise even if subprocess fails."""

    @patch("progress_notifier.subprocess.run", side_effect=FileNotFoundError("no openclaw"))
    def test_send_now_swallows_exception(self, mock_run):
        env = {
            "OPENCLAW_PROGRESS_CHANNEL": "discord",
            "OPENCLAW_PROGRESS_TARGET": "channel:123",
        }
        with patch.dict(os.environ, env, clear=True):
            n = ProgressNotifier()
            # Should not raise
            n.send_now("hello")

    @patch("progress_notifier.subprocess.run", side_effect=OSError("broken"))
    def test_maybe_send_swallows_exception(self, mock_run):
        env = {
            "OPENCLAW_PROGRESS_CHANNEL": "discord",
            "OPENCLAW_PROGRESS_TARGET": "channel:123",
        }
        with patch.dict(os.environ, env, clear=True):
            n = ProgressNotifier(interval_sec=0)
            n.maybe_send("hello")


class TestWrap(unittest.TestCase):
    """Tests for the wrap() subprocess monitor."""

    ENV = {
        "OPENCLAW_PROGRESS_CHANNEL": "discord",
        "OPENCLAW_PROGRESS_TARGET": "channel:123",
    }

    @patch("progress_notifier.subprocess.run")
    def test_wrap_success_reports(self, mock_run):
        from progress_notifier import wrap

        mock_run.return_value = type("R", (), {"returncode": 0})()
        with patch.dict(os.environ, self.ENV, clear=True):
            code = wrap(["echo", "hi"], label="test-job")
        self.assertEqual(code, 0)
        # First call is subprocess.run(["echo", "hi"]), second is the openclaw message send
        self.assertEqual(mock_run.call_args_list[0][0][0], ["echo", "hi"])

    @patch("progress_notifier.subprocess.run")
    def test_wrap_nonzero_exit(self, mock_run):
        from progress_notifier import wrap

        mock_run.return_value = type("R", (), {"returncode": 42})()
        with patch.dict(os.environ, self.ENV, clear=True):
            code = wrap(["false"])
        self.assertEqual(code, 42)

    @patch("progress_notifier.subprocess.run")
    def test_wrap_signal_kill(self, mock_run):
        from progress_notifier import wrap

        mock_run.return_value = type("R", (), {"returncode": -9})()
        with patch.dict(os.environ, self.ENV, clear=True):
            code = wrap(["train.py"], label="mnist")
        self.assertEqual(code, 137)  # 128 + 9

    def test_wrap_empty_argv(self):
        from progress_notifier import wrap
        code = wrap([])
        self.assertEqual(code, 1)

    @patch("progress_notifier.subprocess.run", side_effect=FileNotFoundError("nope"))
    def test_wrap_command_not_found(self, mock_run):
        from progress_notifier import wrap
        with patch.dict(os.environ, self.ENV, clear=True):
            code = wrap(["nonexistent_cmd"])
        self.assertEqual(code, 127)


if __name__ == "__main__":
    unittest.main()


# ---------------------------------------------------------------------------
# Channel-aware formatting tests
# ---------------------------------------------------------------------------

class TestFormatBatchSummary:
    def test_rich_format_discord(self):
        from progress_notifier import format_batch_summary
        jobs = [
            {"Mode": "Vanilla", "Epochs": "100", "Seed": "0"},
            {"Mode": "Regen", "Epochs": "20 gen", "Seed": "42"},
        ]
        result = format_batch_summary("Test batch", jobs, channel="discord")
        assert "🚀 **Test batch**" in result
        assert "▸" in result
        assert "|" not in result  # no table pipes

    def test_table_format_telegram(self):
        from progress_notifier import format_batch_summary
        jobs = [
            {"Mode": "Vanilla", "Epochs": "100", "Seed": "0"},
            {"Mode": "Regen", "Epochs": "20 gen", "Seed": "42"},
        ]
        result = format_batch_summary("Test batch", jobs, channel="telegram")
        assert "| Mode |" in result
        assert "| --- |" in result
        assert "| Vanilla |" in result

    def test_plain_format_signal(self):
        from progress_notifier import format_batch_summary
        jobs = [
            {"Mode": "Vanilla", "Epochs": "100"},
        ]
        result = format_batch_summary("Test batch", jobs, channel="signal")
        assert "[Test batch]" in result
        assert "**" not in result  # no markdown bold

    def test_footer_included(self):
        from progress_notifier import format_batch_summary
        jobs = [{"Mode": "Vanilla"}]
        result = format_batch_summary("Title", jobs, footer="lr=0.002", channel="discord")
        assert "lr=0.002" in result


class TestFormatGrid:
    def test_grid_discord(self):
        from progress_notifier import format_grid
        rows = [["Vanilla", "30.12"], ["Regen", "0.83"]]
        headers = ["Mode", "LSR"]
        result = format_grid(rows, headers=headers, channel="discord")
        assert "**Mode:**" in result
        assert "▸" in result

    def test_grid_telegram(self):
        from progress_notifier import format_grid
        rows = [["Vanilla", "30.12"], ["Regen", "0.83"]]
        headers = ["Mode", "LSR"]
        result = format_grid(rows, headers=headers, channel="telegram")
        assert "| Mode | LSR |" in result
        assert "| Vanilla | 30.12 |" in result

    def test_grid_plain(self):
        from progress_notifier import format_grid
        rows = [["Vanilla", "30.12"], ["Regen", "0.83"]]
        headers = ["Mode", "LSR"]
        result = format_grid(rows, headers=headers, channel="signal")
        assert "**" not in result
        assert "|" not in result


class TestChannelDetection:
    def test_known_channels(self):
        from progress_notifier import _detect_channel_type
        assert _detect_channel_type("telegram") == "table"
        assert _detect_channel_type("discord") == "rich"
        assert _detect_channel_type("slack") == "rich"
        assert _detect_channel_type("whatsapp") == "rich"
        assert _detect_channel_type("signal") == "plain"
        assert _detect_channel_type("imessage") == "plain"

    def test_unknown_defaults_rich(self):
        from progress_notifier import _detect_channel_type
        assert _detect_channel_type("somenewthing") == "rich"
        assert _detect_channel_type(None) == "rich"
