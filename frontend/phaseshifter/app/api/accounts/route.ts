import { NextResponse } from "next/server";
import { prisma } from "@/lib/helpers";

export async function GET() {
  try {
    const accounts = await prisma.account.findMany({
      orderBy: { createdAt: "asc" },
      select: {
        id: true,
        name: true,
        balance: true,
        leverage: true,
        createdAt: true,
        updatedAt: true,
      },
    });
    return NextResponse.json(accounts);
  } catch (err) {
    console.error("Prisma error:", err);
    return NextResponse.json(
      { error: "Failed to load accounts" },
      { status: 500 },
    );
  }
}

export async function POST(req: Request) {
  try {
    const data = await req.json();
    const name = String(data?.name ?? "").trim();
    const balance = Number(data?.balance ?? 0);
    const leverage = Number(data?.leverage ?? 1);

    if (!name) {
      return NextResponse.json({ error: "Name required" }, { status: 400 });
    }
    if (!Number.isFinite(balance) || balance < 0) {
      return NextResponse.json({ error: "Balance invalid" }, { status: 400 });
    }
    if (!Number.isFinite(leverage) || leverage <= 0) {
      return NextResponse.json({ error: "Leverage invalid" }, { status: 400 });
    }

    const created = await prisma.account.create({
      data: {
        name,
        balance,
        leverage,
      },
      select: {
        id: true,
        name: true,
        balance: true,
        leverage: true,
        createdAt: true,
        updatedAt: true,
      },
    });

    return NextResponse.json(created, { status: 201 });
  } catch (err) {
    console.error("Prisma error:", err);
    return NextResponse.json(
      { error: "Failed to create account: " + err },
      { status: 500 },
    );
  }
}
