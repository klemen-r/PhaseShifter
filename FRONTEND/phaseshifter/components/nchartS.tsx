"use client";

import { useEffect, useMemo, useState, useCallback } from "react";
import { Search, Save, Trash2, Check } from "lucide-react";

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
import { Button } from "@/components/ui/button";

export type ChartSettingsData = {
  phaseAmount: number;
  showMidpoint: boolean;
  clusterOpacity: number;
  clusterBullishColor: string;
  clusterBearishColor: string;
  clusterBorderColor: string;
  clusterBorderStyle: "solid" | "dashed" | "none";
  clusterAlerts: boolean;
  lineColor: string;
  bgUpColor: string;
  bgDownColor: string;
  transparency: number;
  candleUpColor: string;
  candleDownColor: string;
  candleBorderUp: string;
  candleBorderDown: string;
};

export type SavedPreset = {
  id: string;
  name: string;
  settings: string;
  updatedAt: string;
};

type ExtraSettingsDialogProps = {
  open: boolean;
  onOpenChange: (open: boolean) => void;

  // Chart behavior
  phaseAmount: number;
  setPhaseAmount: (val: number) => void;
  showMidpoint: boolean;
  setShowMidpoint: (val: boolean) => void;

  // Cluster settings
  clusterOpacity: number;
  setClusterOpacity: (val: number) => void;
  clusterBullishColor: string;
  setClusterBullishColor: (val: string) => void;
  clusterBearishColor: string;
  setClusterBearishColor: (val: string) => void;
  clusterBorderColor: string;
  setClusterBorderColor: (val: string) => void;
  clusterBorderStyle: "solid" | "dashed" | "none";
  setClusterBorderStyle: (val: "solid" | "dashed" | "none") => void;
  clusterAlerts: boolean;
  setClusterAlerts: (val: boolean) => void;

  // Midpoint colors
  lineColor: string;
  setLineColor: (val: string) => void;
  bgUpColor: string;
  setBgUpColor: (val: string) => void;
  bgDownColor: string;
  setBgDownColor: (val: string) => void;
  transparency: number;
  setTransparency: (val: number) => void;

  // Candle colors
  candleUpColor: string;
  candleDownColor: string;
  setCandleUpColor: (val: string) => void;
  setCandleDownColor: (val: string) => void;
  candleBorderUp: string;
  candleBorderDown: string;
  setCandleBorderUp: (val: string) => void;
  setCandleBorderDown: (val: string) => void;

  // Save/Load
  savedPresets: SavedPreset[];
  onRefreshPresets: () => void;
  onApplyPreset: (settings: ChartSettingsData) => void;
};

type SettingItem = {
  id: string;
  label: string;
  category: string;
  keywords: string[];
};

const settingItems: SettingItem[] = [
  {
    id: "phaseWindow",
    label: "Phase Window",
    category: "Chart",
    keywords: ["phase", "window", "donchian", "period"],
  },
  {
    id: "showMidpoint",
    label: "Show Midpoint Line",
    category: "Chart",
    keywords: ["midpoint", "line", "show", "hide", "donchian"],
  },
  {
    id: "clusterOpacity",
    label: "Cluster Opacity",
    category: "Clusters",
    keywords: ["cluster", "opacity", "transparent", "alpha"],
  },
  {
    id: "clusterBullish",
    label: "Cluster Bullish Color",
    category: "Clusters",
    keywords: ["cluster", "bullish", "green", "up", "color"],
  },
  {
    id: "clusterBearish",
    label: "Cluster Bearish Color",
    category: "Clusters",
    keywords: ["cluster", "bearish", "red", "down", "color"],
  },
  {
    id: "clusterBorderColor",
    label: "Cluster Border Color",
    category: "Clusters",
    keywords: ["cluster", "border", "color", "stroke"],
  },
  {
    id: "clusterBorderStyle",
    label: "Cluster Border Style",
    category: "Clusters",
    keywords: ["cluster", "border", "style", "solid", "dashed", "none"],
  },
  {
    id: "clusterAlerts",
    label: "Cluster Entry Alerts",
    category: "Clusters",
    keywords: ["cluster", "alert", "notification", "sound", "entry"],
  },
  {
    id: "lineColor",
    label: "Midpoint Line Color",
    category: "Midpoint",
    keywords: ["line", "color", "midpoint", "donchian"],
  },
  {
    id: "bgUpColor",
    label: "Up Background Color",
    category: "Midpoint",
    keywords: ["background", "up", "bullish", "green", "color"],
  },
  {
    id: "bgDownColor",
    label: "Down Background Color",
    category: "Midpoint",
    keywords: ["background", "down", "bearish", "red", "color"],
  },
  {
    id: "transparency",
    label: "Background Transparency",
    category: "Midpoint",
    keywords: ["transparency", "alpha", "opacity", "background"],
  },
  {
    id: "candleUp",
    label: "Candle Up Color",
    category: "Candles",
    keywords: ["candle", "up", "bullish", "green", "color"],
  },
  {
    id: "candleDown",
    label: "Candle Down Color",
    category: "Candles",
    keywords: ["candle", "down", "bearish", "red", "color"],
  },
  {
    id: "candleBorderUp",
    label: "Candle Border Up",
    category: "Candles",
    keywords: ["candle", "border", "up", "bullish", "color"],
  },
  {
    id: "candleBorderDown",
    label: "Candle Border Down",
    category: "Candles",
    keywords: ["candle", "border", "down", "bearish", "color"],
  },
];

export function ExtraSettingsDialog({
  open,
  onOpenChange,
  phaseAmount,
  setPhaseAmount,
  showMidpoint,
  setShowMidpoint,
  clusterOpacity,
  setClusterOpacity,
  clusterBullishColor,
  setClusterBullishColor,
  clusterBearishColor,
  setClusterBearishColor,
  clusterBorderColor,
  setClusterBorderColor,
  clusterBorderStyle,
  setClusterBorderStyle,
  clusterAlerts,
  setClusterAlerts,
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
  savedPresets,
  onRefreshPresets,
  onApplyPreset,
}: ExtraSettingsDialogProps) {
  const [liveTransparency, setLiveTransparency] = useState(transparency);
  const [liveClusterOpacity, setLiveClusterOpacity] = useState(clusterOpacity);
  const [searchQuery, setSearchQuery] = useState("");
  const [presetName, setPresetName] = useState("");
  const [saving, setSaving] = useState(false);
  const [saveSuccess, setSaveSuccess] = useState(false);

  useEffect(() => {
    setLiveTransparency(transparency);
  }, [transparency]);

  useEffect(() => {
    setLiveClusterOpacity(clusterOpacity);
  }, [clusterOpacity]);

  // Reset search when dialog closes
  useEffect(() => {
    if (!open) {
      setSearchQuery("");
      setSaveSuccess(false);
    }
  }, [open]);

  // Gather current settings into an object
  const getCurrentSettings = useCallback(
    (): ChartSettingsData => ({
      phaseAmount,
      showMidpoint,
      clusterOpacity,
      clusterBullishColor,
      clusterBearishColor,
      clusterBorderColor,
      clusterBorderStyle,
      clusterAlerts,
      lineColor,
      bgUpColor,
      bgDownColor,
      transparency,
      candleUpColor,
      candleDownColor,
      candleBorderUp,
      candleBorderDown,
    }),
    [
      phaseAmount,
      showMidpoint,
      clusterOpacity,
      clusterBullishColor,
      clusterBearishColor,
      clusterBorderColor,
      clusterBorderStyle,
      clusterAlerts,
      lineColor,
      bgUpColor,
      bgDownColor,
      transparency,
      candleUpColor,
      candleDownColor,
      candleBorderUp,
      candleBorderDown,
    ],
  );

  // Save current settings as a preset
  const handleSavePreset = useCallback(async () => {
    if (!presetName.trim()) return;
    setSaving(true);
    setSaveSuccess(false);
    try {
      const res = await fetch("/api/chart-settings", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          name: presetName.trim(),
          settings: getCurrentSettings(),
        }),
      });
      if (res.ok) {
        setSaveSuccess(true);
        onRefreshPresets();
        setTimeout(() => setSaveSuccess(false), 2000);
      }
    } catch (err) {
      console.error("Failed to save preset:", err);
    } finally {
      setSaving(false);
    }
  }, [presetName, getCurrentSettings, onRefreshPresets]);

  // Delete a preset
  const handleDeletePreset = useCallback(
    async (id: string) => {
      try {
        const res = await fetch("/api/chart-settings", {
          method: "DELETE",
          headers: { "Content-Type": "application/json" },
          body: JSON.stringify({ id }),
        });
        if (res.ok) {
          onRefreshPresets();
        }
      } catch (err) {
        console.error("Failed to delete preset:", err);
      }
    },
    [onRefreshPresets],
  );

  // Load a preset
  const handleLoadPreset = useCallback(
    (preset: SavedPreset) => {
      try {
        const settings = JSON.parse(preset.settings) as ChartSettingsData;
        onApplyPreset(settings);
      } catch (err) {
        console.error("Failed to parse preset:", err);
      }
    },
    [onApplyPreset],
  );

  // Filter settings based on search query
  const filteredSettings = useMemo(() => {
    if (!searchQuery.trim()) return null; // null means show all
    const query = searchQuery.toLowerCase();
    return settingItems.filter(
      (item) =>
        item.label.toLowerCase().includes(query) ||
        item.category.toLowerCase().includes(query) ||
        item.keywords.some((kw) => kw.includes(query)),
    );
  }, [searchQuery]);

  const isVisible = (id: string) => {
    if (!filteredSettings) return true;
    return filteredSettings.some((item) => item.id === id);
  };

  const isCategoryVisible = (category: string) => {
    if (!filteredSettings) return true;
    return filteredSettings.some((item) => item.category === category);
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="border-zinc-800 bg-zinc-950/95 backdrop-blur-lg text-zinc-100 max-w-lg">
        <DialogHeader>
          <DialogTitle>Chart Settings</DialogTitle>
        </DialogHeader>

        {/* Presets Section */}
        <div className="space-y-3 border border-zinc-800 rounded-lg p-3 bg-zinc-900/40">
          <div className="text-xs font-medium text-zinc-500 uppercase tracking-wider">
            Presets
          </div>
          <div className="flex gap-2">
            <Input
              placeholder="Preset name..."
              value={presetName}
              onChange={(e) => setPresetName(e.target.value)}
              className="bg-zinc-900 border-zinc-800 flex-1"
              onKeyDown={(e) => e.key === "Enter" && handleSavePreset()}
            />
            <Button
              size="sm"
              onClick={handleSavePreset}
              disabled={saving || !presetName.trim()}
              className="bg-zinc-800 hover:bg-zinc-700 text-zinc-200"
            >
              {saveSuccess ? (
                <Check className="h-4 w-4 text-green-500" />
              ) : (
                <Save className="h-4 w-4" />
              )}
            </Button>
          </div>
          {savedPresets.length > 0 && (
            <div className="flex flex-wrap gap-2 pt-1">
              {savedPresets.map((preset) => (
                <div
                  key={preset.id}
                  className="group flex items-center gap-1 bg-zinc-800 rounded px-2 py-1 text-xs text-zinc-300"
                >
                  <button
                    onClick={() => handleLoadPreset(preset)}
                    className="hover:text-zinc-100"
                  >
                    {preset.name}
                  </button>
                  <button
                    onClick={() => handleDeletePreset(preset.id)}
                    className="opacity-0 group-hover:opacity-100 text-zinc-500 hover:text-red-400 transition-opacity"
                  >
                    <Trash2 className="h-3 w-3" />
                  </button>
                </div>
              ))}
            </div>
          )}
        </div>

        {/* Search Bar */}
        <div className="relative">
          <Search className="absolute left-3 top-1/2 h-4 w-4 -translate-y-1/2 text-zinc-500" />
          <Input
            placeholder="Search settings..."
            value={searchQuery}
            onChange={(e) => setSearchQuery(e.target.value)}
            className="pl-9 bg-zinc-900 border-zinc-800"
          />
        </div>

        <div className="max-h-[50vh] space-y-4 overflow-y-auto pr-1">
          {/* Chart Settings */}
          {isCategoryVisible("Chart") && (
            <>
              <div className="text-xs font-medium text-zinc-500 uppercase tracking-wider">
                Chart
              </div>
              {isVisible("phaseWindow") && (
                <div className="space-y-2">
                  <Label className="text-zinc-200">Phase Window</Label>
                  <Input
                    type="number"
                    min={1}
                    value={phaseAmount}
                    onChange={(e) =>
                      setPhaseAmount(Number(e.target.value) || 1)
                    }
                    className="bg-zinc-900"
                  />
                  <p className="text-xs text-zinc-500">
                    Rolling window for Donchian midpoint calculation.
                  </p>
                </div>
              )}
              {isVisible("showMidpoint") && (
                <div className="flex items-center justify-between rounded-lg border border-zinc-800 bg-zinc-900/60 p-3">
                  <div>
                    <span className="text-sm text-zinc-200">
                      Show Midpoint Line
                    </span>
                    <p className="text-xs text-zinc-500">
                      Display the Donchian midpoint.
                    </p>
                  </div>
                  <Switch
                    checked={showMidpoint}
                    onCheckedChange={setShowMidpoint}
                  />
                </div>
              )}
              {isCategoryVisible("Chart") && (
                <Separator className="bg-zinc-800" />
              )}
            </>
          )}

          {/* Cluster Settings */}
          {isCategoryVisible("Clusters") && (
            <>
              <div className="text-xs font-medium text-zinc-500 uppercase tracking-wider">
                Clusters
              </div>
              {isVisible("clusterOpacity") && (
                <div className="space-y-2">
                  <Label className="text-zinc-200 flex items-center justify-between">
                    Cluster Opacity
                    <span className="text-xs text-zinc-400">
                      {liveClusterOpacity}%
                    </span>
                  </Label>
                  <Input
                    type="range"
                    min={10}
                    max={80}
                    value={liveClusterOpacity}
                    onChange={(e) => {
                      const val = Number(e.target.value);
                      setLiveClusterOpacity(val);
                      setClusterOpacity(val);
                    }}
                  />
                </div>
              )}
              <div className="grid grid-cols-2 gap-4 text-sm">
                {isVisible("clusterBullish") && (
                  <div className="space-y-2">
                    <Label className="text-zinc-200">Bullish Cluster</Label>
                    <Input
                      type="color"
                      value={clusterBullishColor}
                      onChange={(e) => setClusterBullishColor(e.target.value)}
                      className="h-10 cursor-pointer bg-zinc-900"
                    />
                  </div>
                )}
                {isVisible("clusterBearish") && (
                  <div className="space-y-2">
                    <Label className="text-zinc-200">Bearish Cluster</Label>
                    <Input
                      type="color"
                      value={clusterBearishColor}
                      onChange={(e) => setClusterBearishColor(e.target.value)}
                      className="h-10 cursor-pointer bg-zinc-900"
                    />
                  </div>
                )}
                {isVisible("clusterBorderColor") && (
                  <div className="space-y-2">
                    <Label className="text-zinc-200">Border Color</Label>
                    <Input
                      type="color"
                      value={clusterBorderColor}
                      onChange={(e) => setClusterBorderColor(e.target.value)}
                      className="h-10 cursor-pointer bg-zinc-900"
                    />
                  </div>
                )}
                {isVisible("clusterBorderStyle") && (
                  <div className="space-y-2">
                    <Label className="text-zinc-200">Border Style</Label>
                    <select
                      value={clusterBorderStyle}
                      onChange={(e) =>
                        setClusterBorderStyle(
                          e.target.value as "solid" | "dashed" | "none",
                        )
                      }
                      className="w-full h-10 px-3 rounded-md bg-zinc-900 border border-zinc-800 text-zinc-200 text-sm"
                    >
                      <option value="dashed">Dashed</option>
                      <option value="solid">Solid</option>
                      <option value="none">None</option>
                    </select>
                  </div>
                )}
              </div>
              {isVisible("clusterAlerts") && (
                <div className="flex items-center justify-between">
                  <div>
                    <Label className="text-zinc-200">Cluster Zone Alerts</Label>
                    <p className="text-xs text-zinc-500">
                      Notify when price enters a cluster zone
                    </p>
                  </div>
                  <Switch
                    checked={clusterAlerts}
                    onCheckedChange={setClusterAlerts}
                  />
                </div>
              )}
              {isCategoryVisible("Clusters") && (
                <Separator className="bg-zinc-800" />
              )}
            </>
          )}

          {/* Midpoint Colors */}
          {isCategoryVisible("Midpoint") && (
            <>
              <div className="text-xs font-medium text-zinc-500 uppercase tracking-wider">
                Midpoint Colors
              </div>
              <div className="grid grid-cols-2 gap-4 text-sm">
                {isVisible("bgUpColor") && (
                  <div className="space-y-2">
                    <Label className="text-zinc-200">Up Background</Label>
                    <Input
                      type="color"
                      value={bgUpColor}
                      onChange={(e) => setBgUpColor(e.target.value)}
                      className="h-10 cursor-pointer bg-zinc-900"
                    />
                  </div>
                )}
                {isVisible("bgDownColor") && (
                  <div className="space-y-2">
                    <Label className="text-zinc-200">Down Background</Label>
                    <Input
                      type="color"
                      value={bgDownColor}
                      onChange={(e) => setBgDownColor(e.target.value)}
                      className="h-10 cursor-pointer bg-zinc-900"
                    />
                  </div>
                )}
                {isVisible("lineColor") && (
                  <div className="space-y-2">
                    <Label className="text-zinc-200">Line Color</Label>
                    <Input
                      type="color"
                      value={lineColor}
                      onChange={(e) => setLineColor(e.target.value)}
                      className="h-10 cursor-pointer bg-zinc-900"
                    />
                  </div>
                )}
                {isVisible("transparency") && (
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
                  </div>
                )}
              </div>
              {isCategoryVisible("Midpoint") && (
                <Separator className="bg-zinc-800" />
              )}
            </>
          )}

          {/* Candle Colors */}
          {isCategoryVisible("Candles") && (
            <>
              <div className="text-xs font-medium text-zinc-500 uppercase tracking-wider">
                Candle Colors
              </div>
              <div className="grid grid-cols-2 gap-4 text-sm">
                {isVisible("candleUp") && (
                  <div className="space-y-2">
                    <Label className="text-zinc-200">Candle Up</Label>
                    <Input
                      type="color"
                      value={candleUpColor}
                      onChange={(e) => setCandleUpColor(e.target.value)}
                      className="h-10 cursor-pointer bg-zinc-900"
                    />
                  </div>
                )}
                {isVisible("candleDown") && (
                  <div className="space-y-2">
                    <Label className="text-zinc-200">Candle Down</Label>
                    <Input
                      type="color"
                      value={candleDownColor}
                      onChange={(e) => setCandleDownColor(e.target.value)}
                      className="h-10 cursor-pointer bg-zinc-900"
                    />
                  </div>
                )}
                {isVisible("candleBorderUp") && (
                  <div className="space-y-2">
                    <Label className="text-zinc-200">Border Up</Label>
                    <Input
                      type="color"
                      value={candleBorderUp}
                      onChange={(e) => setCandleBorderUp(e.target.value)}
                      className="h-10 cursor-pointer bg-zinc-900"
                    />
                  </div>
                )}
                {isVisible("candleBorderDown") && (
                  <div className="space-y-2">
                    <Label className="text-zinc-200">Border Down</Label>
                    <Input
                      type="color"
                      value={candleBorderDown}
                      onChange={(e) => setCandleBorderDown(e.target.value)}
                      className="h-10 cursor-pointer bg-zinc-900"
                    />
                  </div>
                )}
              </div>
            </>
          )}

          {/* No results message */}
          {filteredSettings && filteredSettings.length === 0 && (
            <div className="py-8 text-center text-sm text-zinc-500">
              No settings match &quot;{searchQuery}&quot;
            </div>
          )}
        </div>
      </DialogContent>
    </Dialog>
  );
}
