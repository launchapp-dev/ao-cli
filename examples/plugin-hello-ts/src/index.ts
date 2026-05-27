// Hello-world Animus subject_backend plugin written in TypeScript.
//
// Run `pnpm install && pnpm run build`, then drop the bundle into
// `~/.animus/plugins/` per the README. The Animus daemon will discover it,
// call `subject/list`, and surface the three subjects below via
// `animus subject list --kind hello_world_demo`.

// TODO: confirm after SDK merge — T1 ships `definePlugin` + the `subject_backend`
// kind tag; T2 ships the wire-typed `Subject` / `SubjectList` / `SubjectFilter`
// shapes. Until both land in `@launchapp-dev/animus-plugin-sdk`, treat the
// import as the source of truth for the contract.
import { definePlugin, type SubjectBackend, type SubjectFilter } from "@launchapp-dev/animus-plugin-sdk";

// SubjectId convention is `"<backend>:<native_id>"` (see
// crates/animus-subject-protocol). The `<backend>` prefix should match the
// registered subject kind so id-only operations route back to this plugin.
const SUBJECT_KIND = "hello_world_demo";
const NOW = new Date().toISOString();

const HELLO_SUBJECTS = [
  { id: `${SUBJECT_KIND}:1`, title: "Read the README",      status: "ready"       as const, priority: 3 },
  { id: `${SUBJECT_KIND}:2`, title: "Modify a subject",     status: "in-progress" as const, priority: 2 },
  { id: `${SUBJECT_KIND}:3`, title: "Ship your own plugin", status: "ready"       as const, priority: 4 },
];

const backend: SubjectBackend = {
  async list(filter: SubjectFilter = {}) {
    // Honor the SubjectFilter contract: AND-combine status, kind, and limit.
    const statusOk = (s: typeof HELLO_SUBJECTS[number]) =>
      !filter.status?.length || filter.status.includes(s.status);
    const kindOk = !filter.kind?.length || filter.kind.includes(SUBJECT_KIND);
    const matches = kindOk ? HELLO_SUBJECTS.filter(statusOk) : [];
    const page = typeof filter.limit === "number" ? matches.slice(0, filter.limit) : matches;
    return {
      subjects: page.map((s) => ({ ...s, kind: SUBJECT_KIND, created_at: NOW, updated_at: NOW })),
      fetched_at: new Date().toISOString(),
    };
  },
  async get(id: string) {
    const hit = HELLO_SUBJECTS.find((s) => s.id === id);
    if (!hit) throw new Error(`subject not found: ${id}`);
    return { ...hit, kind: SUBJECT_KIND, created_at: NOW, updated_at: NOW };
  },
  async schema() {
    return { kinds: [SUBJECT_KIND], status_values: ["ready", "in-progress", "done"],
      supports_watch: false, supports_create: false, supports_pagination: false };
  },
};

// definePlugin wires the SDK's stdio JSON-RPC loop + lifecycle methods
// (initialize / health/check / shutdown) around your backend. It also handles
// the `--manifest` probe the host fires before installing the plugin.
definePlugin({
  kind: "subject_backend",
  name: "animus-plugin-hello-ts",
  version: "0.1.0",
  description: "Hello-world TypeScript subject backend (returns 3 demo subjects)",
  subjectKinds: [SUBJECT_KIND],
  impl: backend,
});
