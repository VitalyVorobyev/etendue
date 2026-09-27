import { fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import { FileDrop } from "./FileDrop";

describe("FileDrop", () => {
  afterEach(() => {
    vi.restoreAllMocks();
  });

  it("opens the hidden input from its button", () => {
    const click = vi.spyOn(HTMLInputElement.prototype, "click").mockImplementation(() => undefined);
    render(<FileDrop onFiles={() => undefined} />);
    fireEvent.click(screen.getByRole("button", { name: "Open files…" }));
    expect(click).toHaveBeenCalledTimes(1);
  });

  it("offers a folder picker with `directory`, and a custom label", () => {
    const { container } = render(<FileDrop onFiles={() => undefined} directory buttonLabel="Open scenario…" />);
    expect(screen.getByRole("button", { name: "Open scenario…" })).toBeTruthy();
    expect(container.querySelector("input")?.multiple).toBe(true);
  });
});
