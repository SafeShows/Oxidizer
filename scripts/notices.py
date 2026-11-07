"""Write THIRD-PARTY-NOTICES.txt for the release zip.

Covers every crate compiled into the oxidizer executable on Windows or Linux (from `cargo metadata`) and the
libraries inside bridge/0Harmony.dll. Usage: python scripts/notices.py <out file>
"""

import json
import pathlib
import re
import subprocess
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
MIT = """Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE."""

# Merged into 0Harmony.dll; their licenses are not in the Lib.Harmony package.
HARMONY_BUNDLED = [
    ("MonoMod", "https://github.com/MonoMod/MonoMod", "Copyright (c) 2015 - 2020 0x0ade"),
    ("Mono.Cecil", "https://github.com/jbevain/cecil",
     "Copyright (c) 2008 - 2015 Jb Evain\nCopyright (c) 2008 - 2011 Novell, Inc."),
]


def crates():
    meta = json.loads(subprocess.check_output(
        ["cargo", "metadata", "--format-version", "1",
         "--filter-platform", "x86_64-pc-windows-msvc", "--filter-platform", "x86_64-unknown-linux-gnu"],
        cwd=ROOT))
    pkgs = {p["id"]: p for p in meta["packages"]}
    nodes = {n["id"]: n for n in meta["resolve"]["nodes"]}
    root = next(i for i, p in pkgs.items() if p["name"] == "oxidizer" and p["source"] is None)
    seen, stack = set(), [root]
    def proc_macro(i):  # runs at build time only, nothing of it ships
        return any("proc-macro" in t["kind"] for t in pkgs[i]["targets"])

    while stack:  # normal (runtime) dependencies only
        for dep in nodes[stack.pop()]["deps"]:
            if any(k["kind"] is None for k in dep["dep_kinds"]) and dep["pkg"] not in seen and not proc_macro(dep["pkg"]):
                seen.add(dep["pkg"])
                stack.append(dep["pkg"])
    ours = {"oxidizer", "oxidizer_api"}
    return sorted((pkgs[i] for i in seen if pkgs[i]["name"] not in ours), key=lambda p: p["name"])


def license_files(pkg):
    d = pathlib.Path(pkg["manifest_path"]).parent
    return sorted(f for f in d.iterdir() if f.is_file() and re.match(r"(LICEN[CS]E|COPYING|NOTICE)", f.name, re.I))


def harmony_license():
    csproj = (ROOT / "bridge" / "Oxidizer.Bridge.csproj").read_text()
    version = re.search(r'Include="Lib.Harmony" Version="([^"]+)"', csproj).group(1)
    path = pathlib.Path.home() / ".nuget" / "packages" / "lib.harmony" / version / "LICENSE"
    return version, path.read_text(encoding="utf-8").strip()


def main(out):
    rule = "=" * 78
    parts = ["Oxidizer includes the following third-party software.\n"]
    version, text = harmony_license()
    parts += [rule, f"Harmony {version} (bridge/0Harmony.dll)\nhttps://github.com/pardeike/Harmony\nMIT", rule, text, ""]
    for name, url, copyright in HARMONY_BUNDLED:
        parts += [rule, f"{name} (merged into bridge/0Harmony.dll)\n{url}\nMIT", rule, f"MIT License\n\n{copyright}\n\n{MIT}", ""]
    for p in crates():
        head = f"{p['name']} {p['version']} (compiled into oxidizer)\n{p.get('repository') or 'https://crates.io/crates/' + p['name']}\n{p['license']}"
        files = license_files(p)
        body = "\n\n".join(f"--- {f.name} ---\n{f.read_text(encoding='utf-8', errors='replace').strip()}" for f in files)
        parts += [rule, head, rule, body or f"No license file ships with this crate. It is licensed under {p['license']}; see its repository.", ""]
    pathlib.Path(out).write_text("\n".join(parts), encoding="utf-8")


if __name__ == "__main__":
    main(sys.argv[1] if len(sys.argv) > 1 else "THIRD-PARTY-NOTICES.txt")
