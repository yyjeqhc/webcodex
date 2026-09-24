import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { App } from "./App.js";
import { UiProvider } from "../ui/UiProvider.js";
import "@mantine/core/styles.css";
import "../ui/foundation.css";
import "./styles.css";
import "./styles-production.css";
import "./styles-agents.css";
import "./styles-responsive.css";
import "./styles-glass.css";

const root = document.getElementById("root");
if (!root) throw new Error("Runtime WebUI root element is missing");

createRoot(root).render(
  <StrictMode>
    <UiProvider><App /></UiProvider>
  </StrictMode>,
);

import "./styles-workflow.css";
