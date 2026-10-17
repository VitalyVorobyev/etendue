import { describe, expect, it } from "bun:test";

import { HEX, RAW_PALETTE } from "./tokens-only.js";

describe("tokens-only patterns", () => {
  it.each([
    "bg-gray-500",
    "px-2 text-blue-600",
    "hover:ring-red-400 p-1",
    "border-slate-200/50",
    "border-t-zinc-700",
    "fill-emerald-500",
  ])("flags palette class in %p", (s) => expect(RAW_PALETTE.test(s)).toBe(true));

  it.each(["bg-surface", "text-fg-muted", "ring-signal", "text-2xl", "bg-grayish-500", "gray-500", "grid-cols-3"])(
    "allows %p",
    (s) => expect(RAW_PALETTE.test(s)).toBe(false),
  );

  it.each(["#fff", "#1e293b", "fill: #abcdef", "#ffffff80"])("flags hex in %p", (s) => expect(HEX.test(s)).toBe(true));

  it.each(["#section", "&#123;", "#12", "#12345", "a#b"])("allows %p", (s) => expect(HEX.test(s)).toBe(false));
});
