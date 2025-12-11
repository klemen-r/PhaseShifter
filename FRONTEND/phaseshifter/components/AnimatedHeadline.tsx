"use client";

import { useEffect, useRef } from "react";
import { createTimeline } from "animejs";

const TEXT = "Welcome to PhaseShifter!";

export default function AnimatedHeadline() {
  const rootRef = useRef<HTMLHeadingElement | null>(null);
  const lettersRef = useRef<HTMLSpanElement | null>(null);

  useEffect(() => {
    const root = rootRef.current;
    const lettersEl = lettersRef.current;
    if (!root || !lettersEl) return;

    const lineEls = root.querySelectorAll(".line");
    const letterEls = root.querySelectorAll(".letter");

    const rafId = requestAnimationFrame(() => {
      const lettersWidth = lettersEl.getBoundingClientRect().width;

      const timeline = createTimeline({
        loop: true,
        defaults: { ease: "outExpo" },
      });

      timeline
        .add(lineEls, {
          scaleY: [0, 1],
          opacity: [0.5, 1],
          duration: 700,
        })
        .add(lineEls, {
          x: [0, lettersWidth + 10],
          duration: 700,
          delay: 100,
        })
        .add(
          letterEls,
          {
            opacity: [0, 1],
            duration: 600,
            delay: (_el, i) => 34 * (i + 1),
          },
          "<-=775",
        )
        .add(root, {
          opacity: [1, 0],
          duration: 1000,
          delay: 1000,
        });

      // cleanup on unmount
      return () => {
        timeline.cancel();
      };
    });

    return () => {
      cancelAnimationFrame(rafId);
    };
  }, []);

  return (
    <h1 ref={rootRef} className="ml11">
      <span className="text-wrapper">
        <span className="line line1" />
        <span className="letters" ref={lettersRef}>
          {TEXT.split("").map((ch, i) => (
            <span key={i} className="letter">
              {ch === " " ? "\u00A0" : ch}
            </span>
          ))}
        </span>
      </span>
    </h1>
  );
}
