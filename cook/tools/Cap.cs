using System;
using System.Drawing;
using System.Drawing.Imaging;
using System.Runtime.InteropServices;
using System.Threading;

public static class Cap {
    [DllImport("gdi32.dll", SetLastError = true, CharSet = CharSet.Unicode)]
    static extern IntPtr CreateDC(string lpszDriver, string lpszDevice, string lpszOutput, IntPtr lpInitData);

    [DllImport("gdi32.dll", SetLastError = true)]
    static extern bool DeleteDC(IntPtr hdc);

    [DllImport("gdi32.dll", SetLastError = true)]
    static extern IntPtr CreateCompatibleDC(IntPtr hdc);

    [DllImport("gdi32.dll", SetLastError = true)]
    static extern IntPtr CreateCompatibleBitmap(IntPtr hdc, int w, int h);

    [DllImport("gdi32.dll", SetLastError = true)]
    static extern IntPtr SelectObject(IntPtr hdc, IntPtr obj);

    [DllImport("gdi32.dll", SetLastError = true)]
    static extern bool BitBlt(IntPtr hdcDest, int nXDest, int nYDest, int w, int h, IntPtr hdcSrc, int nXSrc, int nYSrc, uint dwRop);

    [DllImport("gdi32.dll", SetLastError = true)]
    static extern bool DeleteObject(IntPtr obj);

    public static void Still(int x, int y, int w, int h, string path) {
        IntPtr hdcSrc = CreateDC(@"\\.\DISPLAY1", @"\\.\DISPLAY1", null, IntPtr.Zero);
        if (hdcSrc == IntPtr.Zero) hdcSrc = CreateDC("DISPLAY", null, null, IntPtr.Zero);
        IntPtr hdcDest = CreateCompatibleDC(hdcSrc);
        IntPtr hBitmap = CreateCompatibleBitmap(hdcSrc, w, h);
        IntPtr hOld = SelectObject(hdcDest, hBitmap);
        BitBlt(hdcDest, 0, 0, w, h, hdcSrc, x, y, 0x00CC0020);
        SelectObject(hdcDest, hOld);
        DeleteDC(hdcDest);
        DeleteDC(hdcSrc);

        using (Bitmap bmp = Image.FromHbitmap(hBitmap)) {
            bmp.Save(path, ImageFormat.Png);
        }
        DeleteObject(hBitmap);
    }
}

public static class Mouse07 {
    [DllImport("user32.dll")] public static extern IntPtr GetDlgItem(IntPtr parent, int id);
    [DllImport("user32.dll", CharSet = CharSet.Unicode)] static extern IntPtr FindWindow(string c, string t);
    public static IntPtr Backdrop() { return FindWindow(null, "ROQA evaluation gray backdrop"); }
    [StructLayout(LayoutKind.Sequential)] public struct Input { public uint type; public Mouse data; }
    [StructLayout(LayoutKind.Sequential)] public struct Mouse { public int dx, dy; public uint mouseData, dwFlags, time; public UIntPtr extra; }
    [DllImport("user32.dll")] static extern uint SendInput(uint count, Input[] inputs, int size);
    [DllImport("user32.dll")] static extern void mouse_event(uint flags, int dx, int dy, uint data, UIntPtr extra);
    [DllImport("user32.dll")] static extern bool SetCursorPos(int x, int y);
    static void Send(uint flags) {
        var input = new Input { type = 0, data = new Mouse { dwFlags = flags } };
        if (SendInput(1, new[] { input }, Marshal.SizeOf(typeof(Input))) != 1) {
            mouse_event(flags, 0, 0, 0, UIntPtr.Zero);
        }
    }
    public static void Move(int x, int y) { SetCursorPos(x, y); Send(1); }
    public static void Click(int x, int y) { Move(x, y); Thread.Sleep(60); Send(2); Thread.Sleep(35); Send(4); }
    public static void Right(int x, int y) { Move(x, y); Thread.Sleep(60); Send(8); Thread.Sleep(35); Send(16); }
}
