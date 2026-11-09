using System;
using System.Runtime.InteropServices;
using System.Text;
public static class PickerProbe {
    [StructLayout(LayoutKind.Sequential)] public struct Rect { public int Left,Top,Right,Bottom; }
    [StructLayout(LayoutKind.Sequential)] struct GuiInfo {
        public uint Size,Flags; public IntPtr Active,Focus,Capture,MenuOwner,MoveSize,Caret; public Rect CaretRect;
    }
    [StructLayout(LayoutKind.Sequential)] struct KeyInput { public ushort Key,Scan; public uint Flags,Time; public IntPtr Extra; }
    [StructLayout(LayoutKind.Sequential)] struct MouseInput { public int Dx,Dy; public uint Data,Flags,Time; public IntPtr Extra; }
    [StructLayout(LayoutKind.Explicit,Size=32)] struct Union { [FieldOffset(0)] public KeyInput Key; [FieldOffset(0)] public MouseInput Mouse; }
    [StructLayout(LayoutKind.Sequential)] struct Input { public uint Type; public Union Data; }
    [StructLayout(LayoutKind.Sequential, CharSet=CharSet.Unicode)] struct LogFont {
        public int Height,Width,Escapement,Orientation,Weight;
        public byte Italic,Underline,StrikeOut,FontCharSet,OutPrecision,ClipPrecision,Quality,PitchAndFamily;
        [MarshalAs(UnmanagedType.ByValTStr,SizeConst=32)] public string Face;
    }
    [DllImport("user32.dll")] public static extern IntPtr GetDlgItem(IntPtr parent,int id);
    [DllImport("user32.dll")] public static extern bool IsIconic(IntPtr window);
    [DllImport("user32.dll")] static extern bool SetCursorPos(int x,int y);
    [DllImport("user32.dll")] public static extern bool AllowSetForegroundWindow(uint pid);
    [DllImport("user32.dll")] static extern uint GetWindowThreadProcessId(IntPtr window,out uint pid);
    [DllImport("user32.dll")] static extern bool GetGUIThreadInfo(uint thread,ref GuiInfo info);
    [DllImport("user32.dll")] static extern uint SendInput(uint count,Input[] inputs,int size);
    [DllImport("user32.dll")] static extern IntPtr SendMessage(IntPtr window,uint message,IntPtr wparam,IntPtr lparam);
    [DllImport("user32.dll",CharSet=CharSet.Unicode)] static extern int GetWindowText(IntPtr window,StringBuilder text,int maximum);
    [DllImport("gdi32.dll",CharSet=CharSet.Unicode)] static extern int GetObject(IntPtr value,int bytes,ref LogFont font);
    [DllImport("imm32.dll")] static extern IntPtr ImmGetContext(IntPtr window);
    [DllImport("imm32.dll")] static extern bool ImmReleaseContext(IntPtr window,IntPtr context);
    public static IntPtr Focused(IntPtr parent) {
        uint pid; uint thread=GetWindowThreadProcessId(parent,out pid);
        GuiInfo state=new GuiInfo(); state.Size=(uint)Marshal.SizeOf(typeof(GuiInfo));
        if (!GetGUIThreadInfo(thread,ref state)) throw new InvalidOperationException("Cannot read UI thread focus.");
        return state.Focus;
    }
    public static void Key(ushort key) {
        Input[] inputs=new Input[2]; inputs[0].Type=inputs[1].Type=1;
        inputs[0].Data.Key.Key=inputs[1].Data.Key.Key=key; inputs[1].Data.Key.Flags=2;
        if (SendInput(2,inputs,Marshal.SizeOf(typeof(Input)))!=2) throw new InvalidOperationException("Key injection failed.");
    }
    public static void Click(int x,int y) {
        if (!SetCursorPos(x,y)) throw new InvalidOperationException("Cursor move failed.");
        Input[] inputs=new Input[2]; inputs[0].Data.Mouse.Flags=2; inputs[1].Data.Mouse.Flags=4; // left down, left up
        if (SendInput(2,inputs,Marshal.SizeOf(typeof(Input)))!=2) throw new InvalidOperationException("Click injection failed.");
    }
    public static string Text(IntPtr window) { StringBuilder text=new StringBuilder(4096); GetWindowText(window,text,text.Capacity); return text.ToString(); }
    public static int FontHeight(IntPtr window) {
        IntPtr handle=SendMessage(window,0x0031,IntPtr.Zero,IntPtr.Zero); LogFont font=new LogFont();
        if (GetObject(handle,Marshal.SizeOf(typeof(LogFont)),ref font)==0) throw new InvalidOperationException("Cannot inspect native font.");
        return font.Height;
    }
    public static bool HasImeContext(IntPtr window) {
        IntPtr context=ImmGetContext(window); if (context==IntPtr.Zero) return false;
        ImmReleaseContext(window,context); return true;
    }
}
