using System.Windows;
using System.Windows.Media;
using System.Windows.Media.Animation;
using System.Windows.Shapes;
using System.Windows.Threading;

namespace WuwaVHLauncher
{
    public partial class SplashWindow : Window
    {

        private sealed class Branch
        {
            public Branch? Parent;
            public readonly List<Branch> Children = new();
            public Point P0, P1;
            public int Level;
            public int Life = 20;
            public double Vx, Vy;
            public Branch(Branch? parent, int level, double x, double y)
            {
                Parent = parent;
                Level = level;
                P0 = parent != null ? parent.P1 : new Point(x, y);
                P1 = new Point(x, y);
            }
        }

        private const int MaxLevels = 7;
        private const int MaxBranches = 200;
        private static readonly Brush InkBrush;
        private static readonly Brush LeafBrush;

        static SplashWindow()
        {
            InkBrush = new SolidColorBrush(Color.FromRgb(0x1B, 0x13, 0x0C)); InkBrush.Freeze();
            LeafBrush = new SolidColorBrush(Color.FromRgb(0xBE, 0x3A, 0x34)); LeafBrush.Freeze();
        }

        private Branch _root = null!;
        private Branch _curBranch = null!;
        private int _nBranches;
        private DispatcherTimer? _growTimer;
        private readonly Random _rng = Random.Shared;

        private readonly string[] _sequences =
        {
            "淵上有城名玄方",
            "樞旋字移玄機藏",
            "機巧自轉鳶鳥翔…",
            "耳畔蒼翎響遠音",
            "扇間朝暉道謎情",
            "故鎖舊契囚執念",
            "幽境今人亦獨行",
            "仙音寒芒鎮雲關",
            "朝月清輝照孤城",
            "峰巒疊嶂間",
            "此城何處尋？",
            "有緣人至，山門自開",
        };

        public SplashWindow()
        {
            InitializeComponent();
            treeCanvas.SizeChanged += (_, _) =>
            {
                if (_growTimer == null && treeCanvas.ActualWidth > 0)
                    StartTree();
            };
            RunSequence();
        }

        private const double TrunkAngle = -Math.PI / 4;

        private void StartTree()
        {
            _root = new Branch(null, MaxLevels, -8, treeCanvas.ActualHeight + 8);
            _curBranch = _root;
            _growTimer = new DispatcherTimer { Interval = TimeSpan.FromMilliseconds(45) };
            _growTimer.Tick += (_, _) => Grow();
            _growTimer.Start();
        }

        private void Grow()
        {
            if (IsClosed) return;
            GrowBranch(_root);

            if (_rng.NextDouble() > 0.55)
            {
                var b = new Branch(_curBranch, _curBranch.Level, _curBranch.P1.X, _curBranch.P1.Y);
                double a = TrunkAngle + (_rng.NextDouble() * 1.4 - 0.7);
                b.Vx = Math.Cos(a) * 7; b.Vy = Math.Sin(a) * 7;
                b.Life = _rng.Next(MaxLevels) + 2;
                _curBranch.Children.Add(b);
                if (_rng.NextDouble() > 0.8) _curBranch.Children.Add(NewBranch(_curBranch));
                _curBranch = b;
                _nBranches++;
            }
            if (_nBranches > MaxBranches && _root.Children.Count > 0)
            {
                _root = _root.Children[0];
                _nBranches--;
            }
        }

        private void GrowBranch(Branch br)
        {
            for (int i = 0; i < br.Children.Count; i++) GrowBranch(br.Children[i]);
            if (br.Life > 1)
            {
                br.P1 = new Point(br.P1.X + br.Vx, br.P1.Y + br.Vy);
                if (br.Level > 0)
                {
                    if (br.Parent != null)
                    {
                        var fig = new PathFigure { StartPoint = br.Parent.P0 };
                        fig.Segments.Add(new QuadraticBezierSegment(br.P0, br.P1, true));
                        var geo = new PathGeometry(); geo.Figures.Add(fig);
                        treeCanvas.Children.Add(new Path
                        {
                            Data = geo,
                            Stroke = InkBrush,
                            StrokeThickness = br.Level * 2.0 - 1.4,
                            StrokeStartLineCap = PenLineCap.Round,
                            StrokeEndLineCap = PenLineCap.Round,
                            Opacity = 0.82,
                        });
                    }
                }
                else
                {
                    treeCanvas.Children.Add(new Line
                    {
                        X1 = br.P0.X, Y1 = br.P0.Y, X2 = br.P1.X, Y2 = br.P1.Y,
                        Stroke = LeafBrush, StrokeThickness = 3.2,
                        StrokeStartLineCap = PenLineCap.Round, StrokeEndLineCap = PenLineCap.Round,
                        Opacity = 0.85,
                    });
                }
            }
            if (br.Life == 1 && br.Level > 0 && br.Level < MaxLevels)
            {
                br.Children.Add(NewBranch(br));
                br.Children.Add(NewBranch(br));
            }
            br.Life--;
        }

        private Branch NewBranch(Branch parent)
        {
            var b = new Branch(parent, parent.Level - 1, parent.P1.X, parent.P1.Y);
            double baseAngle = parent.Level == MaxLevels
                ? TrunkAngle
                : Math.Atan2(parent.P1.Y - parent.P0.Y, parent.P1.X - parent.P0.X);
            double angle = baseAngle + (_rng.NextDouble() * 1.4 - 0.7);
            b.Vx = Math.Cos(angle) * 7;
            b.Vy = Math.Sin(angle) * 7;
            b.Life = b.Level <= 1 ? 5 : _rng.Next(b.Level * 2) + 2;
            return b;
        }

        private async void RunSequence()
        {
            foreach (var line in _sequences)
            {
                if (IsClosed) return;
                currentLine.Text = line;
                await Task.Delay(420 + _rng.Next(260));
            }
            currentLine.Text = "山門自開";
        }

        private bool IsClosed;

        public void FadeOutAndClose()
        {
            IsClosed = true;
            _growTimer?.Stop();
            var fade = new DoubleAnimation(1, 0, new Duration(TimeSpan.FromMilliseconds(300)));
            fade.Completed += (_, _) => Close();
            BeginAnimation(OpacityProperty, fade);
        }
    }
}
