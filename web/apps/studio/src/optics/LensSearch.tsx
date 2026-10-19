/**
 * The lens search: every lens of the project's library optimised on the head, ranked, each one
 * applicable (lens and setting together).
 */

import { Button, Table } from "@vitavision/ui";
import { Search } from "lucide-react";
import { useState } from "react";

import { type Project, itemsOf } from "../io/project";
import type { HeadParams } from "./design";
import { type LensCandidate, type LensMiss, searchLenses } from "./lenses";

export function LensSearch({
  params,
  project,
  tiltFree,
  onApply,
}: {
  params: HeadParams;
  project: Project | null;
  tiltFree: boolean;
  onApply: (p: HeadParams) => void;
}) {
  const [found, setFound] = useState<{
    for: HeadParams;
    tiltFree: boolean;
    ranked: LensCandidate[];
    misses: LensMiss[];
  } | null>(null);
  const lenses = project
    ? itemsOf(project, "lens").flatMap(([ref, item]) => (item.lens ? [{ ref, name: item.name, lens: item.lens }] : []))
    : [];
  if (lenses.length === 0) return null;
  const shown = found?.for === params && found.tiltFree === tiltFree ? found : null;
  return (
    <div className="flex flex-col gap-2">
      <Button
        size="sm"
        variant="ghost"
        icon={<Search className="size-3.5" />}
        onClick={() => setFound({ for: params, tiltFree, ...searchLenses(params, lenses, tiltFree) })}
        title="Optimise the head with every lens of the library, and rank them"
      >
        Search {lenses.length} lenses
      </Button>
      {shown && (
        <div className="flex flex-col gap-1 text-xs" data-testid="lens-search">
          <Table
            columns={[
              {
                key: "lens",
                header: "Lens",
                cell: (c: LensCandidate) => (
                  <span className={c.params.lens.name === params.lens.name ? "font-medium" : ""}>{c.name}</span>
                ),
              },
              {
                key: "seen",
                header: "Seen",
                cell: (c: LensCandidate) => <span className="tabular-nums">{(c.coverage * 100).toFixed(1)} %</span>,
              },
              {
                key: "blur",
                header: "Worst blur",
                cell: (c: LensCandidate) => <span className="tabular-nums">{c.worstBlurPx.toFixed(2)} px</span>,
              },
              {
                key: "depth",
                header: "Depth / px",
                cell: (c: LensCandidate) => (
                  <span className="tabular-nums">≤ {(c.worstDepthPerPx * 1e6).toFixed(0)} µm</span>
                ),
              },
              {
                key: "setting",
                header: "Setting",
                cell: (c: LensCandidate) => (
                  <span className="font-mono tabular-nums">
                    f/{c.params.f_number.toFixed(1)} · {(c.params.focus_distance_m * 1000).toFixed(0)} mm
                  </span>
                ),
              },
              {
                key: "apply",
                header: "",
                cell: (c: LensCandidate) => (
                  <Button
                    size="sm"
                    variant="ghost"
                    onClick={() => {
                      onApply(c.params);
                      setFound(null);
                    }}
                  >
                    Apply
                  </Button>
                ),
              },
            ]}
            rows={shown.ranked}
            rowKey={(c) => c.ref}
            caption="Most of the window seen first, then the smallest worst blur"
          />
          {shown.misses.map((m) => (
            <p key={m.ref} className="text-fg-muted">
              {m.name}: {m.reason}
            </p>
          ))}
        </div>
      )}
    </div>
  );
}
