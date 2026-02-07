"use client";

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { toast } from "sonner";

import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Dialog, DialogContent, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { ScrollArea } from "@/components/ui/scroll-area";
import { Separator } from "@/components/ui/separator";

type Account = {
  id: string;
  name: string;
  balance: number;
  leverage: number;
  createdAt: string;
  updatedAt: string;
};

type Trade = {
  id: string;
  accountId: string;
  symbol: string;
  side: "LONG" | "SHORT";
  qty: number;
  leverage: number;
  entryPrice: number;
  exitPrice: number | null;
  openedAt: string;
  closedAt: string | null;
};

type CloseTradeResponse = {
  kind: "ok";
  trade: Trade;
  account: Account;
  realizedPnl: number;
};

type AccountPanelProps = {
  ticker: string;
  currentPrice: number | null;
  getCurrentPrice: (ticker: string) => number | null;
  subscribeTicker: (ticker: string) => void;
  subscribedTickers: Set<string>;
};

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

function getSizingLabel(symbol: string) {
  const upper = normalizeSymbol(symbol);
  if (Object.keys(FUTURES_MULTIPLIERS).some((key) => upper.startsWith(key))) {
    return "Contracts";
  }
  if (upper.includes("BTC")) return "Lots";
  return "Units";
}

function floorToDecimals(value: number, decimals: number) {
  const factor = 10 ** decimals;
  return Math.floor(value * factor) / factor;
}

function formatMoney(value: number) {
  return value.toLocaleString(undefined, {
    minimumFractionDigits: 2,
    maximumFractionDigits: 2,
  });
}

function computeMaxQty({
  symbol,
  balance,
  leverage,
  price,
}: {
  symbol: string;
  balance: number;
  leverage: number;
  price: number;
}) {
  const multiplier = getContractMultiplier(symbol);
  const notional = balance * leverage;
  const rawQty = notional / (price * multiplier);
  const isFutures = Object.keys(FUTURES_MULTIPLIERS).some((key) =>
    normalizeSymbol(symbol).startsWith(key),
  );
  const qty = isFutures ? Math.floor(rawQty) : floorToDecimals(rawQty, 3);
  return { qty, notional, multiplier };
}

function computeTradePnl(
  trade: Trade,
  currentPrice: number | null,
): number | null {
  const price = trade.closedAt ? trade.exitPrice : currentPrice;
  if (!price) return null;
  const multiplier = getContractMultiplier(trade.symbol);
  const direction = trade.side === "LONG" ? 1 : -1;
  return (price - trade.entryPrice) * direction * trade.qty * multiplier;
}

export function AccountPanel({
  ticker,
  currentPrice,
  getCurrentPrice,
  subscribeTicker,
  subscribedTickers,
}: AccountPanelProps) {
  const [accounts, setAccounts] = useState<Account[]>([]);
  const [activeAccountId, setActiveAccountId] = useState<string | null>(null);
  const [trades, setTrades] = useState<Trade[]>([]);
  const [detailsOpen, setDetailsOpen] = useState(false);
  const [createOpen, setCreateOpen] = useState(false);
  const [tradeSettingsOpen, setTradeSettingsOpen] = useState(false);

  const [draftLeverage, setDraftLeverage] = useState("");
  const [orderQty, setOrderQty] = useState("");
  const [newName, setNewName] = useState("");
  const [newBalance, setNewBalance] = useState("10000");
  const [newLeverage, setNewLeverage] = useState("1");

  const activeAccount = useMemo(
    () => accounts.find((a) => a.id === activeAccountId) ?? null,
    [accounts, activeAccountId],
  );

  const loadAccounts = useCallback(async () => {
    try {
      const res = await fetch("/api/accounts");
      if (!res.ok) return;
      const data = (await res.json()) as Account[];
      setAccounts(data);
      if (!activeAccountId && data.length > 0) {
        setActiveAccountId(data[0].id);
      }
    } catch (err) {
      console.error("Failed to load accounts:", err);
    }
  }, [activeAccountId]);

  const loadTrades = useCallback(async (accountId: string) => {
    try {
      const res = await fetch(`/api/trades?accountId=${accountId}`);
      if (!res.ok) return;
      const data = (await res.json()) as Trade[];
      setTrades(data);
    } catch (err) {
      console.error("Failed to load trades:", err);
    }
  }, []);

  useEffect(() => {
    loadAccounts();
  }, [loadAccounts]);

  useEffect(() => {
    if (!activeAccountId) {
      setTrades([]);
      return;
    }
    loadTrades(activeAccountId);
  }, [activeAccountId, loadTrades]);

  useEffect(() => {
    if (!activeAccount) return;
    setDraftLeverage(String(activeAccount.leverage));
  }, [activeAccount]);

  useEffect(() => {
    const openTrades = trades.filter((t) => !t.closedAt);
    for (const trade of openTrades) {
      const symbol = normalizeSymbol(trade.symbol);
      if (!subscribedTickers.has(symbol)) {
        subscribeTicker(symbol);
      }
    }
  }, [trades, subscribedTickers, subscribeTicker]);

  const openTrades = useMemo(
    () => trades.filter((trade) => !trade.closedAt),
    [trades],
  );

  const realizedPnl = useMemo(() => {
    return trades.reduce((sum, trade) => {
      if (!trade.closedAt) return sum;
      const pnl = computeTradePnl(trade, null);
      return pnl !== null ? sum + pnl : sum;
    }, 0);
  }, [trades]);

  const unrealizedPnl = useMemo(() => {
    return openTrades.reduce((sum, trade) => {
      const price = getCurrentPrice(trade.symbol);
      const pnl = computeTradePnl(trade, price);
      return pnl !== null ? sum + pnl : sum;
    }, 0);
  }, [openTrades, getCurrentPrice]);

  const totalPnl = realizedPnl + unrealizedPnl;

  const equity = activeAccount ? activeAccount.balance + unrealizedPnl : 0;

  const handleUpdateAccount = useCallback(async () => {
    if (!activeAccount) return;
    const leverage = Number(draftLeverage);
    if (!Number.isFinite(leverage) || leverage <= 0) {
      toast.error("Leverage must be greater than 0");
      return;
    }
    try {
      const res = await fetch(`/api/accounts/${activeAccount.id}`, {
        method: "PATCH",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ leverage }),
      });
      if (!res.ok) throw new Error("Failed to update account");
      const updated = (await res.json()) as Account;
      setAccounts((prev) =>
        prev.map((acc) => (acc.id === updated.id ? updated : acc)),
      );
      toast.success("Account updated");
    } catch (err) {
      console.error(err);
      toast.error("Failed to update account");
    }
  }, [activeAccount, draftLeverage]);

  const handleCreateAccount = useCallback(async () => {
    const name = newName.trim();
    const balance = Number(newBalance);
    const leverage = Number(newLeverage);
    if (!name) {
      toast.error("Account name is required");
      return;
    }
    if (!Number.isFinite(balance) || balance < 0) {
      toast.error("Balance must be valid");
      return;
    }
    if (!Number.isFinite(leverage) || leverage <= 0) {
      toast.error("Leverage must be greater than 0");
      return;
    }
    try {
      const res = await fetch("/api/accounts", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ name, balance, leverage }),
      });
      if (!res.ok) throw new Error("Failed to create account");
      const created = (await res.json()) as Account;
      setAccounts((prev) => [...prev, created]);
      setActiveAccountId(created.id);
      setCreateOpen(false);
      setNewName("");
      toast.success("Account created");
    } catch (err) {
      console.error(err);
      toast.error("Failed to create account");
    }
  }, [newName, newBalance, newLeverage]);

  const handleCreateTrade = useCallback(
    async (side: "LONG" | "SHORT") => {
      if (!activeAccount) {
        toast.error("Create an account first");
        return;
      }
      if (!currentPrice || currentPrice <= 0) {
        toast.error("Current price not available");
        return;
      }
      const max = computeMaxQty({
        symbol: ticker,
        balance: activeAccount.balance,
        leverage: activeAccount.leverage,
        price: currentPrice,
      });
      if (!Number.isFinite(max.qty) || max.qty <= 0) {
        toast.error("Account size too small for a position");
        return;
      }
      const rawQty = Number(orderQty || max.qty);
      const isFutures = Object.keys(FUTURES_MULTIPLIERS).some((key) =>
        normalizeSymbol(ticker).startsWith(key),
      );
      const qty = isFutures ? Math.floor(rawQty) : rawQty;
      if (!Number.isFinite(qty) || qty <= 0) {
        toast.error("Trade size must be greater than 0");
        return;
      }
      if (qty > max.qty) {
        toast.error("Trade size exceeds max position");
        return;
      }
      try {
        const res = await fetch("/api/trades", {
          method: "POST",
          headers: { "Content-Type": "application/json" },
          body: JSON.stringify({
            accountId: activeAccount.id,
            symbol: ticker,
            side,
            qty,
            entryPrice: currentPrice,
            leverage: activeAccount.leverage,
          }),
        });
        if (!res.ok) throw new Error("Failed to create trade");
        const created = (await res.json()) as Trade;
        setTrades((prev) => [created, ...prev]);
        toast.success(`${side === "LONG" ? "Buy" : "Sell"} executed`);
      } catch (err) {
        console.error(err);
        toast.error("Failed to create trade");
      }
    },
    [activeAccount, currentPrice, orderQty, ticker],
  );

  const handleCloseTrade = useCallback(
    async (trade: Trade) => {
      if (trade.closedAt) return;

      const exitPrice = getCurrentPrice(trade.symbol);
      if (!exitPrice || exitPrice <= 0) {
        toast.error(`Live price unavailable for ${trade.symbol}`);
        return;
      }

      try {
        const res = await fetch("/api/trades", {
          method: "PATCH",
          headers: { "Content-Type": "application/json" },
          body: JSON.stringify({
            id: trade.id,
            exitPrice,
          }),
        });

        const payload = (await res.json()) as
          | CloseTradeResponse
          | { error?: string };

        if (!res.ok || payload == null || !("kind" in payload)) {
          const message =
            (payload && "error" in payload && payload.error) ||
            "Failed to close trade";
          throw new Error(message);
        }

        setTrades((prev) =>
          prev.map((t) => (t.id === payload.trade.id ? payload.trade : t)),
        );
        setAccounts((prev) =>
          prev.map((acc) => (acc.id === payload.account.id ? payload.account : acc)),
        );

        toast.success(
          `Trade closed (${payload.realizedPnl >= 0 ? "+" : ""}${formatMoney(payload.realizedPnl)})`,
        );
      } catch (err) {
        console.error(err);
        toast.error(err instanceof Error ? err.message : "Failed to close trade");
      }
    },
    [getCurrentPrice],
  );

  const maxSizing = useMemo(() => {
    if (!activeAccount || !currentPrice) return null;
    return computeMaxQty({
      symbol: ticker,
      balance: activeAccount.balance,
      leverage: activeAccount.leverage,
      price: currentPrice,
    });
  }, [activeAccount, currentPrice, ticker]);

  const lastAccountRef = useRef<string | null>(null);
  useEffect(() => {
    if (!activeAccount || !maxSizing) return;
    if (lastAccountRef.current !== activeAccount.id && !orderQty) {
      setOrderQty(String(maxSizing.qty));
      lastAccountRef.current = activeAccount.id;
    }
  }, [activeAccount, maxSizing, orderQty]);

  const sizingLabel = getSizingLabel(ticker);
  const tradeRows = trades.map((trade) => {
    const price = getCurrentPrice(trade.symbol);
    const pnl = computeTradePnl(trade, price);
    return { trade, pnl };
  });
  const openTradeRows = openTrades.map((trade) => {
    const price = getCurrentPrice(trade.symbol);
    const pnl = computeTradePnl(trade, price);
    return { trade, pnl, price };
  });

  return (
    <Card className="border-zinc-800 bg-zinc-950/40 backdrop-blur">
      <CardHeader className="pb-2">
        <div className="flex items-center justify-between">
          <CardTitle className="text-base">
            Account Settings
            <Badge variant="outline" className="ml-2 font-mono">
              {ticker}
            </Badge>
          </CardTitle>
          <Button
            variant="outline"
            size="sm"
            onClick={() => setDetailsOpen(true)}
          >
            More Details
          </Button>
        </div>
      </CardHeader>
      <CardContent className="space-y-4">
        {!activeAccount && (
          <div className="text-center text-sm text-zinc-500">
            No accounts yet.
            <div className="mt-3">
              <Button onClick={() => setCreateOpen(true)}>
                Create Account
              </Button>
            </div>
          </div>
        )}

        {activeAccount && (
          <>
            <div className="flex items-center justify-between gap-3">
              <div>
                <div className="text-xs text-zinc-500">Active Account</div>
                <div className="text-sm font-medium">{activeAccount.name}</div>
              </div>
              <div className="grid grid-cols-3 gap-4 text-right">
                <div>
                  <div className="text-[10px] text-zinc-500">Realized</div>
                  <div
                    className={`text-xs font-mono ${
                      realizedPnl >= 0 ? "text-emerald-400" : "text-red-400"
                    }`}
                  >
                    {realizedPnl >= 0 ? "+" : ""}
                    {formatMoney(realizedPnl)}
                  </div>
                </div>
                <div>
                  <div className="text-[10px] text-zinc-500">Unrealized</div>
                  <div
                    className={`text-xs font-mono ${
                      unrealizedPnl >= 0 ? "text-emerald-400" : "text-red-400"
                    }`}
                  >
                    {unrealizedPnl >= 0 ? "+" : ""}
                    {formatMoney(unrealizedPnl)}
                  </div>
                </div>
                <div>
                  <div className="text-[10px] text-zinc-500">Total</div>
                  <div
                    className={`text-xs font-mono ${
                      totalPnl >= 0 ? "text-emerald-400" : "text-red-400"
                    }`}
                  >
                    {totalPnl >= 0 ? "+" : ""}
                    {formatMoney(totalPnl)}
                  </div>
                </div>
              </div>
            </div>

            <Separator className="bg-zinc-800" />

            <div>
              <div className="text-sm font-medium">Open Trades</div>
              <ScrollArea className="mt-2 h-[140px] rounded-md border border-zinc-800">
                {openTradeRows.length === 0 && (
                  <div className="p-4 text-center text-sm text-zinc-500">
                    No open trades.
                  </div>
                )}
                {openTradeRows.length > 0 && (
                  <div className="divide-y divide-zinc-800 text-sm">
                    <div className="grid grid-cols-5 gap-2 px-3 py-2 text-xs text-zinc-500">
                      <div>Side</div>
                      <div>Ticker</div>
                      <div>Starting Price</div>
                      <div>P&L</div>
                      <div className="text-right">Action</div>
                    </div>
                    {openTradeRows.map(({ trade, pnl, price }) => (
                      <div
                        key={trade.id}
                        className="grid grid-cols-5 gap-2 px-3 py-2"
                      >
                        <div className="flex items-center gap-2">
                          <Badge
                            variant="outline"
                            className={
                              trade.side === "LONG"
                                ? "text-emerald-400 border-emerald-400/30"
                                : "text-red-400 border-red-400/30"
                            }
                          >
                            {trade.side}
                          </Badge>
                        </div>
                        <div className="font-mono text-xs">
                          {trade.symbol}
                        </div>
                        <div className="font-mono text-xs">
                          {formatMoney(trade.entryPrice)}
                        </div>
                        <div
                          className={`text-xs font-mono ${
                            (pnl ?? 0) >= 0
                              ? "text-emerald-400"
                              : "text-red-400"
                          }`}
                        >
                          {pnl === null ? "--" : formatMoney(pnl)}
                        </div>
                        <div className="text-right">
                          <Button
                            size="sm"
                            variant="outline"
                            className="h-7 px-2 text-[11px]"
                            onClick={() => handleCloseTrade(trade)}
                            disabled={!price || price <= 0}
                          >
                            Close
                          </Button>
                        </div>
                      </div>
                    ))}
                  </div>
                )}
              </ScrollArea>
            </div>

            <div className="flex items-center gap-2">
              <Button
                variant="outline"
                className="w-full"
                onClick={() => setTradeSettingsOpen(true)}
              >
                Trade Settings
              </Button>
            </div>
            <div className="flex items-center gap-2">
              <Button
                className="flex-1"
                onClick={() => handleCreateTrade("LONG")}
                disabled={!currentPrice}
              >
                Buy (Long)
              </Button>
              <Button
                className="flex-1"
                variant="outline"
                onClick={() => handleCreateTrade("SHORT")}
                disabled={!currentPrice}
              >
                Sell (Short)
              </Button>
            </div>
          </>
        )}
      </CardContent>

      <Dialog open={tradeSettingsOpen} onOpenChange={setTradeSettingsOpen}>
        <DialogContent className="max-w-md">
          <DialogHeader>
            <DialogTitle>Trade Settings</DialogTitle>
          </DialogHeader>
          <div className="space-y-3 text-sm">
            <div className="space-y-1">
              <Label className="text-xs text-zinc-400">Leverage</Label>
              <Input
                value={draftLeverage}
                onChange={(e) => setDraftLeverage(e.target.value)}
              />
            </div>
            <div className="space-y-1">
              <Label className="text-xs text-zinc-400">Current Money</Label>
              <div className="rounded-md border border-zinc-800 bg-zinc-900/40 px-3 py-2 text-xs font-mono">
                {formatMoney(equity)}
              </div>
            </div>
            <div className="grid grid-cols-2 gap-3">
              <div className="space-y-1">
                <Label className="text-xs text-zinc-400">Realized P&amp;L</Label>
                <div
                  className={`rounded-md border border-zinc-800 bg-zinc-900/40 px-3 py-2 text-xs font-mono ${
                    realizedPnl >= 0 ? "text-emerald-400" : "text-red-400"
                  }`}
                >
                  {realizedPnl >= 0 ? "+" : ""}
                  {formatMoney(realizedPnl)}
                </div>
              </div>
              <div className="space-y-1">
                <Label className="text-xs text-zinc-400">Unrealized P&amp;L</Label>
                <div
                  className={`rounded-md border border-zinc-800 bg-zinc-900/40 px-3 py-2 text-xs font-mono ${
                    unrealizedPnl >= 0 ? "text-emerald-400" : "text-red-400"
                  }`}
                >
                  {unrealizedPnl >= 0 ? "+" : ""}
                  {formatMoney(unrealizedPnl)}
                </div>
              </div>
            </div>
            <div className="space-y-1">
              <Label className="text-xs text-zinc-400">Max Size</Label>
              <div className="rounded-md border border-zinc-800 bg-zinc-900/40 px-3 py-2 text-xs">
                {maxSizing ? `${maxSizing.qty} ${sizingLabel}` : "--"}
              </div>
            </div>
            <div className="space-y-1">
              <Label className="text-xs text-zinc-400">
                Trade Size ({sizingLabel})
              </Label>
              <div className="flex items-center gap-2">
                <Input
                  value={orderQty}
                  onChange={(e) => setOrderQty(e.target.value)}
                  placeholder={maxSizing ? String(maxSizing.qty) : "0"}
                />
                <Button
                  variant="outline"
                  size="sm"
                  onClick={() => {
                    if (maxSizing) setOrderQty(String(maxSizing.qty));
                  }}
                >
                  Max
                </Button>
              </div>
            </div>
            <div className="flex justify-end gap-2 pt-2">
              <Button
                variant="outline"
                onClick={() => setTradeSettingsOpen(false)}
              >
                Close
              </Button>
              <Button onClick={handleUpdateAccount}>Save</Button>
            </div>
          </div>
        </DialogContent>
      </Dialog>

      <Dialog open={detailsOpen} onOpenChange={setDetailsOpen}>
        <DialogContent className="max-w-4xl w-[92vw]">
          <DialogHeader>
            <DialogTitle>Account Details</DialogTitle>
          </DialogHeader>
          <div className="space-y-4">
            <div>
              <div className="flex items-center justify-between">
                <div className="text-sm font-medium">Switch Account</div>
                <Button size="sm" onClick={() => setCreateOpen(true)}>
                  New Account
                </Button>
              </div>
              <div className="mt-2 flex flex-wrap gap-2">
                {accounts.map((account) => (
                  <Button
                    key={account.id}
                    size="sm"
                    variant={
                      account.id === activeAccountId ? "default" : "outline"
                    }
                    onClick={() => setActiveAccountId(account.id)}
                  >
                    {account.name}
                  </Button>
                ))}
                {accounts.length === 0 && (
                  <div className="text-xs text-zinc-500">
                    No accounts yet.
                  </div>
                )}
              </div>
            </div>

            <Separator className="bg-zinc-800" />

            <div>
              <div className="text-sm font-medium">Trade History</div>
              <ScrollArea className="mt-2 h-[240px] rounded-md border border-zinc-800">
                {trades.length === 0 && (
                  <div className="p-4 text-center text-sm text-zinc-500">
                    No trades yet.
                  </div>
                )}
                {trades.length > 0 && (
                  <div className="divide-y divide-zinc-800 text-sm">
                    <div className="grid grid-cols-6 gap-2 px-3 py-2 text-xs text-zinc-500">
                      <div>Side</div>
                      <div>Ticker</div>
                      <div>Status</div>
                      <div className="text-right">Starting Price</div>
                      <div className="text-right">P&amp;L</div>
                      <div className="text-right">Action</div>
                    </div>
                    {tradeRows.map(({ trade, pnl }) => {
                      const livePrice = getCurrentPrice(trade.symbol);
                      return (
                        <div
                          key={trade.id}
                          className="grid grid-cols-6 gap-2 px-3 py-2 items-center"
                        >
                          <div className="flex items-center">
                            <Badge
                              variant="outline"
                              className={
                                trade.side === "LONG"
                                  ? "text-emerald-400 border-emerald-400/30"
                                  : "text-red-400 border-red-400/30"
                              }
                            >
                              {trade.side}
                            </Badge>
                          </div>
                          <div className="font-mono text-xs">{trade.symbol}</div>
                          <div>
                            <Badge
                              variant="outline"
                              className={
                                trade.closedAt
                                  ? "text-zinc-400 border-zinc-500/40"
                                  : "text-amber-300 border-amber-300/30"
                              }
                            >
                              {trade.closedAt ? "CLOSED" : "OPEN"}
                            </Badge>
                          </div>
                          <div className="font-mono text-xs text-right">
                            {formatMoney(trade.entryPrice)}
                          </div>
                          <div
                            className={`text-xs font-mono ${
                              (pnl ?? 0) >= 0
                                ? "text-emerald-400"
                                : "text-red-400"
                            }`}
                          >
                            <span className="block text-right">
                              {pnl === null ? "--" : formatMoney(pnl)}
                            </span>
                          </div>
                          <div className="text-right">
                            {!trade.closedAt && (
                              <Button
                                size="sm"
                                variant="outline"
                                className="h-7 px-2 text-[11px]"
                                onClick={() => handleCloseTrade(trade)}
                                disabled={!livePrice || livePrice <= 0}
                              >
                                Close
                              </Button>
                            )}
                          </div>
                        </div>
                      );
                    })}
                  </div>
                )}
              </ScrollArea>
            </div>
          </div>
        </DialogContent>
      </Dialog>

      <Dialog open={createOpen} onOpenChange={setCreateOpen}>
        <DialogContent className="max-w-md">
          <DialogHeader>
            <DialogTitle>New Account</DialogTitle>
          </DialogHeader>
          <div className="space-y-3">
            <div className="space-y-1">
              <Label className="text-xs text-zinc-400">Account Name</Label>
              <Input
                value={newName}
                onChange={(e) => setNewName(e.target.value)}
                placeholder="Primary"
              />
            </div>
            <div className="grid grid-cols-2 gap-3">
              <div className="space-y-1">
                <Label className="text-xs text-zinc-400">Balance</Label>
                <Input
                  value={newBalance}
                  onChange={(e) => setNewBalance(e.target.value)}
                />
              </div>
              <div className="space-y-1">
                <Label className="text-xs text-zinc-400">Leverage</Label>
                <Input
                  value={newLeverage}
                  onChange={(e) => setNewLeverage(e.target.value)}
                />
              </div>
            </div>
            <div className="flex justify-end gap-2">
              <Button variant="outline" onClick={() => setCreateOpen(false)}>
                Cancel
              </Button>
              <Button onClick={handleCreateAccount}>Create</Button>
            </div>
          </div>
        </DialogContent>
      </Dialog>
    </Card>
  );
}
