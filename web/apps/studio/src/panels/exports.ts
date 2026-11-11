/**
 * The File menu's exports of the open cell: the capture poses, the joint trajectory and the motion
 * report of a robot, the scenario and the baked motion. The kernel computes each when it is
 * chosen; a failure (a scenario that does not compile) is shown.
 */

import { toast } from "@vitavision/workbench";

import { exportFile, jsonText } from "../io/export";
import { parentDir } from "../io/tauri";
import { type Baked, describeFailure } from "../kernel/etendue";
import type { ExportItem } from "./Header";

export function exportsOf(current: Baked | null, robot: string | null): ExportItem[] {
  const scenario = current?.loaded.scenario ?? null;
  if (!current || !scenario) return [];
  const dir = current.loaded.origin ? parentDir(current.loaded.origin) : null;
  const exporting = (title: string, name: string, type: string, text: () => string) => () => {
    let body: string;
    try {
      body = text();
    } catch (e) {
      const failure = describeFailure(e);
      toast({ title: `Could not export: ${failure.title}`, description: failure.message, tone: "error" });
      return;
    }
    void exportFile(title, { name, text: body, type }, dir);
  };
  const { session } = current;
  return [
    ...(robot
      ? [
          {
            label: `Captures of ${robot} (CSV)…`,
            run: exporting("Export the capture poses", "captures.csv", "text/csv", () =>
              session.exportCaptures(scenario, robot, "csv"),
            ),
          },
          {
            label: `Captures of ${robot} (JSON)…`,
            run: exporting("Export the capture poses", "captures.json", "application/json", () =>
              session.exportCaptures(scenario, robot, "json"),
            ),
          },
          {
            label: `Trajectory of ${robot} (CSV)…`,
            run: exporting("Export the joint trajectory", "trajectory.csv", "text/csv", () =>
              session.exportTrajectory(scenario, robot),
            ),
          },
          {
            label: "Motion report (JSON)…",
            run: exporting("Export the motion report", "motion_report.json", "application/json", () =>
              jsonText(session.motionReport(scenario)),
            ),
          },
        ]
      : []),
    {
      label: "Scenario (JSON)…",
      run: exporting("Export the scenario", "scenario.json", "application/json", () => jsonText(scenario)),
    },
    {
      label: "Baked motion (JSON)…",
      run: exporting("Export the baked motion", "baked.json", "application/json", () => jsonText(current.baked)),
    },
  ];
}
