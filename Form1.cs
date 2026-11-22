using System;
using System.Drawing;
using System.IO;
using System.Text.Json;
using System.Threading;
using System.Windows.Forms;

namespace NeoYTLiveStats
{
    public class Form1 : Form
    {
        private enum FlashMode
        {
            None,
            WhiteFlash,
            Rainbow
        }

        private string _handle;
        private string _videoId;

        private long _subsFlashStep;
        private long _vidFlashStep;
        private long _chanFlashStep;

        private readonly System.Windows.Forms.Timer _pollTimer = new() { Interval = 1000 };
        private readonly System.Windows.Forms.Timer _animTimer = new() { Interval = 50 };
        private readonly CancellationTokenSource _cts = new();

        private Label _lblSubs = null!;
        private Label _lblChanViews = null!;
        private Label _lblVidViews = null!;
        private Label _lblStatus = null!;
        private Label _lblLive = null!;
        private Label _lblTitle = null!;
        private Panel _root = null!;

        private long? _baselineSubs;
        private long? _baselineVidViews;
        private long? _baselineChanViews;
        private long _subsFlashArmed;
        private long _vidFlashArmed;
        private long _chanFlashArmed;

        private long? _lastSubs;
        private long? _lastChanViews;
        private long? _lastVidViews;
        private long? _serverChanViews;
        private long? _vidViewsAtServerSync;

        private DateTime _lastVidAt = DateTime.MinValue;
        private DateTime _lastChanAt = DateTime.MinValue;

        private int _pollTick;
        private int _vidOk;
        private int _chanOk;

        private string _chanSource = "";
        private string _vidSource = "";
        private string _chanError = "";
        private string _vidError = "";
        private bool _statusError;

        private const int StaleAfterSeconds = 15;
        private static readonly Color FreshVidColor = Color.FromArgb(77, 166, 255);
        private static readonly Color StaleStatColor = Color.FromArgb(110, 110, 130);

        private Label? _pulseLabel;
        private int _pulseTicks;
        private Color _pulseRestore = Color.White;

        private FlashMode _flash;
        private int _flashTick;

        private readonly Color _bg = Color.FromArgb(8, 8, 12);
        private readonly Color[] _rainbow =
        {
            Color.FromArgb(255, 60, 60),
            Color.FromArgb(255, 160, 40),
            Color.FromArgb(255, 230, 60),
            Color.FromArgb(60, 220, 90),
            Color.FromArgb(60, 160, 255),
            Color.FromArgb(140, 80, 255),
            Color.FromArgb(255, 80, 200)
        };

        private int _busyVid;
        private int _busyChan;
        private System.ComponentModel.IContainer? components;

        public Form1(AppSettings settings, bool fullscreen)
        {
            settings = settings.Normalized();
            _handle = settings.Handle;
            _videoId = settings.VideoId;
            _subsFlashStep = settings.SubsFlashStep;
            _vidFlashStep = settings.VideoViewsFlashStep;
            _chanFlashStep = settings.ChannelViewsFlashStep;

            InitializeComponent();
            BuildUi();

            KeyPreview = true;
            KeyDown += (_, e) =>
            {
                if (e.KeyCode == Keys.Escape)
                {
                    FormBorderStyle = FormBorderStyle.Sizable;
                    WindowState = FormWindowState.Normal;
                }
                if (e.KeyCode == Keys.F11)
                {
                    ToggleFullscreen();
                }
                if (e.KeyCode == Keys.F2)
                {
                    OpenSettings();
                }
            };

            if (fullscreen)
            {
                Shown += (_, _) => BeginInvoke(new Action(EnterFullscreen));
            }

            _pollTimer.Tick += (_, _) => OnPollTick();
            _animTimer.Tick += (_, _) => Animate();
            _animTimer.Start();

            Load += (_, _) =>
            {
                OnPollTick();
                _pollTimer.Start();
            };

            FormClosed += (_, _) =>
            {
                _cts.Cancel();
                _pollTimer.Stop();
                _animTimer.Stop();
            };
        }

        private void InitializeComponent()
        {
            components = new System.ComponentModel.Container();
            AutoScaleMode = AutoScaleMode.Font;
        }

        protected override void Dispose(bool disposing)
        {
            if (disposing)
            {
                components?.Dispose();
                _cts.Dispose();
            }
            base.Dispose(disposing);
        }

        private void BuildUi()
        {
            Text = "NeoYT Live Stats";
            BackColor = _bg;
            ForeColor = Color.White;
            MinimumSize = new Size(900, 500);
            StartPosition = FormStartPosition.CenterScreen;
            Size = new Size(1280, 720);
            DoubleBuffered = true;

            _root = new Panel
            {
                Dock = DockStyle.Fill,
                BackColor = _bg,
                Padding = new Padding(36)
            };
            Controls.Add(_root);

            var table = new TableLayoutPanel
            {
                Dock = DockStyle.Fill,
                ColumnCount = 3,
                RowCount = 3,
                BackColor = _bg
            };
            table.ColumnStyles.Add(new ColumnStyle(SizeType.Percent, 33.33f));
            table.ColumnStyles.Add(new ColumnStyle(SizeType.Percent, 33.33f));
            table.ColumnStyles.Add(new ColumnStyle(SizeType.Percent, 33.33f));
            table.RowStyles.Add(new RowStyle(SizeType.Absolute, 40f));
            table.RowStyles.Add(new RowStyle(SizeType.Percent, 100f));
            table.RowStyles.Add(new RowStyle(SizeType.Absolute, 28f));
            _root.Controls.Add(table);

            _lblTitle = new Label
            {
                Text = TitleText(),
                Dock = DockStyle.Fill,
                Font = new Font("Segoe UI Semibold", 16f),
                ForeColor = Color.FromArgb(255, 77, 109),
                TextAlign = ContentAlignment.MiddleLeft
            };
            table.Controls.Add(_lblTitle, 0, 0);

            var settingsButton = new Button
            {
                Text = "\u2699",
                Dock = DockStyle.Right,
                Width = 40,
                FlatStyle = FlatStyle.Flat,
                BackColor = Color.FromArgb(16, 16, 24),
                ForeColor = Color.FromArgb(180, 180, 200),
                Font = new Font("Segoe UI Symbol", 14f),
                TabStop = false
            };
            settingsButton.FlatAppearance.BorderSize = 0;
            settingsButton.Click += (_, _) => OpenSettings();
            table.Controls.Add(settingsButton, 1, 0);

            _lblLive = new Label
            {
                Dock = DockStyle.Fill,
                Font = new Font("Segoe UI Semibold", 12f),
                ForeColor = Color.FromArgb(80, 220, 120),
                TextAlign = ContentAlignment.MiddleRight,
                Text = "\u25cf LIVE"
            };
            table.Controls.Add(_lblLive, 2, 0);

            _lblSubs = MakeBig("\u2014", Color.White);
            _lblChanViews = MakeBig("\u2014", Color.White);
            _lblVidViews = MakeBig("\u2014", Color.FromArgb(77, 166, 255));

            table.Controls.Add(WrapStat(_lblSubs, "SUBS"), 0, 1);
            table.Controls.Add(WrapStat(_lblChanViews, "KANAAL VIEWS"), 1, 1);
            table.Controls.Add(WrapStat(_lblVidViews, "VIDEO VIEWS"), 2, 1);

            _lblStatus = new Label
            {
                Dock = DockStyle.Fill,
                Font = new Font("Segoe UI", 10f),
                ForeColor = Color.FromArgb(90, 90, 110),
                TextAlign = ContentAlignment.MiddleLeft,
                Text = "Laden\u2026"
            };
            table.Controls.Add(_lblStatus, 0, 2);
            table.SetColumnSpan(_lblStatus, 3);
        }

        private static Label MakeBig(string text, Color fg) => new()
        {
            Dock = DockStyle.Fill,
            Font = new Font("Segoe UI Black", 56f, FontStyle.Bold),
            ForeColor = fg,
            TextAlign = ContentAlignment.MiddleCenter,
            Text = text
        };

        private static Control WrapStat(Label value, string caption)
        {
            var panel = new Panel
            {
                Dock = DockStyle.Fill,
                Padding = new Padding(10),
                BackColor = Color.FromArgb(16, 16, 24)
            };
            var label = new Label
            {
                Dock = DockStyle.Top,
                Height = 32,
                Text = caption,
                Font = new Font("Segoe UI Semibold", 13f),
                ForeColor = Color.FromArgb(140, 140, 160),
                TextAlign = ContentAlignment.BottomCenter
            };
            value.Dock = DockStyle.Fill;
            panel.Controls.Add(value);
            panel.Controls.Add(label);
            return panel;
        }

        private void EnterFullscreen()
        {
            FormBorderStyle = FormBorderStyle.None;
            WindowState = FormWindowState.Maximized;
            Bounds = Screen.PrimaryScreen?.Bounds ?? Bounds;
            TopMost = true;
            Activate();
            BeginInvoke(new Action(() => TopMost = false));
        }

        private void ToggleFullscreen()
        {
            if (FormBorderStyle == FormBorderStyle.None)
            {
                FormBorderStyle = FormBorderStyle.Sizable;
                WindowState = FormWindowState.Normal;
            }
            else
            {
                EnterFullscreen();
            }
        }

        private void OnPollTick()
        {
            _pollTick++;

            if (Interlocked.CompareExchange(ref _busyVid, 1, 0) == 0)
            {
                _ = PollVideoAsync();
            }
            if (Interlocked.CompareExchange(ref _busyChan, 1, 0) == 0)
            {
                _ = PollChannelAsync();
            }

            UpdateLiveBadge();
        }

        private async System.Threading.Tasks.Task PollVideoAsync()
        {
            try
            {
                var vid = await YtApi.FetchVideoAsync(_videoId, _cts.Token);
                if (!IsDisposed)
                {
                    BeginInvoke(new Action(() => ApplyVideo(vid)));
                }
            }
            catch (Exception ex)
            {
                if (!IsDisposed)
                {
                    BeginInvoke(new Action(() => _lblStatus.Text = "Video: " + ex.Message));
                }
            }
            finally
            {
                Interlocked.Exchange(ref _busyVid, 0);
            }
        }

        private async System.Threading.Tasks.Task PollChannelAsync()
        {
            try
            {
                var ch = await YtApi.FetchChannelAsync(_handle, _cts.Token);
                if (!IsDisposed)
                {
                    BeginInvoke(new Action(() => ApplyChannel(ch)));
                }
            }
            catch (Exception ex)
            {
                if (!IsDisposed)
                {
                    BeginInvoke(new Action(() => _lblStatus.Text = "Kanaal: " + ex.Message));
                }
            }
            finally
            {
                Interlocked.Exchange(ref _busyChan, 0);
            }
        }

        private void ApplyVideo(YtApi.VideoStats vid)
        {
            _vidSource = vid.Source;
            _vidError = vid.Error;

            if (vid.Views.HasValue)
            {
                _vidOk++;
                _lastVidAt = DateTime.Now;

                string text = YtApi.Fmt(vid.Views);
                bool up = _lastVidViews.HasValue && vid.Views > _lastVidViews;

                if (_lblVidViews.Text != text)
                {
                    _lblVidViews.Text = text;
                    _lblVidViews.Invalidate();
                }
                if (up)
                {
                    Pulse(_lblVidViews, FreshVidColor);
                }
                if (!_lastVidViews.HasValue || vid.Views != _lastVidViews)
                {
                    TrackVidViews(vid.Views.Value);
                }

                _lastVidViews = vid.Views;
                RefreshChannelViewsFromVideo(up);
                _statusError = false;
            }
            else
            {
                // Keep the last known number instead of blanking it; it dims once it goes stale.
                _lblStatus.Text = "Video: " + (string.IsNullOrEmpty(vid.Error)
                    ? "geen views in het antwoord"
                    : vid.Error);
                _statusError = true;
            }

            WriteProof();
            UpdateLiveBadge();
        }

        private void ApplyChannel(YtApi.ChannelStats ch)
        {
            _chanSource = ch.Source;
            _chanError = ch.Error;

            if (ch.Subscribers.HasValue)
            {
                _chanOk++;
                _lastChanAt = DateTime.Now;

                string text = YtApi.Fmt(ch.Subscribers);
                bool up = _lastSubs.HasValue && ch.Subscribers > _lastSubs;

                if (_lblSubs.Text != text)
                {
                    _lblSubs.Text = text;
                    _lblSubs.Invalidate();
                }
                if (up)
                {
                    Pulse(_lblSubs, Color.White);
                }
                if (!_lastSubs.HasValue || ch.Subscribers != _lastSubs)
                {
                    TrackSubs(ch.Subscribers.Value);
                }

                _lastSubs = ch.Subscribers;
            }

            if (ch.Views.HasValue)
            {
                // Channel views come from the about page only once in a while, so
                // they are synced to a baseline and then advanced by video views.
                bool firstSync = !_serverChanViews.HasValue;
                bool serverMoved = !firstSync && ch.Views.Value > _serverChanViews!.Value;
                if (firstSync || serverMoved)
                {
                    _serverChanViews = ch.Views.Value;
                    _vidViewsAtServerSync = _lastVidViews;
                }
                RefreshChannelViewsFromVideo(pulseIfUp: false);
            }

            if (!string.IsNullOrEmpty(ch.Error))
            {
                _lblStatus.Text = ch.Error;
                _statusError = true;
            }
            else if (ch.Subscribers.HasValue)
            {
                _statusError = false;
            }
            else if (!_lastSubs.HasValue && !_serverChanViews.HasValue)
            {
                _lblStatus.Text = "Kanaalcijfers ontbreken";
                _statusError = true;
            }

            WriteProof();
            UpdateLiveBadge();
        }

        private void RefreshChannelViewsFromVideo(bool pulseIfUp)
        {
            if (!_serverChanViews.HasValue)
            {
                return;
            }

            long delta = 0;
            if (_vidViewsAtServerSync.HasValue && _lastVidViews.HasValue
                && _lastVidViews.Value > _vidViewsAtServerSync.Value)
            {
                delta = _lastVidViews.Value - _vidViewsAtServerSync.Value;
            }

            long total = _serverChanViews.Value + delta;
            string text = YtApi.Fmt(total);
            bool up = _lastChanViews.HasValue && total > _lastChanViews.Value;

            if (_lblChanViews.Text != text)
            {
                _lblChanViews.Text = text;
                _lblChanViews.Refresh();
            }
            if (up || (pulseIfUp && delta > 0))
            {
                Pulse(_lblChanViews, Color.White);
            }
            if (!_lastChanViews.HasValue || total != _lastChanViews)
            {
                TrackChanViews(total);
            }

            _lastChanViews = total;
        }

        private void TrackSubs(long current)
        {
            if (!_baselineSubs.HasValue)
            {
                _baselineSubs = current;
                _subsFlashArmed = _subsFlashStep;
                return;
            }
            while (current - _baselineSubs.Value >= _subsFlashArmed)
            {
                TriggerWhiteFlash();
                _subsFlashArmed += _subsFlashStep;
            }
        }

        private void TrackVidViews(long current)
        {
            if (!_baselineVidViews.HasValue)
            {
                _baselineVidViews = current;
                _vidFlashArmed = _vidFlashStep;
                return;
            }
            while (current - _baselineVidViews.Value >= _vidFlashArmed)
            {
                TriggerRainbow();
                _vidFlashArmed += _vidFlashStep;
            }
        }

        private void TrackChanViews(long current)
        {
            if (!_baselineChanViews.HasValue)
            {
                _baselineChanViews = current;
                _chanFlashArmed = _chanFlashStep;
                return;
            }
            while (current - _baselineChanViews.Value >= _chanFlashArmed)
            {
                TriggerRainbow();
                _chanFlashArmed += _chanFlashStep;
            }
        }

        private void UpdateLiveBadge()
        {
            int vidAge = AgeSeconds(_lastVidAt);
            int chanAge = AgeSeconds(_lastChanAt);
            bool stale = IsStale(_lastVidAt) || IsStale(_lastChanAt);

            _lblLive.Text = $"\u25cf LIVE  {_pollTick}  v{vidAge}s c{chanAge}s" + (stale ? "  \u00b7 stale" : "");
            _lblLive.ForeColor = _pollTick % 2 == 0
                ? Color.FromArgb(80, 220, 120)
                : Color.FromArgb(40, 160, 80);

            ApplyStaleness();

            if (!_statusError)
            {
                _lblStatus.Text = $"@{_handle} | {_videoId} | src {ShortSource(_chanSource)}/{ShortSource(_vidSource)} " +
                                  $"| ok v{_vidOk}/c{_chanOk} | 1s";
            }
        }

        /// <summary>Dims numbers that stopped updating, so an old value never looks fresh.</summary>
        private void ApplyStaleness()
        {
            if (_flash != FlashMode.None || _pulseTicks > 0)
            {
                return;
            }

            _lblSubs.ForeColor = IsStale(_lastChanAt) ? StaleStatColor : Color.White;
            _lblChanViews.ForeColor = IsStale(_lastChanAt) ? StaleStatColor : Color.White;
            _lblVidViews.ForeColor = IsStale(_lastVidAt) ? StaleStatColor : FreshVidColor;
        }

        private static bool IsStale(DateTime at) => at == DateTime.MinValue
            || (DateTime.Now - at).TotalSeconds > StaleAfterSeconds;

        private static int AgeSeconds(DateTime at) => at == DateTime.MinValue
            ? -1
            : (int)(DateTime.Now - at).TotalSeconds;

        private static string ShortSource(string source) => string.IsNullOrEmpty(source)
            ? "-"
            : source.Replace("innertube-", "it-");

        private void Pulse(Label lbl, Color baseColor)
        {
            _pulseLabel = lbl;
            _pulseRestore = baseColor;
            _pulseTicks = 12;
            lbl.ForeColor = Color.FromArgb(255, 255, 120);
        }

        private string TitleText() => "@" + _handle + "  \u00b7  Can-Caneurysm";

        private void OpenSettings()
        {
            var current = new AppSettings
            {
                Handle = _handle,
                VideoId = _videoId,
                SubsFlashStep = _subsFlashStep,
                VideoViewsFlashStep = _vidFlashStep,
                ChannelViewsFlashStep = _chanFlashStep
            };

            using var dlg = new SettingsForm(current);
            if (dlg.ShowDialog(this) == DialogResult.OK)
            {
                ApplySettings(dlg.Result);
            }
        }

        private void ApplySettings(AppSettings settings)
        {
            settings = settings.Normalized();
            bool targetsChanged = !string.Equals(_handle, settings.Handle, StringComparison.OrdinalIgnoreCase)
                                  || !string.Equals(_videoId, settings.VideoId, StringComparison.Ordinal);

            _handle = settings.Handle;
            _videoId = settings.VideoId;
            _subsFlashStep = settings.SubsFlashStep;
            _vidFlashStep = settings.VideoViewsFlashStep;
            _chanFlashStep = settings.ChannelViewsFlashStep;

            settings.Save();
            _lblTitle.Text = TitleText();

            if (targetsChanged)
            {
                ResetTracking();
                _statusError = false;
                _chanSource = "";
                _vidSource = "";
                _chanError = "";
                _vidError = "";
                _lblStatus.Text = "Laden\u2026";
                _lblStatus.ForeColor = Color.FromArgb(90, 90, 110);
                _lastVidAt = DateTime.MinValue;
                _lastChanAt = DateTime.MinValue;
                OnPollTick();
            }
        }

        /// <summary>Clears the baselines so the new channel/video starts from a clean slate.</summary>
        private void ResetTracking()
        {
            _baselineSubs = null;
            _baselineVidViews = null;
            _baselineChanViews = null;
            _subsFlashArmed = 0;
            _vidFlashArmed = 0;
            _chanFlashArmed = 0;
            _lastSubs = null;
            _lastChanViews = null;
            _lastVidViews = null;
            _serverChanViews = null;
            _vidViewsAtServerSync = null;
            _lblSubs.Text = "\u2014";
            _lblChanViews.Text = "\u2014";
            _lblVidViews.Text = "\u2014";
        }

        /// <summary>Dumps the current numbers to %TEMP%\neoyt_last.json for inspection.</summary>
        private void WriteProof()
        {
            try
            {
                File.WriteAllText(Path.Combine(Path.GetTempPath(), "neoyt_last.json"), JsonSerializer.Serialize(new
                {
                    pollTick = _pollTick,
                    vidOk = _vidOk,
                    chanOk = _chanOk,
                    subs = _lastSubs,
                    channelViews = _lastChanViews,
                    vidViews = _lastVidViews,
                    uiSubs = _lblSubs.Text,
                    uiChanViews = _lblChanViews.Text,
                    uiVidViews = _lblVidViews.Text,
                    chanSource = _chanSource,
                    vidSource = _vidSource,
                    chanError = _chanError,
                    vidError = _vidError,
                    stale = IsStale(_lastVidAt) || IsStale(_lastChanAt),
                    serverChan = _serverChanViews,
                    vidAtSync = _vidViewsAtServerSync,
                    at = DateTime.Now.ToString("o")
                }));
            }
            catch
            {
                // proof output is best effort only
            }
        }

        private void TriggerWhiteFlash()
        {
            if (_flash != FlashMode.Rainbow)
            {
                _flash = FlashMode.WhiteFlash;
                _flashTick = 0;
            }
        }

        private void TriggerRainbow()
        {
            _flash = FlashMode.Rainbow;
            _flashTick = 0;
        }

        private void Animate()
        {
            if (_pulseTicks > 0 && _pulseLabel != null)
            {
                _pulseTicks--;
                if (_pulseTicks == 0)
                {
                    _pulseLabel.ForeColor = _pulseRestore;
                }
            }

            if (_flash == FlashMode.None)
            {
                return;
            }

            if (_flash == FlashMode.WhiteFlash)
            {
                if (_flashTick / 4 >= 5)
                {
                    EndFlash();
                    return;
                }
                bool on = _flashTick / 2 % 2 == 0;
                ApplyFlashColor(on ? Color.White : _bg, on ? Color.Black : Color.White);
                _flashTick++;
            }
            else if (_flash == FlashMode.Rainbow)
            {
                if (_flashTick >= 40)
                {
                    EndFlash();
                    return;
                }
                ApplyFlashColor(_rainbow[_flashTick % _rainbow.Length], Color.White);
                _flashTick++;
            }
        }

        private void ApplyFlashColor(Color bg, Color fg)
        {
            BackColor = bg;
            _root.BackColor = bg;
            foreach (Control c in _root.Controls)
            {
                Tint(c, bg, fg);
            }
        }

        private void Tint(Control c, Color bg, Color fg)
        {
            c.BackColor = bg;
            if (c is Label label && (label == _lblSubs || label == _lblChanViews || label == _lblVidViews))
            {
                label.ForeColor = fg;
            }
            foreach (Control child in c.Controls)
            {
                Tint(child, bg, fg);
            }
        }

        private void EndFlash()
        {
            _flash = FlashMode.None;
            _flashTick = 0;

            BackColor = _bg;
            _root.BackColor = _bg;
            RestoreColors(_root);

            _lblSubs.ForeColor = Color.White;
            _lblChanViews.ForeColor = Color.White;
            _lblVidViews.ForeColor = Color.FromArgb(77, 166, 255);
            _lblStatus.ForeColor = Color.FromArgb(90, 90, 110);
            _lblLive.ForeColor = Color.FromArgb(80, 220, 120);
        }

        private void RestoreColors(Control c)
        {
            if (c is Panel)
            {
                c.BackColor = c == _root ? _bg : Color.FromArgb(16, 16, 24);
            }
            foreach (Control child in c.Controls)
            {
                RestoreColors(child);
            }
        }
    }
}
