// Physics forces from native mods (API version 6).
//
// Every force on a vehicle is summed in the static KSA.PhysicsStates.ComputeDerivatives, once or more per physics
// substep, on the game's vehicle worker threads (several vehicles at once). It takes ReadOnlySpan arguments, so the
// generic object[] hooks can't patch it (Harmony emits invalid IL for them). Instead a transpiler injects, before each
// `ret`: copy the vehicle's state into a per-thread ForceInput, call Oxidizer (which calls the mods' force callbacks),
// and add the returned force and torque to the method's Disturbances result with the game's own AddForceBody and
// AddPureTorqueBody. No boxing, reflection or allocation per call.
//
// The bridge isn't compiled against KSA.dll, so the injected IL names KSA types and members found by reflection when
// the patch is made; a game update that renames one makes Enable fail with a message instead of breaking physics.
//
// ComputeDerivatives doesn't get the vehicle. VehicleUpdateState.GetNewStates (the parallel per-vehicle step) and
// GetReadOnlyStates (the serial constrained step) are called right before it on the same thread, so a postfix on
// both records the vehicle, per thread, for the next ComputeDerivatives.
using System;
using System.Collections.Generic;
using System.Linq;
using System.Reflection;
using System.Reflection.Emit;
using System.Runtime.CompilerServices;
using System.Runtime.InteropServices;
using System.Threading;
using HarmonyLib;

namespace Oxidizer;

/// What a force callback gets. Must match `ForceInput` in oxidizer_api/src/lib.rs.
/// "Bub" is the physics frame: centered on the parent body, with axes inertial (Frame 0, Cci) or rotating with the
/// body (Frame 1, Ccf). "Body" axes are the vehicle's (the same as its assembly axes).
[StructLayout(LayoutKind.Sequential)]
public struct ForceInput
{
    public ulong Vehicle;               // Oxidizer's key for the vehicle (oxidizer_api vehicle_key), 0 if unknown
    public int Frame;                   // 0 Cci (inertial), 1 Ccf (rotating with the parent body)
    public int InPhysicsRadius;         // 1 when the game applies drag/buoyancy at all
    public double Dt;                   // substep length, s
    public double IntraStepTime;        // time into the step, s
    public double PositionX, PositionY, PositionZ;     // vehicle CoM relative to the parent body, Bub, m
    public double VelocityX, VelocityY, VelocityZ;     // Bub, m/s (in Ccf this is already relative to the air)
    public double Body2PhysX, Body2PhysY, Body2PhysZ, Body2PhysW; // rotation body -> Bub (quaternion)
    public double AngularVelocityX, AngularVelocityY, AngularVelocityZ; // Bub, rad/s
    public double ParentSpinX, ParentSpinY, ParentSpinZ; // parent body angular velocity (Cci), rad/s
    public double Density, Pressure, Temperature, SpeedOfSound; // atmosphere at the vehicle (0 above it)
    public double OceanDensity, OceanVolume, OceanSurfaceArea;  // immersed part, if any
    public double Mass;                 // kg
    public double CenterOfMassX, CenterOfMassY, CenterOfMassZ; // vehicle assembly frame, m
    public double BoundingSphereRadius; // about the CoM, m
    public double SurfaceArea, Volume;  // the game's estimates, m², m³
    public double StockCdAPositiveX, StockCdAPositiveY, StockCdAPositiveZ; // the game's own drag areas (m²)
    public double StockCdANegativeX, StockCdANegativeY, StockCdANegativeZ; // per body axis and direction
}

/// What a force callback adds: body axes, newtons and newton-metres about the center of mass.
[StructLayout(LayoutKind.Sequential)]
public struct ForceOutput
{
    public double ForceX, ForceY, ForceZ;
    public double TorqueX, TorqueY, TorqueZ;
}

public static unsafe class Forces
{
    /// Oxidizer's dispatcher: fills output, returns 1 when there is a force to add.
    internal static delegate* unmanaged<ForceInput*, ForceOutput*, byte> Native;

    [ThreadStatic] static ForceInput* _in;
    [ThreadStatic] static ForceOutput* _out;
    [ThreadStatic] static ulong _vehicle;

    // VehicleUpdateState -> its vehicle's key, and Vehicle -> key. Weak, so unloaded vehicles go away.
    static readonly ConditionalWeakTable<object, StrongBox<ulong>> StateKeys = new(), VehicleKeys = new();
    static long _lastKey;
    static FieldInfo? _stateVehicle;
    static bool _enabled;
    static readonly object Gate = new();

    /// The key of a vehicle, the same one force callbacks get in ForceInput.Vehicle.
    public static ulong KeyOf(object vehicle) => VehicleKeys.GetValue(vehicle, _ => new((ulong)Interlocked.Increment(ref _lastKey))).Value;

    /// Patches the game, once. Returns false (and says why) when it can't.
    public static bool Enable(Harmony harmony, Action<string> log, string physicsStates = "KSA.PhysicsStates, KSA", string updateState = "KSA.VehicleUpdateState, KSA")
    {
        lock (Gate)
        {
            if (_enabled) return true;
            try
            {
                var states = Type.GetType(physicsStates, true)!;
                var compute = states.GetMethods(BindingFlags.Public | BindingFlags.Static)
                    .Single(m => m.Name == "ComputeDerivatives" && m.GetParameters().Length == 10);
                Injection.Prepare(compute); // fails here, before patching, if the game changed
                var vus = Type.GetType(updateState, true)!;
                _stateVehicle = vus.GetField("ReadOnlyVehicle") ?? throw new MissingFieldException(vus.FullName, "ReadOnlyVehicle");
                var scope = new HarmonyMethod(typeof(Forces), nameof(ScopeVehicle));
                foreach (var name in new[] { "GetNewStates", "GetReadOnlyStates" })
                    harmony.Patch(vus.GetMethod(name, Type.EmptyTypes) ?? throw new MissingMethodException(vus.FullName, name), postfix: scope);
                harmony.Patch(compute, transpiler: new HarmonyMethod(typeof(Injection), nameof(Injection.Transpile)));
                _enabled = true;
                return true;
            }
            catch (Exception e)
            {
                log($"physics forces unavailable: {e.Message}");
                return false;
            }
        }
    }

    static void ScopeVehicle(object __instance) =>
        _vehicle = StateKeys.GetValue(__instance, s => new(KeyOf(_stateVehicle!.GetValue(s)!))).Value;

    // ---- called by the injected IL ----

    public static ForceInput* In()
    {
        if (_in == null)
        {
            // Per thread, never freed: the game's worker threads live as long as the game.
            _in = (ForceInput*)NativeMemory.AllocZeroed((nuint)sizeof(ForceInput));
            _out = (ForceOutput*)NativeMemory.AllocZeroed((nuint)sizeof(ForceOutput));
        }
        return _in;
    }

    public static ForceOutput* Out() => _out;

    public static bool Run()
    {
        var native = Native;
        if (native == null) return false;
        _in->Vehicle = _vehicle;
        *_out = default;
        try { return native(_in, _out) != 0; }
        catch { return false; } // the native side catches panics; this is only a guard
    }
}

/// Builds the IL injected into ComputeDerivatives.
static class Injection
{
    static MethodBase? _target;
    static List<(FieldInfo to, Func<ILGenerator, List<CodeInstruction>> load)>? _fields;

    /// Resolves every KSA member the injection needs, so a mismatch fails before anything is patched.
    public static void Prepare(MethodBase target)
    {
        _target = target;
        var F = (string n) => typeof(ForceInput).GetField(n)!;
        var list = new List<(FieldInfo, Func<ILGenerator, List<CodeInstruction>>)>();
        void Add(string field, params string[] path) => list.Add((F(field), il => Load(il, path)));
        void Sum(string field, string[] a, string[] b) => list.Add((F(field), il => [.. Load(il, a), .. Load(il, b), new(OpCodes.Add)]));

        Add("Frame", "origin", "BubFrame");
        Add("InPhysicsRadius", "environment", "InPhysicsRadius");
        Add("Dt", "dt");
        Add("IntraStepTime", "baseIntraStepTime");
        foreach (var c in "XYZ")
        {
            Sum($"Position{c}", ["positionOriginBub", $"{c}"], ["kinematic", "PositionPhys", $"{c}"]);
            Sum($"Velocity{c}", ["velocityOriginBub", $"{c}"], ["kinematic", "VelocityPhys", $"{c}"]);
            Add($"AngularVelocity{c}", "kinematic", "AngularVelocityPhys", $"{c}");
            Add($"ParentSpin{c}", "origin", "Parent", "GetAngularVelocityCci()", $"{c}");
            Add($"CenterOfMass{c}", "props", "CenterOfMassAsmb", $"{c}");
            Add($"StockCdAPositive{c}", "props", "AerodynamicCdABody", "Positive", $"{c}");
            Add($"StockCdANegative{c}", "props", "AerodynamicCdABody", "Negative", $"{c}");
        }
        foreach (var c in "XYZW") Add($"Body2Phys{c}", "kinematic", "Body2Phys", $"{c}");
        Add("Density", "environment", "AtmosphericDensity");
        Add("Pressure", "environment", "AtmosphericPressure");
        Add("Temperature", "environment", "AtmosphericTemperature");
        Add("SpeedOfSound", "environment", "SpeedOfSound");
        Add("OceanDensity", "environment", "OceanDensity");
        Add("OceanVolume", "environment", "OceanVolume");
        Add("OceanSurfaceArea", "environment", "OceanSurfaceArea");
        Add("Mass", "props", "TotalMass");
        Add("BoundingSphereRadius", "props", "ComputeBoundingSphereRadiusAsmb()");
        Add("SurfaceArea", "props", "TotalSurfaceArea");
        Add("Volume", "props", "TotalVolume");
        _fields = list;

        // Resolve everything once now (a scratch method's generator), so errors come out of Prepare.
        var probe = new DynamicMethod("probe", typeof(void), Type.EmptyTypes).GetILGenerator();
        foreach (var (_, load) in list) load(probe);
        ResultMembers(((MethodInfo)target).ReturnType);
    }

    static (MethodInfo addForce, MethodInfo addTorque, ConstructorInfo vec) ResultMembers(Type disturbances)
    {
        var addForce = disturbances.GetMethod("AddForceBody") ?? throw new MissingMethodException(disturbances.FullName, "AddForceBody");
        var addTorque = disturbances.GetMethod("AddPureTorqueBody") ?? throw new MissingMethodException(disturbances.FullName, "AddPureTorqueBody");
        var vec = addForce.GetParameters()[1].ParameterType;
        var ctor = vec.GetConstructor([typeof(double), typeof(double), typeof(double)]) ?? throw new MissingMethodException(vec.FullName, ".ctor(double, double, double)");
        return (addForce, addTorque, ctor);
    }

    public static IEnumerable<CodeInstruction> Transpile(IEnumerable<CodeInstruction> code, ILGenerator il, MethodBase original)
    {
        var resultType = ((MethodInfo)original).ReturnType;
        var (addForce, addTorque, vec) = ResultMembers(resultType);
        var result = il.DeclareLocal(resultType);
        var input = il.DeclareLocal(typeof(ForceInput*));
        var output = il.DeclareLocal(typeof(ForceOutput*));
        foreach (var ci in code)
        {
            if (ci.opcode != OpCodes.Ret)
            {
                yield return ci;
                continue;
            }
            // The returned Disturbances is on the stack: keep it, add to it, return it. Jumps to this ret land on the stloc.
            var keep = new CodeInstruction(OpCodes.Stloc, result) { labels = [.. ci.labels], blocks = [.. ci.blocks] };
            yield return keep;
            var done = il.DefineLabel();
            yield return CodeInstruction.Call(typeof(Forces), nameof(Forces.In));
            yield return new CodeInstruction(OpCodes.Stloc, input);
            foreach (var (to, load) in _fields!)
            {
                yield return new CodeInstruction(OpCodes.Ldloc, input);
                foreach (var x in load(il)) yield return x;
                yield return new CodeInstruction(to.FieldType == typeof(int) ? OpCodes.Conv_I4 : OpCodes.Conv_R8);
                yield return new CodeInstruction(OpCodes.Stfld, to);
            }
            yield return CodeInstruction.Call(typeof(Forces), nameof(Forces.Run));
            yield return new CodeInstruction(OpCodes.Brfalse, done);
            yield return CodeInstruction.Call(typeof(Forces), nameof(Forces.Out));
            yield return new CodeInstruction(OpCodes.Stloc, output);
            // result.AddForceBody(double3(0,0,0), F): force at the CoM (its torque comes separately).
            yield return new CodeInstruction(OpCodes.Ldloca, result);
            for (int i = 0; i < 3; i++) yield return new CodeInstruction(OpCodes.Ldc_R8, 0.0);
            yield return new CodeInstruction(OpCodes.Newobj, vec);
            foreach (var x in Vector(output, vec, "Force")) yield return x;
            yield return new CodeInstruction(OpCodes.Call, addForce);
            yield return new CodeInstruction(OpCodes.Ldloca, result);
            foreach (var x in Vector(output, vec, "Torque")) yield return x;
            yield return new CodeInstruction(OpCodes.Call, addTorque);
            yield return new CodeInstruction(OpCodes.Ldloc, result) { labels = [done] };
            yield return new CodeInstruction(OpCodes.Ret);
        }
    }

    static IEnumerable<CodeInstruction> Vector(LocalBuilder output, ConstructorInfo vec, string name)
    {
        foreach (var c in "XYZ")
        {
            yield return new CodeInstruction(OpCodes.Ldloc, output);
            yield return new CodeInstruction(OpCodes.Ldfld, typeof(ForceOutput).GetField(name + c));
        }
        yield return new CodeInstruction(OpCodes.Newobj, vec);
    }

    /// IL that leaves `arg.member.member...` on the stack. Members are fields, properties, or "Method()".
    /// Struct members are reached through addresses; a struct returned by a call goes through a temporary local.
    static List<CodeInstruction> Load(ILGenerator il, string[] path)
    {
        var ps = _target!.GetParameters();
        int index = Array.FindIndex(ps, p => p.Name == path[0]);
        if (index < 0) throw new MissingMemberException(_target.Name, path[0]);
        var t = ps[index].ParameterType;
        var code = new List<CodeInstruction>();
        bool byRef = t.IsByRef;
        if (byRef) t = t.GetElementType()!;
        // Value-type arguments passed by value: take their address to read members.
        code.Add(CodeInstruction.LoadArgument(index, useAddress: !byRef && t.IsValueType && path.Length > 1));
        for (int i = 1; i < path.Length; i++)
        {
            bool last = i == path.Length - 1;
            var name = path[i];
            const BindingFlags Any = BindingFlags.Public | BindingFlags.NonPublic | BindingFlags.Instance;
            if (name.EndsWith("()") || t.GetProperty(name, Any) is not null)
            {
                var m = name.EndsWith("()")
                    ? t.GetMethod(name[..^2], Any, Type.EmptyTypes) ?? Interfaces(t).Select(x => x.GetMethod(name[..^2], Type.EmptyTypes)).FirstOrDefault(x => x != null)
                    : t.GetProperty(name, Any)!.GetMethod;
                if (m == null) throw new MissingMethodException(t.FullName, name);
                code.Add(new CodeInstruction(t.IsValueType ? OpCodes.Call : OpCodes.Callvirt, m));
                t = m.ReturnType;
                if (!last && t.IsValueType)
                {
                    var tmp = il.DeclareLocal(t);
                    code.Add(new CodeInstruction(OpCodes.Stloc, tmp));
                    code.Add(new CodeInstruction(OpCodes.Ldloca, tmp));
                }
            }
            else
            {
                var f = t.GetField(name, Any) ?? throw new MissingFieldException(t.FullName, name);
                code.Add(new CodeInstruction(!last && f.FieldType.IsValueType ? OpCodes.Ldflda : OpCodes.Ldfld, f));
                t = f.FieldType;
            }
        }
        return code;
    }

    static IEnumerable<Type> Interfaces(Type t) => t.IsInterface ? [t, .. t.GetInterfaces()] : t.GetInterfaces();
}
