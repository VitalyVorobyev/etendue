import { describe, expect, it } from "vitest";

import { formatNumber, parseNumber } from "./numberText";

describe("formatNumber", () => {
  it("prints a fixed number of decimals", () => {
    expect(formatNumber(1.23456, 3)).toBe("1.235");
    expect(formatNumber(2, 1)).toBe("2.0");
    expect(formatNumber(-12.5, 0)).toBe("-13");
  });

  it("never prints a negative zero", () => {
    expect(formatNumber(-0.0001, 3)).toBe("0.000");
    expect(formatNumber(-0, 0)).toBe("0");
    expect(formatNumber(-0.0006, 3)).toBe("-0.001");
  });

  it("prints non-finite values as themselves", () => {
    expect(formatNumber(Number.NaN, 2)).toBe("NaN");
    expect(formatNumber(Number.NEGATIVE_INFINITY, 2)).toBe("-Infinity");
  });
});

describe("parseNumber", () => {
  it("reads numbers, including exponents", () => {
    expect(parseNumber(" 1.5 ")).toBe(1.5);
    expect(parseNumber("-2e-3")).toBe(-0.002);
  });

  it("rejects empty, partial and non-numeric text", () => {
    expect(parseNumber("")).toBeNull();
    expect(parseNumber("-")).toBeNull();
    expect(parseNumber("1e")).toBeNull();
    expect(parseNumber("abc")).toBeNull();
    expect(parseNumber("Infinity")).toBeNull();
  });
});
