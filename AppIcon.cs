using System;
using System.Drawing;
using System.IO;
using System.Windows.Forms;

namespace NeoYTLiveStats
{
    /// <summary>
    /// The application icon, embedded in the exe as <c>app.ico</c> so the window, the taskbar, the
    /// Start Menu shortcut and Apps &amp; features all show the same artwork at their own size.
    /// Falls back to the executable's own icon when the resource is missing.
    /// </summary>
    internal static class AppIcon
    {
        private const string ResourceName = "NeoYTLiveStats.app.ico";

        private static Icon? _cached;

        public static Icon? Get()
        {
            if (_cached != null)
            {
                return _cached;
            }

            try
            {
                using (Stream? stream = typeof(AppIcon).Assembly.GetManifestResourceStream(ResourceName))
                {
                    if (stream != null)
                    {
                        _cached = new Icon(stream);
                    }
                }
            }
            catch (Exception)
            {
                _cached = null;
            }

            if (_cached == null)
            {
                try
                {
                    _cached = Icon.ExtractAssociatedIcon(Application.ExecutablePath);
                }
                catch (Exception)
                {
                    _cached = null;
                }
            }

            return _cached;
        }
    }
}
