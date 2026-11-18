// A pass-through to StarMap's event bus (StarMap.API.Services.IEventBus, StarMap 0.5.2+), for Oxidizer's Rust and
// Rhai mods. StarMap matches messages by their C# type, so Oxidizer mods name it ("Namespace.Type, Assembly").
//
// Everything here goes through reflection on an `object`: standalone Oxidizer never has StarMap.API loaded, and
// this class must not need it. The bus comes from StarMapEntry's constructor (StarMap injects it).
//
// Delivery: StarMap calls a subscriber on whatever thread published. The handler only queues the message (a handle
// to it, and its JSON); Oxidizer takes the queue at the start of the next frame, on the game's thread.
using System;
using System.Collections.Concurrent;
using System.Linq;
using System.Reflection;
using System.Runtime.InteropServices;
using System.Text;
using System.Text.Json;
using System.Text.Json.Serialization;
using System.Threading;

namespace Oxidizer;

public static unsafe class StarMapEvents
{
    internal static object? Bus;
    const int MaxQueued = 10_000;

    static readonly ConcurrentDictionary<long, object> Subs = new();
    /// Subscriptions to types not loaded yet (a StarMap mod that loads after Oxidizer.StarMap): attached as soon as
    /// an assembly loads that has the type.
    static readonly ConcurrentDictionary<long, (string Type, string Channel)> Pending = new();
    static int _assembliesLoaded;
    static bool _watching;
    static readonly ConcurrentDictionary<(Type, string), object> Publishers = new();
    static readonly ConcurrentQueue<(long Id, nint Handle, string Json)> Queue = new();
    static long _nextId;
    [ThreadStatic] static string? _error;
    [ThreadStatic] static string? _json;

    static readonly JsonSerializerOptions Json = new()
    {
        IncludeFields = true,
        PropertyNameCaseInsensitive = true,
        ReferenceHandler = ReferenceHandler.IgnoreCycles,
        MaxDepth = 16,
    };

    static Type BusInterface => Bus!.GetType().GetInterfaces().First(i => i.FullName == "StarMap.API.Services.IEventBus");

    /// "Namespace.Type, Assembly" or "Namespace.Type", searched in every loaded assembly (StarMap mods load into
    /// their own load contexts, which Type.GetType doesn't search).
    static Type? TryFindType(string name)
    {
        try { return FindType(name); }
        catch (TypeLoadException) { return null; }
    }

    static Type FindType(string name)
    {
        var (full, asm) = name.IndexOf(',') is var i and >= 0 ? (name[..i].Trim(), name[(i + 1)..].Trim()) : (name.Trim(), null);
        foreach (var a in AppDomain.CurrentDomain.GetAssemblies())
            if ((asm == null || a.GetName().Name == asm) && a.GetType(full) is { } t)
                return t;
        throw new TypeLoadException($"no loaded type {name} (is the StarMap mod that defines it installed?)");
    }

    static long Fail(Exception e)
    {
        if (e is TargetInvocationException { InnerException: { } inner }) e = inner;
        _error = e.Message;
        return 0;
    }

    static Action<T> Handler<T>(long id) => message =>
    {
        while (Queue.Count >= MaxQueued && Queue.TryDequeue(out var old))
            if (old.Handle != 0) GCHandle.FromIntPtr(old.Handle).Free(); // a runaway publisher: drop the oldest
        string json;
        try { json = JsonSerializer.Serialize(message, typeof(T), Json); }
        catch { json = "null"; }
        Queue.Enqueue((id, Bridge.Handle(message), json));
    };

    [UnmanagedCallersOnly]
    internal static byte Available() => Bus != null ? (byte)1 : (byte)0;

    /// Subscribes to a type (and channel, "" for the default one); the subscription's id, or 0 (see Error).
    [UnmanagedCallersOnly]
    internal static long Subscribe(byte* ty, nuint tyLen, byte* ch, nuint chLen)
    {
        try
        {
            if (Bus == null) throw new InvalidOperationException("StarMap's event bus is only there when Oxidizer runs as a StarMap mod (StarMap 0.5.2 or later)");
            var (name, channel) = (Bridge.Str(ty, tyLen), Bridge.Str(ch, chLen));
            var id = Interlocked.Increment(ref _nextId);
            if (TryFindType(name) is { } t)
                Attach(id, t, channel);
            else
            {
                if (!_watching)
                {
                    _watching = true;
                    AppDomain.CurrentDomain.AssemblyLoad += (_, _) => Interlocked.Increment(ref _assembliesLoaded);
                }
                Pending[id] = (name, channel);
            }
            return id;
        }
        catch (Exception e) { return Fail(e); }
    }

    static void Attach(long id, Type t, string channel)
    {
        var handler = typeof(StarMapEvents).GetMethod(nameof(Handler), BindingFlags.NonPublic | BindingFlags.Static)!.MakeGenericMethod(t).Invoke(null, [id])!;
        var subscribe = BusInterface.GetMethods().First(m => m.Name == "Subscribe" && m.GetParameters().Length == (channel == "" ? 1 : 2)).MakeGenericMethod(t);
        Subs[id] = subscribe.Invoke(Bus, channel == "" ? [handler] : [handler, channel])!;
    }

    /// Attaches pending subscriptions whose type has appeared (checked when assemblies were loaded since).
    static void AttachPending()
    {
        if (Pending.IsEmpty || Interlocked.Exchange(ref _assembliesLoaded, 0) == 0) return;
        foreach (var (id, p) in Pending)
            if (TryFindType(p.Type) is { } t && Pending.TryRemove(id, out _))
            {
                try { Attach(id, t, p.Channel); }
                catch (Exception e) { Bridge.Log($"StarMap subscription to {p.Type}: {(e as TargetInvocationException)?.InnerException?.Message ?? e.Message}"); }
            }
    }

    /// The type names still waiting for their StarMap mod, one per line.
    [UnmanagedCallersOnly]
    internal static nuint Waiting(byte* buf, nuint cap)
    {
        AttachPending();
        return Copy(string.Join('\n', Pending.Values.Select(p => p.Type).Distinct()), buf, cap);
    }

    [UnmanagedCallersOnly]
    internal static void Unsubscribe(long id)
    {
        Pending.TryRemove(id, out _);
        if (Subs.TryRemove(id, out var s)) (s as IDisposable)?.Dispose();
    }

    /// Publishes `handle` (a game object of the type) or, when it is 0, the JSON made into one. 1 when sent.
    [UnmanagedCallersOnly]
    internal static byte Publish(byte* ty, nuint tyLen, byte* ch, nuint chLen, byte* json, nuint jsonLen, nint handle)
    {
        try
        {
            if (Bus == null) throw new InvalidOperationException("StarMap's event bus is only there when Oxidizer runs as a StarMap mod (StarMap 0.5.2 or later)");
            var t = FindType(Bridge.Str(ty, tyLen));
            var channel = Bridge.Str(ch, chLen);
            var message = handle != 0 ? Bridge.Obj(handle) : JsonSerializer.Deserialize(Bridge.Str(json, jsonLen), t, Json);
            if (message != null && !t.IsInstanceOfType(message)) throw new ArgumentException($"a {message.GetType().FullName} is not a {t.FullName}");
            var publisher = Publishers.GetOrAdd((t, channel), _ =>
                BusInterface.GetMethods().First(m => m.Name == "GetPublisher" && m.GetParameters().Length == (channel == "" ? 0 : 1))
                    .MakeGenericMethod(t).Invoke(Bus, channel == "" ? [] : [channel])!);
            var publish = publisher.GetType().GetInterfaces().First(i => i.Name.StartsWith("IEventPublisher")).GetMethod("Publish")!;
            publish.Invoke(publisher, [message]);
            return 1;
        }
        catch (Exception e) { return (byte)Fail(e); }
    }

    /// The next queued message: its subscription and a handle to it (the caller frees it). 0 when there is none;
    /// its JSON is then readable with LastJson.
    [UnmanagedCallersOnly]
    internal static byte Next(long* id, nint* handle)
    {
        AttachPending();
        if (!Queue.TryDequeue(out var m)) return 0;
        (*id, *handle, _json) = (m.Id, m.Handle, m.Json);
        return 1;
    }

    static nuint Copy(string? s, byte* buf, nuint cap)
    {
        var b = Encoding.UTF8.GetBytes(s ?? "");
        Marshal.Copy(b, 0, (nint)buf, (int)Math.Min((nuint)b.Length, cap));
        return (nuint)b.Length;
    }

    [UnmanagedCallersOnly]
    internal static nuint LastJson(byte* buf, nuint cap) => Copy(_json, buf, cap);

    [UnmanagedCallersOnly]
    internal static nuint LastError(byte* buf, nuint cap) => Copy(_error, buf, cap);

    /// The game is closing: let go of everything on StarMap's bus.
    internal static void DisposeAll()
    {
        foreach (var s in Subs.Values) (s as IDisposable)?.Dispose();
        foreach (var p in Publishers.Values) (p as IDisposable)?.Dispose();
        Subs.Clear();
        Publishers.Clear();
    }
}
