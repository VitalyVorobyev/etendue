import "@fontsource-variable/ibm-plex-sans";
import "@fontsource/ibm-plex-mono";
import "./styles.css";

import { TooltipProvider, initTheme } from "@vitavision/ui";
import { StrictMode } from "react";
import { createRoot } from "react-dom/client";

import { App } from "./App";
import { ErrorBoundary } from "./panels/ErrorBoundary";

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <TooltipProvider>
      <ErrorBoundary area="The studio" recover={{ label: "Reload", run: () => window.location.reload() }}>
        <App />
      </ErrorBoundary>
    </TooltipProvider>
  </StrictMode>,
);
initTheme();
