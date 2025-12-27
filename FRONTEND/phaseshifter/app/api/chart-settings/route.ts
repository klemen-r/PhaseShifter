import { NextResponse } from "next/server";
import { prisma } from "@/lib/helpers";

export async function GET() {
  try {
    const settings = await prisma.chartSettings.findMany({
      orderBy: { updatedAt: "desc" },
      select: { id: true, name: true, settings: true, updatedAt: true },
    });
    return NextResponse.json(settings);
  } catch (err) {
    console.error("Prisma error:", err);
    return NextResponse.json(
      { error: "Failed to load settings" },
      { status: 500 }
    );
  }
}

export async function POST(req: Request) {
  try {
    const data = await req.json();
    const name = String(data?.name ?? "").trim();
    const settings = data?.settings;

    if (!name) {
      return NextResponse.json({ error: "Name required" }, { status: 400 });
    }
    if (!settings || typeof settings !== "object") {
      return NextResponse.json({ error: "Settings required" }, { status: 400 });
    }

    // Upsert: update if exists, create if not
    const saved = await prisma.chartSettings.upsert({
      where: { name },
      update: { settings: JSON.stringify(settings) },
      create: { name, settings: JSON.stringify(settings) },
    });

    return NextResponse.json(saved, { status: 201 });
  } catch (err) {
    console.error("Prisma error:", err);
    return NextResponse.json(
      { error: "Failed to save settings: " + err },
      { status: 500 }
    );
  }
}

export async function DELETE(req: Request) {
  try {
    const data = await req.json();
    const id = data?.id;

    if (!id) {
      return NextResponse.json({ error: "ID required" }, { status: 400 });
    }

    const deleted = await prisma.chartSettings.delete({
      where: { id },
    });

    return NextResponse.json(deleted, { status: 200 });
  } catch (err) {
    console.error("Prisma error:", err);
    return NextResponse.json(
      { error: "Failed to delete settings: " + err },
      { status: 500 }
    );
  }
}
