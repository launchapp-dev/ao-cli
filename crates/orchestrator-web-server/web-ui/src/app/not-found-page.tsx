import { Link } from "react-router-dom";
import { Button } from "@/components/ui/button";

export function NotFoundPage() {
  return (
    <div className="space-y-4 py-12 text-center">
      <h1 className="text-4xl font-bold">404</h1>
      <p className="text-muted-foreground">The requested page does not exist.</p>
      <Link to="/dashboard">
        <Button variant="outline">Go to Dashboard</Button>
      </Link>
    </div>
  );
}
