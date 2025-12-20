"use client";

import { useContext } from "react";
import {
  TradingDataContext,
  type TradingDataContextValue,
  type TickerData,
} from "./TradingDataContext";

export type { TickerData };
export type UseTradingDataReturn = TradingDataContextValue;

export function useTradingData(): UseTradingDataReturn {
  const context = useContext(TradingDataContext);

  if (!context) {
    throw new Error(
      "useTradingData must be used within a TradingDataProvider"
    );
  }

  return context;
}
