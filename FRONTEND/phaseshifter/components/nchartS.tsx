"use client";

import { useEffect, useState } from "react";

import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Label } from "@/components/ui/label";
import { Input } from "@/components/ui/input";
import { Separator } from "@/components/ui/separator";
import { Switch } from "@/components/ui/switch";

type ExtraSettingsDialogProps = {
  open: boolean;
  onOpenChange: (open: boolean) => void;

  lineColor: string;
  setLineColor: (val: string) => void;
  bgUpColor: string;
  setBgUpColor: (val: string) => void;
  bgDownColor: string;
  setBgDownColor: (val: string) => void;
  transparency: number;
  setTransparency: (val: number) => void;

  candleUpColor: string;
  candleDownColor: string;
  setCandleUpColor: (val: string) => void;
  setCandleDownColor: (val: string) => void;

  candleBorderUp: string;
  candleBorderDown: string;
  setCandleBorderUp: (val: string) => void;
  setCandleBorderDown: (val: string) => void;
};

export function ExtraSettingsDialog({
  open,
  onOpenChange,
  lineColor,
  setLineColor,
  bgUpColor,
  setBgUpColor,
  bgDownColor,
  setBgDownColor,
  transparency,
  setTransparency,
  candleUpColor,
  candleDownColor,
  setCandleUpColor,
  setCandleDownColor,
  candleBorderUp,
  candleBorderDown,
  setCandleBorderUp,
  setCandleBorderDown,
}: ExtraSettingsDialogProps) {
  const [liveTransparency, setLiveTransparency] = useState(transparency);

  useEffect(() => {
    setLiveTransparency(transparency);
  }, [transparency]);

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="border-zinc-800 bg-zinc-950/95 backdrop-blur-lg text-zinc-100">
        <DialogHeader>
          <DialogTitle>Extra settings</DialogTitle>
          <p className="text-sm text-zinc-400">
            Fine-tune the Donchian midpoint styling to mirror your Pine palette.
          </p>
        </DialogHeader>

        <div className="max-h-[70vh] space-y-4 overflow-y-auto pr-1">
          <div className="grid grid-cols-2 gap-4 text-sm">
            <div className="space-y-2">
              <Label className="text-zinc-200">Up background</Label>
              <Input
                type="color"
                value={bgUpColor}
                onChange={(e) => setBgUpColor(e.target.value)}
                className="h-10 cursor-pointer bg-zinc-900"
              />
              <p className="text-xs text-zinc-500">
                Used while the wave slopes up.
              </p>
            </div>
            <div className="space-y-2">
              <Label className="text-zinc-200">Down background</Label>
              <Input
                type="color"
                value={bgDownColor}
                onChange={(e) => setBgDownColor(e.target.value)}
                className="h-10 cursor-pointer bg-zinc-900"
              />
              <p className="text-xs text-zinc-500">
                Used while the wave slopes down.
              </p>
            </div>

            <div className="space-y-2">
              <Label className="text-zinc-200">Line color</Label>
              <Input
                type="color"
                value={lineColor}
                onChange={(e) => setLineColor(e.target.value)}
                className="h-10 cursor-pointer bg-zinc-900"
              />
              <p className="text-xs text-zinc-500">
                Stepped Donchian midpoint.
              </p>
            </div>

            <div className="space-y-2">
              <Label className="text-zinc-200 flex items-center justify-between">
                Transparency
                <span className="text-xs text-zinc-400">
                  {liveTransparency}%
                </span>
              </Label>
              <Input
                type="range"
                min={0}
                max={100}
                value={liveTransparency}
                onChange={(e) => {
                  const val = Number(e.target.value);
                  setLiveTransparency(val);
                  setTransparency(val);
                }}
              />
              <p className="text-xs text-zinc-500">
                0 = opaque, 100 = invisible.
              </p>
            </div>
          </div>

          <Separator className="bg-zinc-800" />

          <div className="grid grid-cols-2 gap-4 text-sm">
            <div className="space-y-2">
              <Label className="text-zinc-200">Candle up color</Label>
              <Input
                type="color"
                value={candleUpColor}
                onChange={(e) => setCandleUpColor(e.target.value)}
                className="h-10 cursor-pointer bg-zinc-900"
              />
            </div>
            <div className="space-y-2">
              <Label className="text-zinc-200">Candle down color</Label>
              <Input
                type="color"
                value={candleDownColor}
                onChange={(e) => setCandleDownColor(e.target.value)}
                className="h-10 cursor-pointer bg-zinc-900"
              />
            </div>
            <div className="space-y-2">
              <Label className="text-zinc-200">Candle border up color</Label>
              <Input
                type="color"
                value={candleBorderUp}
                onChange={(e) => setCandleBorderUp(e.target.value)}
                className="h-10 cursor-pointer bg-zinc-900"
              />
            </div>
            <div className="space-y-2">
              <Label className="text-zinc-200">Candle border down color</Label>
              <Input
                type="color"
                value={candleBorderDown}
                onChange={(e) => setCandleBorderDown(e.target.value)}
                className="h-10 cursor-pointer bg-zinc-900"
              />
            </div>
          </div>

          <Separator className="bg-zinc-800" />

          <div className="rounded-lg border border-zinc-800 bg-zinc-900/60 p-3 text-xs text-zinc-400">
            <div className="flex items-center justify-between">
              <span className="text-zinc-200">Stepped line</span>
              <Switch
                checked
                disabled
                className="opacity-60"
              />
            </div>
            <p className="mt-1">
              Matches the Pine script: Donchian midpoint with step style and
              persistent background tint based on slope.
            </p>
          </div>
        </div>
      </DialogContent>
    </Dialog>
  );
}
