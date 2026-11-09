using System;
using System.Drawing;
using System.Drawing.Imaging;
using System.Runtime.InteropServices;
using System.Threading;

public static class NativeProbe {
    [StructLayout(LayoutKind.Sequential)] public struct Rect {
        public int Left, Top, Right, Bottom;
        public int Width { get { return Right - Left; } }
        public int Height { get { return Bottom - Top; } }
    }
    [StructLayout(LayoutKind.Sequential)] public struct Point { public int X, Y; }
    [StructLayout(LayoutKind.Sequential)] struct MonitorInfo {
        public int Size; public Rect Monitor, Work; public uint Flags;
    }
    [StructLayout(LayoutKind.Sequential)] struct Message {
        public IntPtr Window; public uint Id; public UIntPtr WParam; public IntPtr LParam;
        public uint Time; public Point Position; public uint Private;
    }
    [StructLayout(LayoutKind.Sequential, CharSet=CharSet.Unicode)] struct WindowClass {
        public uint Style; public WindowProcedure Procedure; public int ClassExtra, WindowExtra;
        public IntPtr Instance, Icon, Cursor, Background;
        public string Menu, Name;
    }
    delegate IntPtr WindowProcedure(IntPtr window, uint message, UIntPtr wparam, IntPtr lparam);
    static readonly WindowProcedure AnchorProcedure = AnchorCallback;
    static readonly ManualResetEvent AnchorReady = new ManualResetEvent(false);
    static IntPtr anchor;
    static Thread anchorThread;
    static Exception anchorFailure;

    [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern IntPtr FindWindow(string className, string title);
    [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
    [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr window);
    [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr window, out Rect rect);
    [DllImport("user32.dll")] public static extern uint GetDpiForWindow(IntPtr window);
    [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr window, out uint pid);
    [DllImport("user32.dll")] public static extern bool GetCursorPos(out Point point);
    [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
    [DllImport("user32.dll")] public static extern bool SetWindowPos(IntPtr window, IntPtr after, int x, int y, int width, int height, uint flags);
    [DllImport("user32.dll")] public static extern uint GetGuiResources(IntPtr process, uint flags);
    [DllImport("user32.dll")] public static extern IntPtr SendMessage(IntPtr window, uint message, UIntPtr wparam, IntPtr lparam);
    [DllImport("user32.dll")] public static extern bool PostMessage(IntPtr window, uint message, UIntPtr wparam, IntPtr lparam);
    [DllImport("user32.dll")] static extern IntPtr MonitorFromPoint(Point point, uint flags);
    [DllImport("user32.dll", CharSet=CharSet.Unicode)] static extern bool GetMonitorInfo(IntPtr monitor, ref MonitorInfo info);
    [DllImport("user32.dll", CharSet=CharSet.Unicode)] static extern ushort RegisterClass(ref WindowClass descriptor);
    [DllImport("user32.dll", CharSet=CharSet.Unicode)] static extern IntPtr CreateWindowEx(uint exStyle, string className, string title,
        uint style, int x, int y, int width, int height, IntPtr parent, IntPtr menu, IntPtr instance, IntPtr parameter);
    [DllImport("user32.dll")] static extern bool ShowWindow(IntPtr window, int command);
    [DllImport("user32.dll")] static extern bool UpdateWindow(IntPtr window);
    [DllImport("user32.dll", CharSet=CharSet.Unicode)] static extern IntPtr DefWindowProc(IntPtr window, uint message, UIntPtr wparam, IntPtr lparam);
    [DllImport("user32.dll", CharSet=CharSet.Unicode)] static extern int GetMessage(out Message message, IntPtr window, uint first, uint last);
    [DllImport("user32.dll")] static extern bool TranslateMessage(ref Message message);
    [DllImport("user32.dll", CharSet=CharSet.Unicode)] static extern IntPtr DispatchMessage(ref Message message);
    [DllImport("user32.dll")] static extern void PostQuitMessage(int code);
    [DllImport("user32.dll", CharSet=CharSet.Unicode)] static extern bool UnregisterClass(string className, IntPtr instance);
    [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern bool SetWindowText(IntPtr window, string text);
    public static void ShowFixture(IntPtr window, int command) { ShowWindow(window, command); }
    [DllImport("user32.dll")] static extern IntPtr SetThreadDpiAwarenessContext(IntPtr context);

    public static void EnablePhysicalPixels() {
        if (SetThreadDpiAwarenessContext(new IntPtr(-4)) == IntPtr.Zero)
            throw new InvalidOperationException("Cannot enter per-monitor-v2 test context.");
    }
    public static Rect WorkArea() {
        MonitorInfo monitor = new MonitorInfo(); monitor.Size = Marshal.SizeOf(typeof(MonitorInfo));
        if (!GetMonitorInfo(MonitorFromPoint(new Point(), 1), ref monitor))
            throw new InvalidOperationException("Cannot inspect primary monitor.");
        return monitor.Work;
    }
    public static void DpiChange(IntPtr window, uint dpi) {
        Rect proposed;
        if (!GetWindowRect(window, out proposed)) throw new InvalidOperationException("Cannot inspect DPI probe target.");
        IntPtr rectangle = Marshal.AllocHGlobal(Marshal.SizeOf(typeof(Rect)));
        try {
            Marshal.StructureToPtr(proposed, rectangle, false);
            SendMessage(window, 0x02E0, new UIntPtr(dpi | (dpi << 16)), rectangle);
        } finally { Marshal.FreeHGlobal(rectangle); }
    }
    public static IntPtr StartAnchor() {
        if (anchorThread != null && anchorThread.IsAlive) throw new InvalidOperationException("A smoke fixture is already active.");
        AnchorReady.Reset(); anchor = IntPtr.Zero; anchorFailure = null;
        Thread thread = new Thread(delegate() {
            try {
                EnablePhysicalPixels();
                WindowClass descriptor = new WindowClass();
                descriptor.Procedure = AnchorProcedure; descriptor.Name = "ROQA.SmokeAnchor";
                descriptor.Background = new IntPtr(6);
                if (RegisterClass(ref descriptor) == 0) throw new InvalidOperationException("Cannot register smoke anchor.");
                anchor = CreateWindowEx(0, descriptor.Name, "ROQA smoke test", 0x00CF0000,
                    40, 180, 300, 120, IntPtr.Zero, IntPtr.Zero, IntPtr.Zero, IntPtr.Zero);
                if (anchor == IntPtr.Zero) throw new InvalidOperationException("Cannot create smoke anchor.");
                ShowWindow(anchor, 1); UpdateWindow(anchor); SetForegroundWindow(anchor);
                AnchorReady.Set();
                Message message;
                int status;
                while ((status = GetMessage(out message, IntPtr.Zero, 0, 0)) > 0) {
                    TranslateMessage(ref message); DispatchMessage(ref message);
                }
                if (status < 0) anchorFailure = new InvalidOperationException("Anchor message loop failed.");
            } catch (Exception failure) { anchorFailure = failure; AnchorReady.Set(); }
            finally { UnregisterClass("ROQA.SmokeAnchor", IntPtr.Zero); }
        });
        anchorThread = thread;
        thread.IsBackground = true; thread.SetApartmentState(ApartmentState.STA); thread.Start();
        if (!AnchorReady.WaitOne(5000)) throw new TimeoutException("Smoke anchor startup timed out.");
        if (anchorFailure != null) throw new InvalidOperationException("Smoke anchor failed.", anchorFailure);
        return anchor;
    }
    [DllImport("user32.dll", SetLastError=true)]
    static extern bool PostThreadMessage(uint threadId, uint message, UIntPtr wparam, IntPtr lparam);
    public sealed class QueueTraffic {
        public long Posted;
        public long Rejected;
        public Thread Worker;
    }
    public static QueueTraffic StartQueueTraffic(IntPtr window, int milliseconds) {
        uint process;
        uint target = GetWindowThreadProcessId(window, out process);
        if (target == 0) throw new InvalidOperationException("No owned ROQA thread for queue test.");
        QueueTraffic traffic = new QueueTraffic();
        traffic.Worker = new Thread(delegate() {
            System.Diagnostics.Stopwatch clock = System.Diagnostics.Stopwatch.StartNew();
            while (clock.ElapsedMilliseconds < milliseconds) {
                if (PostThreadMessage(target, 0x8000 + 50, UIntPtr.Zero, IntPtr.Zero))
                    Interlocked.Increment(ref traffic.Posted);
                else Interlocked.Increment(ref traffic.Rejected);
            }
        });
        traffic.Worker.IsBackground = true;
        traffic.Worker.Start();
        return traffic;
    }
    public static Thread StartTitleStorm(IntPtr window, int milliseconds) {
        Thread storm = new Thread(delegate() {
            var clock = System.Diagnostics.Stopwatch.StartNew();
            int generation = 0;
            while (clock.ElapsedMilliseconds < milliseconds) {
                SetWindowText(window, "ROQA shutdown storm " + generation++);
                Thread.Sleep(1);
            }
        });
        storm.IsBackground = true; storm.Start(); return storm;
    }
    public static void StopAnchor(IntPtr window) {
        if (anchorThread == null) return;
        if (anchorThread.IsAlive) {
            PostMessage(window, 0x0010, UIntPtr.Zero, IntPtr.Zero);
            if (!anchorThread.Join(5000)) throw new TimeoutException("Smoke fixture did not stop.");
        }
        anchorThread = null; anchor = IntPtr.Zero;
    }
    static IntPtr AnchorCallback(IntPtr window, uint message, UIntPtr wparam, IntPtr lparam) {
        if (message == 2) { PostQuitMessage(0); return IntPtr.Zero; }
        return DefWindowProc(window, message, wparam, lparam);
    }
    [DllImport("user32.dll")] static extern int GetWindowRgnBox(IntPtr window, out Rect rect);
    // Window-relative "x,y,width,height" of the window region, or "none" when the whole window is the region.
    public static string RegionBox(IntPtr window) {
        Rect box;
        if (GetWindowRgnBox(window, out box) == 0) return "none";
        return String.Format("{0},{1},{2},{3}", box.Left, box.Top, box.Width, box.Height);
    }
    public static string PixelAt(string path, int x, int y) {
        using (Bitmap bitmap = new Bitmap(path)) {
            Color pixel = bitmap.GetPixel(x, y);
            return String.Format("{0},{1},{2}", pixel.R, pixel.G, pixel.B);
        }
    }
    public static string CheckInteriorPixels(string path, string expected) {
        using (Bitmap bitmap = new Bitmap(path)) {
            for (int y = 1; y < bitmap.Height - 1; y++) {
                for (int x = 1; x < bitmap.Width - 1; x++) {
                    // Native corners have at most an 18-pixel radius; omit those four squares.
                    if ((x < 18 || x >= bitmap.Width - 18) && (y < 18 || y >= bitmap.Height - 18)) continue;
                    Color pixel = bitmap.GetPixel(x, y);
                    string rgb = String.Format("{0},{1},{2}", pixel.R, pixel.G, pixel.B);
                    if (rgb != expected) return String.Format("({0},{1}) expected {2}, observed {3}", x, y, expected, rgb);
                }
            }
        }
        return "";
    }
    public static string Capture(IntPtr window, string path) {
        Rect rect;
        if (!GetWindowRect(window, out rect) || rect.Width <= 0 || rect.Height <= 0)
            throw new InvalidOperationException("Invalid capture rectangle.");
        using (Bitmap bitmap = new Bitmap(rect.Width, rect.Height, PixelFormat.Format32bppArgb)) {
            using (Graphics graphics = Graphics.FromImage(bitmap)) {
                graphics.CopyFromScreen(rect.Left, rect.Top, 0, 0, new Size(rect.Width, rect.Height), CopyPixelOperation.SourceCopy);
            }
            bitmap.Save(path, ImageFormat.Png);
            Color center = bitmap.GetPixel(rect.Width / 2, rect.Height / 2);
            return String.Format("{0},{1},{2}", center.R, center.G, center.B);
        }
    }
}
