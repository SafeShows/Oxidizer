// The Game API page: searches game-api.json (written by `oxidizer --bindgen <file>.json`) and shows matching types
// with their members. Runs only on the page that has #game-api.
(function () {
  const script = document.currentScript;
  function start() {
    const root = document.getElementById("game-api");
    if (!root) return;
    const input = document.getElementById("game-api-search");
    const status = document.getElementById("game-api-status");
    const results = document.getElementById("game-api-results");
    const MAX_TYPES = 60;
    let api = null;

    const esc = (s) => String(s ?? "").replace(/[&<>"]/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" })[c]);

    // Colors a C#, Rust or Rhai signature: keywords, types, functions, module paths, numbers, punctuation.
    const KEYWORDS = new Set(["pub", "fn", "self", "Self", "mut", "get", "set", "new", "obj", "static"]);
    const PRIMITIVES = new Set(["f64", "i64", "bool", "str", "int", "float", "String"]);
    function color(sig) {
      const s = String(sig ?? "");
      let out = "";
      const re = /([A-Za-z_][A-Za-z0-9_`]*)|(\d+(?:\.\d+)?)|(::|->|[&(){}<>,:;=.\[\]?])|(\s+)|(.)/g;
      let m;
      while ((m = re.exec(s))) {
        const [tok, word, num, punct] = m;
        if (word) {
          const rest = s.slice(re.lastIndex);
          const cls = KEYWORDS.has(word) ? "k"
            : rest.startsWith("::") ? "m"
            : rest.startsWith("(") || rest.trimStart().startsWith("{") ? "f"
            : PRIMITIVES.has(word) || /^[A-Z]/.test(word) ? "t"
            : rest.startsWith(":") ? "v" : "";
          out += cls ? `<span class="${cls}">${esc(word)}</span>` : esc(word);
        } else if (num) out += `<span class="n">${num}</span>`;
        else if (punct) out += `<span class="p">${esc(punct)}</span>`;
        else out += esc(tok);
      }
      return out;
    }

    function memberRows(members, q) {
      return members
        .map((m) => {
          const hit = q && [m.cs, m.rust, m.rhai].some((x) => String(x).toLowerCase().includes(q));
          return `<tr${hit ? ' class="game-api-hit"' : ""}><td>${esc(m.kind)}${m.static ? " <em>static</em>" : ""}</td>` +
            `<td><code>${color(m.cs)}</code></td><td><code>${color(m.rust)}</code></td><td><code>${color(m.rhai)}</code></td></tr>`;
        })
        .join("");
    }

    function typeBlock(t, q, open) {
      const base = t.base ? ` · base <a href="#${esc(t.base)}" class="game-api-base">${esc(t.base)}</a>` : "";
      const details = document.createElement("details");
      details.id = t.name;
      details.open = open;
      details.innerHTML =
        `<summary><code>${esc(t.name)}</code> <span class="game-api-meta">${esc(t.kind)} · ${t.members.length} members · Rust <code>${esc(t.rust)}</code>${base}</span></summary>`;
      const fill = () => {
        if (details.dataset.filled) return;
        details.dataset.filled = "1";
        details.insertAdjacentHTML(
          "beforeend",
          t.members.length
            ? `<table class="game-api-members"><thead><tr><th>Kind</th><th>C#</th><th>Rust</th><th>Rhai</th></tr></thead><tbody>${memberRows(t.members, q)}</tbody></table>`
            : "<p>No members mods can use.</p>"
        );
      };
      if (open) fill();
      details.addEventListener("toggle", () => details.open && fill());
      return details;
    }

    function render() {
      if (!api) return;
      const q = input.value.trim().toLowerCase();
      results.textContent = "";
      let matches;
      if (!q) {
        matches = api.types.filter((t) => /^KSA\.[A-Z][A-Za-z]*$/.test(t.name) && t.members.length > 20).slice(0, MAX_TYPES);
        status.textContent = `KSA ${api.game} · ${api.types.length} types · ${api.count} members. Search, or start with some of the bigger types:`;
      } else {
        const byType = [], byMember = [];
        for (const t of api.types) {
          if (t.name.toLowerCase().includes(q) || t.rust.toLowerCase().includes(q)) byType.push(t);
          else if (t.members.some((m) => [m.cs, m.rust, m.rhai].some((x) => String(x).toLowerCase().includes(q)))) byMember.push(t);
        }
        byType.sort((a, b) => a.name.length - b.name.length);
        matches = byType.concat(byMember);
        status.textContent = matches.length
          ? `${matches.length} types match` + (matches.length > MAX_TYPES ? ` (showing ${MAX_TYPES}; type more to narrow it down)` : "") + "."
          : "Nothing matches.";
        matches = matches.slice(0, MAX_TYPES);
      }
      const openFirst = q && matches.length <= 3;
      for (const t of matches) results.appendChild(typeBlock(t, q, openFirst || (q && !t.name.toLowerCase().includes(q))));
    }

    function showHash() {
      const name = decodeURIComponent(location.hash.slice(1));
      if (!api || !name) return;
      const t = api.types.find((x) => x.name === name);
      if (!t) return;
      input.value = t.name;
      render();
      const el = document.getElementById(t.name);
      if (el) { el.open = true; el.dispatchEvent(new Event("toggle")); el.scrollIntoView(); }
    }

    let timer;
    input.addEventListener("input", () => { clearTimeout(timer); timer = setTimeout(render, 150); });
    window.addEventListener("hashchange", showHash);

    fetch(new URL("game-api.json", script.src))
      .then((r) => r.json())
      .then((data) => {
        api = data;
        api.count = data.types.reduce((n, t) => n + t.members.length, 0);
        render();
        showHash();
      })
      .catch((e) => { status.textContent = "Could not load the game's API: " + e; });
  }
  if (document.readyState === "loading") document.addEventListener("DOMContentLoaded", start);
  else start();
})();
