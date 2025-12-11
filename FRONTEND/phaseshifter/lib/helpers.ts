// Server-only Prisma helpers
import { PrismaClient } from "@prisma/client";
import type { Prisma } from "@prisma/client";

declare global {
  // eslint-disable-next-line no-var
  var __prisma__: PrismaClient | undefined;
}

// Lazily create a single PrismaClient instance across hot reloads in dev.
export function getPrisma(): PrismaClient {
  if (typeof window !== "undefined") {
    throw new Error("Prisma should only be used on the server");
  }
  if (process.env.NODE_ENV === "production") {
    return new PrismaClient();
  }
  if (!global.__prisma__) {
    global.__prisma__ = new PrismaClient();
  }
  return global.__prisma__!;
}

export const prisma = getPrisma();

// Type guards for common Prisma error codes
export function isPrismaKnownError(
  err: unknown,
): err is Prisma.PrismaClientKnownRequestError {
  return (
    !!err &&
    typeof err === "object" &&
    // @ts-expect-error runtime check
    err["code"] !== undefined &&
    // @ts-expect-error runtime check
    err["clientVersion"] !== undefined
  );
}

export function isUniqueConstraintError(
  err: unknown,
): err is Prisma.PrismaClientKnownRequestError {
  return isPrismaKnownError(err) && err.code === "P2002";
}

export function isNotFoundError(
  err: unknown,
): err is Prisma.PrismaClientKnownRequestError {
  return isPrismaKnownError(err) && err.code === "P2025";
}

// Wrap a query that may return null and throw a helpful error instead
export async function findOrThrow<T>(
  query: Promise<T | null>,
  message = "Not found",
): Promise<T> {
  const result = await query;
  if (result == null) throw new Error(message);
  return result;
}

// Simple transaction helper
export async function transactional<T>(
  fn: (tx: Prisma.TransactionClient) => Promise<T>,
): Promise<T> {
  return prisma.$transaction(async (tx) => fn(tx));
}

// Retry helper for transient Prisma errors
const transientCodes = new Set(["P1001", "P1008", "P2024"]);
export async function withRetry<T>(
  fn: () => Promise<T>,
  retries = 2,
): Promise<T> {
  let attempt = 0;
  // Exponential backoff starting at 100ms
  const backoff = (i: number) =>
    new Promise((r) => setTimeout(r, 100 * 2 ** i));

  // eslint-disable-next-line no-constant-condition
  while (true) {
    try {
      return await fn();
    } catch (err) {
      if (attempt >= retries) throw err;
      if (isPrismaKnownError(err) && transientCodes.has(err.code)) {
        await backoff(attempt);
        attempt += 1;
        continue;
      }
      throw err;
    }
  }
}

// Basic cursor pagination utility for list queries
export type CursorPage<TItem, TCursor = string | number> = {
  items: TItem[];
  nextCursor: TCursor | null;
};

export async function paginateById<TItem extends { id: string | number }>(
  fetchPage: (args: {
    take: number;
    cursor?: string | number;
  }) => Promise<TItem[]>,
  opts: { take: number; cursor?: string | number },
): Promise<CursorPage<TItem>> {
  const page = await fetchPage({ take: opts.take + 1, cursor: opts.cursor });
  const hasMore = page.length > opts.take;
  const items = hasMore ? page.slice(0, -1) : page;
  const nextCursor = hasMore ? (items[items.length - 1]?.id ?? null) : null;
  return { items, nextCursor };
}

// Example usage:
// const user = await findOrThrow(prisma.user.findUnique({ where: { id } }), "User not found");
// const result = await transactional((tx) => tx.user.create({ data }));
// const list = await withRetry(() => prisma.post.findMany({ take: 20 }));
