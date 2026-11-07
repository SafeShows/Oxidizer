// Entry point when StarMap loads the bridge as the mod Oxidizer.StarMap. StarMap has already started .NET and
// the game (and handles -InstancePath), so this only loads Oxidizer's core library from the mod folder and hands
// it Bridge.Init, the same function the standalone launcher calls. Standalone Oxidizer never touches this class,
// so StarMap.API (referenced compile-only, provided by StarMap at runtime) is never needed there.
using System;
using System.IO;
using System.Runtime.InteropServices;
using StarMap.API;

namespace Oxidizer;

[StarMapMod]
public unsafe class StarMapEntry
{
    static delegate* unmanaged<void> _shutdown;

    [StarMapBeforeMain]
    public void BeforeMain()
    {
        try
        {
            var dir = Path.GetDirectoryName(typeof(StarMapEntry).Assembly.Location)!;
            var file = OperatingSystem.IsWindows() ? "oxidizer_core.dll"
                : OperatingSystem.IsMacOS() ? "liboxidizer_core.dylib"
                : "liboxidizer_core.so";
            var lib = NativeLibrary.Load(Path.Combine(dir, file));
            var attach = (delegate* unmanaged<delegate* unmanaged<Api*, Host*, Internal*, int>, int>)NativeLibrary.GetExport(lib, "oxidizer_attach");
            _shutdown = (delegate* unmanaged<void>)NativeLibrary.GetExport(lib, "oxidizer_shutdown");
            if (attach(&Bridge.Init) == 0) Console.WriteLine("[oxidizer] could not start (see above)");
        }
        catch (Exception e)
        {
            Console.WriteLine($"[oxidizer] could not start: {e}");
        }
    }

    [StarMapUnload]
    public void Unload()
    {
        if (_shutdown != null) _shutdown();
    }
}
