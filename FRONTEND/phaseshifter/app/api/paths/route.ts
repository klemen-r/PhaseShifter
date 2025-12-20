import { NextResponse } from "next/server";
import { prisma } from "@/lib/helpers";

export async function GET() {
  try {
    const items = await prisma.pathEntry.findMany({
      orderBy: { createdAt: "asc" },
      select: { id: true, path: true, createdAt: true },
    });
    return NextResponse.json(items);
  } catch (err) {
    console.error("Prisma error:", err); // Add this line
    return NextResponse.json(
      { error: "Failed to load paths" },
      { status: 500 },
    );
  }
}

export async function POST(req: Request) {
  try {
    const data = await req.json();
    let path = String(data?.path ?? "").trim();
    if (!path)
      return NextResponse.json({ error: "Path required" }, { status: 400 });
    // Normalize: remove leading slash but allow nested segments
    if (path.startsWith("/")) path = path.replace(/^\/+/, "");
    if (!/^[a-z0-9\-/]+$/i.test(path)) {
      return NextResponse.json(
        { error: "Invalid characters in path" },
        { status: 400 },
      );
    }
    const created = await prisma.pathEntry.create({ data: { path } });
    return NextResponse.json(created, { status: 201 });
  } catch (err) {
    return NextResponse.json(
      { error: "Failed to create path: " + err },
      { status: 500 },
    );
  }
}

export async function DELETE(req: Request) {
  try {
    const data = await req.json();
    const pathId = data.patId;
    const deleted = await prisma.pathEntry.delete({
      where: { id: pathId },
    });
    return NextResponse.json(deleted, { status: 201 });
  } catch (err) {
    return NextResponse.json(
      { error: "Failed to delete path" + err },
      { status: 500 },
    );
  }
}
