"use client";

import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { ScrollArea } from "@/components/ui/scroll-area";
import { Badge } from "@/components/ui/badge";
import { Separator } from "@/components/ui/separator";
import type { ClustersData, ClusterItem } from "@/lib/websocket";

interface ClustersDisplayProps {
  ticker: string;
  data: ClustersData | null;
}

export function ClustersDisplay({ ticker, data }: ClustersDisplayProps) {
  const formatPrice = (price: number) => {
    return price.toLocaleString(undefined, {
      minimumFractionDigits: 2,
      maximumFractionDigits: 2,
    });
  };

  if (!data) {
    return (
      <Card className="border-zinc-800 bg-zinc-950/40 backdrop-blur max-h-[calc(100vh-777px)]">
        <CardHeader className="pb-2">
          <CardTitle className="text-base">
            Cluster Projections
            <Badge variant="outline" className="ml-2 font-mono">
              {ticker}
            </Badge>
          </CardTitle>
        </CardHeader>
        <CardContent>
          <div className="py-8 text-center text-zinc-500">
            No cluster data. Click refresh to request.
          </div>
        </CardContent>
      </Card>
    );
  }

  const bullishClusters = data.clusters.filter((c) => c.side === "bullish");
  const bearishClusters = data.clusters.filter((c) => c.side === "bearish");

  return (
    <Card className="border-zinc-800 bg-zinc-950/40 backdrop-blur max-h-[calc(100vh-712px)]">
      <CardHeader className="pb-2">
        <div className="flex items-center justify-between">
          <CardTitle className="text-base">
            Cluster Projections
            <Badge variant="outline" className="ml-2 font-mono">
              {ticker}
            </Badge>
          </CardTitle>
          {data.anchor && (
            <span className="text-xs text-zinc-400">
              Anchor: {formatPrice(data.anchor)}
            </span>
          )}
        </div>
        <div className="text-xs text-zinc-500">
          Generated: {new Date(data.generated_at).toLocaleString()}
        </div>
      </CardHeader>
      <CardContent className="overflow-hidden">
        <ScrollArea className="h-[400px]">
          <div className="space-y-4">
            {bullishClusters.length > 0 && (
              <div>
                <div className="text-sm font-medium text-emerald-400 mb-2">
                  Bullish Targets ({bullishClusters.length})
                </div>
                <div className="space-y-2">
                  {bullishClusters.map((cluster, idx) => (
                    <ClusterCard
                      key={`bull-${idx}`}
                      cluster={cluster}
                      anchor={data.anchor}
                    />
                  ))}
                </div>
              </div>
            )}

            {bullishClusters.length > 0 && bearishClusters.length > 0 && (
              <Separator className="bg-zinc-800" />
            )}

            {bearishClusters.length > 0 && (
              <div>
                <div className="text-sm font-medium text-red-400 mb-2">
                  Bearish Targets ({bearishClusters.length})
                </div>
                <div className="space-y-2">
                  {bearishClusters.map((cluster, idx) => (
                    <ClusterCard
                      key={`bear-${idx}`}
                      cluster={cluster}
                      anchor={data.anchor}
                    />
                  ))}
                </div>
              </div>
            )}

            {data.clusters.length === 0 && (
              <div className="py-4 text-center text-zinc-500">
                No clusters found
              </div>
            )}

            {data.nodes.length > 0 && (
              <>
                <Separator className="bg-zinc-800" />
                <div>
                  <div className="text-sm font-medium text-zinc-300 mb-2">
                    All Nodes ({data.nodes.length})
                  </div>
                  <div className="grid grid-cols-2 gap-2 text-xs">
                    {data.nodes.slice(0, 20).map((node, idx) => (
                      <div
                        key={idx}
                        className={`rounded border p-2 ${
                          node.side === "bullish"
                            ? "border-emerald-800/50 bg-emerald-950/20"
                            : "border-red-800/50 bg-red-950/20"
                        }`}
                      >
                        <div className="font-mono font-medium">
                          {formatPrice(node.value)}
                        </div>
                        <div className="text-zinc-500">
                          {node.interval} / pw{node.phase_window}
                        </div>
                      </div>
                    ))}
                  </div>
                  {data.nodes.length > 20 && (
                    <div className="text-xs text-zinc-500 mt-2 text-center">
                      +{data.nodes.length - 20} more nodes
                    </div>
                  )}
                </div>
              </>
            )}
          </div>
        </ScrollArea>
      </CardContent>
    </Card>
  );
}

function ClusterCard({
  cluster,
  anchor,
}: {
  cluster: ClusterItem;
  anchor: number | null;
}) {
  const formatPrice = (price: number) =>
    price.toLocaleString(undefined, {
      minimumFractionDigits: 2,
      maximumFractionDigits: 2,
    });

  const range = cluster.high - cluster.low;
  const midpoint = (cluster.low + cluster.high) / 2;
  const pctFromAnchor = anchor
    ? (((midpoint - anchor) / anchor) * 100).toFixed(2)
    : null;

  const isBullish = cluster.side === "bullish";

  return (
    <div
      className={`rounded-lg border p-3 ${
        isBullish
          ? "border-emerald-800/50 bg-emerald-950/30"
          : "border-red-800/50 bg-red-950/30"
      }`}
    >
      <div className="flex items-center justify-between mb-2">
        <div className="flex items-center gap-2">
          <Badge
            variant="outline"
            className={
              isBullish
                ? "text-emerald-400 border-emerald-400/30"
                : "text-red-400 border-red-400/30"
            }
          >
            {isBullish ? "BULL" : "BEAR"}
          </Badge>
          {pctFromAnchor && (
            <span className="text-xs text-zinc-400">
              {isBullish ? "+" : ""}
              {pctFromAnchor}%
            </span>
          )}
        </div>
        <span className="text-xs text-zinc-400">
          {cluster.count} nodes / {cluster.unique_scenarios} scenarios
        </span>
      </div>
      <div className="grid grid-cols-3 gap-2 text-sm font-mono">
        <div>
          <span className="text-zinc-500 text-xs">Low</span>
          <div className="text-zinc-200">{formatPrice(cluster.low)}</div>
        </div>
        <div className="text-center">
          <span className="text-zinc-500 text-xs">Range</span>
          <div className="text-zinc-200">{formatPrice(range)}</div>
        </div>
        <div className="text-right">
          <span className="text-zinc-500 text-xs">High</span>
          <div className="text-zinc-200">{formatPrice(cluster.high)}</div>
        </div>
      </div>
    </div>
  );
}
