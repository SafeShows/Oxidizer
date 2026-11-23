// KSA's own UI kit (KSA.ConsoleStyle windows, KSA.ConsoleWidgets controls) for Rust and Rhai mods, through one
// entry point, `Call(op, ...)`, so the table between the bridge and the loader grows by a single function.
//
// Compiled against the game's assemblies (KsaDir in the .csproj). Each operation is its own lambda, so it is
// compiled on first use: if a game update renames one, only that operation fails (logged once).
using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;
using System.Text;
using Brutal.ImGuiApi;
using Brutal.Numerics;
using KSA;

public static unsafe class KsaUi
{
    /// (strings, floats, ints, bools) -> result: bool as 0/1; Segmented's new index goes to ints[0].
    delegate byte Op(string[] s, float* f, int* i, byte* b);

    static byte B(bool v) => v ? (byte)1 : (byte)0;
    static byte Done(Action a)
    {
        a();
        return 1;
    }

    static ref bool Bool(byte* b) => ref *(bool*)b;
    static string[] Lines(string s) => s.Length == 0 ? [] : s.Split('\n');

    // The numbers are the API (oxidizer_api::ksa_ui): only ever append.
    static readonly Op[] Ops =
    [
        /* 0 */ (s, f, i, b) => B(ConsoleStyle.BeginWindow(s[0], s[1], s[2], ref Bool(b), new float2(f[0], f[1]), (ImGuiWindowFlags)i[0], ImGuiCond.FirstUseEver)),
        /* 1 */ (s, f, i, b) => B(ConsoleStyle.BeginWindow(s[0], s[1], s[2], new float2(f[0], f[1]), (ImGuiWindowFlags)i[0], ImGuiCond.FirstUseEver)),
        /* 2 */ (s, f, i, b) => Done(ConsoleStyle.EndWindow),
        /* 3 */ (s, f, i, b) => Done(ConsoleStyle.BeginBody),
        /* 4 */ (s, f, i, b) => Done(ConsoleStyle.EndBody),
        /* 5 */ (s, f, i, b) => Done(ConsoleStyle.BeginFooter),
        /* 6 */ (s, f, i, b) => Done(ConsoleStyle.EndFooter),
        /* 7 */ (s, f, i, b) => Done(() => ConsoleStyle.FooterStatus(s[0], b[0] != 0)),
        /* 8 */ (s, f, i, b) => Done(ConsoleStyle.PushWidgetStyle),
        /* 9 */ (s, f, i, b) => Done(ConsoleStyle.PopWidgetStyle),
        /* 10 */ (s, f, i, b) => B(ConsoleWidgets.Button(s[0])),
        /* 11 */ (s, f, i, b) => B(ConsoleWidgets.PrimaryButton(s[0])),
        /* 12 */ (s, f, i, b) => B(ConsoleWidgets.PositiveButton(s[0])),
        /* 13 */ (s, f, i, b) => B(ConsoleWidgets.DangerButton(s[0])),
        /* 14 */ (s, f, i, b) => Done(() => ConsoleWidgets.RegionHeader(s[0])),
        /* 15 */ (s, f, i, b) => Done(ConsoleWidgets.Rule),
        /* 16 */ (s, f, i, b) => Done(() => ConsoleWidgets.Readout(s[0], s[1])),
        /* 17 */ (s, f, i, b) => Done(() => ConsoleWidgets.ValueText(s[0])),
        /* 18 */ (s, f, i, b) => Done(() => ConsoleWidgets.BeginRow(s[0])),
        /* 19 */ (s, f, i, b) => Done(ConsoleWidgets.EndRow),
        /* 20 */ (s, f, i, b) => B(ConsoleWidgets.Checkbox(s[0], ref Bool(b), b[1] != 0)),
        /* 21 */ (s, f, i, b) => B(ConsoleWidgets.SliderFloat(s[0], ref f[0], f[1], f[2], s[1], b[0] != 0)),
        /* 22 */ (s, f, i, b) => B(ConsoleWidgets.SliderInt(s[0], ref i[0], i[1], i[2], s[1], b[0] != 0)),
        /* 23 */ (s, f, i, b) => B(ConsoleWidgets.DragFloat(s[0], ref f[0], f[1], f[2], s[1], b[0] != 0)),
        /* 24 */ (s, f, i, b) => B(ConsoleWidgets.DragInt(s[0], ref i[0], i[1], i[2], s[1], b[0] != 0)),
        /* 25 */ (s, f, i, b) => B(ConsoleWidgets.BeginComboControl(s[0], s[1], b[0] != 0)),
        /* 26 */ (s, f, i, b) => Done(ConsoleWidgets.EndComboControl),
        /* 27 */ (s, f, i, b) => B(ConsoleWidgets.TextLine(s[0], ref s[1], b[0] != 0, out Bool(b + 1))),
        /* 28 */ (s, f, i, b) => B(ConsoleWidgets.ColorEdit3(s[0], ref *(float3*)f, b[0] != 0)),
        /* 29 */ (s, f, i, b) => Done(() => i[0] = ConsoleWidgets.Segmented(s[0], Lines(s[1]), i[0])),
        /* 30 */ (s, f, i, b) => Done(() => ConsoleWidgets.ItemTooltip(s[0])),
    ];

    /// (op, strings as UTF-8 (pointer, length) pairs, floats, ints, bools, text out buffer) -> result. When the
    /// second string is a ref argument (TextLine) and it changed, its UTF-8 goes to outBuf (up to cap) and its full
    /// length to *outLen; otherwise *outLen is 0.
    [UnmanagedCallersOnly]
    public static byte Call(int op, nuint* strs, nuint nstrs, float* f, int* i, byte* b, byte* outBuf, nuint cap, nuint* outLen)
    {
        try
        {
            if (outLen != null) *outLen = 0;
            if (op < 0 || op >= Ops.Length) return 0;
            var s = new string[(int)nstrs];
            for (var k = 0; k < s.Length; k++) s[k] = Encoding.UTF8.GetString((byte*)strs[2 * k], (int)strs[2 * k + 1]);
            var before = s.Length > 1 ? s[1] : null;
            var r = Ops[op](s, f, i, b);
            if (outLen != null && before != null && s[1] != before)
            {
                var bytes = Encoding.UTF8.GetBytes(s[1]);
                *outLen = (nuint)bytes.Length;
                if (outBuf != null) bytes.AsSpan(0, Math.Min(bytes.Length, (int)cap)).CopyTo(new Span<byte>(outBuf, (int)cap));
            }
            return r;
        }
        catch (Exception e)
        {
            lock (Complained)
                if (Complained.Add(op)) System.Console.WriteLine($"[oxidizer] KSA UI call {op} failed: {e.Message}");
            return 0;
        }
    }

    static readonly HashSet<int> Complained = [];
}
