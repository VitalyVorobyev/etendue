/**
 * Every story in this package as a test — rendered, `play` run — in Chromium (lab-ui PLAN §4.3).
 * The stories render real WebGL, so this is also the rendering test of `@vitavision/three`.
 */

import { runStories } from "@vitavision/config-vitest/stories";

runStories(import.meta.glob("./**/*.stories.tsx", { eager: true }));
