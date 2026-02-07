import { NextResponse } from "next/server";
import { prisma } from "@/lib/helpers";

export async function PATCH(
  req: Request,
  { params }: { params: Promise<{ id: string }> },
) {
  try {
    const data = await req.json();
    const { id } = await params;

    if (!id) {
      return NextResponse.json({ error: "ID required" }, { status: 400 });
    }

    const update: {
      name?: string;
      balance?: number;
      leverage?: number;
    } = {};

    if (data?.name !== undefined) {
      const name = String(data.name).trim();
      if (!name) {
        return NextResponse.json({ error: "Name required" }, { status: 400 });
      }
      update.name = name;
    }
    if (data?.balance !== undefined) {
      const balance = Number(data.balance);
      if (!Number.isFinite(balance) || balance < 0) {
        return NextResponse.json({ error: "Balance invalid" }, { status: 400 });
      }
      update.balance = balance;
    }
    if (data?.leverage !== undefined) {
      const leverage = Number(data.leverage);
      if (!Number.isFinite(leverage) || leverage <= 0) {
        return NextResponse.json({ error: "Leverage invalid" }, { status: 400 });
      }
      update.leverage = leverage;
    }

    const updated = await prisma.account.update({
      where: { id },
      data: update,
      select: {
        id: true,
        name: true,
        balance: true,
        leverage: true,
        createdAt: true,
        updatedAt: true,
      },
    });

    return NextResponse.json(updated, { status: 200 });
  } catch (err) {
    console.error("Prisma error:", err);
    return NextResponse.json(
      { error: "Failed to update account: " + err },
      { status: 500 },
    );
  }
}
