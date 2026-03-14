import { useCallback, useMemo, useState } from "react";
import { Link, useNavigate, useParams, useSearchParams } from "react-router-dom";
import { useQuery, useMutation } from "@/lib/graphql/client";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Alert, AlertDescription } from "@/components/ui/alert";
import { Tabs, TabsList, TabsTrigger, TabsContent } from "@/components/ui/tabs";
import { Separator } from "@/components/ui/separator";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { WorkflowDefinitionsDocument } from "@/lib/graphql/generated/graphql";
import { PageLoading, PageError } from "./shared";
import {
  Plus,
  ChevronRight,
  ChevronLeft,
  ArrowLeft,
  X,
  Pencil,
  Copy,
  Trash2,
  Eye,
  CheckCircle2,
  AlertCircle,
  Save,
  Layers,
  PaintBucket,
  FileText,
} from "lucide-react";

interface PhaseEntry {
  id: string;
  maxReworkAttempts: number;
  skipIf: string[];
  onVerdict: {
    advance: { target: string | null };
    rework: { target: string | null; allowAgentTarget: boolean };
    fail: { target: string | null };
  };
}

interface VariableEntry {
  name: string;
  description: string;
  required: boolean;
  default: string;
}

interface WorkflowDef {
  id: string;
  name: string;
  description: string;
  phases: PhaseEntry[];
  variables: VariableEntry[];
}

function makePhaseEntry(id: string): PhaseEntry {
  return {
    id,
    maxReworkAttempts: 3,
    skipIf: [],
    onVerdict: {
      advance: { target: null },
      rework: { target: null, allowAgentTarget: false },
      fail: { target: null },
    },
  };
}

function makeVariableEntry(): VariableEntry {
  return { name: "", description: "", required: false, default: "" };
}

const TEMPLATES: Record<string, { name: string; description: string; phases: string[] }> = {
  standard: {
    name: "Standard",
    description: "A typical development workflow with requirements analysis, implementation, code review, and testing phases.",
    phases: ["requirements", "implementation", "code-review", "testing"],
  },
  "ui-ux": {
    name: "UI/UX",
    description: "Extended workflow for user interface work including research, wireframing, and mockup review before implementation.",
    phases: ["requirements", "ux-research", "wireframe", "mockup-review", "implementation", "code-review", "testing"],
  },
  blank: {
    name: "Blank",
    description: "Start from scratch with an empty workflow definition. Add phases as needed.",
    phases: [],
  },
};

const ID_PATTERN = /^[a-z0-9][a-z0-9-]*$/;

interface ValidationResult {
  valid: boolean;
  errors: { message: string; phaseId?: string }[];
  warnings: { message: string }[];
}

function validateDef(def: WorkflowDef): ValidationResult {
  const errors: { message: string; phaseId?: string }[] = [];
  const warnings: { message: string }[] = [];

  if (!def.id.trim()) {
    errors.push({ message: "Workflow ID is required" });
  } else if (!ID_PATTERN.test(def.id)) {
    errors.push({ message: "ID must start with a lowercase letter or digit and contain only lowercase letters, digits, and hyphens" });
  }

  if (!def.name.trim()) {
    errors.push({ message: "Workflow name is required" });
  }

  if (def.phases.length === 0) {
    errors.push({ message: "At least one phase is required" });
  }

  const seen = new Set<string>();
  const phaseIds = new Set(def.phases.map((p) => p.id));
  for (const phase of def.phases) {
    if (!phase.id.trim()) {
      errors.push({ message: "Phase ID cannot be empty", phaseId: phase.id });
    } else if (!ID_PATTERN.test(phase.id)) {
      errors.push({ message: `Phase "${phase.id}" has an invalid ID format`, phaseId: phase.id });
    }
    if (seen.has(phase.id)) {
      errors.push({ message: `Duplicate phase ID "${phase.id}"`, phaseId: phase.id });
    }
    seen.add(phase.id);

    if (phase.maxReworkAttempts < 1) {
      errors.push({ message: `Phase "${phase.id}" must have max rework attempts > 0`, phaseId: phase.id });
    }

    for (const [verdict, cfg] of Object.entries(phase.onVerdict)) {
      const target = (cfg as { target: string | null }).target;
      if (target && !phaseIds.has(target)) {
        errors.push({ message: `Phase "${phase.id}" ${verdict} target "${target}" does not exist`, phaseId: phase.id });
      }
    }
  }

  if (def.phases.length > 0 && !errors.some((e) => e.phaseId)) {
    warnings.push({ message: `${def.phases.length} phase(s) configured` });
  }

  return { valid: errors.length === 0, errors, warnings };
}

function defToPreview(def: WorkflowDef): string {
  const obj: Record<string, unknown> = {
    id: def.id,
    name: def.name,
  };
  if (def.description) obj.description = def.description;
  obj.phases = def.phases.map((p) => {
    const phase: Record<string, unknown> = { id: p.id };
    if (p.maxReworkAttempts !== 3) phase.max_rework_attempts = p.maxReworkAttempts;
    if (p.skipIf.length > 0) phase.skip_if = p.skipIf;
    const onVerdict: Record<string, unknown> = {};
    if (p.onVerdict.advance.target) onVerdict.advance = { target: p.onVerdict.advance.target };
    if (p.onVerdict.rework.target || p.onVerdict.rework.allowAgentTarget) {
      const rw: Record<string, unknown> = {};
      if (p.onVerdict.rework.target) rw.target = p.onVerdict.rework.target;
      if (p.onVerdict.rework.allowAgentTarget) rw.allow_agent_target = true;
      onVerdict.rework = rw;
    }
    if (p.onVerdict.fail.target) onVerdict.fail = { target: p.onVerdict.fail.target };
    if (Object.keys(onVerdict).length > 0) phase.on_verdict = onVerdict;
    return phase;
  });
  if (def.variables.length > 0) {
    obj.variables = def.variables.map((v) => {
      const ve: Record<string, unknown> = { name: v.name };
      if (v.description) ve.description = v.description;
      if (v.required) ve.required = true;
      if (v.default) ve.default = v.default;
      return ve;
    });
  }
  return JSON.stringify(obj, null, 2);
}

function PhaseNode({
  phase,
  index,
  total,
  selected,
  hasError,
  onSelect,
  onMoveLeft,
  onMoveRight,
  onRemove,
}: {
  phase: PhaseEntry;
  index: number;
  total: number;
  selected: boolean;
  hasError: boolean;
  onSelect: () => void;
  onMoveLeft: () => void;
  onMoveRight: () => void;
  onRemove: () => void;
}) {
  return (
    <div className="flex items-center gap-0">
      <button
        type="button"
        onClick={onSelect}
        className={`group relative flex items-center gap-2 rounded-lg border px-3 py-2 transition-colors ${
          selected
            ? "border-primary/40 bg-primary/5"
            : hasError
              ? "border-destructive/40 bg-destructive/5"
              : "border-border/40 bg-card/60 hover:border-border/60"
        }`}
      >
        <span
          className={`inline-block h-2.5 w-2.5 rounded-full shrink-0 ${hasError ? "bg-destructive" : "bg-primary/60"}`}
        />
        <span className="font-mono text-xs">{phase.id}</span>
        <div className="absolute -top-2 -right-1 hidden group-hover:flex items-center gap-0.5">
          {index > 0 && (
            <button
              type="button"
              onClick={(e) => { e.stopPropagation(); onMoveLeft(); }}
              className="h-4 w-4 rounded bg-muted/80 flex items-center justify-center hover:bg-muted"
            >
              <ChevronLeft className="h-3 w-3" />
            </button>
          )}
          {index < total - 1 && (
            <button
              type="button"
              onClick={(e) => { e.stopPropagation(); onMoveRight(); }}
              className="h-4 w-4 rounded bg-muted/80 flex items-center justify-center hover:bg-muted"
            >
              <ChevronRight className="h-3 w-3" />
            </button>
          )}
          <button
            type="button"
            onClick={(e) => { e.stopPropagation(); onRemove(); }}
            className="h-4 w-4 rounded bg-destructive/20 flex items-center justify-center hover:bg-destructive/40"
          >
            <X className="h-3 w-3 text-destructive" />
          </button>
        </div>
      </button>
      {index < total - 1 && <ChevronRight className="h-4 w-4 text-muted-foreground/40 mx-1 shrink-0" />}
    </div>
  );
}

function PhaseDetailPanel({
  phase,
  allPhaseIds,
  onChange,
}: {
  phase: PhaseEntry;
  allPhaseIds: string[];
  onChange: (updated: PhaseEntry) => void;
}) {
  const otherPhases = allPhaseIds.filter((id) => id !== phase.id);
  const [newSkipGuard, setNewSkipGuard] = useState("");

  const updateField = <K extends keyof PhaseEntry>(key: K, value: PhaseEntry[K]) => {
    onChange({ ...phase, [key]: value });
  };

  const updateVerdict = (
    verdict: "advance" | "rework" | "fail",
    field: string,
    value: unknown,
  ) => {
    onChange({
      ...phase,
      onVerdict: {
        ...phase.onVerdict,
        [verdict]: { ...phase.onVerdict[verdict], [field]: value },
      },
    });
  };

  return (
    <div className="w-72 shrink-0 space-y-4 ao-fade-in">
      <Card className="border-border/40 bg-card/60">
        <CardHeader className="pb-2 pt-3 px-4">
          <CardTitle className="text-xs uppercase tracking-wider text-muted-foreground/60 font-medium">Phase Config</CardTitle>
        </CardHeader>
        <CardContent className="px-4 pb-4 space-y-4">
          <div>
            <label className="text-[11px] uppercase tracking-wider text-muted-foreground/60 font-medium">Phase ID</label>
            <Input
              value={phase.id}
              onChange={(e) => updateField("id", e.target.value)}
              className="mt-1 font-mono text-xs h-8"
            />
          </div>

          <div>
            <label className="text-[11px] uppercase tracking-wider text-muted-foreground/60 font-medium">Max Rework Attempts</label>
            <Input
              type="number"
              min={1}
              value={phase.maxReworkAttempts}
              onChange={(e) => updateField("maxReworkAttempts", Math.max(1, parseInt(e.target.value) || 1))}
              className="mt-1 text-xs h-8"
            />
          </div>

          <div>
            <label className="text-[11px] uppercase tracking-wider text-muted-foreground/60 font-medium">Skip If Guards</label>
            <div className="mt-1 space-y-1">
              {phase.skipIf.map((guard, i) => (
                <div key={i} className="flex items-center gap-1">
                  <span className="text-xs font-mono flex-1 truncate">{guard}</span>
                  <button
                    type="button"
                    onClick={() => updateField("skipIf", phase.skipIf.filter((_, j) => j !== i))}
                    className="text-muted-foreground hover:text-destructive"
                  >
                    <X className="h-3 w-3" />
                  </button>
                </div>
              ))}
              <div className="flex gap-1">
                <Input
                  value={newSkipGuard}
                  onChange={(e) => setNewSkipGuard(e.target.value)}
                  placeholder="Guard condition"
                  className="text-xs h-7 flex-1"
                  onKeyDown={(e) => {
                    if (e.key === "Enter" && newSkipGuard.trim()) {
                      updateField("skipIf", [...phase.skipIf, newSkipGuard.trim()]);
                      setNewSkipGuard("");
                    }
                  }}
                />
                <Button
                  size="sm"
                  variant="outline"
                  className="h-7 px-2"
                  disabled={!newSkipGuard.trim()}
                  onClick={() => {
                    if (newSkipGuard.trim()) {
                      updateField("skipIf", [...phase.skipIf, newSkipGuard.trim()]);
                      setNewSkipGuard("");
                    }
                  }}
                >
                  <Plus className="h-3 w-3" />
                </Button>
              </div>
            </div>
          </div>

          <Separator className="opacity-30" />

          <div>
            <label className="text-[11px] uppercase tracking-wider text-muted-foreground/60 font-medium">On Advance</label>
            <Select
              value={phase.onVerdict.advance.target ?? "__none__"}
              onValueChange={(v) => updateVerdict("advance", "target", v === "__none__" ? null : v)}
            >
              <SelectTrigger size="sm" className="mt-1 w-full text-xs">
                <SelectValue placeholder="Next phase (auto)" />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="__none__">Auto (next phase)</SelectItem>
                {otherPhases.map((id) => (
                  <SelectItem key={id} value={id}>{id}</SelectItem>
                ))}
              </SelectContent>
            </Select>
          </div>

          <div>
            <label className="text-[11px] uppercase tracking-wider text-muted-foreground/60 font-medium">On Rework</label>
            <Select
              value={phase.onVerdict.rework.target ?? "__none__"}
              onValueChange={(v) => updateVerdict("rework", "target", v === "__none__" ? null : v)}
            >
              <SelectTrigger size="sm" className="mt-1 w-full text-xs">
                <SelectValue placeholder="Same phase (default)" />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="__none__">Same phase (default)</SelectItem>
                {otherPhases.map((id) => (
                  <SelectItem key={id} value={id}>{id}</SelectItem>
                ))}
              </SelectContent>
            </Select>
            <label className="flex items-center gap-2 mt-2 text-xs text-muted-foreground cursor-pointer">
              <input
                type="checkbox"
                checked={phase.onVerdict.rework.allowAgentTarget}
                onChange={(e) => updateVerdict("rework", "allowAgentTarget", e.target.checked)}
                className="rounded"
              />
              Allow agent to override target
            </label>
          </div>

          <div>
            <label className="text-[11px] uppercase tracking-wider text-muted-foreground/60 font-medium">On Fail</label>
            <Select
              value={phase.onVerdict.fail.target ?? "__none__"}
              onValueChange={(v) => updateVerdict("fail", "target", v === "__none__" ? null : v)}
            >
              <SelectTrigger size="sm" className="mt-1 w-full text-xs">
                <SelectValue placeholder="Stop workflow (default)" />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="__none__">Stop workflow (default)</SelectItem>
                {otherPhases.map((id) => (
                  <SelectItem key={id} value={id}>{id}</SelectItem>
                ))}
              </SelectContent>
            </Select>
          </div>
        </CardContent>
      </Card>
    </div>
  );
}

function VariableCard({
  variable,
  onChange,
  onRemove,
}: {
  variable: VariableEntry;
  onChange: (v: VariableEntry) => void;
  onRemove: () => void;
}) {
  return (
    <Card className="border-border/40 bg-card/60">
      <CardContent className="pt-3 pb-3 px-4 space-y-3">
        <div className="flex items-start justify-between">
          <div className="flex-1 grid grid-cols-2 gap-3">
            <div>
              <label className="text-[11px] uppercase tracking-wider text-muted-foreground/60 font-medium">Name</label>
              <Input
                value={variable.name}
                onChange={(e) => onChange({ ...variable, name: e.target.value })}
                className="mt-1 font-mono text-xs h-8"
                placeholder="variable_name"
              />
            </div>
            <div>
              <label className="text-[11px] uppercase tracking-wider text-muted-foreground/60 font-medium">Default</label>
              <Input
                value={variable.default}
                onChange={(e) => onChange({ ...variable, default: e.target.value })}
                className="mt-1 text-xs h-8"
                placeholder="Default value"
              />
            </div>
          </div>
          <button type="button" onClick={onRemove} className="ml-2 mt-4 text-muted-foreground hover:text-destructive">
            <X className="h-4 w-4" />
          </button>
        </div>
        <div>
          <label className="text-[11px] uppercase tracking-wider text-muted-foreground/60 font-medium">Description</label>
          <Input
            value={variable.description}
            onChange={(e) => onChange({ ...variable, description: e.target.value })}
            className="mt-1 text-xs h-8"
            placeholder="What this variable controls"
          />
        </div>
        <label className="flex items-center gap-2 text-xs text-muted-foreground cursor-pointer">
          <input
            type="checkbox"
            checked={variable.required}
            onChange={(e) => onChange({ ...variable, required: e.target.checked })}
            className="rounded"
          />
          Required
        </label>
      </CardContent>
    </Card>
  );
}

function TransitionsTable({ phases }: { phases: PhaseEntry[] }) {
  if (phases.length === 0) {
    return <p className="text-sm text-muted-foreground/60 py-4 text-center">No phases configured</p>;
  }

  return (
    <div className="overflow-x-auto">
      <table className="w-full text-xs">
        <thead>
          <tr className="border-b border-border/30">
            <th className="text-left py-2 pr-4 text-[11px] uppercase tracking-wider text-muted-foreground/60 font-medium">Phase</th>
            <th className="text-left py-2 pr-4 text-[11px] uppercase tracking-wider text-muted-foreground/60 font-medium">Advance</th>
            <th className="text-left py-2 pr-4 text-[11px] uppercase tracking-wider text-muted-foreground/60 font-medium">Rework</th>
            <th className="text-left py-2 text-[11px] uppercase tracking-wider text-muted-foreground/60 font-medium">Fail</th>
          </tr>
        </thead>
        <tbody>
          {phases.map((p, i) => (
            <tr key={p.id} className="border-b border-border/20">
              <td className="py-2 pr-4 font-mono font-medium">{p.id}</td>
              <td className="py-2 pr-4 text-muted-foreground font-mono">
                {p.onVerdict.advance.target ?? (i < phases.length - 1 ? `${phases[i + 1].id} (auto)` : "end")}
              </td>
              <td className="py-2 pr-4 text-muted-foreground font-mono">
                {p.onVerdict.rework.target ?? `${p.id} (self)`}
                {p.onVerdict.rework.allowAgentTarget && <Badge variant="outline" className="ml-1 text-[9px]">agent</Badge>}
              </td>
              <td className="py-2 text-muted-foreground font-mono">
                {p.onVerdict.fail.target ?? "stop"}
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

const UPSERT_MUTATION = `mutation UpsertWorkflowDefinition($id: String!, $name: String!, $description: String, $phases: String!, $variables: String) { upsertWorkflowDefinition(id: $id, name: $name, description: $description, phases: $phases, variables: $variables) }`;

function EditorCore({
  initial,
  isNew,
}: {
  initial: WorkflowDef;
  isNew: boolean;
}) {
  const [def, setDef] = useState<WorkflowDef>(initial);
  const [dirty, setDirty] = useState(isNew);
  const [selectedPhaseIdx, setSelectedPhaseIdx] = useState<number | null>(null);
  const [showPreview, setShowPreview] = useState(false);
  const [validation, setValidation] = useState<ValidationResult | null>(null);
  const [saveMsg, setSaveMsg] = useState<string | null>(null);
  const [saveError, setSaveError] = useState<string | null>(null);
  const [activeTab, setActiveTab] = useState(0);
  const [, upsertDef] = useMutation(UPSERT_MUTATION);

  const errorPhaseIds = useMemo(() => {
    if (!validation) return new Set<string>();
    return new Set(validation.errors.filter((e) => e.phaseId).map((e) => e.phaseId!));
  }, [validation]);

  const updateDef = useCallback((updater: (prev: WorkflowDef) => WorkflowDef) => {
    setDef((prev) => {
      const next = updater(prev);
      setDirty(true);
      return next;
    });
  }, []);

  const addPhase = () => {
    const idx = def.phases.length;
    const id = `new-phase-${idx + 1}`;
    updateDef((d) => ({ ...d, phases: [...d.phases, makePhaseEntry(id)] }));
    setSelectedPhaseIdx(idx);
  };

  const removePhase = (idx: number) => {
    updateDef((d) => ({ ...d, phases: d.phases.filter((_, i) => i !== idx) }));
    if (selectedPhaseIdx === idx) setSelectedPhaseIdx(null);
    else if (selectedPhaseIdx !== null && selectedPhaseIdx > idx) setSelectedPhaseIdx(selectedPhaseIdx - 1);
  };

  const movePhase = (from: number, to: number) => {
    updateDef((d) => {
      const next = [...d.phases];
      const [moved] = next.splice(from, 1);
      next.splice(to, 0, moved);
      return { ...d, phases: next };
    });
    if (selectedPhaseIdx === from) setSelectedPhaseIdx(to);
  };

  const updatePhase = (idx: number, updated: PhaseEntry) => {
    updateDef((d) => ({
      ...d,
      phases: d.phases.map((p, i) => (i === idx ? updated : p)),
    }));
  };

  const onValidate = () => {
    setValidation(validateDef(def));
  };

  const onSave = async () => {
    const result = validateDef(def);
    setValidation(result);
    if (!result.valid) return;
    setSaveError(null);
    const phasesJson = JSON.stringify(def.phases.map((p) => ({
      id: p.id,
      max_rework_attempts: p.maxReworkAttempts,
      skip_if: p.skipIf.length > 0 ? p.skipIf : undefined,
      on_verdict: {
        advance: p.onVerdict.advance.target ? { target: p.onVerdict.advance.target } : undefined,
        rework: p.onVerdict.rework.target || p.onVerdict.rework.allowAgentTarget ? {
          target: p.onVerdict.rework.target ?? undefined,
          allow_agent_target: p.onVerdict.rework.allowAgentTarget || undefined,
        } : undefined,
        fail: p.onVerdict.fail.target ? { target: p.onVerdict.fail.target } : undefined,
      },
    })));
    const variablesJson = def.variables.length > 0 ? JSON.stringify(def.variables.map((v) => ({
      name: v.name,
      description: v.description || undefined,
      required: v.required || undefined,
      default: v.default || undefined,
    }))) : undefined;
    const { error: err } = await upsertDef({
      id: def.id,
      name: def.name,
      description: def.description || null,
      phases: phasesJson,
      variables: variablesJson,
    });
    if (err) {
      setSaveError(err.message);
    } else {
      setDirty(false);
      setSaveMsg("Workflow saved");
      setTimeout(() => setSaveMsg(null), 3000);
    }
  };

  const selectedPhase = selectedPhaseIdx !== null ? def.phases[selectedPhaseIdx] : null;
  const allPhaseIds = def.phases.map((p) => p.id);

  return (
    <div className="space-y-6">
      <div className="flex items-center justify-between">
        <div className="flex items-center gap-3">
          <Link to="/workflows/builder" className="text-muted-foreground hover:text-foreground transition-colors">
            <ArrowLeft className="h-4 w-4" />
          </Link>
          <div className="flex items-center gap-2">
            <h1 className="text-2xl font-semibold tracking-tight">{def.name || "Untitled Workflow"}</h1>
            {dirty && <span className="h-2 w-2 rounded-full bg-amber-500" title="Unsaved changes" />}
          </div>
        </div>
        <div className="flex items-center gap-2">
          <Button size="sm" variant="outline" onClick={() => setShowPreview(!showPreview)}>
            <Eye className="h-3.5 w-3.5 mr-1.5" />
            Preview
          </Button>
          <Button size="sm" variant="outline" onClick={onValidate}>
            <CheckCircle2 className="h-3.5 w-3.5 mr-1.5" />
            Validate
          </Button>
          <Button size="sm" onClick={onSave}>
            <Save className="h-3.5 w-3.5 mr-1.5" />
            Save
          </Button>
        </div>
      </div>

      {saveMsg && (
        <Alert className="ao-fade-in">
          <AlertDescription>{saveMsg}</AlertDescription>
        </Alert>
      )}

      {saveError && (
        <Alert variant="destructive" role="alert" className="ao-fade-in">
          <AlertDescription>{saveError}</AlertDescription>
        </Alert>
      )}

      {validation && !validation.valid && (
        <Card className="border-destructive/40 bg-destructive/5 ao-fade-in">
          <CardContent className="pt-3 pb-3 px-4">
            <p className="text-xs uppercase tracking-wider text-destructive/80 font-medium mb-2">Validation Errors</p>
            <ul className="space-y-1">
              {validation.errors.map((e, i) => (
                <li key={i} className="text-xs text-destructive flex items-start gap-1.5">
                  <AlertCircle className="h-3 w-3 mt-0.5 shrink-0" />
                  {e.message}
                </li>
              ))}
            </ul>
          </CardContent>
        </Card>
      )}

      <Card className="border-border/40 bg-card/60">
        <CardContent className="pt-4 pb-4 px-4">
          <div className="grid grid-cols-3 gap-4">
            <div>
              <label className="text-[11px] uppercase tracking-wider text-muted-foreground/60 font-medium">Workflow ID</label>
              <Input
                value={def.id}
                onChange={(e) => updateDef((d) => ({ ...d, id: e.target.value }))}
                disabled={!isNew}
                className="mt-1 font-mono text-xs h-8"
                placeholder="my-workflow"
              />
            </div>
            <div>
              <label className="text-[11px] uppercase tracking-wider text-muted-foreground/60 font-medium">Name</label>
              <Input
                value={def.name}
                onChange={(e) => updateDef((d) => ({ ...d, name: e.target.value }))}
                className="mt-1 text-xs h-8"
                placeholder="My Workflow"
              />
            </div>
            <div>
              <label className="text-[11px] uppercase tracking-wider text-muted-foreground/60 font-medium">Description</label>
              <Input
                value={def.description}
                onChange={(e) => updateDef((d) => ({ ...d, description: e.target.value }))}
                className="mt-1 text-xs h-8"
                placeholder="What this workflow does"
              />
            </div>
          </div>
        </CardContent>
      </Card>

      <Tabs value={activeTab} onValueChange={setActiveTab}>
        <TabsList>
          <TabsTrigger value={0}>Phases</TabsTrigger>
          <TabsTrigger value={1}>Variables</TabsTrigger>
          <TabsTrigger value={2}>Transitions</TabsTrigger>
        </TabsList>

        <TabsContent value={0}>
          <div className="flex gap-4 mt-4">
            <div className="flex-1 space-y-4">
              <div className="flex items-center flex-wrap gap-0">
                {def.phases.map((phase, i) => (
                  <PhaseNode
                    key={`${phase.id}-${i}`}
                    phase={phase}
                    index={i}
                    total={def.phases.length}
                    selected={selectedPhaseIdx === i}
                    hasError={errorPhaseIds.has(phase.id)}
                    onSelect={() => setSelectedPhaseIdx(selectedPhaseIdx === i ? null : i)}
                    onMoveLeft={() => movePhase(i, i - 1)}
                    onMoveRight={() => movePhase(i, i + 1)}
                    onRemove={() => removePhase(i)}
                  />
                ))}
                <Button size="sm" variant="outline" onClick={addPhase} className="ml-2">
                  <Plus className="h-3.5 w-3.5 mr-1" />
                  Add Phase
                </Button>
              </div>

              {def.phases.length === 0 && (
                <div className="flex flex-col items-center justify-center py-8 gap-3">
                  <p className="text-sm text-muted-foreground/60">No phases yet</p>
                  <Button variant="outline" onClick={addPhase}>
                    <Plus className="h-3.5 w-3.5 mr-1.5" />
                    Add First Phase
                  </Button>
                </div>
              )}
            </div>

            {selectedPhase && selectedPhaseIdx !== null && (
              <PhaseDetailPanel
                phase={selectedPhase}
                allPhaseIds={allPhaseIds}
                onChange={(updated) => updatePhase(selectedPhaseIdx, updated)}
              />
            )}
          </div>
        </TabsContent>

        <TabsContent value={1}>
          <div className="mt-4 space-y-3">
            {def.variables.map((v, i) => (
              <VariableCard
                key={i}
                variable={v}
                onChange={(updated) =>
                  updateDef((d) => ({
                    ...d,
                    variables: d.variables.map((vv, j) => (j === i ? updated : vv)),
                  }))
                }
                onRemove={() =>
                  updateDef((d) => ({
                    ...d,
                    variables: d.variables.filter((_, j) => j !== i),
                  }))
                }
              />
            ))}
            <Button
              size="sm"
              variant="outline"
              onClick={() => updateDef((d) => ({ ...d, variables: [...d.variables, makeVariableEntry()] }))}
            >
              <Plus className="h-3.5 w-3.5 mr-1" />
              Add Variable
            </Button>
            {def.variables.length === 0 && (
              <p className="text-sm text-muted-foreground/60 text-center py-4">No variables defined</p>
            )}
          </div>
        </TabsContent>

        <TabsContent value={2}>
          <div className="mt-4">
            <Card className="border-border/40 bg-card/60">
              <CardHeader className="pb-2 pt-3 px-4">
                <CardTitle className="text-xs uppercase tracking-wider text-muted-foreground/60 font-medium">Transition Map</CardTitle>
              </CardHeader>
              <CardContent className="px-4 pb-4">
                <TransitionsTable phases={def.phases} />
              </CardContent>
            </Card>
          </div>
        </TabsContent>
      </Tabs>

      {showPreview && (
        <Card className="border-border/40 bg-card/60 ao-fade-in">
          <CardHeader className="pb-2 pt-3 px-4">
            <CardTitle className="text-xs uppercase tracking-wider text-muted-foreground/60 font-medium">Preview (JSON)</CardTitle>
          </CardHeader>
          <CardContent className="px-4 pb-4">
            <pre className="text-xs font-mono overflow-auto max-h-96 p-3 rounded bg-muted/20">
              {defToPreview(def)}
            </pre>
          </CardContent>
        </Card>
      )}
    </div>
  );
}

const DELETE_MUTATION = `mutation DeleteWorkflowDefinition($id: ID!) { deleteWorkflowDefinition(id: $id) }`;

export function WorkflowBuilderBrowsePage() {
  const navigate = useNavigate();
  const [result, reexecute] = useQuery({ query: WorkflowDefinitionsDocument });
  const [, deleteDef] = useMutation(DELETE_MUTATION);
  const [duplicateTarget, setDuplicateTarget] = useState<string | null>(null);
  const [deleteTarget, setDeleteTarget] = useState<string | null>(null);
  const [deleteError, setDeleteError] = useState<string | null>(null);

  const { data, fetching, error } = result;

  if (fetching) return <PageLoading />;
  if (error) return <PageError message={error.message} />;

  const definitions = data?.workflowDefinitions ?? [];

  return (
    <div className="space-y-6">
      <div className="flex items-center justify-between">
        <div>
          <h1 className="text-2xl font-semibold tracking-tight">Workflow Builder</h1>
          <p className="text-sm text-muted-foreground">Create and manage workflow definitions</p>
        </div>
        <Button onClick={() => navigate("/workflows/builder/new")}>
          <Plus className="h-4 w-4 mr-1.5" />
          New Workflow
        </Button>
      </div>

      {definitions.length === 0 && (
        <div className="flex flex-col items-center justify-center py-12 gap-3">
          <p className="text-sm text-muted-foreground/60">No workflow definitions yet</p>
          <Button variant="outline" onClick={() => navigate("/workflows/builder/new")}>
            <Plus className="h-4 w-4 mr-1.5" />
            Create Your First Workflow
          </Button>
        </div>
      )}

      <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4">
        {definitions.map((def) => (
          <Card key={def.id} className="border-border/40 bg-card/60 hover:border-border/60 transition-colors">
            <CardContent className="pt-4 pb-4 px-4 space-y-3">
              <div>
                <p className="font-mono text-xs text-muted-foreground/60">{def.id}</p>
                <p className="font-medium mt-0.5">{def.name}</p>
                {def.description && (
                  <p className="text-sm text-muted-foreground line-clamp-2 mt-1">{def.description}</p>
                )}
              </div>

              <div className="flex items-center gap-1">
                {def.phases.map((phaseId, i) => (
                  <div key={phaseId} className="flex items-center gap-1">
                    <span className="h-2 w-2 rounded-full bg-primary/50" />
                    {i < def.phases.length - 1 && <span className="w-3 h-px bg-border" />}
                  </div>
                ))}
              </div>

              <div className="flex items-center justify-between">
                <span className="text-xs text-muted-foreground">{def.phases.length} phase{def.phases.length !== 1 ? "s" : ""}</span>
                <div className="flex items-center gap-1">
                  <Button
                    size="sm"
                    variant="ghost"
                    className="h-7 px-2"
                    onClick={() => setDuplicateTarget(def.id)}
                  >
                    <Copy className="h-3 w-3" />
                  </Button>
                  <Button
                    size="sm"
                    variant="ghost"
                    className="h-7 px-2 text-destructive/60 hover:text-destructive"
                    onClick={() => setDeleteTarget(def.id)}
                  >
                    <Trash2 className="h-3 w-3" />
                  </Button>
                  <Button
                    size="sm"
                    variant="outline"
                    onClick={() => navigate(`/workflows/builder/${def.id}`)}
                  >
                    <Pencil className="h-3 w-3 mr-1" />
                    Edit
                  </Button>
                </div>
              </div>
            </CardContent>
          </Card>
        ))}
      </div>

      {duplicateTarget && (
        <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/50">
          <Card className="border-border/40 bg-card w-80">
            <CardContent className="pt-4 pb-4 px-4 space-y-3">
              <p className="text-sm font-medium">Duplicate Workflow</p>
              <p className="text-xs text-muted-foreground">Coming soon</p>
              <div className="flex justify-end">
                <Button size="sm" variant="outline" onClick={() => setDuplicateTarget(null)}>Close</Button>
              </div>
            </CardContent>
          </Card>
        </div>
      )}

      {deleteTarget && (
        <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/50">
          <Card className="border-border/40 bg-card w-80">
            <CardContent className="pt-4 pb-4 px-4 space-y-3">
              <p className="text-sm font-medium">Delete Workflow</p>
              <p className="text-xs text-muted-foreground">Are you sure you want to delete <span className="font-mono">{deleteTarget}</span>? This cannot be undone.</p>
              {deleteError && (
                <Alert variant="destructive" role="alert">
                  <AlertDescription>{deleteError}</AlertDescription>
                </Alert>
              )}
              <div className="flex justify-end gap-2">
                <Button size="sm" variant="outline" onClick={() => { setDeleteTarget(null); setDeleteError(null); }}>Cancel</Button>
                <Button size="sm" variant="destructive" onClick={async () => {
                  const { error: err } = await deleteDef({ id: deleteTarget });
                  if (err) {
                    setDeleteError(err.message);
                  } else {
                    setDeleteTarget(null);
                    setDeleteError(null);
                    reexecute();
                  }
                }}>Delete</Button>
              </div>
            </CardContent>
          </Card>
        </div>
      )}
    </div>
  );
}

export function WorkflowBuilderNewPage() {
  const navigate = useNavigate();
  const [searchParams] = useSearchParams();
  const templateId = searchParams.get("template");

  if (templateId && TEMPLATES[templateId]) {
    const template = TEMPLATES[templateId];
    const initial: WorkflowDef = {
      id: "",
      name: template.name,
      description: template.description,
      phases: template.phases.map((id) => makePhaseEntry(id)),
      variables: [],
    };
    return <EditorCore initial={initial} isNew />;
  }

  return (
    <div className="space-y-6">
      <div className="flex items-center gap-3">
        <Link to="/workflows/builder" className="text-muted-foreground hover:text-foreground transition-colors">
          <ArrowLeft className="h-4 w-4" />
        </Link>
        <div>
          <h1 className="text-2xl font-semibold tracking-tight">New Workflow</h1>
          <p className="text-sm text-muted-foreground">Choose a starting template</p>
        </div>
      </div>

      <div className="grid grid-cols-1 md:grid-cols-3 gap-4">
        <button
          type="button"
          onClick={() => navigate("/workflows/builder/new?template=standard")}
          className="text-left"
        >
          <Card className="border-border/40 bg-card/60 hover:border-primary/40 hover:bg-primary/5 transition-colors h-full">
            <CardContent className="pt-4 pb-4 px-4 space-y-3">
              <div className="h-10 w-10 rounded-lg bg-primary/10 flex items-center justify-center">
                <Layers className="h-5 w-5 text-primary/70" />
              </div>
              <div>
                <p className="font-medium">Standard</p>
                <p className="text-sm text-muted-foreground mt-1">{TEMPLATES.standard.description}</p>
              </div>
              <div className="flex items-center gap-1">
                {TEMPLATES.standard.phases.map((id, i) => (
                  <div key={id} className="flex items-center gap-1">
                    <span className="h-2 w-2 rounded-full bg-primary/50" />
                    {i < TEMPLATES.standard.phases.length - 1 && <span className="w-3 h-px bg-border" />}
                  </div>
                ))}
              </div>
              <p className="text-xs text-muted-foreground font-mono">
                {TEMPLATES.standard.phases.join(" \u2192 ")}
              </p>
              <Button size="sm" variant="outline" className="w-full pointer-events-none">Use Template</Button>
            </CardContent>
          </Card>
        </button>

        <button
          type="button"
          onClick={() => navigate("/workflows/builder/new?template=ui-ux")}
          className="text-left"
        >
          <Card className="border-border/40 bg-card/60 hover:border-primary/40 hover:bg-primary/5 transition-colors h-full">
            <CardContent className="pt-4 pb-4 px-4 space-y-3">
              <div className="h-10 w-10 rounded-lg bg-primary/10 flex items-center justify-center">
                <PaintBucket className="h-5 w-5 text-primary/70" />
              </div>
              <div>
                <p className="font-medium">UI/UX</p>
                <p className="text-sm text-muted-foreground mt-1">{TEMPLATES["ui-ux"].description}</p>
              </div>
              <div className="flex items-center gap-1">
                {TEMPLATES["ui-ux"].phases.map((id, i) => (
                  <div key={id} className="flex items-center gap-1">
                    <span className="h-2 w-2 rounded-full bg-primary/50" />
                    {i < TEMPLATES["ui-ux"].phases.length - 1 && <span className="w-3 h-px bg-border" />}
                  </div>
                ))}
              </div>
              <p className="text-xs text-muted-foreground font-mono">
                {TEMPLATES["ui-ux"].phases.join(" \u2192 ")}
              </p>
              <Button size="sm" variant="outline" className="w-full pointer-events-none">Use Template</Button>
            </CardContent>
          </Card>
        </button>

        <button
          type="button"
          onClick={() => navigate("/workflows/builder/new?template=blank")}
          className="text-left"
        >
          <Card className="border-border/40 bg-card/60 hover:border-primary/40 hover:bg-primary/5 transition-colors h-full">
            <CardContent className="pt-4 pb-4 px-4 space-y-3">
              <div className="h-10 w-10 rounded-lg bg-muted/30 flex items-center justify-center">
                <FileText className="h-5 w-5 text-muted-foreground/70" />
              </div>
              <div>
                <p className="font-medium">Blank</p>
                <p className="text-sm text-muted-foreground mt-1">{TEMPLATES.blank.description}</p>
              </div>
              <div className="h-2" />
              <p className="text-xs text-muted-foreground">No phases — start from scratch</p>
              <Button size="sm" variant="outline" className="w-full pointer-events-none">Start Blank</Button>
            </CardContent>
          </Card>
        </button>
      </div>
    </div>
  );
}

export function WorkflowBuilderEditPage() {
  const { definitionId } = useParams<{ definitionId: string }>();

  const [result] = useQuery({
    query: WorkflowDefinitionsDocument,
  });

  const { data, fetching, error } = result;
  if (fetching) return <PageLoading />;
  if (error) return <PageError message={error.message} />;

  const definitions = data?.workflowDefinitions ?? [];
  const found = definitions.find((d) => d.id === definitionId);
  if (!found) return <PageError message={`Workflow definition "${definitionId}" not found.`} />;

  const initial: WorkflowDef = {
    id: found.id,
    name: found.name,
    description: found.description ?? "",
    phases: found.phases.map((id) => makePhaseEntry(id)),
    variables: [],
  };

  return <EditorCore key={definitionId} initial={initial} isNew={false} />;
}
