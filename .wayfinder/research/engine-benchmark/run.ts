// Grammachy engine benchmark runner (HUF-171).
// Usage: bun run.ts [engine ...]   engines: languagetool harper qwen3b qwen7b claude
// All binaries live under ~/.cache/grammachy-bench (see REPORT.md for setup).

import { spawn } from "bun";

const HOME = process.env.HOME!;
const BENCH = `${HOME}/.cache/grammachy-bench`;
const HERE = import.meta.dir;

type Span = { start: number; end: number; text: string } | null;
type Item = {
  id: string;
  native: "zh" | "ms" | "fr" | "es";
  text: string;
  expected_span: Span;
  expected_fix: string | null;
  error_type: string;
};
type Issue = { start: number; end: number; original: string; fix: string; reason: string };
type ItemResult = {
  id: string;
  latency_ms: number;
  issues: Issue[];
  found: boolean | null;
  found_precise: boolean | null;
  fix_match: boolean | null;
  false_positive: boolean | null;
  error?: string;
};
type EngineResult = {
  engine: string;
  version: string;
  offline: boolean;
  license: string;
  cold_start_ms: number | null;
  rss_kb: number | null;
  items: ItemResult[];
  summary: Record<string, number>;
};

const NATIVE_NAME: Record<string, string> = {
  zh: "Mandarin Chinese",
  ms: "Malay",
  fr: "French",
  es: "Spanish",
};
// LanguageTool motherTongue codes. Malay is not a LanguageTool language.
const LT_MOTHER: Record<string, string | null> = { zh: "zh-CN", ms: null, fr: "fr", es: "es" };

const items: Item[] = JSON.parse(await Bun.file(`${HERE}/testset.json`).text());

const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));
const norm = (s: string) => s.trim().toLowerCase().replace(/\s+/g, " ");

function overlaps(a: Span, b: { start: number; end: number }) {
  return !!a && a.start < b.end && b.start < a.end;
}

function score(item: Item, issues: Issue[]): Pick<ItemResult, "found" | "found_precise" | "fix_match" | "false_positive"> {
  if (!item.expected_span) return { found: null, found_precise: null, fix_match: null, false_positive: issues.length > 0 };
  const hits = issues.filter((i) => overlaps(item.expected_span, i));
  // A span that covers more than half the sentence does not localize the mistake for the user.
  const precise = hits.some((i) => i.end - i.start <= item.text.length / 2);
  const fixMatch = hits.some((i) => {
    // Compare the sentence after applying the engine fix with the sentence after the expected fix.
    const apply = (s: number, e: number, f: string) => norm(item.text.slice(0, s) + f + item.text.slice(e));
    return apply(i.start, i.end, i.fix) === apply(item.expected_span!.start, item.expected_span!.end, item.expected_fix!);
  });
  return { found: hits.length > 0, found_precise: precise, fix_match: fixMatch, false_positive: null };
}

async function waitHttp(url: string, timeoutMs: number): Promise<number> {
  const t0 = performance.now();
  while (performance.now() - t0 < timeoutMs) {
    try {
      const r = await fetch(url);
      if (r.ok) return Math.round(performance.now() - t0);
    } catch {}
    await sleep(50);
  }
  throw new Error(`timeout waiting for ${url}`);
}

async function rssOfTree(pid: number): Promise<number> {
  // Sum RSS of the process and its children (java and llama-server are single processes; bash wrappers are not used).
  const p = spawn(["ps", "-o", "rss=", "--ppid", String(pid), "-p", String(pid)]);
  const out = await new Response(p.stdout).text();
  return out.trim().split("\n").map((l) => parseInt(l.trim(), 10) || 0).reduce((a, b) => a + b, 0);
}

async function runItems(fn: (item: Item) => Promise<Issue[]>): Promise<ItemResult[]> {
  const out: ItemResult[] = [];
  for (const item of items) {
    const t0 = performance.now();
    try {
      const issues = await fn(item);
      out.push({ id: item.id, latency_ms: Math.round(performance.now() - t0), issues, ...score(item, issues) });
    } catch (e: any) {
      out.push({ id: item.id, latency_ms: Math.round(performance.now() - t0), issues: [], found: null, found_precise: null, fix_match: null, false_positive: null, error: String(e?.message ?? e) });
    }
    process.stderr.write(".");
  }
  process.stderr.write("\n");
  return out;
}

function summarize(res: ItemResult[]) {
  const errItems = res.filter((r) => r.found !== null);
  const okItems = res.filter((r) => r.false_positive !== null);
  const lat = res.map((r) => r.latency_ms).sort((a, b) => a - b);
  const byNative: Record<string, number> = {};
  for (const n of ["zh", "ms", "fr", "es"]) {
    const sub = errItems.filter((r) => r.id.startsWith(n));
    byNative[`catch_${n}`] = sub.filter((r) => r.found).length;
  }
  return {
    error_items: errItems.length,
    caught: errItems.filter((r) => r.found).length,
    caught_precise: errItems.filter((r) => r.found_precise).length,
    fix_exact: errItems.filter((r) => r.fix_match).length,
    correct_items: okItems.length,
    false_positives: okItems.filter((r) => r.false_positive).length,
    errors: res.filter((r) => r.error).length,
    issues_per_error_item: Math.round((errItems.reduce((a, r) => a + r.issues.length, 0) / Math.max(1, errItems.length)) * 100) / 100,
    noise_issues_dropped: noiseIssues,
    latency_p50_ms: lat[Math.floor(lat.length / 2)],
    latency_p90_ms: lat[Math.floor(lat.length * 0.9)],
    ...byNative,
  };
}

// ---------- LanguageTool ----------
async function runLanguageTool(): Promise<EngineResult> {
  const dir = `${BENCH}/LanguageTool-6.6`;
  const port = 8081;
  const proc = spawn(["java", "-cp", "languagetool-server.jar", "org.languagetool.server.HTTPServer", "--port", String(port)], { cwd: dir, stdout: "ignore", stderr: "ignore" });
  try {
    const cold = await waitHttp(`http://localhost:${port}/v2/languages`, 120_000);
    // Warm the JIT once so the measured latencies reflect steady state.
    await fetch(`http://localhost:${port}/v2/check`, { method: "POST", headers: { "content-type": "application/x-www-form-urlencoded" }, body: "language=en-US&text=Warm+up." });
    const res = await runItems(async (item) => {
      const params = new URLSearchParams({ language: "en-US", text: item.text });
      const mt = LT_MOTHER[item.native];
      if (mt) params.set("motherTongue", mt);
      const r = await fetch(`http://localhost:${port}/v2/check`, { method: "POST", headers: { "content-type": "application/x-www-form-urlencoded" }, body: params.toString() });
      const j: any = await r.json();
      return j.matches.map((m: any) => ({
        start: m.offset,
        end: m.offset + m.length,
        original: item.text.slice(m.offset, m.offset + m.length),
        fix: m.replacements?.[0]?.value ?? "",
        reason: `${m.rule.id}: ${m.message}`,
      }));
    });
    const rss = await rssOfTree(proc.pid);
    return { engine: "languagetool", version: "6.6 (standalone, en-US, motherTongue set)", offline: true, license: "LGPL-2.1", cold_start_ms: cold, rss_kb: rss, items: res, summary: summarize(res) };
  } finally {
    proc.kill();
    await proc.exited;
  }
}

// ---------- Harper ----------
async function harperLint(text: string): Promise<{ issues: Issue[]; rssKb: number }> {
  const file = `/tmp/grammachy-harper-${process.pid}.txt`;
  await Bun.write(file, text);
  const p = spawn([`${BENCH}/harper-cli`, "lint", "--format", "json", "--quiet", "--dialect", "us", file], { stdout: "pipe", stderr: "ignore" });
  let rssKb = 0;
  const poll = (async () => {
    while (p.exitCode === null) {
      try {
        const st = await Bun.file(`/proc/${p.pid}/status`).text();
        const m = st.match(/VmHWM:\s+(\d+)/);
        if (m) rssKb = Math.max(rssKb, parseInt(m[1], 10));
      } catch {}
      await sleep(2);
    }
  })();
  const out = await new Response(p.stdout).text();
  await p.exited;
  await poll;
  const start = out.indexOf("[");
  if (start < 0) return { issues: [], rssKb };
  const j: any[] = JSON.parse(out.slice(start));
  const lints = j[0]?.lints ?? [];
  return {
    rssKb,
    issues: lints.map((l: any) => ({
      start: l.span.char_start,
      end: l.span.char_end,
      original: l.matched_text,
      fix: (l.suggestions?.[0] ?? "").replace(/^Replace with: [“"]?/, "").replace(/[”"]$/, ""),
      reason: `${l.rule}: ${l.message}`,
    })),
  };
}

async function runHarper(): Promise<EngineResult> {
  const t0 = performance.now();
  const first = await harperLint("Warm up sentence for cold start timing.");
  const cold = Math.round(performance.now() - t0);
  let rss = first.rssKb;
  const res = await runItems(async (item) => {
    const r = await harperLint(item.text);
    rss = Math.max(rss, r.rssKb);
    return r.issues;
  });
  return { engine: "harper", version: "harper-cli 2.8.0 (core 0.1.0 reported)", offline: true, license: "Apache-2.0", cold_start_ms: cold, rss_kb: rss, items: res, summary: summarize(res) };
}

// ---------- LLM prompt (shared by llama.cpp and Claude) ----------
function llmPrompt(item: Item) {
  return [
    `You are a grammar and spelling checker for en-US English.`,
    `The writer's native language is ${NATIVE_NAME[item.native]}. Look for mistakes such native speakers make when writing English (articles, tense, plural, false friends, word order, prepositions, agreement).`,
    `Report only grammar and spelling mistakes. Do not report style or word choice that is already correct.`,
    `Return ONLY a JSON array. Each element is {"original": <the shortest exact substring of the text that contains the mistake, usually one to three words, never the whole sentence>, "fix": <replacement for that substring only>, "reason": <short reason>}. Return [] if the text is correct. No prose, no markdown.`,
    ``,
    `Text: ${JSON.stringify(item.text)}`,
  ].join("\n");
}

const ISSUE_SCHEMA = {
  type: "array",
  items: {
    type: "object",
    properties: { original: { type: "string" }, fix: { type: "string" }, reason: { type: "string" } },
    required: ["original", "fix", "reason"],
    additionalProperties: false,
  },
};

let noiseIssues = 0;

function parseLlmIssues(text: string, raw: string): Issue[] {
  const a = raw.indexOf("[");
  const b = raw.lastIndexOf("]");
  if (a < 0 || b < a) throw new Error("no JSON array in: " + raw.slice(0, 120));
  const arr: any[] = JSON.parse(raw.slice(a, b + 1));
  const issues: Issue[] = [];
  for (const x of arr) {
    if (!x || typeof x.original !== "string") continue;
    // A fix equal to the original is noise, not an Issue. Small models emit many of these.
    if (norm(String(x.fix ?? "")) === norm(x.original)) {
      noiseIssues++;
      continue;
    }
    const start = text.indexOf(x.original);
    if (start < 0) {
      // The model quoted something that is not in the text. Count it as an issue over the whole sentence.
      issues.push({ start: 0, end: text.length, original: x.original, fix: String(x.fix ?? ""), reason: `[unanchored] ${x.reason ?? ""}` });
      continue;
    }
    issues.push({ start, end: start + x.original.length, original: x.original, fix: String(x.fix ?? ""), reason: String(x.reason ?? "") });
  }
  return issues;
}

// ---------- llama.cpp ----------
async function runLlama(name: string, gguf: string, label: string): Promise<EngineResult> {
  const port = 8082;
  const bin = `${BENCH}/llama-b10615/llama-server`;
  const proc = spawn([bin, "-m", gguf, "--port", String(port), "-c", "2048", "-t", "12", "--temp", "0", "--jinja"], {
    stdout: "ignore",
    stderr: "ignore",
    env: { ...process.env, LD_LIBRARY_PATH: `${BENCH}/llama-b10615` },
  });
  try {
    const cold = await waitHttp(`http://localhost:${port}/health`, 300_000);
    noiseIssues = 0;
    const chat = async (item: Item) => {
      const r = await fetch(`http://localhost:${port}/v1/chat/completions`, {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({
          messages: [{ role: "user", content: llmPrompt(item) }],
          temperature: 0,
          max_tokens: 600,
          // llama.cpp turns the schema into a grammar, so the output is always a well-formed array.
          response_format: { type: "json_schema", json_schema: { name: "issues", schema: ISSUE_SCHEMA } },
        }),
      });
      const j: any = await r.json();
      return parseLlmIssues(item.text, j.choices[0].message.content);
    };
    await chat(items[0]);
    const res = await runItems(chat);
    const rss = await rssOfTree(proc.pid);
    return { engine: name, version: label, offline: true, license: "llama.cpp MIT; Qwen2.5 Apache-2.0", cold_start_ms: cold, rss_kb: rss, items: res, summary: summarize(res) };
  } finally {
    proc.kill();
    await proc.exited;
  }
}

// ---------- Claude API ----------
async function runClaude(): Promise<EngineResult | null> {
  const key = process.env.ANTHROPIC_API_KEY;
  if (!key) {
    console.error("claude: ANTHROPIC_API_KEY unset, skipped");
    return null;
  }
  const model = "claude-3-5-haiku-latest";
  const t0 = performance.now();
  const chat = async (item: Item) => {
    const r = await fetch("https://api.anthropic.com/v1/messages", {
      method: "POST",
      headers: { "content-type": "application/json", "x-api-key": key, "anthropic-version": "2023-06-01" },
      body: JSON.stringify({ model, max_tokens: 300, temperature: 0, messages: [{ role: "user", content: llmPrompt(item) }] }),
    });
    const j: any = await r.json();
    if (!r.ok) throw new Error(JSON.stringify(j));
    return parseLlmIssues(item.text, j.content[0].text);
  };
  await chat(items[0]);
  const cold = Math.round(performance.now() - t0);
  const res = await runItems(chat);
  return { engine: "claude", version: model, offline: false, license: "Commercial API, pay per token", cold_start_ms: cold, rss_kb: 0, items: res, summary: summarize(res) };
}

// ---------- main ----------
const wanted = process.argv.slice(2);
const all: Record<string, () => Promise<EngineResult | null>> = {
  languagetool: runLanguageTool,
  harper: runHarper,
  qwen3b: () => runLlama("qwen2.5-3b", `${BENCH}/qwen2.5-3b-instruct-q4_k_m.gguf`, "Qwen2.5-3B-Instruct Q4_K_M via llama.cpp b10615, 12 threads"),
  qwen7b: () => runLlama("qwen2.5-7b", `${BENCH}/qwen2.5-7b-instruct-q4_k_m-00001-of-00002.gguf`, "Qwen2.5-7B-Instruct Q4_K_M via llama.cpp b10615, 12 threads"),
  claude: runClaude,
};
const names = wanted.length ? wanted : Object.keys(all);
for (const name of names) {
  console.error(`== ${name}`);
  try {
    const r = await all[name]();
    if (!r) continue;
    await Bun.write(`${HERE}/results-${name}.json`, JSON.stringify(r, null, 2) + "\n");
    console.log(name, JSON.stringify(r.summary), `cold=${r.cold_start_ms}ms rss=${Math.round((r.rss_kb ?? 0) / 1024)}MB`);
  } catch (e) {
    console.error(name, "failed:", e);
  }
}
