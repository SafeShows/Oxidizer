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
    // API version 3 fields follow, filled by the Rust core after Init (see oxidizer_api::Api); the bridge never touches them.
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
    public delegate* unmanaged<void> Loaded; // the game finished loading its content and mods
    public delegate* unmanaged<ForceInput*, ForceOutput*, byte> Force; // mods' physics forces (see Forces.cs)
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
    public static int Init(Api* api, Host* host, Internal* internals)
    {
        try
        {
            Describer.Fill(internals);
            _api = api;
            _host = *host;
            Forces.Native = host->Force; // the game is only patched once a mod registers a force (EnableForces)
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
            // The game finished loading its content and mods (StarMap's AllModsLoaded is the same moment). Optional:
            // if a game update renames it, mods just don't get loaded().
            try
            {
                var library = Type.GetType("KSA.ModLibrary, KSA", true)!;
                Harmony.Patch(AccessTools.Method(library, "LoadAll"), postfix: new HarmonyMethod(typeof(Bridge), nameof(AfterLoadAll)));
            }
            catch (Exception e)
            {
                Console.WriteLine($"[oxidizer] can't tell mods when the game has loaded (KSA.ModLibrary.LoadAll): {e.Message}");
            }
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

    static void AfterLoadAll() => _host.Loaded();

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

    // ---- overloads ----

    static readonly Dictionary<string, string> Keywords = new(StringComparer.OrdinalIgnoreCase)
    {
        ["bool"] = "Boolean", ["byte"] = "Byte", ["sbyte"] = "SByte", ["short"] = "Int16", ["ushort"] = "UInt16",
        ["int"] = "Int32", ["uint"] = "UInt32", ["long"] = "Int64", ["ulong"] = "UInt64", ["float"] = "Single",
        ["double"] = "Double", ["decimal"] = "Decimal", ["char"] = "Char", ["string"] = "String", ["object"] = "Object",
    };

    /// "Name" or "Name(double, bool)": the name, and the parameter types when given.
    static (string name, string[]? sig) ParseMember(string member)
    {
        int open = member.IndexOf('(');
        if (open < 0 || !member.EndsWith(')')) return (member.Trim(), null);
        var inner = member[(open + 1)..^1].Trim();
        return (member[..open].Trim(), inner.Length == 0 ? [] : inner.Split(',').Select(x => x.Trim()).ToArray());
    }

    /// A parameter type written as a C# keyword (double), a short name (SimSpeed) or a full name (KSA.SimSpeed).
    static bool TypeIs(Type t, string want)
    {
        if (t.IsByRef) t = t.GetElementType()!;
        if (Keywords.TryGetValue(want, out var k)) want = k;
        var full = t.FullName?.Replace('+', '.');
        return t.Name == want || full == want || (full?.EndsWith("." + want) ?? false);
    }

    /// All methods named `name` on `t` and its base classes, public or not.
    static List<MethodInfo> Overloads(Type t, string name)
    {
        // From the type up through its bases; a base method that a derived one overrides or hides (same parameter
        // types) is left out, so `UncompressedSave.Write` is the override, not that plus `GameSave`'s abstract one.
        var found = new List<MethodInfo>();
        for (var c = t; c != null; c = c.BaseType)
            foreach (var m in c.GetMethods(All | BindingFlags.DeclaredOnly).Where(x => x.Name == name))
                if (!found.Any(f => f.GetParameters().Select(p => p.ParameterType).SequenceEqual(m.GetParameters().Select(p => p.ParameterType))))
                    found.Add(m);
        return found;
    }

    static string Describe(IEnumerable<MethodInfo> ms) =>
        string.Join("; ", ms.Select(x => $"{x.Name}({string.Join(", ", x.GetParameters().Select(p => p.ParameterType.Name))})"));

    /// The overload to call or patch. With a signature, that exact one. Otherwise, for a call (`args` given): the
    /// only one with that many parameters, or the first whose parameter types are exactly the arguments', or the first
    /// that can take them. For a patch (`args` null): the only overload. Errors list the overloads to choose from.
    static MethodInfo PickMethod(Type t, string member, object?[]? args)
    {
        var (name, sig) = ParseMember(member);
        var all = Overloads(t, name);
        if (all.Count == 0) throw new MissingMethodException($"{t.FullName} has no method {name}");
        if (sig != null)
            return all.FirstOrDefault(x => x.GetParameters() is var ps && ps.Length == sig.Length && ps.Zip(sig).All(p => TypeIs(p.First.ParameterType, p.Second)))
                ?? throw new MissingMethodException($"{t.FullName} has no {member}; there are: {Describe(all)}");
        if (args == null)
            return all.Count == 1 ? all[0]
                : throw new AmbiguousMatchException($"{t.FullName}.{name} has {all.Count} overloads, name one like \"{name}(double)\": {Describe(all)}");
        var fit = all.Where(x => x.GetParameters().Length == args.Length).ToList();
        if (fit.Count == 1)
            return Accepts(fit[0].GetParameters(), args) ? fit[0]
                : throw new ArgumentException($"{Describe(fit)} can't take these arguments; there are: {Describe(all)}; name one like \"{name}(double)\"");
        if (fit.Count == 0) throw new MissingMethodException($"{t.FullName}.{name} takes no {args.Length} arguments; there are: {Describe(all)}");
        return fit.FirstOrDefault(x => x.GetParameters().Zip(args).All(p => p.Second == null || p.First.ParameterType.IsInstanceOfType(p.Second)))
            ?? fit.FirstOrDefault(x => Accepts(x.GetParameters(), args))
            ?? throw new AmbiguousMatchException($"no overload of {t.FullName}.{name} takes these arguments, name one like \"{name}(double)\": {Describe(fit)}");
    }

    static bool Accepts(ParameterInfo[] ps, object?[] args)
    {
        try
        {
            for (int i = 0; i < ps.Length; i++) Convert(args[i], ps[i].ParameterType);
            return true;
        }
        catch { return false; }
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
            var given = new object?[(int)n];
            for (int i = 0; i < given.Length; i++) given[i] = Obj(args[i]);
            var method = PickMethod(t, Str(m, mLen), given);
            var ps = method.GetParameters();
            var a = new object?[given.Length];
            for (int i = 0; i < a.Length; i++) a[i] = Convert(given[i], ps[i].ParameterType);
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
            var method = PickMethod(t, Str(m, mLen), null);
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

    /// Patches the game's physics for mods' forces (the first time a mod registers one): 1 when in place.
    [UnmanagedCallersOnly]
    internal static byte EnableForces() => (byte)(Forces.Enable(Harmony, Log) ? 1 : 0);

    /// The key force callbacks get for a vehicle (KSA.Vehicle), 0 for null or not an object.
    [UnmanagedCallersOnly]
    internal static ulong VehicleKey(nint h)
    {
        try { return Obj(h) is { } v ? Forces.KeyOf(v) : 0; }
        catch (Exception e) { Error("vehicle_key", e); return 0; }
    }

    /// Removes a hook (callback and user value) from every method: a Rust mod being reloaded. The Harmony patch
    /// stays; a method without hooks left just runs as before.
    [UnmanagedCallersOnly]
    internal static void Unhook(nint cb, nint user)
    {
        lock (Hooks)
            foreach (var (method, list) in Hooks)
                Hooks[method] = [.. list.Where(h => h.cb != cb || h.user != user)];
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
