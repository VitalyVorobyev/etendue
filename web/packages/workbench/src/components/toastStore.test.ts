import { afterEach, describe, expect, it, vi } from "vitest";

import { createToastStore, defaultToastStore, toast } from "./toastStore";

describe("createToastStore", () => {
  it("adds toasts with defaults, oldest first", () => {
    const store = createToastStore();
    const a = store.toast({ title: "Loaded" });
    const b = store.toast({ title: "Failed", tone: "error", description: "Bad JSON" });
    expect(store.getSnapshot()).toEqual([
      { id: a, title: "Loaded", description: undefined, tone: "info", duration: 5000 },
      { id: b, title: "Failed", description: "Bad JSON", tone: "error", duration: 8000 },
    ]);
  });

  it("replaces a toast in place by id", () => {
    const store = createToastStore();
    store.toast({ id: "bake", title: "Baking…", duration: Infinity });
    store.toast({ title: "Other" });
    store.toast({ id: "bake", title: "Baked", tone: "success" });
    expect(store.getSnapshot().map((t) => t.title)).toEqual(["Baked", "Other"]);
  });

  it("dismisses one or all, notifying only on a change", () => {
    const store = createToastStore();
    const listener = vi.fn();
    const unsubscribe = store.subscribe(listener);
    const a = store.toast({ title: "A" });
    store.toast({ title: "B" });
    store.dismiss(a);
    expect(store.getSnapshot().map((t) => t.title)).toEqual(["B"]);
    store.dismiss("missing");
    store.dismiss();
    store.dismiss();
    expect(store.getSnapshot()).toEqual([]);
    expect(listener).toHaveBeenCalledTimes(4);
    unsubscribe();
    store.toast({ title: "C" });
    expect(listener).toHaveBeenCalledTimes(4);
  });

  it("keeps the same snapshot until something changes", () => {
    const store = createToastStore();
    const before = store.getSnapshot();
    expect(store.getSnapshot()).toBe(before);
    store.toast({ title: "A" });
    expect(store.getSnapshot()).not.toBe(before);
  });
});

describe("toast", () => {
  afterEach(() => toast.dismiss());

  it("writes to the default store", () => {
    const id = toast({ title: "Hello", tone: "warn", duration: 100 });
    expect(defaultToastStore.getSnapshot()).toEqual([
      { id, title: "Hello", description: undefined, tone: "warn", duration: 100 },
    ]);
    toast.dismiss(id);
    expect(defaultToastStore.getSnapshot()).toEqual([]);
  });
});
