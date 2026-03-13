import { FormEvent, useState } from "react";
import { useMutation } from "@/lib/graphql/client";
import { Card, CardContent } from "@/components/ui/card";
import { Button } from "@/components/ui/button";
import { Textarea } from "@/components/ui/textarea";
import { Alert, AlertDescription } from "@/components/ui/alert";
import { ReviewHandoffDocument } from "@/lib/graphql/generated/graphql";

export function ReviewHandoffPage() {
  const [, handoff] = useMutation(ReviewHandoffDocument);
  const [targetRole, setTargetRole] = useState("em");
  const [question, setQuestion] = useState("");
  const [context, setContext] = useState("");
  const [feedback, setFeedback] = useState<{ kind: "ok" | "error"; message: string } | null>(null);

  const onSubmit = async (e: FormEvent) => {
    e.preventDefault();
    if (!question.trim()) return;
    const { error } = await handoff({
      targetRole,
      question: question.trim(),
      context: context.trim() || undefined,
    });
    if (error) setFeedback({ kind: "error", message: error.message });
    else {
      setFeedback({ kind: "ok", message: "Review handoff submitted." });
      setQuestion("");
      setContext("");
    }
  };

  return (
    <div className="space-y-4">
      <h1 className="text-2xl font-semibold tracking-tight">Review Handoff</h1>

      {feedback && (
        <Alert variant={feedback.kind === "error" ? "destructive" : "default"}>
          <AlertDescription>{feedback.message}</AlertDescription>
        </Alert>
      )}

      <Card>
        <CardContent className="pt-4">
          <form onSubmit={onSubmit} className="space-y-4">
            <div>
              <label className="text-sm font-medium">Target Role</label>
              <select
                value={targetRole}
                onChange={(e) => setTargetRole(e.target.value)}
                className="mt-1 h-9 w-full rounded-md border border-input bg-background px-3 text-sm"
              >
                <option value="em">em</option>
                <option value="reviewer">reviewer</option>
                <option value="qa">qa</option>
              </select>
            </div>
            <div>
              <label className="text-sm font-medium">Question</label>
              <Textarea
                value={question}
                onChange={(e) => setQuestion(e.target.value)}
                rows={3}
                required
                className="mt-1"
              />
            </div>
            <div>
              <label className="text-sm font-medium">Context (optional)</label>
              <Textarea
                value={context}
                onChange={(e) => setContext(e.target.value)}
                rows={3}
                className="mt-1"
              />
            </div>
            <Button type="submit">Submit Handoff</Button>
          </form>
        </CardContent>
      </Card>
    </div>
  );
}
