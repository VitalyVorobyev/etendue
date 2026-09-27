import { dom } from "@vitavision/config-vitest";

// The studio's pure logic (file resolution, the scene tree, target geometry) in happy-dom;
// the app as a whole is covered by the Playwright suite in e2e/.
export default dom({ resolve: { conditions: ["@vitavision/source"] } });
