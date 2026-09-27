import { act, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { Toaster } from "./Toaster";
import { createToastStore } from "./toastStore";

describe("Toaster", () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });
  afterEach(() => {
    vi.useRealTimers();
  });

  it("dismisses a toast after its duration", () => {
    const store = createToastStore();
    render(<Toaster store={store} />);
    act(() => {
      store.toast({ title: "Saved", duration: 1000 });
    });
    expect(screen.getByText("Saved")).toBeTruthy();
    act(() => {
      vi.advanceTimersByTime(999);
    });
    expect(screen.queryByText("Saved")).not.toBeNull();
    act(() => {
      vi.advanceTimersByTime(1);
    });
    expect(screen.queryByText("Saved")).toBeNull();
  });

  it("keeps the rest of the countdown across a focus pause", () => {
    const store = createToastStore();
    render(<Toaster store={store} />);
    act(() => {
      store.toast({ title: "Saved", duration: 1000 });
    });
    act(() => {
      vi.advanceTimersByTime(600);
    });
    const dismiss = screen.getByRole("button", { name: "Dismiss" });
    fireEvent.focus(dismiss);
    act(() => {
      vi.advanceTimersByTime(5000);
    });
    expect(screen.queryByText("Saved")).not.toBeNull();
    // Focus moving within the stack is not leaving it.
    fireEvent.blur(dismiss, { relatedTarget: dismiss });
    act(() => {
      vi.advanceTimersByTime(5000);
    });
    expect(screen.queryByText("Saved")).not.toBeNull();
    fireEvent.blur(dismiss, { relatedTarget: document.body });
    act(() => {
      vi.advanceTimersByTime(399);
    });
    expect(screen.queryByText("Saved")).not.toBeNull();
    act(() => {
      vi.advanceTimersByTime(1);
    });
    expect(screen.queryByText("Saved")).toBeNull();
  });

  it("restarts the countdown when a toast is replaced in place, and never ends an infinite one", () => {
    const store = createToastStore();
    render(<Toaster store={store} />);
    act(() => {
      store.toast({ id: "bake", title: "Baking…", duration: Infinity });
    });
    act(() => {
      vi.advanceTimersByTime(60_000);
    });
    expect(screen.queryByText("Baking…")).not.toBeNull();
    act(() => {
      store.toast({ id: "bake", title: "Baked", tone: "success", duration: 500 });
    });
    act(() => {
      vi.advanceTimersByTime(500);
    });
    expect(screen.queryByText("Baked")).toBeNull();
  });
});
