/**
 * The robot library: every robot the studio carries, each with its joints, their limits and
 * speeds, and its reach (the kernel's forward kinematics). One is added to the cell, attached to
 * the selected frame, or replaces the selected robot.
 */

import { Badge, Button, Dialog, Table } from "@vitavision/ui";
import { Bot } from "lucide-react";
import { useEffect, useState } from "react";
import type { LibraryRobot } from "virtual:etendue-robots";

import type { LoadedRobot } from "../io/load";
import { type RobotFacts, robotFacts } from "./facts";

const DEG = 180 / Math.PI;

type Joint = RobotFacts["joints"][number];

export function RobotLibrary({
  open,
  onOpenChange,
  robots,
  load,
  attachTo,
  replace,
  onAdd,
  onReplace,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  robots: readonly LibraryRobot[];
  /** Read a library robot (its manifest and URDF). */
  load: (robot: LibraryRobot) => Promise<LoadedRobot>;
  /** Where an added robot is attached. */
  attachTo: string;
  /** The cell's robot a choice would replace, if one is selected. */
  replace: string | null;
  onAdd: (robot: LibraryRobot) => void;
  onReplace: (robot: LibraryRobot) => void;
}) {
  const [chosen, setChosen] = useState<string | null>(null);
  const robot = robots.find((r) => r.manifest === chosen) ?? robots[0] ?? null;
  const [facts, setFacts] = useState<{ manifest: string; facts: RobotFacts | string } | null>(null);
  useEffect(() => {
    if (!open || !robot) return;
    let live = true;
    void load(robot)
      .then((loaded) => robotFacts(loaded))
      .catch((e: unknown) => (e instanceof Error ? e.message : String(e)))
      .then((f) => live && setFacts({ manifest: robot.manifest, facts: f }));
    return () => {
      live = false;
    };
  }, [open, robot, load]);
  const shown = facts && robot && facts.manifest === robot.manifest ? facts.facts : null;
  return (
    <Dialog
      open={open}
      onOpenChange={onOpenChange}
      title="Robot library"
      description="The robots the studio carries: their joints, limits and reach."
      className="w-[min(44rem,calc(100vw-2rem))]"
      footer={
        robot && (
          <div className="flex flex-wrap justify-end gap-2">
            {replace && (
              <Button
                variant="ghost"
                onClick={() => {
                  onReplace(robot);
                  onOpenChange(false);
                }}
              >
                Replace {replace}
              </Button>
            )}
            <Button
              icon={<Bot className="size-4" />}
              onClick={() => {
                onAdd(robot);
                onOpenChange(false);
              }}
            >
              Add to {attachTo}
            </Button>
          </div>
        )
      }
    >
      <div className="grid min-h-72 grid-cols-[minmax(0,12rem)_minmax(0,1fr)] gap-4" data-testid="robot-library">
        <ul className="flex flex-col gap-0.5" aria-label="Robots">
          {robots.map((r) => (
            <li key={r.manifest}>
              <button
                type="button"
                aria-current={r === robot}
                className={`w-full rounded px-2 py-1.5 text-left text-sm ${
                  r === robot ? "bg-surface-raised font-medium text-fg" : "text-fg-muted hover:bg-surface-raised"
                }`}
                onClick={() => setChosen(r.manifest)}
              >
                {r.name}
                {r.local && <span className="ml-1 text-xs text-fg-muted">(local only)</span>}
              </button>
            </li>
          ))}
        </ul>
        <div className="flex min-w-0 flex-col gap-3" data-testid="robot-facts">
          {shown === null ? (
            <p className="text-sm text-fg-muted">Reading the robot…</p>
          ) : typeof shown === "string" ? (
            <p className="text-sm text-defect">{shown}</p>
          ) : (
            <>
              <div className="flex flex-wrap items-center gap-2 text-sm">
                <Badge tone="neutral">{shown.dof} axes</Badge>
                <span>
                  TCP up to <span className="font-mono tabular-nums">{(shown.reach_m * 1000).toFixed(0)} mm</span> from
                  the base
                </span>
                <span className="text-fg-muted">({(shown.home_m * 1000).toFixed(0)} mm at rest)</span>
                {!shown.meshes_redistributable && <Badge tone="defect">meshes not redistributable</Badge>}
              </div>
              <Table
                columns={[
                  {
                    key: "name",
                    header: "Joint",
                    cell: (j: Joint) => <span className="font-mono text-xs">{j.name}</span>,
                  },
                  {
                    key: "range",
                    header: "Range",
                    cell: (j: Joint) => (
                      <span className="font-mono text-xs tabular-nums">
                        {(j.lower * DEG).toFixed(0)}° … {(j.upper * DEG).toFixed(0)}°
                      </span>
                    ),
                  },
                  {
                    key: "speed",
                    header: "Speed",
                    cell: (j: Joint) => (
                      <span className="font-mono text-xs tabular-nums">{(j.max_velocity * DEG).toFixed(0)}°/s</span>
                    ),
                  },
                ]}
                rows={shown.joints}
                rowKey={(j) => j.name}
                caption="Joint limits, from the robot's manifest"
              />
              <p className="text-xs text-fg-muted">
                The farthest the TCP gets from the base frame's origin, over 4096 joint vectors spread through the
                limits. A datasheet's reach is usually taken from the shoulder axis, so it is shorter.
              </p>
            </>
          )}
        </div>
      </div>
    </Dialog>
  );
}
