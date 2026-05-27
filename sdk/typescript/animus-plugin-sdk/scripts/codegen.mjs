#!/usr/bin/env node
// Code-generates TypeScript types from the Rust-emitted JSON Schemas in
// schemas/animus-{plugin,subject}-protocol/_all.json.
//
// Strategy: feed each $defs entry to json-schema-to-typescript individually
// (with sibling $defs hoisted so cross-refs resolve), then concatenate.
// Compiling the bundle as a single document causes j-s-t-s to emit some
// types twice (once for the inline $ref usage, once for the $defs entry)
// with numeric suffixes (HealthStatus / HealthStatus1). The per-def loop
// avoids that and produces one named export per Rust type.
//
// Post-processing: open-string enums on the Rust side (PluginKind,
// TriggerActionHint, TriggerAckStatus — all have an `Other(String)`
// variant that flattens to `string` on the wire) are widened from
// `string` to `"known1" | "known2" | ... | (string & {})` so downstream
// code gets autocomplete on the known values while still accepting
// unknown values for forward-compat. PluginKind is also injected because
// it does not appear in the schema bundle at all (its values come from
// `PLUGIN_KIND_*` constants in animus-plugin-protocol).

import { compile } from "json-schema-to-typescript";
import { readFileSync, writeFileSync, mkdirSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = dirname(fileURLToPath(import.meta.url));
const repoRoot = resolve(__dirname, "../../../..");
// Output dir is overridable so the drift check can target a staging
// directory without copying the script outside its node_modules.
const outDir = process.env.ANIMUS_CODEGEN_OUT_DIR
  ? resolve(process.env.ANIMUS_CODEGEN_OUT_DIR)
  : resolve(__dirname, "../src/types");

const bundles = [
  {
    source: "schemas/animus-plugin-protocol/_all.json",
    outFile: "plugin-protocol.ts",
  },
  {
    source: "schemas/animus-subject-protocol/_all.json",
    outFile: "subject-protocol.ts",
  },
];

// Open-string enum vocabularies derived from
// crates/animus-plugin-protocol/src/lib.rs PLUGIN_KIND_*,
// TriggerActionHint::*, TriggerAckStatus::* constants. Keep in sync
// when a new variant is added on the Rust side.
const openEnums = {
  PluginKind: [
    "provider",
    "subject_backend",
    "task_backend",
    "trigger_backend",
    "log_storage_backend",
    "custom",
  ],
  TriggerActionHint: ["create_task", "run_workflow"],
  TriggerAckStatus: [
    "dispatched",
    "queued",
    "unmatched",
    "skipped",
    "failed",
    "shutdown",
  ],
};

function openEnumUnion(name) {
  const values = openEnums[name];
  const literals = values.map((v) => `"${v}"`).join(" | ");
  // `string & {}` keeps autocomplete on the known literals while still
  // accepting arbitrary strings for forward-compat with Rust's `Other(String)`.
  return `${literals} | (string & {})`;
}

async function compileBundle({ source, outFile }) {
  const bundlePath = resolve(repoRoot, source);
  const bundle = JSON.parse(readFileSync(bundlePath, "utf8"));
  const defs = bundle.$defs ?? {};
  const names = Object.keys(defs).sort();

  const parts = [];
  for (const name of names) {
    const schema = {
      $schema: bundle.$schema,
      title: name,
      ...defs[name],
      // Hoist sibling $defs so cross-`$ref`s resolve. We set
      // `declareExternallyReferenced: false` so sibling types are NOT
      // re-emitted by this iteration — each name gets exactly one
      // emission across the whole bundle.
      $defs: defs,
    };
    const ts = await compile(schema, name, {
      bannerComment: "",
      additionalProperties: false,
      declareExternallyReferenced: false,
      unreachableDefinitions: false,
      strictIndexSignatures: false,
      style: { singleQuote: false, semi: true },
    });
    parts.push(ts.trim());
  }

  let body = parts.join("\n\n") + "\n";

  // Normalize j-s-t-s name-collision suffixes back to the base name.
  // When a $ref has a sibling `description` the compiler treats it as a
  // derivative type and appends `1`, `2`, ... to avoid the collision —
  // but each base name in our $defs loop is already emitted exactly once,
  // so the suffixed reference is always equivalent to the base. Without
  // this pass, `Subject.id` ends up typed as `SubjectId1`, which is
  // never declared anywhere and breaks `tsc`.
  const declaredNames = new Set(names);
  for (const name of declaredNames) {
    const suffixed = new RegExp(`\\b${name}\\d+\\b`, "g");
    body = body.replace(suffixed, name);
  }

  // Replace open-string enums with widened unions. The base schema
  // emits `export type X = string;`; we swap to a literal-union form.
  for (const name of Object.keys(openEnums)) {
    const re = new RegExp(`export type ${name} = string;`);
    if (re.test(body)) {
      body = body.replace(re, `export type ${name} = ${openEnumUnion(name)};`);
    }
  }

  // Wire fields that semantically hold an open-enum value but were
  // declared as plain `string` in the schema (because the Rust side uses
  // `#[schemars(with = "String")]` for forward-compat) to the typed
  // union. The mapping is by field name and is intentionally narrow —
  // we only touch fields the host actually populates with these values.
  const fieldEnumMap = {
    plugin_kind: "PluginKind",
  };
  for (const [field, typeName] of Object.entries(fieldEnumMap)) {
    if (!source.includes("plugin-protocol")) continue;
    const re = new RegExp(`(\\b${field}: )string;`, "g");
    body = body.replace(re, `$1${typeName};`);
  }

  // Widen JSON-RPC envelope fields that the Rust side declares as
  // `serde_json::Value` (no type constraint in the schema) but j-s-t-s
  // falls back to rendering as `{ [k: string]: unknown }`. That object
  // shape rejects valid JSON-RPC ids (`1`, `"abc"`, `null`) and forces
  // consumers to cast every plain `result`/`params`/`payload`.
  //
  // The mapping is keyed by (interface, field) so we don't accidentally
  // rewrite an unrelated `id` field on another type.
  const widenFields = [
    // [interface, field, replacement type]
    ["RpcRequest", "id", "string | number | null"],
    ["RpcRequest", "params", "unknown"],
    ["RpcResponse", "id", "string | number | null"],
    ["RpcResponse", "result", "unknown"],
    ["RpcNotification", "params", "unknown"],
    ["RpcError", "data", "unknown"],
    ["McpTool", "input_schema", "unknown"],
    ["TriggerEvent", "payload", "unknown"],
    ["TriggerWatchParams", "config", "unknown"],
    ["TriggerWatchParams", "cursor", "unknown"],
  ];
  for (const [iface, field, repl] of widenFields) {
    if (!source.includes("plugin-protocol")) continue;
    // Match `export interface IFACE { ... <field>?: { [k:string]:unknown };`
    // scoped to the interface body so we don't accidentally rewrite
    // a similarly-named field on another interface.
    const ifaceRe = new RegExp(
      String.raw`(export interface ${iface}\b[\s\S]*?\n  ${field}\??: )\{\s*\[k: string\]: unknown;\s*\};`,
    );
    body = body.replace(ifaceRe, `$1${repl};`);
  }

  // PluginKind is not declared as a $def in the schema (it is referenced
  // only by free-form prose on PluginInfo.plugin_kind / PluginManifest.plugin_kind).
  // Inject it so SDK consumers can `import type { PluginKind }`.
  if (source.includes("plugin-protocol") && !/export type PluginKind\b/.test(body)) {
    const pluginKindDecl = [
      "/**",
      " * Discriminant identifying the role a plugin plays in the host.",
      " *",
      " * Wire representation is the snake_case string in the inner literal",
      " * union; unknown values round-trip as `string` to preserve forward-",
      " * compat with hosts that introduce new kinds.",
      " *",
      " * Mirrors the `PLUGIN_KIND_*` constants in",
      " * `crates/animus-plugin-protocol/src/lib.rs`.",
      " */",
      `export type PluginKind = ${openEnumUnion("PluginKind")};`,
      "",
    ].join("\n");
    body = pluginKindDecl + body;
  }

  const banner = [
    `// AUTO-GENERATED FROM ../../../${source} — DO NOT EDIT BY HAND.`,
    `// Regenerate via: pnpm run codegen`,
    "",
    "",
  ].join("\n");

  return { outFile, content: banner + body, typeCount: names.length };
}

async function main() {
  mkdirSync(outDir, { recursive: true });
  const results = [];
  for (const bundle of bundles) {
    const result = await compileBundle(bundle);
    const outPath = resolve(outDir, result.outFile);
    writeFileSync(outPath, result.content);
    results.push({ ...result, outPath });
    console.log(`wrote ${outPath} (${result.typeCount} types)`);
  }

  // Barrel re-export. Use .js suffix on imports so the emitted ESM works
  // with NodeNext module resolution.
  const barrel = [
    "// AUTO-GENERATED — DO NOT EDIT BY HAND.",
    "// Regenerate via: pnpm run codegen",
    "",
    `export * from "./plugin-protocol.js";`,
    `export * from "./subject-protocol.js";`,
    "",
  ].join("\n");
  const barrelPath = resolve(outDir, "generated.ts");
  writeFileSync(barrelPath, barrel);
  console.log(`wrote ${barrelPath} (barrel)`);
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});
