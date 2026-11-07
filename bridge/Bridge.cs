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
    // API version 2
    public delegate* unmanaged<nint> ResultGet;
    public delegate* unmanaged<nint, byte> ResultSet;
    public delegate* unmanaged<byte*, nuint, byte> KeyPressed;
    public delegate* unmanaged<byte*, nuint, byte> KeyDown;
    public delegate* unmanaged<nint, int, nint*, nuint, nint> Invoke;
    public delegate* unmanaged<nint, int, nint> FieldGet;
    public delegate* unmanaged<nint, int, nint, byte> FieldSet;
    public delegate* unmanaged<byte*, nuint, nuint> KsaVersion;
    public delegate* unmanaged<nint, long*, byte> AsI64;
    public delegate* unmanaged<long, nint> BoxI64;
}

// Must match `Host` in src/main.rs.
[StructLayout(LayoutKind.Sequential)]
public unsafe struct Host
{
    public delegate* unmanaged<double, double, void> Frame;
    public delegate* unmanaged<double, void> Gui;
    public delegate* unmanaged<void> Menu;
    public byte* Instance; // UTF-8 instance folder, empty for the normal Documents folder
    public nuint InstanceLen;
}

public static unsafe class Bridge
{
    const BindingFlags All = BindingFlags.Public | BindingFlags.NonPublic | BindingFlags.Instance | BindingFlags.Static | BindingFlags.FlattenHierarchy;

    static Api* _api;
    static Host _host;
    static string? _instance;
    static readonly Harmony Harmony = new("oxidizer");
    static readonly ConcurrentDictionary<string, Type?> Types = new();
    static readonly ConcurrentDictionary<MethodBase, (bool post, nint cb, nint user)[]> Hooks = new();
    static readonly HashSet<string> Logged = new();
    static readonly HashSet<string> Pressed = new(StringComparer.OrdinalIgnoreCase), Down = new(StringComparer.OrdinalIgnoreCase);
    static Module? _ksa;
    static readonly ConcurrentDictionary<int, MethodBase> Methods = new();
    static readonly ConcurrentDictionary<int, FieldInfo> Fields = new();

    // The hook currently running on this thread: its return value and type (null for void methods).
    [ThreadStatic] static object? _result;
    [ThreadStatic] static Type? _resultType;

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
            api->ResultGet = &ResultGet;
            api->ResultSet = &ResultSet;
            api->KeyPressed = &KeyPressed;
            api->KeyDown = &KeyDown;
            api->Invoke = &Invoke;
            api->FieldGet = &FieldGet;
            api->FieldSet = &FieldSet;
            api->KsaVersion = &KsaVersion;
            api->AsI64 = &AsI64;
            api->BoxI64 = &BoxI64;

            var program = Type.GetType("KSA.Program, KSA", true)!;
            _ksa = program.Module;
            Harmony.Patch(AccessTools.Method(program, "OnFrame"), postfix: new HarmonyMethod(typeof(Bridge), nameof(AfterFrame)));
            Harmony.Patch(AccessTools.Method(program, "OnDrawUiViewports"), postfix: new HarmonyMethod(typeof(Bridge), nameof(AfterGui)));
            // Empty method the game calls inside its menu bar, after its own menus.
            Harmony.Patch(AccessTools.Method(program, "DrawProgramMenusHook"), postfix: new HarmonyMethod(typeof(Bridge), nameof(AfterMenus)));
            // Keys the game acts on (not ones typed into a text field or the console).
            Harmony.Patch(AccessTools.Method(program, "DispatchKeyEvent"), prefix: new HarmonyMethod(typeof(Bridge), nameof(OnKeyEvent)));

            if (host->InstanceLen > 0)
            {
                // Borea instance: the game's whole documents folder (mods, saves, settings) lives there.
                _instance = Str(host->Instance, host->InstanceLen);
                var constants = Type.GetType("KSA.Constants, KSA", true)!;
                Harmony.Patch(AccessTools.PropertyGetter(constants, "DocumentsFolderPath"), prefix: new HarmonyMethod(typeof(Bridge), nameof(DocumentsFolder)));
                Log($"instance: {_instance}");
            }
            return 1;
        }
        catch (Exception e)
        {
            Console.WriteLine($"[oxidizer] bridge init failed: {e}");
            return 0;
        }
    }

    static void AfterFrame(double currentPlayerTime, double dtPlayer)
    {
        _host.Frame(currentPlayerTime, dtPlayer);
        lock (Pressed) Pressed.Clear(); // "pressed" lasts one frame
    }

    // RenderCore.Input.GlfwKeyEvent is an internal struct with fields (Window, Action, Key, Mods, Button, IsMouse);
    // read them by name so no game types are referenced.
    static void OnKeyEvent(object[] __args)
    {
        var e = __args[0];
        var t = e.GetType();
        if (AccessTools.Field(t, "IsMouse")?.GetValue(e) is true) return;
        var key = AccessTools.Field(t, "Key")?.GetValue(e)?.ToString();
        var action = AccessTools.Field(t, "Action")?.GetValue(e)?.ToString();
        if (key == null) return;
        lock (Pressed)
        {
            if (action == "Press") { Pressed.Add(key); Down.Add(key); }
            else if (action == "Release") Down.Remove(key);
        }
    }
    static void AfterGui(double dt) => _host.Gui(dt);
    static void AfterMenus() => _host.Menu();

    static bool DocumentsFolder(ref string __result)
    {
        __result = _instance!;
        return false;
    }

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
                var returns = method is MethodInfo mi && mi.ReturnType != typeof(void);
                if (isNew)
                    Harmony.Patch(method,
                        prefix: new HarmonyMethod(typeof(Bridge), returns ? nameof(HookPrefixResult) : nameof(HookPrefix)),
                        postfix: new HarmonyMethod(typeof(Bridge), returns ? nameof(HookPostfixResult) : nameof(HookPostfix)));
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

    static bool RunHooks(MethodBase original, object? instance, object[] args, bool post, ref object? result)
    {
        if (!Hooks.TryGetValue(original, out var list)) return true;
        var (outerResult, outerType) = (_result, _resultType); // hooks can nest
        (_result, _resultType) = (result, (original as MethodInfo)?.ReturnType is { } rt && rt != typeof(void) ? rt : null);
        bool run = true;
        try
        {
            foreach (var (p, cb, user) in list)
                if (p == post) // handles are owned (freed) by the callback
                    run &= ((delegate* unmanaged<nint, nint, nint, byte>)cb)(user, Handle(instance), Handle(args)) != 0;
            result = _result;
        }
        finally { (_result, _resultType) = (outerResult, outerType); }
        return run;
    }

    // Harmony can't give __result for void methods, so those get their own pair.
    static bool HookPrefix(MethodBase __originalMethod, object? __instance, object[] __args)
    {
        object? none = null;
        return RunHooks(__originalMethod, __instance, __args, false, ref none);
    }

    static void HookPostfix(MethodBase __originalMethod, object? __instance, object[] __args)
    {
        object? none = null;
        RunHooks(__originalMethod, __instance, __args, true, ref none);
    }

    static bool HookPrefixResult(MethodBase __originalMethod, object? __instance, object[] __args, ref object? __result) =>
        RunHooks(__originalMethod, __instance, __args, false, ref __result);

    static void HookPostfixResult(MethodBase __originalMethod, object? __instance, object[] __args, ref object? __result) =>
        RunHooks(__originalMethod, __instance, __args, true, ref __result);

    [UnmanagedCallersOnly]
    static nint ResultGet() => _resultType == null ? 0 : Handle(_result);

    [UnmanagedCallersOnly]
    static byte ResultSet(nint value)
    {
        try
        {
            if (_resultType == null) return 0; // not inside a hook of a method that returns something
            _result = Convert(Obj(value), _resultType);
            return 1;
        }
        catch (Exception e) { Error("set_result", e); return 0; }
    }

    [UnmanagedCallersOnly]
    static byte KeyPressed(byte* p, nuint n) { lock (Pressed) return Pressed.Contains(Str(p, n)) ? (byte)1 : (byte)0; }

    [UnmanagedCallersOnly]
    static byte KeyDown(byte* p, nuint n) { lock (Pressed) return Down.Contains(Str(p, n)) ? (byte)1 : (byte)0; }

    // ---- typed bindings: members of KSA.dll addressed by metadata token ----

    static MethodBase Method(int token) => Methods.GetOrAdd(token, t => _ksa!.ResolveMethod(t)!);
    static FieldInfo Field(int token) => Fields.GetOrAdd(token, t => _ksa!.ResolveField(t)!);

    [UnmanagedCallersOnly]
    static nint Invoke(nint h, int token, nint* args, nuint n)
    {
        try
        {
            var m = Method(token);
            var ps = m.GetParameters();
            var a = new object?[(int)n];
            for (int i = 0; i < a.Length; i++) a[i] = Convert(Obj(args[i]), ps[i].ParameterType);
            return Handle(m is ConstructorInfo c ? c.Invoke(a) : m.Invoke(Obj(h), a));
        }
        catch (Exception e) { Error($"invoke 0x{token:x8}", e); return 0; }
    }

    [UnmanagedCallersOnly]
    static nint FieldGet(nint h, int token)
    {
        try { return Handle(Field(token).GetValue(Obj(h))); }
        catch (Exception e) { Error($"field 0x{token:x8}", e); return 0; }
    }

    [UnmanagedCallersOnly]
    static byte FieldSet(nint h, int token, nint value)
    {
        try
        {
            var f = Field(token);
            f.SetValue(Obj(h), Convert(Obj(value), f.FieldType));
            return 1;
        }
        catch (Exception e) { Error($"field 0x{token:x8}", e); return 0; }
    }

    /// The KSA.dll build (module version id); bindings refuse to run against another one.
    [UnmanagedCallersOnly]
    static nuint KsaVersion(byte* buf, nuint cap)
    {
        var b = Encoding.UTF8.GetBytes(_ksa!.ModuleVersionId.ToString());
        Marshal.Copy(b, 0, (nint)buf, (int)Math.Min((nuint)b.Length, cap));
        return (nuint)b.Length;
    }

    [UnmanagedCallersOnly]
    static byte AsI64(nint h, long* outV)
    {
        try
        {
            switch (Obj(h))
            {
                case Enum e: *outV = System.Convert.ToInt64(e); return 1;
                case IConvertible c and not string: *outV = c.ToInt64(null); return 1;
                default: return 0;
            }
        }
        catch { return 0; }
    }

    [UnmanagedCallersOnly]
    static nint BoxI64(long v) => Handle(v);
}
