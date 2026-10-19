/** The Optics workspace's main view: the head's section (2-D, with handles) or its anatomy in 3-D. */

import { SegmentedControl } from "@vitavision/ui";
import { useState } from "react";

import type { HeadAnalysis } from "./analysis";
import { Anatomy3D } from "./Anatomy3D";
import type { HeadParams } from "./design";
import { SectionView } from "./SectionView";

type View = "section" | "3d";

const VIEWS: { value: View; label: string }[] = [
  { value: "section", label: "Section" },
  { value: "3d", label: "3-D" },
];

export function HeadView({
  analysis,
  params,
  onChange,
}: {
  analysis: HeadAnalysis;
  params: HeadParams;
  onChange: (p: HeadParams) => void;
}) {
  const [view, setView] = useState<View>("section");
  return (
    <div className="relative h-full min-h-0">
      {view === "section" ? (
        <SectionView section={analysis.section} params={params} onChange={onChange} />
      ) : (
        <Anatomy3D analysis={analysis} params={params} />
      )}
      <div className="absolute top-2 left-2">
        <SegmentedControl
          aria-label="Head view"
          value={view}
          options={VIEWS}
          onValueChange={(v) => {
            const next = VIEWS.find((x) => x.value === v);
            if (next) setView(next.value);
          }}
        />
      </div>
    </div>
  );
}
