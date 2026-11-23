using System;
using System.Windows.Forms;

namespace YTLiveStats
{
    internal static class Program
    {
        [STAThread]
        private static void Main(string[] args)
        {
            // Saved settings first (moving the pre-rename %APPDATA% folder over), then command-line
            // arguments override them for this run.
            AppSettings.MigrateLegacyFolder();
            AppSettings settings = AppSettings.Load();
            bool fullscreen = false;

            for (int i = 0; i < args.Length; i++)
            {
                string arg = args[i];
                if (arg == "--fullscreen" || arg == "-f")
                {
                    fullscreen = true;
                    continue;
                }
                if ((arg == "--handle" || arg == "-h") && i + 1 < args.Length)
                {
                    settings.Handle = args[++i];
                    continue;
                }
                if ((arg == "--video" || arg == "-v") && i + 1 < args.Length)
                {
                    settings.VideoId = args[++i];
                }
            }

            settings.Normalized();

            Application.EnableVisualStyles();
            Application.SetCompatibleTextRenderingDefault(false);
            Application.SetHighDpiMode(HighDpiMode.SystemAware);

            var form = new Form1(settings, fullscreen);
            form.Icon = AppIcon.Get();
            Application.Run(form);
        }
    }
}
