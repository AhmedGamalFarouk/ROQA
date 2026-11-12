using System;
using System.Runtime.InteropServices;
public static class PickerProbe {
    [StructLayout(LayoutKind.Sequential)] struct MouseInput { public int Dx,Dy; public uint Data,Flags,Time; public IntPtr Extra; }
    [StructLayout(LayoutKind.Explicit,Size=32)] struct Union { [FieldOffset(0)] public MouseInput Mouse; }
    [StructLayout(LayoutKind.Sequential)] struct Input { public uint Type; public Union Data; }
    [DllImport("user32.dll")] static extern bool SetCursorPos(int x,int y);
    [DllImport("user32.dll")] static extern uint SendInput(uint count,Input[] inputs,int size);
    public static void Click(int x,int y) {
        if (!SetCursorPos(x,y)) throw new InvalidOperationException("Cursor move failed.");
        Input[] inputs=new Input[2]; inputs[0].Data.Mouse.Flags=2; inputs[1].Data.Mouse.Flags=4; // left down, left up
        if (SendInput(2,inputs,Marshal.SizeOf(typeof(Input)))!=2) throw new InvalidOperationException("Click injection failed.");
    }
}
