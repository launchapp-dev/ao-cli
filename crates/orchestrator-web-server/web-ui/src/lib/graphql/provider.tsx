import { QueryClientProvider } from "@tanstack/react-query";
import { queryClient } from "./client";

export function GraphQLProvider({ children }: { children: React.ReactNode }) {
  return <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>;
}
