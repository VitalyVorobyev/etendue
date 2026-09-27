/**
 * Every story in this package as a test — rendered, `play` run — in Chromium (PLAN §4.3).
 * Counts towards this package's coverage; axe and SSR run over the same stories in
 * `apps/storybook`.
 */

import type { Decorator } from "@storybook/react-vite";
import { runStories } from "@vitavision/config-vitest/stories";

import { TooltipProvider } from "@vitavision/ui";

const withTooltips: Decorator = (Story) => (
  <TooltipProvider>
    <Story />
  </TooltipProvider>
);

runStories(import.meta.glob("./**/*.stories.tsx", { eager: true }), { decorators: [withTooltips] });
