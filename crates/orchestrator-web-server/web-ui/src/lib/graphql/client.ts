import { GraphQLClient } from "graphql-request";
import { QueryClient, useQuery as _useQuery, useMutation as _useMutation } from "@tanstack/react-query";
import { createClient as createWSClient } from "graphql-ws";
import { useEffect, useRef, useState } from "react";

const gql = new GraphQLClient(`${window.location.origin}/graphql`);

export const queryClient = new QueryClient({
  defaultOptions: {
    queries: { staleTime: 5_000, refetchOnWindowFocus: true },
  },
});

function gqlRequest<TResult>(doc: unknown, variables?: unknown): Promise<TResult> {
  const query = typeof doc === "string" ? doc : String(doc);
  return gql.request(query, variables as Record<string, unknown> | undefined) as Promise<TResult>;
}

function getOpName(doc: unknown): string {
  const str = typeof doc === "string" ? doc : String(doc);
  const m = str.match(/(?:query|mutation|subscription)\s+(\w+)/);
  return m?.[1] ?? "gql";
}

export function useQuery<TData = any>(opts: {
  query: unknown;
  variables?: Record<string, unknown>;
  pause?: boolean;
}): [{ data: TData | undefined; fetching: boolean; error: { message: string } | undefined }, (...args: any[]) => void] {
  const result = _useQuery<TData>({
    queryKey: [getOpName(opts.query), opts.variables],
    queryFn: () => gqlRequest<TData>(opts.query, opts.variables),
    enabled: !opts.pause,
  });

  return [
    {
      data: result.data,
      fetching: result.isLoading,
      error: result.error ? { message: result.error.message } : undefined,
    },
    () => result.refetch(),
  ];
}

export function useMutation<TData = any, TVars = any>(
  doc: unknown,
): [{ fetching: boolean }, (vars: TVars) => Promise<{ data?: TData; error?: { message: string } }>] {
  const mutation = _useMutation<TData, Error, TVars>({
    mutationFn: (variables: TVars) => gqlRequest<TData>(doc, variables),
  });

  const execute = async (variables: TVars) => {
    try {
      const data = await mutation.mutateAsync(variables);
      return { data, error: undefined };
    } catch (e: any) {
      return { data: undefined, error: { message: e.message } };
    }
  };

  return [{ fetching: mutation.isPending }, execute];
}

const wsProtocol = window.location.protocol === "https:" ? "wss:" : "ws:";
const wsUrl = `${wsProtocol}//${window.location.host}/graphql/ws`;
const wsClient = createWSClient({ url: wsUrl });

export function useSubscription<TData = any, TResult = TData>(
  opts: { query: unknown; variables?: Record<string, unknown>; pause?: boolean },
  handler?: (prev: TResult | undefined, data: TData) => TResult,
): [{ data: TResult | undefined; fetching: boolean; error: { message: string } | undefined }] {
  const [fetching, setFetching] = useState(!opts.pause);
  const [error, setError] = useState<{ message: string } | undefined>();
  const [result, setResult] = useState<TResult | undefined>();
  const resultRef = useRef(result);
  resultRef.current = result;

  useEffect(() => {
    if (opts.pause) {
      setFetching(false);
      return;
    }

    const query = typeof opts.query === "string" ? opts.query : String(opts.query);
    setFetching(true);
    setError(undefined);

    const unsubscribe = wsClient.subscribe(
      { query, variables: opts.variables as Record<string, unknown> },
      {
        next(value) {
          const data = value.data as TData;
          if (handler) {
            const next = handler(resultRef.current, data);
            setResult(next);
          } else {
            setResult(data as unknown as TResult);
          }
        },
        error(err) {
          setError({ message: err instanceof Error ? err.message : String(err) });
          setFetching(false);
        },
        complete() {
          setFetching(false);
        },
      },
    );

    return () => unsubscribe();
  }, [opts.pause, String(opts.query), JSON.stringify(opts.variables)]);

  return [{ data: result, fetching, error }];
}
