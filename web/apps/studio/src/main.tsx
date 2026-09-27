import "@fontsource-variable/ibm-plex-sans";
import "@fontsource/ibm-plex-mono";
import "./styles.css";

import { TooltipProvider, initTheme } from "@vitavision/ui";
import { StrictMode } from "react";
import { createRoot } from "react-dom/client";

import { App } from "./App";

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <TooltipProvider>
      <App />
    </TooltipProvider>
  </StrictMode>,
);
initTheme();
