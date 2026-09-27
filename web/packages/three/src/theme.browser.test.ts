import { Color } from "three";
import { describe, expect, it } from "vitest";

import { normalizeColor } from "./theme";

describe("normalizeColor (Chromium's parser)", () => {
  it("turns modern CSS colour syntax into something three parses", () => {
    for (const css of ["hsl(191 75% 34%)", "oklch(0.7 0.1 200)", "color-mix(in srgb, red 50%, blue)", "rgb(10 20 30 / 50%)"]) {
      const out = normalizeColor(css);
      const c = new Color("black");
      c.setStyle(out);
      expect(out, css).not.toBe(css);
      expect(c.getHex(), css).not.toBe(0);
    }
  });

  it("keeps plain values, and falls back for empty and invalid ones", () => {
    expect(normalizeColor(" red ")).toBe("rgb(255, 0, 0)");
    expect(normalizeColor("")).toBe("gray");
    expect(normalizeColor("not a colour")).toBe("not a colour");
  });
});
