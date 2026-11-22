using System;
using System.Collections.Generic;
using System.Drawing;
using System.Globalization;
using System.Windows.Forms;

namespace NeoYTLiveStats
{
    /// <summary>Small modal dialog for the channel, the video and the flash thresholds.</summary>
    internal sealed class SettingsForm : Form
    {
        private readonly TextBox _handleBox = new();
        private readonly TextBox _videoBox = new();
        private readonly NumericUpDown _subsStep = new();
        private readonly NumericUpDown _videoStep = new();
        private readonly NumericUpDown _chanStep = new();
        private readonly Label _error = new();
        private readonly Label _testResult = new();
        private readonly Button _testButton = new();

        public AppSettings Result { get; private set; }

        public SettingsForm(AppSettings current)
        {
            Result = current.Clone().Normalized();
            Icon = AppIcon.Get();
            BuildUi();
        }

        private void BuildUi()
        {
            Text = "Instellingen";
            FormBorderStyle = FormBorderStyle.FixedDialog;
            StartPosition = FormStartPosition.CenterParent;
            MaximizeBox = false;
            MinimizeBox = false;
            ShowInTaskbar = false;
            BackColor = Color.FromArgb(8, 8, 12);
            ForeColor = Color.White;
            Font = new Font("Segoe UI", 10f);
            ClientSize = new Size(560, 376);

            var table = new TableLayoutPanel
            {
                Dock = DockStyle.Fill,
                ColumnCount = 2,
                RowCount = 7,
                Padding = new Padding(20),
                BackColor = BackColor
            };
            table.ColumnStyles.Add(new ColumnStyle(SizeType.Absolute, 220f));
            table.ColumnStyles.Add(new ColumnStyle(SizeType.Percent, 100f));
            for (int i = 0; i < 5; i++)
            {
                table.RowStyles.Add(new RowStyle(SizeType.Absolute, 44f));
            }
            table.RowStyles.Add(new RowStyle(SizeType.Percent, 100f));
            table.RowStyles.Add(new RowStyle(SizeType.Absolute, 46f));
            Controls.Add(table);

            StyleText(_handleBox);
            _handleBox.Text = Result.Handle;
            AddRow(table, 0, "Kanaalhandle (@naam)", _handleBox);

            StyleText(_videoBox);
            _videoBox.Text = Result.VideoId;
            AddRow(table, 1, "Video-id of link", _videoBox);

            StyleStep(_subsStep, Result.SubsFlashStep);
            AddRow(table, 2, "Wit-flits per N subs", _subsStep);

            StyleStep(_videoStep, Result.VideoViewsFlashStep);
            AddRow(table, 3, "Regenboog per N video views", _videoStep);

            StyleStep(_chanStep, Result.ChannelViewsFlashStep);
            AddRow(table, 4, "Regenboog per N kanaal views", _chanStep);

            var messages = new FlowLayoutPanel
            {
                Dock = DockStyle.Fill,
                FlowDirection = FlowDirection.TopDown,
                WrapContents = false,
                BackColor = BackColor
            };

            _error.AutoSize = true;
            _error.MaximumSize = new Size(500, 0);
            _error.ForeColor = Color.FromArgb(255, 120, 130);
            _error.Text = "";
            messages.Controls.Add(_error);

            _testResult.AutoSize = true;
            _testResult.MaximumSize = new Size(500, 0);
            _testResult.Padding = new Padding(0, 8, 0, 0);
            _testResult.ForeColor = Color.FromArgb(160, 220, 180);
            _testResult.Text = "";
            messages.Controls.Add(_testResult);

            table.Controls.Add(messages, 0, 5);
            table.SetColumnSpan(messages, 2);

            var buttons = new FlowLayoutPanel
            {
                Dock = DockStyle.Fill,
                FlowDirection = FlowDirection.RightToLeft,
                WrapContents = false
            };
            var cancel = new Button { Text = "Annuleren", DialogResult = DialogResult.Cancel, Width = 110, Height = 32 };
            var save = new Button { Text = "Opslaan", Width = 110, Height = 32 };
            _testButton.Text = "Test";
            _testButton.Width = 110;
            _testButton.Height = 32;
            save.Click += (_, _) => OnSave();
            _testButton.Click += OnTestClick;
            StyleButton(cancel);
            StyleButton(save);
            StyleButton(_testButton);
            buttons.Controls.Add(cancel);
            buttons.Controls.Add(save);
            buttons.Controls.Add(_testButton);
            table.Controls.Add(buttons, 0, 6);
            table.SetColumnSpan(buttons, 2);

            AcceptButton = save;
            CancelButton = cancel;
        }

        private static void AddRow(TableLayoutPanel table, int row, string caption, Control input)
        {
            var label = new Label
            {
                Text = caption,
                Dock = DockStyle.Fill,
                TextAlign = ContentAlignment.MiddleLeft,
                ForeColor = Color.FromArgb(160, 160, 180)
            };
            input.Dock = DockStyle.Fill;
            table.Controls.Add(label, 0, row);
            table.Controls.Add(input, 1, row);
        }

        private static void StyleText(TextBox box)
        {
            box.BackColor = Color.FromArgb(24, 24, 32);
            box.ForeColor = Color.White;
            box.BorderStyle = BorderStyle.FixedSingle;
        }

        private static void StyleStep(NumericUpDown box, long value)
        {
            box.Minimum = 1;
            box.Maximum = 1_000_000;
            box.Value = Math.Clamp(value, 1, 1_000_000);
            box.BackColor = Color.FromArgb(24, 24, 32);
            box.ForeColor = Color.White;
            box.BorderStyle = BorderStyle.FixedSingle;
        }

        private static void StyleButton(Button button)
        {
            button.FlatStyle = FlatStyle.Flat;
            button.BackColor = Color.FromArgb(28, 28, 38);
            button.ForeColor = Color.White;
            button.FlatAppearance.BorderColor = Color.FromArgb(60, 60, 80);
        }

        /// <summary>
        /// Runs the same fallback chain the overlay uses, for whatever handle and video are in the
        /// dialog right now, and reports which source answered. Nothing is saved by testing.
        /// </summary>
        private async void OnTestClick(object? sender, EventArgs e)
        {
            string handle = _handleBox.Text.Trim().TrimStart('@');
            if (string.IsNullOrWhiteSpace(handle))
            {
                _error.Text = "Vul een kanaalhandle in.";
                return;
            }

            string? videoId = YtApi.ExtractVideoId(_videoBox.Text.Trim());
            if (videoId == null)
            {
                _error.Text = "Geen geldige video-id of link.";
                return;
            }

            _error.Text = "";
            _testButton.Enabled = false;
            _testResult.ForeColor = Color.FromArgb(200, 200, 210);
            _testResult.Text = "Bronnen testen…";

            try
            {
                var channel = await YtApi.FetchChannelAsync(handle);
                var video = await YtApi.FetchVideoAsync(videoId);
                if (IsDisposed)
                {
                    return;
                }

                _testResult.ForeColor = Color.FromArgb(160, 220, 180);
                _testResult.Text = string.Join(Environment.NewLine,
                    DescribeChannel(channel), DescribeVideo(video));
            }
            catch (Exception ex)
            {
                if (IsDisposed)
                {
                    return;
                }

                _testResult.ForeColor = Color.FromArgb(255, 120, 130);
                _testResult.Text = "Test mislukt: " + ex.Message;
            }
            finally
            {
                if (!IsDisposed)
                {
                    _testButton.Enabled = true;
                }
            }
        }

        private static string DescribeChannel(YtApi.ChannelStats c)
        {
            var parts = new List<string>();
            parts.Add(c.Subscribers.HasValue ? Numbers(c.Subscribers.Value) + " subs" : "geen subs");
            if (c.Views.HasValue)
            {
                parts.Add(Numbers(c.Views.Value) + " kanaal views");
            }

            string source = string.IsNullOrEmpty(c.Source) ? "geen bron" : c.Source;
            string error = string.IsNullOrEmpty(c.Error) ? "" : " — " + c.Error;
            return "Kanaal via " + source + ": " + string.Join(" · ", parts) + error;
        }

        private static string DescribeVideo(YtApi.VideoStats v)
        {
            string numbers = v.Views.HasValue ? Numbers(v.Views.Value) + " views" : "geen views";
            string source = string.IsNullOrEmpty(v.Source) ? "geen bron" : v.Source;
            string error = string.IsNullOrEmpty(v.Error) ? "" : " — " + v.Error;
            return "Video via " + source + ": " + numbers + error;
        }

        private static string Numbers(long value) => value.ToString("N0", CultureInfo.GetCultureInfo("nl-NL"));

        private void OnSave()
        {
            string handle = _handleBox.Text.Trim().TrimStart('@');
            if (string.IsNullOrWhiteSpace(handle))
            {
                _error.Text = "Vul een kanaalhandle in.";
                return;
            }

            string? videoId = YtApi.ExtractVideoId(_videoBox.Text.Trim());
            if (videoId == null)
            {
                _error.Text = "Geen geldige video-id of link.";
                return;
            }

            Result = new AppSettings
            {
                Handle = handle,
                VideoId = videoId,
                SubsFlashStep = (long)_subsStep.Value,
                VideoViewsFlashStep = (long)_videoStep.Value,
                ChannelViewsFlashStep = (long)_chanStep.Value
            }.Normalized();

            DialogResult = DialogResult.OK;
            Close();
        }
    }
}
