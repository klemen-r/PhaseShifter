import type { LineData, Time, UTCTimestamp } from "lightweight-charts";

export type Candle = {
  time: UTCTimestamp | string;
  open: number;
  high: number;
  low: number;
  close: number;
  volume?: number;
};

export function calculateDonchianMidpoint({
  phaseAmount,
  data,
}: {
  phaseAmount: number;
  data: Candle[];
}): LineData<Time>[] {
  const midpointData: LineData<Time>[] = [];

  for (let i = phaseAmount - 1; i < data.length; i++) {
    let upper = -Infinity;
    let lower = Infinity;

    for (let j = i - phaseAmount + 1; j <= i; j++) {
      const bar = data[j];
      if (bar.high > upper) upper = bar.high;
      if (bar.low < lower) lower = bar.low;
    }

    const mid = (upper + lower) / 2;

    midpointData.push({
      time: data[i].time,
      value: mid,
    });
  }

  return midpointData;
}
