"""Gate G13.1: the native `etendue calibrate` against this wheel's closed loop, row by row.

    etendue calibrate <dir> [--camera ID] -o report.json
    uv run --locked --project tools/closed-loop tools/closed-loop/parity.py report.json <dir> [--camera ID]

Runs closed_loop.py's `main` on `<dir>` (with its arguments), captures its comparison rows
instead of printing them, and checks that the report's rows have the same labels and values
within max(1e-9, 1e-6 · |value|).
"""

import json
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))
import closed_loop  # noqa: E402

ABS, REL = 1e-9, 1e-6


def main() -> int:
    report_path, *args = sys.argv[1:]
    captured: list[tuple[str, float]] = []

    def capture(title, rows, gate="G5.1", gated=True):
        captured.extend((label, float(value)) for label, value, _ in rows)
        return True

    closed_loop.report = capture
    sys.argv = ["closed_loop.py", *args]
    closed_loop.main()
    native = [(r["label"], r["value"]) for r in json.loads(Path(report_path).read_text())["reports"][0]["rows"]]
    if [label for label, _ in native] != [label for label, _ in captured]:
        print(f"G13.1: FAIL, the rows differ:\n  wheel  {[l for l, _ in captured]}\n  native {[l for l, _ in native]}")
        return 1
    worst, outside = 0.0, []
    for (label, a), (_, b) in zip(captured, native):
        worst = max(worst, abs(a - b))
        if abs(a - b) > max(ABS, REL * abs(a)):
            outside.append(f"{label}: wheel {a:.6e}, native {b:.6e}")
    print(f"G13.1 {args[0]}: {len(native)} rows, max |Δ| {worst:.3e}: {'PASS' if not outside else 'FAIL'}")
    for line in outside:
        print(f"  {line}")
    return 1 if outside else 0


if __name__ == "__main__":
    sys.exit(main())
