// For Oxidizer's own use (not the mod API): what the Rust core needs to give Rhai scripts typed access to the game.
// Must match `Internal` in src/lib.rs.
using System;
using System.Collections.Generic;
using System.Linq;
using System.Reflection;
using System.Runtime.InteropServices;
using System.Text;

namespace Oxidizer;

[StructLayout(LayoutKind.Sequential)]
public unsafe struct Internal
{
    public delegate* unmanaged<byte*, nuint, nuint> Describe;
    public delegate* unmanaged<nint, byte> Kind;
}

public static unsafe class Describer
{
    const BindingFlags Declared = BindingFlags.Public | BindingFlags.Instance | BindingFlags.Static | BindingFlags.DeclaredOnly;
    static byte[]? _description;

    public static void Fill(Internal* i)
    {
        i->Describe = &Describe;
        i->Kind = &Kind;
    }

    /// The public types and members of KSA.dll, one per line, tab-separated:
    ///   S  type  kind  name  arity  token  setter  params  returns
    ///        static member: m method, c constructor, p property (token = getter, setter = setter token or 0),
    ///        f field (setter = 1 if writable); params "name:Type,..." and returns a type name, for editors
    ///   E  type  name  value                        enum value
    ///   I  kind  name  arity  writable              instance member name (m, p or f), of any KSA or game library type
    /// Types are full names with '+' for nested types. Members whose types can't cross the bridge are left out.
    [UnmanagedCallersOnly]
    static nuint Describe(byte* buf, nuint cap)
    {
        try
        {
            _description ??= Encoding.UTF8.GetBytes(Build());
            Marshal.Copy(_description, 0, (nint)buf, (int)Math.Min((nuint)_description.Length, cap));
            return (nuint)_description.Length;
        }
        catch (Exception e)
        {
            Console.WriteLine($"[oxidizer] describe failed: {e}");
            return 0;
        }
    }

    /// 0 null, 1 bool, 2 integer or enum, 3 floating point, 4 string, 5 other object.
    [UnmanagedCallersOnly]
    static byte Kind(nint h) => (h == 0 ? null : GCHandle.FromIntPtr(h).Target) switch
    {
        null => 0,
        bool => 1,
        Enum or sbyte or byte or short or ushort or int or uint or long or ulong or char => 2,
        float or double or decimal => 3,
        string => 4,
        _ => 5,
    };

    static Type[] TypesOf(Assembly a)
    {
        try { return a.GetTypes(); }
        catch (ReflectionTypeLoadException e) { return e.Types.Where(t => t != null).ToArray()!; }
    }

    static bool Visible(Type t) => t.IsPublic || (t.IsNestedPublic && Visible(t.DeclaringType!));

    static bool Usable(Type t) =>
        !t.IsGenericType && !t.Name.Contains('<') && Visible(t) && !typeof(Delegate).IsAssignableFrom(t);

    /// Whether a value of this type can cross the bridge (no refs, pointers, spans or open generics).
    static bool Passable(Type t) => !(t.IsByRef || t.IsPointer || t.IsByRefLike || t.ContainsGenericParameters);

    static bool Callable(MethodBase m) =>
        !m.IsGenericMethodDefinition && !m.Name.Contains('<') && m.GetParameters().All(p => Passable(p.ParameterType))
        && (m is not MethodInfo mi || mi.ReturnType == typeof(void) || Passable(mi.ReturnType));

    static string Params(MethodBase m) => string.Join(",", m.GetParameters().Select(p => $"{p.Name}:{p.ParameterType.Name}"));

    static string Build()
    {
        var ksa = Type.GetType("KSA.Program, KSA", true)!.Assembly;
        var sb = new StringBuilder();
        var instance = new Dictionary<(char kind, string name, int arity), bool>();

        void AddInstance(char kind, string name, int arity, bool writable) =>
            instance[(kind, name, arity)] = writable || instance.GetValueOrDefault((kind, name, arity));

        foreach (var t in TypesOf(ksa).Where(Usable))
        {
            var name = t.FullName!;
            if (t.IsEnum)
            {
                foreach (var f in t.GetFields(BindingFlags.Public | BindingFlags.Static))
                    sb.Append($"E\t{name}\t{f.Name}\t{System.Convert.ToInt64(f.GetRawConstantValue())}\n");
                continue;
            }
            if (!t.IsAbstract && !t.IsValueType)
                foreach (var c in t.GetConstructors(BindingFlags.Public | BindingFlags.Instance).Where(Callable))
                    sb.Append($"S\t{name}\tc\tnew\t{c.GetParameters().Length}\t{c.MetadataToken}\t0\t{Params(c)}\t{t.Name}\n");
            foreach (var p in t.GetProperties(Declared))
            {
                if (p.GetIndexParameters().Length > 0 || !Passable(p.PropertyType)) continue;
                var get = p.GetMethod is { IsPublic: true } g ? g : null;
                var set = p.SetMethod is { IsPublic: true } s ? s : null;
                if (get?.IsStatic ?? set?.IsStatic ?? false)
                {
                    if (get != null) sb.Append($"S\t{name}\tp\t{p.Name}\t0\t{get.MetadataToken}\t{set?.MetadataToken ?? 0}\t\t{p.PropertyType.Name}\n");
                }
                else if (get != null) AddInstance('p', p.Name, 0, set != null);
            }
            foreach (var f in t.GetFields(Declared))
            {
                if (f.Name.Contains('<') || !Passable(f.FieldType)) continue;
                var writable = !f.IsInitOnly && !f.IsLiteral;
                if (f.IsStatic) sb.Append($"S\t{name}\tf\t{f.Name}\t0\t{f.MetadataToken}\t{(writable ? 1 : 0)}\t\t{f.FieldType.Name}\n");
                else AddInstance('f', f.Name, 0, writable);
            }
            foreach (var m in t.GetMethods(Declared).Where(m => !m.IsSpecialName && Callable(m)))
            {
                if (m.IsStatic) sb.Append($"S\t{name}\tm\t{m.Name}\t{m.GetParameters().Length}\t{m.MetadataToken}\t0\t{Params(m)}\t{m.ReturnType.Name}\n");
                else AddInstance('m', m.Name, m.GetParameters().Length, false);
            }
        }

        // Instance members of the game's other libraries too (e.g. the vector types in Brutal.Numerics), by name only.
        // (Loaded here: before the game starts, KSA.dll's references aren't loaded yet.)
        var libraries = ksa.GetReferencedAssemblies()
            .Where(n => !n.Name!.StartsWith("System") && !n.Name.StartsWith("Microsoft") && n.Name != "mscorlib" && n.Name != "netstandard");
        foreach (var lib in libraries)
        {
            Assembly a;
            try { a = Assembly.Load(lib); } catch { continue; }
            foreach (var t in TypesOf(a).Where(Usable))
            {
                foreach (var p in t.GetProperties(BindingFlags.Public | BindingFlags.Instance | BindingFlags.DeclaredOnly))
                    if (p.GetIndexParameters().Length == 0 && p.GetMethod is { IsPublic: true } && Passable(p.PropertyType))
                        AddInstance('p', p.Name, 0, p.SetMethod is { IsPublic: true });
                foreach (var f in t.GetFields(BindingFlags.Public | BindingFlags.Instance | BindingFlags.DeclaredOnly))
                    if (Passable(f.FieldType)) AddInstance('f', f.Name, 0, !f.IsInitOnly);
                foreach (var m in t.GetMethods(BindingFlags.Public | BindingFlags.Instance | BindingFlags.DeclaredOnly).Where(m => !m.IsSpecialName && Callable(m)))
                    AddInstance('m', m.Name, m.GetParameters().Length, false);
            }
        }
        foreach (var ((kind, name, arity), writable) in instance)
            sb.Append($"I\t{kind}\t{name}\t{arity}\t{(writable ? 1 : 0)}\n");
        return sb.ToString();
    }
}
