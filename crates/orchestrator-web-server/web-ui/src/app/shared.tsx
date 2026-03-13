import { ReactNode } from "react";
import { Badge } from "@/components/ui/badge";
import { Card, CardContent } from "@/components/ui/card";
import { Skeleton } from "@/components/ui/skeleton";
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";

export function statusColor(status: string): "default" | "secondary" | "destructive" | "outline" {
  const s = status.toLowerCase().replace(/[_\s]/g, "-");
  if (["done", "completed", "approved", "implemented"].includes(s)) return "default";
  if (["in-progress", "running", "inprogress"].includes(s)) return "secondary";
  if (["blocked", "failed", "cancelled", "crashed"].includes(s)) return "destructive";
  return "outline";
}

export function priorityColor(p: string): "default" | "secondary" | "destructive" | "outline" {
  const v = (p || "").toLowerCase();
  if (v === "critical") return "destructive";
  if (v === "high") return "secondary";
  return "outline";
}

export function StatusDot({ status }: { status: string }) {
  const s = status.toLowerCase().replace(/[_\s]/g, "-");
  let cls = "ao-status-dot ao-status-dot--idle";
  if (["done", "completed", "approved", "healthy"].includes(s)) cls = "ao-status-dot ao-status-dot--live";
  else if (["in-progress", "running", "inprogress"].includes(s)) cls = "ao-status-dot ao-status-dot--running";
  else if (["blocked", "failed", "cancelled", "crashed", "error"].includes(s)) cls = "ao-status-dot ao-status-dot--error";
  return <span className={cls} />;
}

export function PageLoading() {
  return (
    <div className="space-y-4 ao-fade-in">
      <Skeleton className="h-7 w-44 bg-muted/40" />
      <div className="grid grid-cols-2 md:grid-cols-4 gap-3">
        {[0, 1, 2, 3].map((i) => (
          <Skeleton key={i} className="h-20 bg-muted/30 rounded-lg" />
        ))}
      </div>
      <Skeleton className="h-48 w-full bg-muted/20 rounded-lg" />
    </div>
  );
}

export function PageError({ message }: { message: string }) {
  return (
    <Alert variant="destructive" className="ao-fade-in border-destructive/30 bg-destructive/8">
      <AlertTitle className="text-sm font-medium">Error</AlertTitle>
      <AlertDescription className="text-xs mt-1 font-mono opacity-80">{message}</AlertDescription>
    </Alert>
  );
}

export function StatCard({ label, value, accent }: { label: string; value: number | string; accent?: boolean }) {
  return (
    <Card className={`border-border/40 bg-card/60 backdrop-blur-sm transition-colors hover:border-border/60 ${accent ? "ao-glow-border" : ""}`}>
      <CardContent className="pt-3 pb-3 px-4">
        <p className="text-[11px] text-muted-foreground/70 uppercase tracking-wider font-medium">{label}</p>
        <p className={`text-xl font-semibold font-mono mt-0.5 ${accent ? "text-primary" : "text-foreground/90"}`}>{value}</p>
      </CardContent>
    </Card>
  );
}

export function SectionHeading({ children }: { children: ReactNode }) {
  return <h2 className="text-xs uppercase tracking-wider text-muted-foreground/60 font-medium">{children}</h2>;
}
