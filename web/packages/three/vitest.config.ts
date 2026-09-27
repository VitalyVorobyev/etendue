import { DOD_COVERAGE, dom } from "@vitavision/config-vitest";

// No components, so no stories project: the unit suite (happy-dom) is the whole suite, held
// to the DoD coverage thresholds. Rendering is exercised by @vitavision/three-react's stories.
export default dom({ test: { coverage: { thresholds: DOD_COVERAGE } } });
