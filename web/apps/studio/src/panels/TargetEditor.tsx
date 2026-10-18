import { type TargetSpec, poleCircumferences } from "@etendue/wasm";
import { NumberInput, Section, Select } from "@vitavision/ui";
import type { ReactNode } from "react";

/** The geometry of a target. */
type Geometry = TargetSpec["geometry"];

/** A target as calib-targets prints it. */
type PrintedTarget = Extract<Geometry, { type: "printed" }>["target"];

const DICTIONARIES = [
  "DICT_4X4_50",
  "DICT_4X4_100",
  "DICT_4X4_250",
  "DICT_4X4_1000",
  "DICT_5X5_50",
  "DICT_5X5_100",
  "DICT_5X5_250",
  "DICT_5X5_1000",
  "DICT_6X6_50",
  "DICT_6X6_100",
  "DICT_6X6_250",
  "DICT_6X6_1000",
  "DICT_7X7_50",
  "DICT_7X7_100",
  "DICT_7X7_250",
  "DICT_7X7_1000",
  "DICT_ARUCO_ORIGINAL",
];

const KINDS = [
  { value: "chessboard", label: "Chessboard" },
  { value: "charuco", label: "ChArUco" },
  { value: "marker_board", label: "Marker board" },
  { value: "puzzle_board", label: "PuzzleBoard" },
  { value: "puzzlepole", label: "PuzzlePole (cylinder)" },
  { value: "rectangle", label: "Plain rectangle" },
];

/**
 * A marker board's three circles as calib-targets places them by default: an L on an
 * even-parity square near the centre, so each disk contrasts with its square
 * (calib-targets-print `MarkerBoardTargetSpec::default_circles`).
 */
function defaultCircles(innerRows: number, innerCols: number) {
  const j = Math.max(0, Math.floor((innerRows + 1) / 2) - 1);
  let i = Math.max(0, Math.floor((innerCols + 1) / 2) - 1);
  if ((i + j) % 2 !== 0) i = i >= 1 ? i - 1 : i + 1;
  return [
    { i, j, polarity: "white" },
    { i: i + 1, j, polarity: "black" },
    { i: i + 1, j: j + 1, polarity: "white" },
  ] as [
    { i: number; j: number; polarity: "white" },
    { i: number; j: number; polarity: "black" },
    { i: number; j: number; polarity: "white" },
  ];
}

/** A new target of `kind`, sized like `from` where that makes sense. */
function newGeometry(kind: string, square: number): Geometry {
  const s = square;
  switch (kind) {
    case "chessboard":
      return { type: "printed", target: { kind, inner_rows: 6, inner_cols: 9, square_size_mm: s } };
    case "charuco":
      return {
        type: "printed",
        target: { kind, rows: 5, cols: 7, square_size_mm: s, marker_size_rel: 0.75, dictionary: "DICT_4X4_50" },
      };
    case "marker_board":
      return {
        type: "printed",
        target: { kind, inner_rows: 6, inner_cols: 9, square_size_mm: s, circles: defaultCircles(6, 9) },
      };
    case "puzzle_board":
      return { type: "printed", target: { kind, rows: 10, cols: 12, square_size_mm: s } };
    case "puzzlepole": {
      const period = poleCircumferences().find((p) => p.squares === 12) ?? poleCircumferences()[0]!;
      return {
        type: "printed",
        target: {
          kind,
          circumference_squares: period.squares,
          start_row: period.start_row,
          axial_squares: 6,
          square_size_mm: s,
        },
      };
    }
    default:
      return { type: "rectangle", width: 0.3, height: 0.2 };
  }
}

function Row({ label, children }: { label: string; children: ReactNode }) {
  return (
    <>
      <span className="self-center truncate text-xs text-fg-muted" title={label}>
        {label}
      </span>
      <div className="min-w-0">{children}</div>
    </>
  );
}

function Count({
  label,
  value,
  min,
  onChange,
}: {
  label: string;
  value: number;
  min: number;
  onChange: (v: number) => void;
}) {
  return (
    <Row label={label}>
      <NumberInput
        aria-label={label}
        value={value}
        precision={0}
        min={min}
        step={1}
        onValueChange={(v) => onChange(Math.round(v))}
      />
    </Row>
  );
}

function Size({
  label,
  value,
  unit,
  onChange,
  min = 0.01,
  max,
  precision = 2,
}: {
  label: string;
  value: number;
  unit: string;
  onChange: (v: number) => void;
  min?: number;
  max?: number;
  precision?: number;
}) {
  return (
    <Row label={label}>
      <NumberInput
        aria-label={label}
        value={value}
        unit={unit}
        precision={precision}
        min={min}
        max={max}
        step="any"
        onValueChange={onChange}
      />
    </Row>
  );
}

/** The print parameters of one printed target, editable. */
function PrintedFields({
  target,
  set: setField,
}: {
  target: PrintedTarget;
  set: (t: PrintedTarget, field: string) => void;
}) {
  // Each change names the field it sets, so a burst of edits of one field undoes together and a
  // change of another field is its own undo step.
  const set = (t: PrintedTarget) =>
    setField(
      t,
      Object.keys(t)
        .filter((k) => (t as Record<string, unknown>)[k] !== (target as Record<string, unknown>)[k])
        .join(", ")
        .replaceAll("_", " "),
    );
  const square = (
    <Size
      label="square"
      value={target.square_size_mm}
      unit="mm"
      onChange={(square_size_mm) => set({ ...target, square_size_mm })}
    />
  );
  switch (target.kind) {
    case "chessboard":
      return (
        <>
          <Count
            label="inner rows"
            value={target.inner_rows}
            min={1}
            onChange={(inner_rows) => set({ ...target, inner_rows })}
          />
          <Count
            label="inner cols"
            value={target.inner_cols}
            min={1}
            onChange={(inner_cols) => set({ ...target, inner_cols })}
          />
          {square}
        </>
      );
    case "marker_board":
      return (
        <>
          <Count
            label="inner rows"
            value={target.inner_rows}
            min={2}
            onChange={(inner_rows) =>
              set({ ...target, inner_rows, circles: defaultCircles(inner_rows, target.inner_cols) })
            }
          />
          <Count
            label="inner cols"
            value={target.inner_cols}
            min={2}
            onChange={(inner_cols) =>
              set({ ...target, inner_cols, circles: defaultCircles(target.inner_rows, inner_cols) })
            }
          />
          {square}
          <Size
            label="circle"
            value={(target.circle_diameter_rel ?? 0.5) * 100}
            unit="%"
            min={5}
            max={95}
            precision={0}
            onChange={(v) => set({ ...target, circle_diameter_rel: v / 100 })}
          />
        </>
      );
    case "charuco":
      return (
        <>
          <Count label="rows" value={target.rows} min={2} onChange={(rows) => set({ ...target, rows })} />
          <Count label="cols" value={target.cols} min={2} onChange={(cols) => set({ ...target, cols })} />
          {square}
          <Size
            label="marker"
            value={target.marker_size_rel * 100}
            unit="%"
            min={10}
            max={95}
            precision={0}
            onChange={(v) => set({ ...target, marker_size_rel: v / 100 })}
          />
          <Row label="dictionary">
            <Select
              aria-label="Dictionary"
              value={target.dictionary}
              options={DICTIONARIES.map((d) => ({ value: d, label: d }))}
              onValueChange={(dictionary) => dictionary !== "" && set({ ...target, dictionary })}
            />
          </Row>
        </>
      );
    case "puzzle_board":
      return (
        <>
          <Count label="rows" value={target.rows} min={2} onChange={(rows) => set({ ...target, rows })} />
          <Count label="cols" value={target.cols} min={2} onChange={(cols) => set({ ...target, cols })} />
          {square}
          <Count
            label="origin row"
            value={target.origin_row ?? 0}
            min={0}
            onChange={(origin_row) => set({ ...target, origin_row })}
          />
          <Count
            label="origin col"
            value={target.origin_col ?? 0}
            min={0}
            onChange={(origin_col) => set({ ...target, origin_col })}
          />
        </>
      );
    case "puzzlepole": {
      const periods = poleCircumferences();
      const key = `${target.circumference_squares}:${target.start_row}`;
      const diameter = (target.circumference_squares * target.square_size_mm) / Math.PI;
      return (
        <>
          <Row label="around">
            <Select
              aria-label="Squares around"
              value={key}
              options={periods.map((p) => ({
                value: `${p.squares}:${p.start_row}`,
                label: `${p.squares} squares`,
                note: `seam row ${p.start_row}`,
              }))}
              onValueChange={(v) => {
                const p = periods.find((x) => `${x.squares}:${x.start_row}` === v);
                if (p) set({ ...target, circumference_squares: p.squares, start_row: p.start_row });
              }}
            />
          </Row>
          <Count
            label="along"
            value={target.axial_squares}
            min={2}
            onChange={(axial_squares) => set({ ...target, axial_squares })}
          />
          {square}
          <Row label="cylinder">
            <span className="font-mono text-xs">
              ⌀ {diameter.toFixed(1)} mm × {(target.axial_squares * target.square_size_mm).toFixed(0)} mm
            </span>
          </Row>
        </>
      );
    }
  }
}

/**
 * A target's kind and parameters: any calib-targets target (as printed, millimetres), or a plain
 * rectangle. Changing the kind starts from that kind's defaults at the same square size.
 */
export function TargetEditor({
  target,
  extent,
  set,
}: {
  target: TargetSpec;
  extent: [number, number] | undefined;
  /** Apply a change of `field`; `null` for a read-only view. */
  set: ((t: TargetSpec, field: string) => void) | null;
}) {
  const g = target.geometry;
  const kind = g.type === "printed" ? g.target.kind : g.type === "rectangle" ? "rectangle" : null;
  const square =
    g.type === "printed"
      ? g.target.square_size_mm
      : g.type === "board" && "square_size_m" in g.board
        ? g.board.square_size_m * 1000
        : 25;
  const setGeometry = (geometry: Geometry, field: string) => set?.({ ...target, geometry }, field);
  let fields: ReactNode = null;
  if (g.type === "printed" && set) {
    fields = <PrintedFields target={g.target} set={(t, field) => setGeometry({ type: "printed", target: t }, field)} />;
  } else if (g.type === "rectangle" && set) {
    fields = (
      <>
        <Size
          label="width"
          value={g.width * 1000}
          unit="mm"
          precision={1}
          onChange={(v) => setGeometry({ ...g, width: v / 1000 }, "width")}
        />
        <Size
          label="height"
          value={g.height * 1000}
          unit="mm"
          precision={1}
          onChange={(v) => setGeometry({ ...g, height: v / 1000 }, "height")}
        />
      </>
    );
  } else if (g.type === "board") {
    fields = (
      <Row label="layout">
        <span className="font-mono text-xs">
          {Object.entries(g.board)
            .map(([key, v]) => `${key} ${String(v)}`)
            .join(", ")}
        </span>
      </Row>
    );
  }
  return (
    <Section title="Target">
      <div className="grid grid-cols-[minmax(4.5rem,auto)_1fr] gap-x-3 gap-y-1.5">
        <Row label="kind">
          {set ? (
            <Select
              aria-label="Target kind"
              value={kind ?? ""}
              placeholder={g.type === "board" ? `${g.board.kind} (dataset board)` : undefined}
              options={KINDS}
              onValueChange={(k) => k !== "" && k !== kind && setGeometry(newGeometry(k, square), "kind")}
            />
          ) : (
            <span className="font-mono text-xs">{kind ?? (g.type === "board" ? g.board.kind : "")}</span>
          )}
        </Row>
        {fields}
        {extent && (
          <Row label="extent">
            <span className="font-mono text-xs">
              {(extent[0] * 1000).toFixed(1)} × {(extent[1] * 1000).toFixed(1)} mm
            </span>
          </Row>
        )}
      </div>
    </Section>
  );
}
