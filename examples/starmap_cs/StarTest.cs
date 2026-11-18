// A C# StarMap mod that publishes a Ping every 3 seconds and logs every Pong it gets. Oxidizer's starmap_rhai and
// starmap_rust examples answer its pings.
using System;
using StarMap.API;
using StarMap.API.Events;
using StarMap.API.Services;

namespace StarTest;

public sealed record Ping(int Count);
public sealed record Pong(string From, int Count);

[StarMapMod]
public sealed class StarTestMod
{
    readonly IEventBus _events;
    IEventPublisher<Ping>? _pings;
    ISubscription<Pong>? _pongs;
    double _since;
    int _count;

    public StarTestMod(IEventBus events) => _events = events;

    [StarMapBeforeMain]
    public void Init()
    {
        _pings = _events.GetPublisher<Ping>();
        _pongs = _events.Subscribe<Pong>(p => Console.WriteLine($"[StarTest] pong {p.Count} from {p.From}"));
    }

    [StarMapAfterOnFrame]
    public void Frame(double time, double dt)
    {
        _since += dt;
        if (_since < 3) return;
        _since = 0;
        _count++;
        Console.WriteLine($"[StarTest] ping {_count}");
        _pings?.Publish(new Ping(_count));
    }

    [StarMapUnload]
    public void Unload()
    {
        _pings?.Dispose();
        _pongs?.Dispose();
    }
}
