using System.Collections.ObjectModel;
using System.Windows;
using System.Windows.Controls;
using System.Windows.Media;
using System.Windows.Media.Animation;
using System.Windows.Threading;

namespace WuwaIDLauncher
{
    public partial class SplashWindow : Window
    {
        private ObservableCollection<string> _logs = new();
        private static readonly char[] _glitchChars = "!@#$%^&*<>/?\\|[]{}01234567ABCDEF¥§Ω≠∆∇".ToCharArray();

        private static readonly string[] _easterEggs = { "LUCY", "REBECCA", "DAVID", "JOHNNY", "ROVER", "AEMEATH", "SHOREKEEPER" };
        private static readonly char[] _rainChars = "ｦｧｨｩｪｫｬｭｮｯｰｱｲｳｴｵｶｷｸｹｺｻｼｽｾｿﾀﾁﾂﾃﾄﾅﾆﾇﾈﾉﾊﾋﾌﾍﾎﾏﾐﾑﾒﾓﾔﾕﾖﾗﾘﾙﾚﾛﾜﾝ0123456789ABCDEF".ToCharArray();
        private DispatcherTimer? _rainTimer;
        private struct RainColumn { public double X; public double Y; public double Speed; public string? EggWord; public int EggPos; }
        private RainColumn[] _cols = Array.Empty<RainColumn>();
        private TextBlock[] _colBlocks = Array.Empty<TextBlock>();
        private const int ColCharSize = 9;
        private const int MaxRows = 24;

        private string[] _sequences = {
            "Good morning, Night City.",
            "Synchronizing resonance with Solaris-3...",
            "I will take you to the moon. I promise.",
            "Echo frequency locked — Rover signal detected.",
            "Wake up, Samurai. We have a city to burn.",
            "Tacet Field anomaly suppressed... [STABLE]",
            "Never fade away.",
            "Waveform identity confirmed: ROVER_01",
            "You are the only one who can hear the Waves.",
            "WUTHERING WAVES INTERFACE — READY"
        };

        public SplashWindow()
        {
            InitializeComponent();
            logItems.ItemsSource = _logs;
            matrixCanvas.SizeChanged += (_, _) =>
            {
                if (_cols.Length == 0 && matrixCanvas.ActualWidth > 0)
                    StartMatrixRain();
            };
            RunSequence();
        }

        private void StartMatrixRain()
        {
            double w = matrixCanvas.ActualWidth;
            int count = Math.Max(1, (int)(w / (ColCharSize + 4)));
            _cols = new RainColumn[count];
            _colBlocks = new TextBlock[count];
            var rng = Random.Shared;

            for (int i = 0; i < count; i++)
            {
                bool hasEgg = rng.Next(4) == 0;
                string? egg = hasEgg ? _easterEggs[rng.Next(_easterEggs.Length)] : null;
                _cols[i] = new RainColumn
                {
                    X = i * (ColCharSize + 4),
                    Y = -rng.Next(MaxRows) * ColCharSize,
                    Speed = 0.6 + rng.NextDouble() * 1.2,
                    EggWord = egg,
                    EggPos = egg != null ? rng.Next(Math.Max(1, MaxRows - egg.Length)) : 0
                };
                var tb = new TextBlock
                {
                    FontFamily = new FontFamily("Consolas"),
                    FontSize = ColCharSize - 1,
                    Foreground = new SolidColorBrush(Color.FromRgb(0, 200, 180)),
                    Text = "",
                    Width = ColCharSize + 4,
                    TextWrapping = TextWrapping.NoWrap,
                };
                Canvas.SetLeft(tb, _cols[i].X);
                Canvas.SetTop(tb, 0);
                matrixCanvas.Children.Add(tb);
                _colBlocks[i] = tb;
            }

            _rainTimer = new DispatcherTimer { Interval = TimeSpan.FromMilliseconds(60) };
            _rainTimer.Tick += RainTick;
            _rainTimer.Start();
        }

        private void RainTick(object? sender, EventArgs e)
        {
            var rng = Random.Shared;
            double h = matrixCanvas.ActualHeight;
            for (int i = 0; i < _cols.Length; i++)
            {
                ref var col = ref _cols[i];
                col.Y += col.Speed * ColCharSize;
                if (col.Y > h + MaxRows * ColCharSize)
                {
                    col.Y = -rng.Next(6) * ColCharSize;
                    bool hasEgg = rng.Next(4) == 0;
                    col.EggWord = hasEgg ? _easterEggs[rng.Next(_easterEggs.Length)] : null;
                    col.EggPos = col.EggWord != null ? rng.Next(Math.Max(1, MaxRows - col.EggWord.Length)) : 0;
                }

                var sb = new System.Text.StringBuilder(MaxRows);
                int headRow = (int)(col.Y / ColCharSize);
                for (int r = 0; r < MaxRows; r++)
                {
                    int absRow = headRow - (MaxRows - 1 - r);
                    if (absRow < 0) { sb.Append(' '); continue; }
                    if (col.EggWord != null)
                    {
                        int eggOff = absRow - col.EggPos;
                        if (eggOff >= 0 && eggOff < col.EggWord.Length)
                        {
                            sb.Append(col.EggWord[eggOff]);
                            continue;
                        }
                    }
                    sb.Append(r == MaxRows - 1 ? (char)0x2588 : _rainChars[rng.Next(_rainChars.Length)]);
                }

                _colBlocks[i].Text = string.Join("\n", sb.ToString().ToCharArray());
                Canvas.SetTop(_colBlocks[i], Math.Max(0, col.Y - MaxRows * ColCharSize));
            }
        }

        private async Task GlitchReveal(string text, int durationMs = 480)
        {
            const int steps = 14;
            int stepDelay = durationMs / steps;
            var rng = Random.Shared;

            for (int step = 0; step <= steps; step++)
            {
                if (IsClosed) return;
                int resolved = (int)(text.Length * ((float)step / steps));
                var sb = new System.Text.StringBuilder(text.Length);
                for (int i = 0; i < text.Length; i++)
                {
                    if (i < resolved || text[i] == ' ')
                        sb.Append(text[i]);
                    else
                        sb.Append(_glitchChars[rng.Next(_glitchChars.Length)]);
                }
                currentLine.Text = "> " + sb;
                await Task.Delay(stepDelay);
            }
            currentLine.Text = "> " + text;
        }

        private async void RunSequence()
        {
            foreach (var line in _sequences)
            {
                if (IsClosed) return;
                currentLine.Text = "";
                await GlitchReveal(line);
                if (IsClosed) return;
                _logs.Add("> " + line);
                if (_logs.Count > 5) _logs.RemoveAt(0);
                currentLine.Text = "";
                await Task.Delay(120 + Random.Shared.Next(220));
            }
            currentLine.Text = "> SYSTEM READY [OK]";
        }

        private bool IsClosed = false;

        public void FadeOutAndClose()
        {
            IsClosed = true;
            _rainTimer?.Stop();
            var fade = new DoubleAnimation(1, 0, new Duration(TimeSpan.FromMilliseconds(300)));
            fade.Completed += (_, _) => Close();
            BeginAnimation(OpacityProperty, fade);
        }
    }
}
