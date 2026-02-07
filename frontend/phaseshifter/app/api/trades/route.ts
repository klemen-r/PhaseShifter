import { NextResponse } from "next/server";
import { prisma } from "@/lib/helpers";

const FUTURES_MULTIPLIERS: Record<string, number> = {
  NQ: 20,
  ES: 50,
  YM: 5,
};

function normalizeSymbol(symbol: string) {
  return symbol.toUpperCase();
}

function getContractMultiplier(symbol: string) {
  const upper = normalizeSymbol(symbol);
  const match = Object.keys(FUTURES_MULTIPLIERS).find((key) =>
    upper.startsWith(key),
  );
  if (match) return FUTURES_MULTIPLIERS[match];
  if (upper.includes("BTC")) return 1;
  return 1;
}

function computeRealizedPnl({
  side,
  qty,
  entryPrice,
  exitPrice,
  symbol,
}: {
  side: "LONG" | "SHORT";
  qty: number;
  entryPrice: number;
  exitPrice: number;
  symbol: string;
}) {
  const direction = side === "LONG" ? 1 : -1;
  const multiplier = getContractMultiplier(symbol);
  return (exitPrice - entryPrice) * direction * qty * multiplier;
}

export async function GET(req: Request) {
  try {
    const { searchParams } = new URL(req.url);
    const accountId = searchParams.get("accountId") ?? "";

    if (!accountId) {
      return NextResponse.json({ error: "accountId required" }, { status: 400 });
    }

    const trades = await prisma.trade.findMany({
      where: { accountId },
      orderBy: { openedAt: "desc" },
      select: {
        id: true,
        accountId: true,
        symbol: true,
        side: true,
        qty: true,
        leverage: true,
        entryPrice: true,
        exitPrice: true,
        openedAt: true,
        closedAt: true,
      },
    });

    return NextResponse.json(trades);
  } catch (err) {
    console.error("Prisma error:", err);
    return NextResponse.json(
      { error: "Failed to load trades" },
      { status: 500 },
    );
  }
}

export async function POST(req: Request) {
  try {
    const data = await req.json();
    const accountId = String(data?.accountId ?? "").trim();
    const symbol = String(data?.symbol ?? "").trim().toUpperCase();
    const side = String(data?.side ?? "").trim().toUpperCase();
    const qty = Number(data?.qty ?? 0);
    const entryPrice = Number(data?.entryPrice ?? 0);
    const leverage = Number(data?.leverage ?? 1);

    if (!accountId) {
      return NextResponse.json({ error: "accountId required" }, { status: 400 });
    }
    if (!symbol) {
      return NextResponse.json({ error: "symbol required" }, { status: 400 });
    }
    if (side !== "LONG" && side !== "SHORT") {
      return NextResponse.json({ error: "side invalid" }, { status: 400 });
    }
    if (!Number.isFinite(qty) || qty <= 0) {
      return NextResponse.json({ error: "qty invalid" }, { status: 400 });
    }
    if (!Number.isFinite(entryPrice) || entryPrice <= 0) {
      return NextResponse.json(
        { error: "entryPrice invalid" },
        { status: 400 },
      );
    }
    if (!Number.isFinite(leverage) || leverage <= 0) {
      return NextResponse.json(
        { error: "leverage invalid" },
        { status: 400 },
      );
    }

    const created = await prisma.trade.create({
      data: {
        accountId,
        symbol,
        side: side === "LONG" ? "LONG" : "SHORT",
        qty,
        leverage,
        entryPrice,
      },
      select: {
        id: true,
        accountId: true,
        symbol: true,
        side: true,
        qty: true,
        leverage: true,
        entryPrice: true,
        exitPrice: true,
        openedAt: true,
        closedAt: true,
      },
    });

    return NextResponse.json(created, { status: 201 });
  } catch (err) {
    console.error("Prisma error:", err);
    return NextResponse.json(
      { error: "Failed to create trade: " + err },
      { status: 500 },
    );
  }
}

export async function PATCH(req: Request) {
  try {
    const data = await req.json();
    const id = String(data?.id ?? "").trim();
    const exitPrice = Number(data?.exitPrice ?? 0);
    const closedAt = data?.closedAt ? new Date(String(data.closedAt)) : new Date();

    if (!id) {
      return NextResponse.json({ error: "id required" }, { status: 400 });
    }
    if (!Number.isFinite(exitPrice) || exitPrice <= 0) {
      return NextResponse.json({ error: "exitPrice invalid" }, { status: 400 });
    }
    if (Number.isNaN(closedAt.getTime())) {
      return NextResponse.json({ error: "closedAt invalid" }, { status: 400 });
    }

    const result = await prisma.$transaction(async (tx) => {
      const existing = await tx.trade.findUnique({
        where: { id },
        select: {
          id: true,
          accountId: true,
          symbol: true,
          side: true,
          qty: true,
          entryPrice: true,
          closedAt: true,
        },
      });

      if (!existing) {
        return { kind: "not_found" as const };
      }
      if (existing.closedAt) {
        return { kind: "already_closed" as const };
      }

      const closeResult = await tx.trade.updateMany({
        where: { id, closedAt: null },
        data: {
          exitPrice,
          closedAt,
        },
      });

      if (closeResult.count === 0) {
        return { kind: "already_closed" as const };
      }

      const side = existing.side === "LONG" ? "LONG" : "SHORT";
      const realizedPnl = computeRealizedPnl({
        side,
        qty: existing.qty,
        entryPrice: existing.entryPrice,
        exitPrice,
        symbol: existing.symbol,
      });

      const updatedAccount = await tx.account.update({
        where: { id: existing.accountId },
        data: { balance: { increment: realizedPnl } },
        select: {
          id: true,
          name: true,
          balance: true,
          leverage: true,
          createdAt: true,
          updatedAt: true,
        },
      });

      const updatedTrade = await tx.trade.findUnique({
        where: { id },
        select: {
          id: true,
          accountId: true,
          symbol: true,
          side: true,
          qty: true,
          leverage: true,
          entryPrice: true,
          exitPrice: true,
          openedAt: true,
          closedAt: true,
        },
      });

      if (!updatedTrade) {
        throw new Error("Closed trade not found after update");
      }

      return {
        kind: "ok" as const,
        trade: updatedTrade,
        account: updatedAccount,
        realizedPnl,
      };
    });

    if (result.kind === "not_found") {
      return NextResponse.json({ error: "Trade not found" }, { status: 404 });
    }
    if (result.kind === "already_closed") {
      return NextResponse.json({ error: "Trade is already closed" }, { status: 409 });
    }

    return NextResponse.json(result, { status: 200 });
  } catch (err) {
    console.error("Prisma error:", err);
    return NextResponse.json(
      { error: "Failed to close trade: " + err },
      { status: 500 },
    );
  }
}
