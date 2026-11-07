// Managed side of the loader: Harmony hooks into KSA + reflection over GC handles for Rust.
// Struct layout must match `Api` in oxidizer_api/src/lib.rs.
using System;
using System.Collections;
using System.Collections.Concurrent;
using System.Collections.Generic;
using System.Linq;
using System.Reflection;
using System.Runtime.InteropServices;
using System.Text;
using HarmonyLib;

namespace Oxidizer;

[StructLayout(LayoutKind.Sequential)]
public unsafe struct Api
{
    public delegate* unmanaged<byte*, nuint, void> Log; // filled by Rust
    public delegate* unmanaged<nint, void> Free;
    public delegate* unmanaged<nint, byte*, nuint, byte*, nuint, nint> Get;
    public delegate* unmanaged<nint, byte*, nuint, byte*, nuint, nint, byte> Set;
    public delegate* unmanaged<nint, byte*, nuint, byte*, nuint, nint*, nuint, nint> Call;
    public delegate* unmanaged<double, nint> BoxF64;
    public delegate* unmanaged<byte*, nuint, nint> BoxStr;
    public delegate* unmanaged<nint, double*, byte> AsF64;
    public delegate* unmanaged<nint, byte*, nuint, nuint> AsStr;
    public delegate* unmanaged<nint, long> Len;
    public delegate* unmanaged<nint, long, nint> Index;
    public delegate* unmanaged<byte*, nuint, byte*, nuint, byte, nint, nint, byte> Patch;
    public delegate* unmanaged<byte*, nuint, nuint> DocsPath;
}

[StructLayout(LayoutKind.Sequential)]
public unsafe struct Host
{
    public delegate* unmanaged<double, double, void> Frame;
    public delegate* unmanaged<double, void> Gui;
}

public static unsafe class Bridge
{
    const BindingFlags All = BindingFlags.Public | BindingFlags.NonPublic | BindingFlags.Instance | BindingFlags.Static | BindingFlags.FlattenHierarchy;

    static Api* _api;
    static Host _host;
    static readonly Harmony Harmony = new("oxidizer");
    static readonly ConcurrentDictionary<string, Type?> Types = new();
    static readonly ConcurrentDictionary<MethodBase, (bool post, nint cb, nint user)[]> Hooks = new();
    static readonly HashSet<string> Logged = new();

    [UnmanagedCallersOnly]
    public static int Init(Api* api, Host* host)
    {
        try
        {
            _api = api;
            _host = *host;
            api->Free = &Free;
            api->Get = &Get;
            api->Set = &Set;
            api->Call = &Call;
            api->BoxF64 = &BoxF64;
            api->BoxStr = &BoxStr;
            api->AsF64 = &AsF64;
            api->AsStr = &AsStr;
            api->Len = &Len;
            api->Index = &Index;
            api->Patch = &Patch;
            api->DocsPath = &DocsPath;

            var program = Type.GetType("KSA.Program, KSA", true)!;
            Harmony.Patch(AccessTools.Method(program, "OnFrame"), postfix: new HarmonyMethod(typeof(Bridge), nameof(AfterFrame)));
            Harmony.Patch(AccessTools.Method(program, "OnDrawUiViewports"), postfix: new HarmonyMethod(typeof(Bridge), nameof(AfterGui)));
            return 1;
        }
        catch (Exception e)
        {
            Console.WriteLine($"[oxidizer] bridge init failed: {e}");
            return 0;
        }
    }

    static void AfterFrame(double currentPlayerTime, double dtPlayer) => _host.Frame(currentPlayerTime, dtPlayer);
    static void AfterGui(double dt) => _host.Gui(dt);

    // ---- helpers ----

    static void Log(string s)
    {
        var b = Encoding.UTF8.GetBytes(s);
        fixed (byte* p = b) _api->Log(p, (nuint)b.Length);
    }

    // Errors usually repeat every frame; report each distinct one once.
    static void Error(string what, Exception e)
    {
        if (e is TargetInvocationException { InnerException: { } inner }) e = inner;
        var msg = $"{what}: {e.GetType().Name}: {e.Message}";
        lock (Logged) if (!Logged.Add(msg)) return;
        Log(msg);
    }

    static string Str(byte* p, nuint n) => n == 0 ? "" : Encoding.UTF8.GetString(p, (int)n);
    static object? Obj(nint h) => h == 0 ? null : GCHandle.FromIntPtr(h).Target;
    static nint Handle(object? o) => o == null ? 0 : GCHandle.ToIntPtr(GCHandle.Alloc(o));

    static Type FindType(string name) =>
        Types.GetOrAdd(name, n => Type.GetType(n) ?? AccessTools.TypeByName(n))
        ?? throw new TypeLoadException($"type not found: {name}");

    // obj == null means static member of `ty`.
    static Type Target(object? obj, byte* ty, nuint tyLen) => obj?.GetType() ?? FindType(Str(ty, tyLen));

    static object? Convert(object? v, Type t)
    {
        if (v == null || t.IsInstanceOfType(v)) return v;
        if (t.IsEnum) return Enum.ToObject(t, System.Convert.ToInt64(v));
        return System.Convert.ChangeType(v, Nullable.GetUnderlyingType(t) ?? t);
    }

    // ---- exported API ----

    [UnmanagedCallersOnly]
    static void Free(nint h)
    {
        if (h != 0) GCHandle.FromIntPtr(h).Free();
    }

    [UnmanagedCallersOnly]
    static nint Get(nint h, byte* ty, nuint tyLen, byte* m, nuint mLen)
    {
        try
        {
            var obj = Obj(h);
            var t = Target(obj, ty, tyLen);
            var name = Str(m, mLen);
            if (AccessTools.Property(t, name) is { } p) return Handle(p.GetValue(obj));
            if (AccessTools.Field(t, name) is { } f) return Handle(f.GetValue(obj));
            throw new MissingMemberException(t.FullName, name);
        }
        catch (Exception e) { Error($"get {Str(m, mLen)}", e); return 0; }
    }

    [UnmanagedCallersOnly]
    static byte Set(nint h, byte* ty, nuint tyLen, byte* m, nuint mLen, nint value)
    {
        try
        {
            var obj = Obj(h);
            var t = Target(obj, ty, tyLen);
            var name = Str(m, mLen);
            if (AccessTools.Property(t, name) is { } p) p.SetValue(obj, Convert(Obj(value), p.PropertyType));
            else if (AccessTools.Field(t, name) is { } f) f.SetValue(obj, Convert(Obj(value), f.FieldType));
            else throw new MissingMemberException(t.FullName, name);
            return 1;
        }
        catch (Exception e) { Error($"set {Str(m, mLen)}", e); return 0; }
    }

    [UnmanagedCallersOnly]
    static nint Call(nint h, byte* ty, nuint tyLen, byte* m, nuint mLen, nint* args, nuint n)
    {
        try
        {
            var obj = Obj(h);
            var t = Target(obj, ty, tyLen);
            var name = Str(m, mLen);
            // ponytail: first overload with matching arity wins; add type-based overload resolution if mods need it.
            var method = t.GetMethods(All).FirstOrDefault(x => x.Name == name && x.GetParameters().Length == (int)n)
                ?? throw new MissingMethodException(t.FullName, name);
            var ps = method.GetParameters();
            var a = new object?[(int)n];
            for (int i = 0; i < a.Length; i++) a[i] = Convert(Obj(args[i]), ps[i].ParameterType);
            return Handle(method.Invoke(obj, a));
        }
        catch (Exception e) { Error($"call {Str(m, mLen)}", e); return 0; }
    }

    [UnmanagedCallersOnly]
    static nint BoxF64(double v) => Handle(v);

    [UnmanagedCallersOnly]
    static nint BoxStr(byte* p, nuint n) => Handle(Str(p, n));

    [UnmanagedCallersOnly]
    static byte AsF64(nint h, double* outV)
    {
        try
        {
            if (Obj(h) is not IConvertible c || c is string) return 0;
            *outV = c.ToDouble(null);
            return 1;
        }
        catch { return 0; }
    }

    [UnmanagedCallersOnly]
    static nuint AsStr(nint h, byte* buf, nuint cap)
    {
        try
        {
            var b = Encoding.UTF8.GetBytes(Obj(h)?.ToString() ?? "null");
            Marshal.Copy(b, 0, (nint)buf, (int)Math.Min((nuint)b.Length, cap));
            return (nuint)b.Length;
        }
        catch { return 0; }
    }

    [UnmanagedCallersOnly]
    static long Len(nint h) => Obj(h) is ICollection c ? c.Count : -1;

    [UnmanagedCallersOnly]
    static nint Index(nint h, long i)
    {
        try
        {
            return Obj(h) switch
            {
                IList l => Handle(l[(int)i]),
                IEnumerable e => Handle(e.Cast<object?>().ElementAt((int)i)),
                _ => 0,
            };
        }
        catch (Exception e) { Error($"index {i}", e); return 0; }
    }

    [UnmanagedCallersOnly]
    static byte Patch(byte* ty, nuint tyLen, byte* m, nuint mLen, byte post, nint cb, nint user)
    {
        try
        {
            var t = FindType(Str(ty, tyLen));
            var method = AccessTools.Method(t, Str(m, mLen)) ?? throw new MissingMethodException(t.FullName, Str(m, mLen));
            lock (Hooks)
            {
                var isNew = !Hooks.TryGetValue(method, out var list);
                Hooks[method] = [.. list ?? [], (post != 0, cb, user)];
                if (isNew)
                    Harmony.Patch(method,
                        prefix: new HarmonyMethod(typeof(Bridge), nameof(HookPrefix)),
                        postfix: new HarmonyMethod(typeof(Bridge), nameof(HookPostfix)));
            }
            return 1;
        }
        catch (Exception e) { Error($"patch {Str(ty, tyLen)}.{Str(m, mLen)}", e); return 0; }
    }

    [UnmanagedCallersOnly]
    static nuint DocsPath(byte* buf, nuint cap)
    {
        var dir = (string)AccessTools.Property(FindType("KSA.Constants, KSA"), "DocumentsFolderPath").GetValue(null)!;
        var b = Encoding.UTF8.GetBytes(dir);
        Marshal.Copy(b, 0, (nint)buf, (int)Math.Min((nuint)b.Length, cap));
        return (nuint)b.Length;
    }

    static bool RunHooks(MethodBase original, object? instance, object[] args, bool post)
    {
        if (!Hooks.TryGetValue(original, out var list)) return true;
        bool run = true;
        foreach (var (p, cb, user) in list)
            if (p == post) // handles are owned (freed) by the callback
                run &= ((delegate* unmanaged<nint, nint, nint, byte>)cb)(user, Handle(instance), Handle(args)) != 0;
        return run;
    }

    static bool HookPrefix(MethodBase __originalMethod, object? __instance, object[] __args) =>
        RunHooks(__originalMethod, __instance, __args, false);

    static void HookPostfix(MethodBase __originalMethod, object? __instance, object[] __args) =>
        RunHooks(__originalMethod, __instance, __args, true);
}
