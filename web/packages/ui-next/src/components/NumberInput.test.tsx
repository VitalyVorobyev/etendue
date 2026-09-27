import { render } from "@testing-library/react";
import { createRef } from "react";
import { describe, expect, it } from "vitest";

import { NumberInput as UiNumberInput } from "@vitavision/ui";

import { NumberInput } from "./NumberInput";

describe("NumberInput", () => {
  it("renders exactly ui's NumberInput when there is no unit", () => {
    const ours = render(<NumberInput defaultValue="3" min={0} className="w-24" aria-describedby="hint" />);
    const theirs = render(<UiNumberInput defaultValue="3" min={0} className="w-24" aria-describedby="hint" />);
    expect(ours.container.innerHTML).toBe(theirs.container.innerHTML);
  });

  it("forwards the ref to the input, with or without a unit", () => {
    const ref = createRef<HTMLInputElement>();
    render(<NumberInput ref={ref} unit="mm" defaultValue="1" />);
    expect(ref.current?.tagName).toBe("INPUT");
  });

  it("keeps the caller's own style, over the room made for the unit", () => {
    const { container } = render(<NumberInput unit="px" defaultValue="1" style={{ paddingInlineEnd: "3rem", color: "red" }} />);
    const input = container.querySelector("input");
    expect(input?.style.paddingInlineEnd).toBe("3rem");
    expect(input?.style.color).toBe("red");
  });
});
